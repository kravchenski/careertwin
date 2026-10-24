"""A project to defend: read safely, searched fast, summarised once into a dossier."""

from __future__ import annotations

import json
import subprocess
import time

import httpx
import pytest

from careertwin.interview import project
from careertwin.interview.brief import InterviewBrief
from careertwin.interview.coach import StreamingCoach
from careertwin.interview.library import ContextController, Library
from careertwin.interview.prompts import SYSTEM, user_prompt
from careertwin.interview.questions import Kind

SERVICE = '''"""Orders: totals, discounts and the state of an order."""

DISCOUNT_RATE = 0.1


class OrderService:
    def __init__(self, repository):
        self.repository = repository

    def total(self, order):
        """Sum the lines, then apply the loyalty discount once."""
        subtotal = sum(line.price * line.quantity for line in order.lines)
        return round(subtotal * (1 - DISCOUNT_RATE), 2) if order.customer.loyal else subtotal

    def cancel(self, order_id):
        order = self.repository.get(order_id)
        order.state = "cancelled"
        self.repository.save(order)
'''


@pytest.fixture
def data_dir(tmp_path, monkeypatch):
    monkeypatch.setattr("careertwin.interview.settings.DATA_DIR", tmp_path / "data")
    monkeypatch.setattr("careertwin.interview.project.interview_dir", lambda: tmp_path / "data" / "interview")
    return tmp_path / "data"


@pytest.fixture
def shop(tmp_path):
    root = tmp_path / "bookshop"
    files = {
        "README.md": "# Bookshop API\nA small FastAPI service for a bookshop: catalogue, orders, loyalty discounts.\n",
        "pyproject.toml": '[project]\nname = "bookshop"\ndependencies = ["fastapi", "sqlalchemy"]\n',
        "app/main.py": "from fastapi import FastAPI\n\napp = FastAPI()\n\n\n@app.get('/health')\ndef health():\n    return {'ok': True}\n",
        "app/orders/service.py": SERVICE,
        "app/models.py": "class Book:\n    title: str\n    price: float\n",
        "app/settings.py": 'API_KEY = "sk-abcdefghijklmnopqrstuvwxyz123456"\nDEBUG = True\n',
        "tests/test_orders.py": "def test_total():\n    assert True\n",
        ".env": "DATABASE_URL=postgres://user:hunter2hunter2@db/shop\n",
        ".env.example": "DATABASE_URL=postgres://user:password@localhost/shop\n",
        "node_modules/left-pad/index.js": "module.exports = 1;\n",
        "package-lock.json": "{}\n",
        "keys/server.pem": "-----BEGIN PRIVATE KEY-----\nabc\n-----END PRIVATE KEY-----\n",
    }
    for relative, text in files.items():
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
    (root / "logo.png").write_bytes(b"\x89PNG\r\n\x1a\n\0\0\0binary")
    (root / "data.bin").write_bytes(b"abc\0def")
    return root


# ---- reading the folder ----------------------------------------------------


def test_the_scan_keeps_the_code_and_leaves_out_secrets_and_junk(shop):
    result = project.scan(shop)
    paths = [file.path for file in result.files]
    assert "app/orders/service.py" in paths and "README.md" in paths and ".env.example" in paths
    for left_out in (".env", "node_modules/left-pad/index.js", "package-lock.json", "keys/server.pem", "logo.png", "data.bin"):
        assert left_out not in paths
    assert result.skipped >= 5 and not result.truncated


def test_keys_and_passwords_are_blanked_before_anything_is_sent(shop):
    text = {file.path: file.text for file in project.scan(shop).files}["app/settings.py"]
    assert "sk-abcdefghijklmnopqrstuvwxyz123456" not in text and "[redacted]" in text and "DEBUG = True" in text
    assert project.redact("password = 'hunter2hunter2'") == "password = '[redacted]'"
    assert project.redact("token: ghp_" + "a" * 36) == "token: [redacted]"
    assert project.redact("-----BEGIN RSA PRIVATE KEY-----\nxx\n-----END RSA PRIVATE KEY-----") == "[redacted]"
    assert project.redact("AKIAABCDEFGHIJKLMNOP is an id") == "[redacted] is an id"
    assert project.redact("max_tokens = 100") == "max_tokens = 100"  # short values are not secrets


def test_a_git_repository_is_read_the_way_git_sees_it(shop):
    (shop / ".gitignore").write_text("generated/\n")
    (shop / "generated").mkdir()
    (shop / "generated" / "client.py").write_text("print('generated')\n")
    subprocess.run(["git", "-C", str(shop), "init", "-q"], check=True)
    paths = [file.path for file in project.scan(shop).files]
    assert "generated/client.py" not in paths and "app/orders/service.py" in paths


def test_big_files_and_big_folders_are_cut(shop, monkeypatch):
    (shop / "huge.py").write_text("x = 1\n" * 100_000)
    assert "huge.py" not in [file.path for file in project.scan(shop).files]
    monkeypatch.setattr(project, "MAX_FILES", 3)
    result = project.scan(shop)
    assert len(result.files) == 3 and result.truncated
    with pytest.raises(project.ProjectError, match="not a folder"):
        project.scan(shop / "missing")


# ---- the search --------------------------------------------------------------


def test_identifiers_are_split_into_the_words_people_use_in_questions():
    tokens = project.tokenize("OrderService.apply_discount(order)")
    assert {"orderservice", "order", "service", "apply", "discount"} <= set(tokens)
    assert "the" not in project.tokenize("the order of the discount")


def test_a_long_file_is_cut_at_line_ends_and_remembers_where_each_piece_starts():
    file = project.SourceFile("a.py", "\n".join(f"line {n} " + "x" * 50 for n in range(1, 200)))
    pieces = project.chunks_of(file, size=500)
    assert len(pieces) > 5 and pieces[0].start == 1 and pieces[1].start > 1
    assert "\n".join(piece.text for piece in pieces) == file.text
    assert all(len(piece.text) <= 600 for piece in pieces)


def analysed(shop, data_dir, dossier="## Overview\nA bookshop API."):
    return project.analyze(shop, lambda system, prompt, max_tokens=0: dossier)


def test_a_question_finds_the_code_that_answers_it(shop, data_dir):
    found = analysed(shop, data_dir).retrieve("How do you apply the discount to an order total?")
    assert "app/orders/service.py" in found and "DISCOUNT_RATE" in found and "loyal" in found
    assert "from line" in found


def test_a_question_about_nothing_in_the_project_finds_nothing(shop, data_dir):
    assert analysed(shop, data_dir).retrieve("What is your favourite colour?") == ""
    assert analysed(shop, data_dir).retrieve("Расскажи о проекте") == ""  # no Latin words to match


def test_searching_thousands_of_chunks_takes_milliseconds():
    chunks = [
        project.Chunk(f"src/module_{n}.py", 1, f"def handler_{n}(request):\n    return process(request, {n})\n" * 20)
        for n in range(3000)
    ]
    chunks.append(project.Chunk("src/billing/invoice.py", 1, "def create_invoice(order):\n    return Invoice(order.total)"))
    context = project.ProjectContext("/x", "x", chunks=chunks)
    context._build()
    started = time.perf_counter()
    found = context.retrieve("how are invoices created for an order?")
    assert time.perf_counter() - started < 0.25
    assert "src/billing/invoice.py" in found


def test_the_excerpts_stay_within_their_budget(shop, data_dir):
    context = analysed(shop, data_dir)
    context.chunks = [project.Chunk(f"f{n}.py", 1, "discount order total " * 100) for n in range(20)]
    context._index = None
    assert len(context.retrieve("discount order total", limit=10, budget=3000)) < 3600


# ---- the dossier -----------------------------------------------------------


def test_the_prompt_for_the_dossier_leads_with_the_manifests_and_the_entry_code(shop):
    prompt = project.digest_prompt("bookshop", project.scan(shop))
    assert prompt.startswith("PROJECT: bookshop")
    assert "FILE TREE" in prompt and "app/orders/service.py (" in prompt
    assert "A small FastAPI service" in prompt and 'name = "bookshop"' in prompt
    assert "class OrderService" in prompt and "def health" in prompt
    assert "sk-abcdefghijklmnopqrstuvwxyz123456" not in prompt and "hunter2" not in prompt


def test_the_prompt_for_a_big_project_stays_inside_its_budget():
    files = [project.SourceFile(f"src/mod{n}/handler.py", "def run():\n    pass\n" * 400) for n in range(400)]
    files.append(project.SourceFile("README.md", "# Big\n" * 50))
    prompt = project.digest_prompt("big", project.Scan(files), budget=20_000)
    assert len(prompt) < 20_000 + 25_000  # the tree comes on top of the budget for files
    assert "README.md" in prompt and "more files; by folder" in prompt


def test_analysis_asks_the_ai_once_and_stores_the_result(shop, data_dir):
    asked, stages = [], []

    def complete(system, prompt, max_tokens=0):
        asked.append((system, prompt, max_tokens))
        return "## Overview\nA bookshop API.\n## Tech stack\nFastAPI."

    result = project.analyze(shop, complete, lambda stage, files: stages.append((stage, files)))
    assert len(asked) == 1 and asked[0][0] == project.DOSSIER_SYSTEM and "(inferred)" in asked[0][0]
    assert stages[0] == ("read", 0) and stages[1][0] == "ask" and stages[1][1] == result.files
    assert result.name == "bookshop" and result.dossier.startswith("## Overview") and result.chunks
    again = project.load(shop)
    assert again.dossier == result.dossier and again.files == result.files and len(again.chunks) == len(result.chunks)
    assert "OrderService" in again.retrieve("what does the order service do")


def test_an_empty_folder_or_an_empty_answer_is_an_error(tmp_path, shop, data_dir):
    (tmp_path / "empty").mkdir()
    with pytest.raises(project.ProjectError, match="No readable text files"):
        project.analyze(tmp_path / "empty", lambda *a, **k: "x")
    with pytest.raises(project.ProjectError, match="no dossier"):
        project.analyze(shop, lambda *a, **k: "  ")
    assert project.load(shop) is None  # nothing half-written is kept


def test_corrections_to_the_dossier_are_kept(shop, data_dir):
    context = analysed(shop, data_dir)
    project.save_dossier(context, "## Overview\nI built it for my uncle's shop.\n")
    assert project.load(shop).dossier == "## Overview\nI built it for my uncle's shop."


def test_a_damaged_store_is_not_a_project(shop, data_dir):
    analysed(shop, data_dir)
    (project.store_dir(shop) / "index.json").write_text("{}")
    assert project.load(shop) is None
    (project.store_dir(shop) / "meta.json").write_text("not json")
    assert project.load(shop) is None


# ---- in the answer ---------------------------------------------------------


@pytest.fixture
def brief(tmp_path, monkeypatch):
    path = tmp_path / "ABOUT.md"
    path.write_text("## Contact\n| Full name | Test Candidate |\n## Skills\n- Languages: Python, Java\n")
    monkeypatch.setenv("CAREERTWIN_ABOUT_PATH", str(path))
    return InterviewBrief.load()


def test_the_dossier_goes_with_every_answer_and_shows_in_the_sources(brief, shop, data_dir):
    assert "PROJECT THE CANDIDATE WILL DEFEND" not in brief.render()
    brief.project = analysed(shop, data_dir, "## Overview\nA bookshop API with loyalty discounts.")
    rendered = brief.render()
    assert "PROJECT THE CANDIDATE WILL DEFEND: bookshop" in rendered and "loyalty discounts" in rendered
    assert ("project", "project bookshop") in brief.sources()


def test_a_long_dossier_is_cut_for_the_prompt(brief, shop, data_dir):
    brief.project = analysed(shop, data_dir, "word " * 10_000)
    assert len(brief.project.for_prompt()) < project.DOSSIER_IN_PROMPT + 200


def test_the_question_prompt_carries_the_code_that_matches(brief):
    plain = user_prompt(brief, "What is REST?", Kind.DEFINITION, [], "en")
    assert "RELEVANT CODE" not in plain
    with_code = user_prompt(brief, "How do discounts work?", Kind.TECHNICAL, [], "en", code="--- app/orders/service.py ---\nDISCOUNT_RATE = 0.1")
    assert with_code.index("RELEVANT CODE FROM THE PROJECT") < with_code.index("INTERVIEWER'S QUESTION")
    assert "DISCOUNT_RATE = 0.1" in with_code


def test_the_rules_tell_the_model_to_answer_as_the_author_and_not_to_invent():
    assert "answer as its author" in SYSTEM and "(inferred)" in SYSTEM and "say so plainly" in SYSTEM


def test_the_coach_searches_the_project_for_each_question(brief, shop, data_dir):
    brief.project = analysed(shop, data_dir)
    seen = []

    def handler(request):
        seen.append(json.loads(request.content))
        body = 'data: {"choices": [{"delta": {"content": "ANSWER:\\nIt applies 10 percent."}}]}\n\ndata: [DONE]\n\n'
        return httpx.Response(200, headers={"content-type": "text/event-stream"}, content=body)

    import threading

    coach = StreamingCoach(brief, url="http://gateway", model="m", transport=httpx.MockTransport(handler))
    "".join(coach.stream("How do you apply the discount to an order?", [], threading.Event(), "en"))
    text = seen[0]["messages"][1]["content"]
    assert "RELEVANT CODE FROM THE PROJECT" in text and "DISCOUNT_RATE" in text
    assert "PROJECT THE CANDIDATE WILL DEFEND" in text
    "".join(coach.stream("What is your favourite colour?", [], threading.Event(), "en"))
    assert "RELEVANT CODE" not in seen[1]["messages"][1]["content"]
    assert "PROJECT THE CANDIDATE WILL DEFEND" in seen[1]["messages"][1]["content"]


# ---- belonging to an interview ----------------------------------------------


def test_each_interview_keeps_its_own_project(brief, shop, data_dir, tmp_path):
    other = tmp_path / "chat"
    other.mkdir()
    (other / "main.py").write_text("print('chat')\n" * 5)
    controller = ContextController(brief, Library())
    first = controller.current.id
    controller.attach_project(analysed(shop, data_dir))
    assert controller.project_path() == str(shop.resolve()) and brief.project.name == "bookshop"
    controller.new_interview("Second")
    assert brief.project is None and controller.project_path() == ""
    controller.attach_project(project.analyze(other, lambda *a, **k: "## Overview\nA chat."))
    assert brief.project.name == "chat"
    controller.switch_interview(first)
    assert brief.project.name == "bookshop"
    reloaded = ContextController(InterviewBrief.load(), Library.load())
    reloaded.restore()
    assert reloaded.brief.project.name == "chat" or reloaded.brief.project.name == "bookshop"


def test_removing_a_project_deletes_its_analysis_unless_another_interview_uses_it(brief, shop, data_dir):
    controller = ContextController(brief, Library())
    controller.attach_project(analysed(shop, data_dir))
    controller.new_interview("Second")
    controller.set_project_path(str(shop.resolve()))
    assert brief.project is not None  # the stored analysis is shared by path
    controller.remove_project()
    assert project.load(shop) is not None  # the first interview still defends it
    controller.switch_interview(controller.interviews()[0][0])
    controller.remove_project()
    assert project.load(shop) is None and brief.project is None
