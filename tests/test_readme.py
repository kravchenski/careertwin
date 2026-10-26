"""The README keeps its promises: its links, pictures, commands and numbers match the repository."""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
README = (ROOT / "README.md").read_text(encoding="utf-8")


def slug(heading: str) -> str:
    """The anchor GitHub makes from a heading."""
    text = re.sub(r"<[^>]+>", "", heading).strip().lower()
    text = re.sub(r"[^\w\- ]", "", text)
    return text.replace(" ", "-")


def headings(text: str) -> set[str]:
    return {slug(match[1]) for match in re.finditer(r"^#{1,6} (.+)$", text, re.M)}


def test_every_local_link_and_picture_exists():
    targets = re.findall(r"\]\((?!https?:|mailto:|#)([^)\s]+)\)", README)
    targets += re.findall(r'(?:src|href)="(?!https?:|#)([^"]+)"', README)
    assert targets, "the README links to nothing local?"
    for target in targets:
        path = target.split("#")[0]
        assert (ROOT / path).exists(), f"README points at {target}, which is not in the repository"
        if path.startswith("docs/media/"):
            assert (ROOT / path).stat().st_size > 5_000, f"{path} looks empty"


def test_every_anchor_points_at_a_heading():
    own = headings(README)
    for anchor in re.findall(r"\]\(#([^)]+)\)", README):
        assert anchor in own, f"#{anchor} is not a heading of the README"
    for file, anchor in re.findall(r"\]\(([\w./-]+\.md)#([^)]+)\)", README):
        assert anchor in headings((ROOT / file).read_text(encoding="utf-8")), f"{file}#{anchor}"


def test_the_code_fences_and_diagrams_are_balanced():
    assert README.count("```") % 2 == 0
    for block in re.findall(r"```mermaid\n(.*?)```", README, re.S):
        assert block.lstrip().startswith("flowchart "), "unexpected diagram type"
        assert block.count("[") == block.count("]") and block.count("(") == block.count(")")
        assert block.count('"') % 2 == 0
    assert README.count("<details>") == README.count("</details>")


def cli_commands() -> set[str]:
    out = subprocess.run([sys.executable, "-m", "careertwin.cli", "--help"], capture_output=True, text=True, cwd=ROOT).stdout
    return set(re.search(r"\{([a-z,-]+)\}", out)[1].split(","))


def test_the_commands_in_the_readme_exist():
    available = cli_commands()
    used = set(re.findall(r"uv run careertwin ([a-z]+)\b", README))
    assert {"serve", "agent"} <= used, "the quick start no longer shows serve and agent"
    assert used <= available, f"the README runs careertwin {sorted(used - available)}, which does not exist"
    scripts = re.search(r"\[project\.scripts\]\n(.*?)\n\n", (ROOT / "pyproject.toml").read_text(), re.S)[1]
    for command in ("careertwin-interview", "careertwin-interview-ctl", "careertwin-mcp"):
        assert f"{command} =" in scripts
        assert command in README or command == "careertwin-mcp"
    for target in re.findall(r"\bmake ([a-z-]+)", README):
        assert re.search(rf"^{target}:", (ROOT / "Makefile").read_text(), re.M), f"make {target}"
    for script in re.findall(r"(?:\./)?scripts/([\w.-]+)", README):
        assert (ROOT / "scripts" / script).exists(), script


def test_the_variables_in_the_configuration_table_are_real():
    table = README.split("<summary><b>Configuration</b></summary>")[1].split("</details>")[0]
    names = set(re.findall(r"`([A-Z][A-Z0-9_]{3,})`", table)) - {"auto"}
    env_example = (ROOT / ".env.example").read_text()
    sources = "\n".join(path.read_text(errors="ignore") for path in (ROOT / "src").rglob("*.py"))
    for name in names:
        assert name in env_example or name in sources, f"{name} is neither in .env.example nor in the code"


def test_the_numbers_in_the_readme_match_the_code():
    from careertwin.agentic import registry
    from careertwin.interview import i18n
    from careertwin.interview.questions import LEXICON
    from careertwin.templates import list_templates

    words = {7: "seven", 9: "nine"}
    assert f"{len(registry.openai_tool_specs())} typed tools" in README
    assert f"{words[len(list_templates())]} PDF templates" in README
    companies = json.loads((ROOT / "src/careertwin/jobs/data/companies.json").read_text())
    count = len(companies if isinstance(companies, list) else companies.get("companies", companies))
    assert f"a catalogue of {count} companies" in README
    assert len(i18n.available()) == len(LEXICON) == 7, "a language was added: update the README's \"seven\""
    assert README.count("seven languages") >= 2


def test_every_language_the_readme_names_has_a_window_translation():
    from careertwin.interview import i18n

    names = {"English": "en", "Russian": "ru", "Polish": "pl", "Ukrainian": "uk", "German": "de", "French": "fr", "Spanish": "es"}
    sentence = re.search(r"window speaks ([^.]+)\.", README.replace("\n", " "))[1]
    for name, code in names.items():
        assert name in sentence and i18n.has(code)


def test_the_readme_makes_no_claim_about_screen_sharing_that_the_code_contradicts():
    """The Windows window keeps itself out of captures, so "never hides" would be false; and the
    README is not the place to advertise evading detection."""
    lowered = README.lower()
    assert "never hides" not in lowered
    assert "undetectable" not in lowered and "invisible to" not in lowered and "stealth" not in lowered
