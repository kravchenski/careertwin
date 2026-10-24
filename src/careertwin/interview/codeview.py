"""Code in an answer: which language it is, and its syntax colours as HTML for the window.

A small tokenizer, not a parser: comments, strings, numbers, keywords and decorators are coloured, which
is what makes a solution readable at a glance during a call. Anything it does not know stays plain.
"""

from __future__ import annotations

import html
import re

COLOURS = {
    "keyword": "#c792ea",
    "string": "#c3e88d",
    "comment": "#6c7a96",
    "number": "#f78c6c",
    "decorator": "#82aaff",
}

ALIASES = {
    "py": "python", "python3": "python", "python": "python", "ipython": "python",
    "js": "clike", "javascript": "clike", "jsx": "clike", "ts": "clike", "typescript": "clike",
    "tsx": "clike", "java": "clike", "kotlin": "clike", "kt": "clike", "scala": "clike",
    "c": "clike", "cpp": "clike", "c++": "clike", "cs": "clike", "csharp": "clike", "c#": "clike",
    "go": "clike", "golang": "clike", "rust": "clike", "rs": "clike", "swift": "clike", "php": "clike",
    "dart": "clike",
    "sql": "sql", "postgresql": "sql", "mysql": "sql", "sqlite": "sql",
    "bash": "shell", "sh": "shell", "shell": "shell", "zsh": "shell", "yaml": "shell", "yml": "shell",
    "toml": "shell", "dockerfile": "shell", "ruby": "shell", "rb": "shell", "makefile": "shell",
    "json": "plain", "text": "plain", "txt": "plain", "plaintext": "plain", "html": "plain", "xml": "plain",
    "css": "plain", "markdown": "plain", "md": "plain", "diff": "plain",
}

KEYWORDS = {
    "python": (
        "def class return if elif else for while in not and or is import from as with try except finally "
        "raise lambda yield pass break continue None True False async await global nonlocal del assert self"
    ),
    "clike": (
        "function const let var return if else for while do switch case default break continue new this "
        "class extends implements interface enum public private protected static final abstract void int "
        "long double float boolean char byte short string bool null undefined true false try catch finally "
        "throw throws import export from package async await typeof instanceof fn func struct impl trait "
        "use pub mut match type namespace using in of yield super delete readonly override"
    ),
    "sql": (
        "select from where join left right inner outer on group by order having limit offset insert into "
        "values update set delete create table index primary key foreign references null not and or in as "
        "distinct union all case when then else end like between exists count sum avg min max desc asc"
    ),
    "shell": (
        "if then else elif fi for while do done case esac function return in export echo cd exit set "
        "true false null FROM RUN CMD COPY ADD WORKDIR ENV EXPOSE ENTRYPOINT def end class module"
    ),
}


def language_of(tag: str, code: str) -> str:
    """The highlighting family for a fence tag, or a guess from the code when there is no tag."""
    tag = tag.strip().lower()
    if tag in ALIASES:
        return ALIASES[tag]
    if re.search(r"^\s*(?:def \w+\(|class \w+.*:\s*$|import \w+\s*$|from \S+ import |elif |print\()", code, re.M):
        return "python"
    if re.search(r"^\s*(?:select|insert into|update|delete from|create table)\b", code, re.M | re.I):
        return "sql"
    if re.search(r"[;{}]\s*$|=>|\bfunction\b|\bpublic\b|\bconst\b|\bfunc\b|\bfn\b", code, re.M):
        return "clike"
    if re.search(r"^\s*(?:\$ |#!/|echo |cd |export )", code, re.M):
        return "shell"
    return "plain"


_STRINGS = r'"""[\s\S]*?"""|\'\'\'[\s\S]*?\'\'\'|"(?:\\.|[^"\\\n])*"|\'(?:\\.|[^\'\\\n])*\''
_PATTERNS: dict[str, re.Pattern[str]] = {}


def _pattern(family: str) -> re.Pattern[str]:
    if family not in _PATTERNS:
        comment = {
            "python": r"#[^\n]*",
            "shell": r"#[^\n]*",
            "clike": r"//[^\n]*|/\*[\s\S]*?(?:\*/|\Z)",
            "sql": r"--[^\n]*|/\*[\s\S]*?(?:\*/|\Z)",
        }[family]
        strings = _STRINGS + (r"|`(?:\\.|[^`\\])*`" if family == "clike" else "")
        flags = re.I if family == "sql" else 0
        words = "|".join(sorted(KEYWORDS[family].split(), key=len, reverse=True))
        _PATTERNS[family] = re.compile(
            rf"(?P<comment>{comment})|(?P<string>{strings})|(?P<decorator>@\w+(?:\.\w+)*)"
            rf"|(?P<number>\b\d+(?:\.\d+)?\b)|(?P<keyword>\b(?:{words})\b)",
            flags,
        )
    return _PATTERNS[family]


def highlight(code: str, tag: str = "") -> str:
    """The code as escaped HTML with colour spans; unknown languages are only escaped."""
    family = language_of(tag, code)
    if family == "plain":
        return html.escape(code)
    out, position = [], 0
    for match in _pattern(family).finditer(code):
        out.append(html.escape(code[position : match.start()]))
        kind = match.lastgroup
        style = f"color:{COLOURS[kind]}" + ("; font-style:italic" if kind == "comment" else "")
        out.append(f'<span style="{style}">{html.escape(match[0])}</span>')
        position = match.end()
    out.append(html.escape(code[position:]))
    return "".join(out)
