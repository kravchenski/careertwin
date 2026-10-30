#!/usr/bin/env python3
"""Decide the next release from the commits since the last tag; optionally write the version into the project.

    python3 scripts/next_release.py            # say what would happen, change nothing
    python3 scripts/next_release.py --apply    # also write the version into pyproject.toml, uv.lock and overlay/

It prints `key=value` lines (`version=`, `reason=`), so the output can go straight to $GITHUB_OUTPUT. An empty
`version` means: no release.

The rules (conventional commits, as in CONTRIBUTING.md), counted over the commits since the last `vX.Y.Z` tag:

* `feat:` makes a minor release, `fix:` and `perf:` a patch release, and a breaking change (`feat!:` or
  `BREAKING CHANGE:` in the body) a minor release while the major version is 0, a major one after that;
* `docs:`, `chore:`, `test:`, `ci:`, `style:`, `refactor:` and `build:` alone make no release;
* a version raised by hand in pyproject.toml (above the last tag) is released as it is;
* a commit that says `[skip release]`, and the commit that records a release, make none.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

TAG = re.compile(r"^v(\d+)\.(\d+)\.(\d+)$")
VERSION = re.compile(r'^version = "(\d+)\.(\d+)\.(\d+)"$', re.M)
BREAKING_SUBJECT = re.compile(r"^[a-z]+(?:\([^)]*\))?!:")
KIND = re.compile(r"^([a-z]+)(?:\([^)]*\))?!?:")
Version = tuple[int, int, int]


def git(root: Path, *args: str) -> str:
    return subprocess.run(["git", "-C", str(root), *args], capture_output=True, text=True, check=True).stdout


def text(version: Version) -> str:
    return ".".join(map(str, version))


def last_tag(root: Path) -> tuple[str, Version] | None:
    """The highest plain `vX.Y.Z` tag (a pre-release such as v0.2.0-rc1 is not a base for the next version)."""
    found = [(tuple(int(part) for part in match.groups()), name) for name in git(root, "tag", "--list", "v[0-9]*").split() if (match := TAG.match(name))]
    if not found:
        return None
    version, name = max(found)
    return name, version


def declared(root: Path) -> Version:
    match = VERSION.search((root / "pyproject.toml").read_text())
    if not match:
        raise SystemExit("pyproject.toml has no `version = \"X.Y.Z\"` line")
    return tuple(int(part) for part in match.groups())  # type: ignore[return-value]


def level(root: Path, since: str, major: int) -> str:
    """`major`, `minor`, `patch` or `` for the commits after `since`, merge commits left out."""
    raw = git(root, "log", "--no-merges", "--format=%s%x1f%b%x1e", f"{since}..HEAD")
    found = ""
    for entry in filter(None, raw.split("\x1e")):
        subject, _, body = entry.strip("\n").partition("\x1f")
        kind = KIND.match(subject.strip())
        breaking = bool(BREAKING_SUBJECT.match(subject.strip())) or "BREAKING CHANGE:" in body
        if breaking:
            return "major" if major >= 1 else "minor"
        if kind and kind[1] == "feat":
            found = "minor"
        elif kind and kind[1] in {"fix", "perf"} and not found:
            found = "patch"
    return found


def bump(version: Version, how: str) -> Version:
    major, minor, patch = version
    return {"major": (major + 1, 0, 0), "minor": (major, minor + 1, 0), "patch": (major, minor, patch + 1)}[how]


def decide(root: Path) -> tuple[Version | None, str]:
    head = git(root, "log", "-1", "--format=%B").strip()
    if "[skip release]" in head:
        return None, "the last commit says [skip release]"
    if head.startswith("chore: release v"):
        return None, "the last commit is a release commit"
    tag = last_tag(root)
    current = declared(root)
    if tag is None:
        return current, f"no release yet: releasing the declared version {text(current)}"
    name, base = tag
    if current > base:
        return current, f"the version was raised by hand to {text(current)} (last release {name})"
    how = level(root, name, base[0])
    if not how:
        return None, f"nothing to release since {name}: only docs, chores, tests or refactors"
    why = {"major": "breaking changes", "minor": "new features", "patch": "fixes"}[how]
    return bump(base, how), f"{how} release: {why} since {name}"


def write(root: Path, version: Version) -> None:
    """Put the version into pyproject.toml and uv.lock (the line of the project's own package), and into the Rust
    window: overlay/Cargo.toml and overlay/Cargo.lock. The window reports this version and compares it with the
    latest release to tell when an update is there."""
    new = text(version)
    pyproject = root / "pyproject.toml"
    pyproject.write_text(VERSION.sub(f'version = "{new}"', pyproject.read_text(), count=1))
    lock = root / "uv.lock"
    if lock.exists():
        updated = re.sub(r'(?m)^(name = "careertwin"\nversion = )"[^"]+"', rf'\1"{new}"', lock.read_text(), count=1)
        lock.write_text(updated)
    cargo = root / "overlay" / "Cargo.toml"
    if cargo.exists():
        cargo.write_text(VERSION.sub(f'version = "{new}"', cargo.read_text(), count=1))
    cargo_lock = root / "overlay" / "Cargo.lock"
    if cargo_lock.exists():
        updated = re.sub(
            r'(?m)^(name = "careertwin-overlay"\nversion = )"[^"]+"', rf'\1"{new}"', cargo_lock.read_text(), count=1
        )
        cargo_lock.write_text(updated)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--apply", action="store_true", help="write the version into pyproject.toml, uv.lock and overlay/")
    parser.add_argument("--root", default=".", help="the repository (default: the current directory)")
    args = parser.parse_args(argv)
    root = Path(args.root)
    version, reason = decide(root)
    if version and args.apply and version != declared(root):
        write(root, version)
    print(f"version={text(version) if version else ''}")
    print(f"reason={reason}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
