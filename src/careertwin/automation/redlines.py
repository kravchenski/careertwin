"""Words an application must never carry, checked on the review page before anything is sent.

The model fills a form from facts and saved answers, and a check of each answer happens as it types. This is the last
line: a plain, deterministic scan of the page the candidate would be sending. It catches what a model can get wrong
twice in a row, such as "Yes, by virtue of citizenship" for a person who works through a residence permit.

The rules come from the candidate's own context (a sentence that says never and quotes a phrase:
`Never answer "by virtue of citizenship"`) and from `data/redlines.txt`, one rule per line: a plain phrase, or
`re:` and a regular expression. A line may carry a message after ` => `. A finding stops the submission and leaves the
application for the candidate to look at.
"""

from __future__ import annotations

import re
from dataclasses import dataclass

from careertwin.settings import DATA_DIR

NEVER_QUOTED = re.compile(r"\bnever\b[^\n\".]{0,60}\"([^\"\n]{3,80})\"", re.I)


@dataclass(frozen=True)
class Rule:
    pattern: re.Pattern[str]
    message: str


def from_context(context: str) -> list[Rule]:
    """`Never answer "by virtue of citizenship"` in the candidate's context becomes a rule for that phrase."""
    rules = []
    for line in context.splitlines():
        for match in NEVER_QUOTED.finditer(line):
            phrase = match.group(1).strip()
            rules.append(Rule(re.compile(re.escape(phrase), re.I), f'the profile says never "{phrase}"'))
    return rules


def from_text(text: str) -> list[Rule]:
    rules = []
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        rule, _, message = line.partition(" => ")
        rule = rule.strip()
        try:
            pattern = re.compile(rule[3:].strip() if rule.startswith("re:") else re.escape(rule), re.I)
        except re.error:
            continue  # a broken line must not stop every submission
        rules.append(Rule(pattern, message.strip() or f"redline: {rule}"))
    return rules


def file_rules() -> list[Rule]:
    path = DATA_DIR / "redlines.txt"
    return from_text(path.read_text(encoding="utf-8")) if path.is_file() else []


def violations(page: str, context: str = "") -> list[str]:
    """What on `page` breaks a rule, each finding once."""
    found = []
    for rule in [*from_context(context), *file_rules()]:
        if rule.pattern.search(page) and rule.message not in found:
            found.append(rule.message)
    return found
