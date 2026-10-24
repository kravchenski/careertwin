"""A project you will defend: read once into a dossier, searched for every question.

The folder is read locally (secrets and binaries skipped, tokens blanked), the AI writes a dossier
of it once, and from then on every answer sees the dossier plus the few code excerpts that best match
the question. Finding the excerpts is a keyword search over the project's own text, so it costs
milliseconds and never waits for the network.
"""

from __future__ import annotations

import hashlib
import json
import math
import os
import re
import subprocess
import threading
from collections import Counter
from collections.abc import Callable
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path

from careertwin.interview.settings import interview_dir

MAX_FILES = 4000
MAX_FILE_BYTES = 200_000
MAX_TOTAL_BYTES = 6_000_000
CHUNK_CHARS = 2_400
DIGEST_CHARS = 36_000
DOSSIER_IN_PROMPT = 12_000


class ProjectError(RuntimeError):
    """The folder cannot be read or analysed."""


SKIP_DIRS = {
    ".git", ".hg", ".svn", "node_modules", "venv", ".venv", "env", "__pycache__", "dist", "build",
    "target", "out", "obj", ".gradle", ".idea", ".vscode", ".next", ".nuxt", "vendor", "coverage",
    ".pytest_cache", ".mypy_cache", ".ruff_cache", ".terraform", ".cache", "site-packages", ".tox",
}
SKIP_SUFFIXES = {
    ".png", ".jpg", ".jpeg", ".gif", ".webp", ".ico", ".svg", ".bmp", ".tiff", ".pdf", ".zip", ".gz",
    ".tar", ".tgz", ".7z", ".rar", ".jar", ".war", ".class", ".exe", ".dll", ".so", ".dylib", ".o",
    ".a", ".pyc", ".pyo", ".whl", ".ttf", ".otf", ".woff", ".woff2", ".eot", ".mp3", ".mp4", ".mov",
    ".avi", ".wav", ".ogg", ".webm", ".sqlite", ".sqlite3", ".db", ".bin", ".dat", ".pkl", ".npy",
    ".npz", ".parquet", ".onnx", ".pt", ".h5", ".map", ".lock",
}
SKIP_NAMES = {
    "package-lock.json", "yarn.lock", "pnpm-lock.yaml", "poetry.lock", "uv.lock", "cargo.lock",
    "gemfile.lock", "composer.lock", ".ds_store", "thumbs.db",
}
SECRET_SUFFIXES = {".pem", ".key", ".p12", ".pfx", ".keystore", ".jks"}
SECRET_NAMES = {"id_rsa", "id_ed25519", "credentials.json", "secrets.json", "secrets.yaml", "secrets.yml"}
ENV_EXAMPLES = (".env.example", ".env.sample", ".env.template", ".env.dist")

_TOKENS = [
    re.compile(r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----"),
    re.compile(r"\b(?:sk|gsk|pk|rk)[-_][A-Za-z0-9_-]{20,}"),
    re.compile(r"\bgh[pousr]_[A-Za-z0-9]{30,}"),
    re.compile(r"\bgithub_pat_[A-Za-z0-9_]{40,}"),
    re.compile(r"\bAKIA[0-9A-Z]{16}\b"),
    re.compile(r"\bxox[abprs]-[A-Za-z0-9-]{10,}"),
    re.compile(r"\bAIza[0-9A-Za-z_-]{35}\b"),
]
_ASSIGNED = re.compile(
    r"(?i)\b([A-Za-z0-9_.-]*(?:api[_-]?key|secret|token|passw(?:or)?d|authorization|credential)[A-Za-z0-9_.-]*"
    r"\s*[:=]\s*)([\"']?)[^\s\"',;)]{8,}\2"
)


def redact(text: str) -> str:
    """Blank anything that looks like a key, token or password before it can leave the machine."""
    for pattern in _TOKENS:
        text = pattern.sub("[redacted]", text)
    return _ASSIGNED.sub(lambda match: f"{match[1]}{match[2]}[redacted]{match[2]}", text)


@dataclass
class SourceFile:
    path: str
    text: str

    @property
    def lines(self) -> int:
        return self.text.count("\n") + 1


@dataclass
class Scan:
    files: list[SourceFile] = field(default_factory=list)
    skipped: int = 0
    truncated: bool = False

    @property
    def size(self) -> int:
        return sum(len(item.text) for item in self.files)


def _wanted(relative: str) -> bool:
    path = Path(relative)
    name = path.name.lower()
    if any(part in SKIP_DIRS for part in path.parts[:-1]):
        return False
    if name in SKIP_NAMES or name in SECRET_NAMES or name.endswith((".min.js", ".min.css")):
        return False
    if name.startswith(".env") and name not in ENV_EXAMPLES:
        return False
    suffix = path.suffix.lower()
    if suffix in SKIP_SUFFIXES or suffix in SECRET_SUFFIXES:
        return False
    return not name.startswith("service-account")


def _git_files(root: Path) -> list[str] | None:
    """What git would show (tracked and untracked, minus ignored), or None outside a repository."""
    try:
        result = subprocess.run(
            ["git", "-C", str(root), "ls-files", "-z", "--cached", "--others", "--exclude-standard"],
            capture_output=True, timeout=20,
        )
    except (OSError, subprocess.SubprocessError):
        return None
    if result.returncode != 0 or not result.stdout:
        return None
    return sorted({item for item in result.stdout.decode(errors="replace").split("\0") if item})


def _walk(root: Path) -> list[str]:
    found = []
    for directory, subdirs, names in os.walk(root):
        subdirs[:] = sorted(name for name in subdirs if name not in SKIP_DIRS)
        base = Path(directory).relative_to(root)
        found += [(base / name).as_posix() for name in sorted(names)]
    return found


def scan(folder: str | Path) -> Scan:
    """Read the text files of a project folder; the result is redacted and size-limited."""
    root = Path(folder).expanduser().resolve()
    if not root.is_dir():
        raise ProjectError(f"{root} is not a folder")
    result, total = Scan(), 0
    for relative in _git_files(root) or _walk(root):
        if not _wanted(relative):
            result.skipped += 1
            continue
        path = root / relative
        try:
            if not path.is_file() or path.stat().st_size > MAX_FILE_BYTES:
                result.skipped += 1
                continue
            data = path.read_bytes()
        except OSError:
            result.skipped += 1
            continue
        if b"\0" in data[:4096]:
            result.skipped += 1
            continue
        text = redact(data.decode("utf-8", errors="replace"))
        if len(result.files) >= MAX_FILES or total + len(text) > MAX_TOTAL_BYTES:
            result.truncated = True
            break
        result.files.append(SourceFile(Path(relative).as_posix(), text))
        total += len(text)
    return result


# ---- the search -------------------------------------------------------------

STOP_WORDS = frozenset(
    "the a an is are was were of to in and or for on with how what why when where which who do does did "
    "you your it its this that these those can could would should be been as at by from about into than "
    "then there here have has had not no yes".split()
)
_WORD = re.compile(r"[A-Za-z][A-Za-z0-9]*")
_CAMEL = re.compile(r"(?<=[a-z0-9])(?=[A-Z])")


def tokenize(text: str) -> list[str]:
    """Lower-case words; an identifier like `OrderService` or `order_service` also gives `order`, `service`."""
    tokens = []
    for word in _WORD.findall(text):
        parts = [part.lower() for part in _CAMEL.split(word) if part]
        tokens += [word.lower()] if len(parts) > 1 else []
        tokens += parts
    return [token for token in tokens if token not in STOP_WORDS and len(token) > 1]


@dataclass
class Chunk:
    path: str
    start: int
    text: str


def chunks_of(file: SourceFile, size: int = CHUNK_CHARS) -> list[Chunk]:
    """A file in pieces of about `size` characters, cut at line ends, each knowing where it starts."""
    pieces, current, length, start = [], [], 0, 1
    for number, line in enumerate(file.text.splitlines(), start=1):
        if current and length + len(line) > size:
            pieces.append(Chunk(file.path, start, "\n".join(current)))
            current, length, start = [], 0, number
        current.append(line)
        length += len(line) + 1
    if current:
        pieces.append(Chunk(file.path, start, "\n".join(current)))
    return pieces


class _Index:
    """BM25 over the chunks; the file path counts as text too, so a question naming a module finds it."""

    K1, B = 1.5, 0.75

    def __init__(self, chunks: list[Chunk]) -> None:
        self.chunks = chunks
        self.counts = [Counter(tokenize(f"{chunk.path} {chunk.path} {chunk.text}")) for chunk in chunks]
        self.lengths = [sum(count.values()) for count in self.counts]
        self.average = (sum(self.lengths) / len(chunks)) if chunks else 1.0
        frequency: Counter[str] = Counter()
        for count in self.counts:
            frequency.update(count.keys())
        total = len(chunks)
        self.idf = {
            term: math.log(1 + (total - seen + 0.5) / (seen + 0.5)) for term, seen in frequency.items()
        }

    def search(self, question: str, limit: int) -> list[tuple[float, Chunk]]:
        terms = [term for term in set(tokenize(question)) if term in self.idf]
        scored = []
        for index, count in enumerate(self.counts):
            norm = self.K1 * (1 - self.B + self.B * self.lengths[index] / self.average)
            score = sum(
                self.idf[term] * count[term] * (self.K1 + 1) / (count[term] + norm)
                for term in terms
                if term in count
            )
            if score > 0:
                scored.append((score, self.chunks[index]))
        scored.sort(key=lambda pair: pair[0], reverse=True)
        return scored[:limit]


@dataclass
class ProjectContext:
    """A project the candidate defends: where it is, its dossier and the searchable code."""

    path: str
    name: str
    dossier: str = ""
    files: int = 0
    characters: int = 0
    analyzed_at: str = ""
    chunks: list[Chunk] = field(default_factory=list)
    _index: _Index | None = field(default=None, repr=False, compare=False)
    _building: threading.Lock = field(default_factory=threading.Lock, repr=False, compare=False)

    def prepare(self) -> None:
        """Build the search index in the background, so the first question does not wait for it."""
        threading.Thread(target=self._build, name="project-index", daemon=True).start()

    def _build(self) -> _Index:
        with self._building:
            if self._index is None:
                self._index = _Index(self.chunks)
            return self._index

    def retrieve(self, question: str, limit: int = 4, budget: int = 3600) -> str:
        """The excerpts of the project that best match the question, ready for a prompt."""
        if not self.chunks:
            return ""
        hits = self._build().search(question, limit)
        if not hits:
            return ""
        best = hits[0][0]
        parts, used = [], 0
        for score, chunk in hits:
            if score < best * 0.35 or (parts and used + len(chunk.text) > budget):
                continue
            body = chunk.text[: max(budget - used, 400)]
            parts.append(f"--- {chunk.path} (from line {chunk.start}) ---\n{body}")
            used += len(body)
        return "\n\n".join(parts)

    def for_prompt(self) -> str:
        if not self.dossier.strip():
            return ""
        return f"PROJECT THE CANDIDATE WILL DEFEND: {self.name}\n{self.dossier.strip()[:DOSSIER_IN_PROMPT]}"


# ---- the dossier ------------------------------------------------------------

DOSSIER_SYSTEM = """\
You study a software project so its author can defend it in a job interview. You get the file tree \
and the most important files. Write a DOSSIER in Markdown, at most 1,500 words, with exactly these \
sections:

## Overview
what it is, who uses it and the problem it solves, in three sentences
## Tech stack
each technology and what it is used for here
## Architecture
the main parts and how data moves between them, naming real files and modules
## Key modules
one line per module: path, then what it is responsible for
## How to run and test
the real commands found in the files
## Decisions and trade-offs
what the code chose (frameworks, structure, data model) and what that costs
## Weak spots and improvements
real limits visible in the code and what to do next
## Likely defense questions
twelve questions an interviewer would ask about this project, each with a two-sentence answer in \
the first person, grounded in the code

Rules
- Use only what the files show. Never invent features, numbers, users, deadlines or history.
- Where the code does not say why something was done, write the most likely reason and mark it \
(inferred).
- Name real files, classes and functions. No preamble and no closing remarks.
"""

KEY_FILE = re.compile(
    r"(?:^|/)(?:readme[^/]*|pyproject\.toml|package\.json|pom\.xml|build\.gradle(?:\.kts)?|"
    r"settings\.gradle(?:\.kts)?|cargo\.toml|go\.mod|requirements[^/]*\.txt|dockerfile|"
    r"docker-compose[^/]*\.ya?ml|makefile|application\.(?:ya?ml|properties))$",
    re.I,
)
ENTRY_NAMES = {"main", "app", "index", "server", "cli", "manage", "program", "application", "startup"}
ROLE_WORDS = (
    "api", "route", "controller", "service", "model", "schema", "entity", "repository", "handler",
    "config", "middleware", "core", "domain", "auth", "store", "db", "view", "component",
)


def _importance(file: SourceFile) -> float:
    lowered = file.path.lower()
    stem = Path(lowered).stem
    score = -2.0 * file.path.count("/")
    score += 6 if stem in ENTRY_NAMES else 0
    score += 3 if any(word in lowered for word in ROLE_WORDS) else 0
    score += 2 * min(file.lines, 200) / 100
    score -= 5 if re.search(r"(^|/)(tests?|__tests__|spec)(/|$)|test_|_test\.|\.test\.|\.spec\.", lowered) else 0
    return score


def _tree(scan_result: Scan, limit: int = 320) -> str:
    rows = [f"{item.path} ({item.lines} lines)" for item in scan_result.files]
    if len(rows) <= limit:
        return "\n".join(rows)
    folders = Counter(item.path.split("/")[0] if "/" in item.path else "." for item in scan_result.files)
    top = [f"{name}/ ({count} files)" if name != "." else f"(top level: {count} files)" for name, count in folders.most_common()]
    return "\n".join(rows[: limit // 2]) + f"\n... {len(rows) - limit // 2} more files; by folder:\n" + "\n".join(top)


def digest_prompt(name: str, scan_result: Scan, budget: int = DIGEST_CHARS) -> str:
    """The prompt for the dossier: the tree, the manifests and READMEs, then the most important code."""
    parts = [f"PROJECT: {name}\nFILES: {len(scan_result.files)}\n\nFILE TREE\n{_tree(scan_result)}"]
    used = len(parts[0])
    key = [item for item in scan_result.files if KEY_FILE.search(item.path)]
    key.sort(key=lambda item: (item.path.count("/"), item.path))
    for item in key:
        body = item.text[:4000]
        if used + len(body) > budget * 0.45:
            break
        parts.append(f"--- {item.path} ---\n{body}")
        used += len(body)
    others = sorted((f for f in scan_result.files if f not in key), key=_importance, reverse=True)
    for item in others:
        body = item.text[:3500]
        if used + len(body) > budget:
            continue
        parts.append(f"--- {item.path} ---\n{body}")
        used += len(body)
    return "\n\n".join(parts)


# ---- storing ----------------------------------------------------------------


def project_id(path: str | Path) -> str:
    return hashlib.sha1(str(Path(path).expanduser().resolve()).encode()).hexdigest()[:12]


def store_dir(path: str | Path) -> Path:
    return interview_dir() / "projects" / project_id(path)


def save(project: ProjectContext) -> None:
    directory = store_dir(project.path)
    directory.mkdir(parents=True, exist_ok=True)
    (directory / "dossier.md").write_text(project.dossier, encoding="utf-8")
    (directory / "index.json").write_text(
        json.dumps([[c.path, c.start, c.text] for c in project.chunks], ensure_ascii=False), encoding="utf-8"
    )
    (directory / "meta.json").write_text(
        json.dumps(
            {"path": project.path, "name": project.name, "files": project.files,
             "characters": project.characters, "analyzed_at": project.analyzed_at},
            ensure_ascii=False,
        ),
        encoding="utf-8",
    )


def save_dossier(project: ProjectContext, text: str) -> None:
    """Keep the candidate's corrections to the dossier."""
    project.dossier = text.strip()
    directory = store_dir(project.path)
    directory.mkdir(parents=True, exist_ok=True)
    (directory / "dossier.md").write_text(project.dossier, encoding="utf-8")


def load(path: str | Path) -> ProjectContext | None:
    """A project that was analysed before, or None."""
    directory = store_dir(path)
    try:
        meta = json.loads((directory / "meta.json").read_text(encoding="utf-8"))
        dossier = (directory / "dossier.md").read_text(encoding="utf-8")
        rows = json.loads((directory / "index.json").read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None
    if not isinstance(rows, list):
        return None
    project = ProjectContext(
        path=str(meta.get("path", path)),
        name=str(meta.get("name", Path(path).name)),
        dossier=dossier,
        files=int(meta.get("files", 0)),
        characters=int(meta.get("characters", 0)),
        analyzed_at=str(meta.get("analyzed_at", "")),
        chunks=[Chunk(str(p), int(s), str(t)) for p, s, t in rows],
    )
    project.prepare()
    return project


def forget(path: str | Path) -> None:
    """Delete the stored analysis of a project."""
    directory = store_dir(path)
    for name in ("dossier.md", "index.json", "meta.json"):
        (directory / name).unlink(missing_ok=True)
    try:
        directory.rmdir()
    except OSError:
        pass


def analyze(
    folder: str | Path,
    complete: Callable[..., str],
    progress: Callable[[str, int], None] = lambda stage, files: None,
) -> ProjectContext:
    """Read the folder, have the AI write the dossier, store both. `complete(system, prompt, max_tokens=)`
    is the AI call (the coach's); its errors are left to the caller. `progress(stage, files)` is told
    "read" and then "ask" (with the number of files read)."""
    progress("read", 0)
    result = scan(folder)
    if not result.files:
        raise ProjectError("No readable text files in that folder")
    root = Path(folder).expanduser().resolve()
    progress("ask", len(result.files))
    dossier = complete(DOSSIER_SYSTEM, digest_prompt(root.name, result), max_tokens=3500).strip()
    if not dossier:
        raise ProjectError("The AI returned no dossier. Try again or pick another model.")
    chunks = [piece for file in result.files for piece in chunks_of(file)]
    project = ProjectContext(
        path=str(root), name=root.name, dossier=dossier, files=len(result.files),
        characters=result.size, analyzed_at=datetime.now().strftime("%Y-%m-%d %H:%M"), chunks=chunks,
    )
    save(project)
    project.prepare()
    return project
