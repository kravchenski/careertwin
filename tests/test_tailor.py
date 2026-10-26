"""A CV written from the candidate's own approved CV, tailored to a vacancy without a claim the facts do not hold."""

from __future__ import annotations

import json
from pathlib import Path
from unittest.mock import AsyncMock

import pytest

from careertwin import db
from careertwin.automation import documents, tailor
from careertwin.automation.models import Settings, Vacancy
from careertwin.automation.store import Store

CONTEXT = """## Contact
| Full name | Test Candidate |
| Email | test@example.com |
| Location | Gdańsk, Poland |
## Experience
### Creator and Lead Maintainer, Switchyard | 2025 to present
- Lead maintainer of an open-source LLM gateway; about 400 of 578 commits; CI in GitHub Actions.
- Deployed on AWS with Docker.
## Skills
| Level | Skills |
| --- | --- |
| Strong | Python, Linux, Docker, Git, GitHub Actions |
| Working | PyTest, FFmpeg (said by the user), LLM APIs, RAG |
| Basic | C, C++ (gcc, gdb) |
"""

BASE = """<!doctype html><html><head><meta charset="utf-8"><style>@page{size:A4;margin:9mm}body{font:9pt Arial}</style></head><body>
<h1>TEST CANDIDATE</h1>
<div class="headline">QA Intern | Python, Linux</div>
<div class="contact">Gdansk | test@example.com</div>
<h2>Summary</h2>
<p>3rd-year BSc student with two years of delivery. Lead maintainer of an open-source LLM gateway (about 400 of 578 commits).</p>
<h2>Technical Skills</h2>
<div class="skills">
<p><b>Languages:</b> Python (PyTest), C and C++ (gcc, gdb)</p>
<p><b>Tools:</b> Linux, Docker, Git, GitHub Actions</p>
</div>
<h2>Experience</h2>
<h3>Creator and Lead Maintainer, Switchyard | 2025 - Present</h3>
<ul><li>Deployed on AWS with Docker.</li></ul>
</body></html>"""

JOB = {"title": "Software Engineering Undergraduate Intern", "company": "Acme", "description": "C, Python, Bash, FFmpeg, GitHub Actions.", "location": "Gdansk"}


@pytest.fixture
def folder(tmp_path):
    (tmp_path / "acme-qa-cv.html").write_text(BASE)
    (tmp_path / "notes.html").write_text("<p>not a CV</p>")
    return tmp_path


def settings():
    return Settings(context=CONTEXT)


def proposal(**override):
    value = {
        "base": "acme-qa-cv",
        "headline": "Software Engineering Undergraduate Intern | Python, C, Linux, FFmpeg",
        "summary": "3rd-year BSc student with two years of delivery. I know FFmpeg and lead an open-source LLM gateway (about 400 of 578 commits).",
        "skills": [{"label": "Languages", "items": "Python (PyTest), C and C++ (gcc, gdb), Kafka"}, {"label": "Media", "items": "FFmpeg, GStreamer"}],
        "must": ["Python", "FFmpeg", "Kubernetes"],
        "nice": ["GitHub Actions"],
    }
    value.update(override)
    return json.dumps(value)


def test_only_files_that_look_like_a_cv_variant_are_read(folder):
    found = tailor.variants(folder)
    assert list(found) == ["acme-qa-cv"]
    variant = found["acme-qa-cv"]
    assert variant.headline.startswith("QA Intern") and "LLM gateway" in variant.summary
    assert variant.skills[0] == ("Languages", "Python (PyTest), C and C++ (gcc, gdb)")


@pytest.mark.asyncio
async def test_a_term_the_facts_do_not_hold_is_removed_and_one_they_hold_is_added(folder, monkeypatch):
    monkeypatch.setattr(tailor, "call_ai", AsyncMock(return_value=proposal()))
    result = await tailor.tailor({"vacancy": JOB}, settings(), folder)
    page = tailor.parse("x", result.html)
    assert page.headline == "Software Engineering Undergraduate Intern | Python, C, Linux, FFmpeg"
    assert "I know FFmpeg" in page.summary
    assert dict(page.skills)["Languages"] == "Python (PyTest), C and C++ (gcc, gdb)"  # Kafka is not in the facts
    assert dict(page.skills)["Media"] == "FFmpeg"  # GStreamer is not either
    assert set(result.report["removed"]) == {"Kafka", "GStreamer"}
    assert "Deployed on AWS" in result.html  # the experience is the candidate's own text, untouched


@pytest.mark.asyncio
async def test_a_summary_with_an_invented_number_or_tool_falls_back_to_the_candidates_own(folder, monkeypatch):
    invented = "3rd-year BSc student. I served 50,000 users with Kubernetes and PostgreSQL."
    monkeypatch.setattr(tailor, "call_ai", AsyncMock(return_value=proposal(summary=invented)))
    result = await tailor.tailor({"vacancy": JOB}, settings(), folder)
    assert "50,000" not in result.html and "about 400 of 578 commits" in result.html
    assert "summary" in result.report["kept_base"]
    assert {"50,000", "PostgreSQL"} <= set(result.report["summary_unproven"])


@pytest.mark.asyncio
async def test_a_headline_with_an_unproven_skill_falls_back(folder, monkeypatch):
    monkeypatch.setattr(tailor, "call_ai", AsyncMock(return_value=proposal(headline="Intern | Rust, Kubernetes")))
    result = await tailor.tailor({"vacancy": JOB}, settings(), folder)
    assert tailor.parse("x", result.html).headline == "QA Intern | Python, Linux"


@pytest.mark.asyncio
async def test_without_variants_or_with_an_unknown_base_nothing_is_tailored(folder, tmp_path_factory, monkeypatch):
    empty = tmp_path_factory.mktemp("empty")
    monkeypatch.setattr(tailor, "call_ai", AsyncMock(return_value=proposal()))
    assert await tailor.tailor({"vacancy": JOB}, settings(), empty) is None
    monkeypatch.setattr(tailor, "call_ai", AsyncMock(return_value=proposal(base="made-up")))
    assert await tailor.tailor({"vacancy": JOB}, settings(), folder) is None
    monkeypatch.setattr(tailor, "call_ai", AsyncMock(return_value="not json"))
    assert await tailor.tailor({"vacancy": JOB}, settings(), folder) is None


@pytest.mark.asyncio
async def test_the_cv_package_is_built_from_the_tailored_variant_and_reports_what_it_cannot_cover(folder, tmp_path, monkeypatch):
    from pypdf import PdfReader

    monkeypatch.setattr(db, "DB_PATH", tmp_path / "t.db")
    store = Store()
    store.initialize()
    store.save_settings(settings())
    monkeypatch.setenv("CAREERTWIN_CV_DIR", str(folder))
    monkeypatch.setattr(documents, "OUTPUT_DIR", tmp_path / "outputs")
    monkeypatch.setattr(tailor, "call_ai", AsyncMock(return_value=proposal()))
    select = AsyncMock()
    monkeypatch.setattr(documents, "call_ai", select)
    job = store.add(Vacancy(url="https://example.com/j", title=JOB["title"], company="Acme", description=JOB["description"] * 10))
    package = await documents.DocumentAgent().prepare(job, store.settings(), store)
    select.assert_not_called()  # the facts-only path did not run
    text = "\n".join(page.extract_text() for page in PdfReader(Path(package["cv_path"])).pages)
    assert "FFmpeg" in text and "test@example.com" in text and "Kafka" not in text
    assert package["checks"]["pages"] == 1 and package["checks"]["missing_must_haves"] == ["Kubernetes"]
    assert isinstance(package["checks"]["not_in_your_facts"], list)
    assert len(store.files(job["id"])) == 6
    evidence = json.loads(Path(package["files"]["evidence"]).read_text())
    assert evidence["base"] == "acme-qa-cv" and evidence["coverage"]["must"]["FFmpeg"] is True


def test_a_term_is_known_whatever_its_case_and_a_description_is_not_judged_word_by_word():
    corpus = "Deployed with Docker; CI/CD in GitHub Actions; AI-assisted work.".casefold()
    corpus_words = tailor.words(corpus)
    assert tailor.known("CI/CD", corpus, corpus_words) and tailor.known("GitHub Actions", corpus, corpus_words)
    assert tailor.known("building and debugging software", corpus, corpus_words)  # a description, no tool or number in it
    assert not tailor.known("Kafka", corpus, corpus_words)
    assert not tailor.known("serving 50,000 users daily", corpus, corpus_words)  # a number that is not there
    assert not tailor.known("daily Terraform automation", corpus, corpus_words)  # a tool that is not there


@pytest.mark.asyncio
async def test_a_summary_that_does_not_hold_is_rewritten_once_with_the_terms_named(folder, monkeypatch):
    bad = proposal(summary="I run Kubernetes and PostgreSQL for 50,000 users.")
    good = proposal(summary="3rd-year BSc student. I know FFmpeg and lead an open-source LLM gateway (about 400 of 578 commits).")
    model = AsyncMock(side_effect=[bad, good])
    monkeypatch.setattr(tailor, "call_ai", model)
    result = await tailor.tailor({"vacancy": JOB}, settings(), folder)
    assert model.await_count == 2
    assert "50,000" in model.await_args_list[1].args[0] and "Kubernetes" in model.await_args_list[1].args[0]
    assert "I know FFmpeg" in tailor.parse("x", result.html).summary and "summary" not in result.report["kept_base"]


@pytest.mark.asyncio
async def test_a_valid_answer_is_not_asked_for_twice(folder, monkeypatch):
    model = AsyncMock(return_value=proposal())
    monkeypatch.setattr(tailor, "call_ai", model)
    await tailor.tailor({"vacancy": JOB}, settings(), folder)
    assert model.await_count == 1


@pytest.mark.asyncio
async def test_a_claim_in_the_variant_but_not_in_the_facts_is_carried_and_named(folder, monkeypatch):
    base = BASE.replace("Lead maintainer of an open-source LLM gateway", "Hands-on vLLM serving and lead maintainer of an open-source LLM gateway")
    (folder / "acme-qa-cv.html").write_text(base)
    log = CONTEXT + "\n## Applications log\n- Applied to a role that lists vLLM and Kafka.\n"
    monkeypatch.setattr(tailor, "call_ai", AsyncMock(return_value=proposal(summary="3rd-year BSc student with hands-on vLLM serving.")))
    result = await tailor.tailor({"vacancy": JOB}, Settings(context=log), folder)
    assert "vLLM" in result.report["only_in_variant"]  # the log's mention of vLLM does not make it a fact
    assert "Kafka" not in tailor.parse("x", result.html).summary
