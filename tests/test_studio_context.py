"""Studio publication and job-scoped salary contracts; no live submissions."""

from __future__ import annotations

import json
from unittest.mock import AsyncMock

import pytest
from fastapi import HTTPException

from careertwin import db as database
from careertwin.api.routes import studio
from careertwin.api.schemas import StudioSubmit
from careertwin.automation import salary
from careertwin.automation.documents import contact, extract_facts
from careertwin.automation.models import Settings
from careertwin.automation.store import Store
from careertwin.profile.context import (
    build_context,
    context_path,
    read_context,
    studio_snapshot,
    write_context,
)
from careertwin.profile.graph_builder import _build_manual_graph

PROFILE = {
    "fullName": "Test Candidate",
    "email": "test@example.com",
    "phone": "+48 123 456 789",
    "location": "Gdańsk, Poland",
    "summary": "Python APIs for commercial clients.",
    "skills": ["Python", "FastAPI"],
    "languages": ["English B2"],
    "experience": [
        {
            "role": "Developer",
            "company": "Freelance",
            "start": "2024",
            "current": True,
            "description": "Built payment integrations.\nDeployed Docker services.",
        }
    ],
    "education": [
        {
            "degree": "BSc",
            "school": "Example University",
            "field": "CS",
            "start": "2024",
            "end": "2028",
        }
    ],
    "certificates": [{"name": "Python course", "issuer": "Example", "year": "2025"}],
}
PREFS = {
    "employmentStatus": "employed_open",
    "jobTitles": ["AI Engineer", "Python Developer"],
    "experienceLevel": "junior",
    "salary": {"mode": "auto"},
    "industries": ["it"],
    "workTypes": ["full_time", "part_time"],
    "startDate": "2026-10-15",
    "willingToRelocate": False,
    "workMode": ["remote", "onsite"],
    "remoteLocations": ["Poland"],
    "onsiteCities": ["Gdańsk"],
    "workAuth": [{"country": "Poland", "status": "Permanent Resident"}],
    "driversLicense": "no",
    "gender": "prefer_not",
    "disability": "prefer_not",
    "veteran": "no",
    "contactEmail": "test@example.com",
    "contactConsentNewsletter": False,
}


@pytest.fixture
def db(tmp_path, monkeypatch):
    monkeypatch.setattr(database, "DB_PATH", tmp_path / "studio.db")
    database.init_db()
    value = Store()
    value.initialize()
    return value


@pytest.mark.asyncio
async def test_all_answers_publish_and_agents_refresh_without_restart(db):
    db.save_settings(Settings(context="old context", queries=["Old Role"]))
    result = await studio.studio_submit(
        StudioSubmit(
            username="default",
            first_name="Test",
            last_name="Candidate",
            profile=PROFILE,
            preferences=PREFS,
        )
    )
    assert result["context_saved"]
    content = read_context()
    assert studio_snapshot(content) == {"profile": PROFILE, "preferences": PREFS}
    assert contact(content)["email"] == PROFILE["email"]
    assert "Mode: AUTO" in content
    assert "Willing to relocate: No" in content
    assert "120000" not in content
    with database.db_conn() as connection:
        row = connection.execute(
            "SELECT full_name, email, skills FROM user_profiles WHERE username='default'"
        ).fetchone()
    assert row[0] == PROFILE["fullName"] and row[1] == PROFILE["email"]
    assert json.loads(row[2]) == PROFILE["skills"]
    assert db.settings().context == content
    assert db.settings().queries == PREFS["jobTitles"]
    # Saving stale automation controls must not roll the shared profile backwards.
    db.save_settings(Settings(context="stale context", queries=["Wrong Role"]))
    assert db.settings().context == content
    assert db.settings().queries == PREFS["jobTitles"]
    facts = extract_facts(content)
    assert any(fact["text"] == "Built payment integrations." for fact in facts)
    assert not any(
        "prefer_not" in fact["text"] or "newsletter" in fact["text"].lower() for fact in facts
    )
    download = await studio.studio_about()
    assert download.body.decode() == content
    assert download.headers["content-disposition"].endswith('filename="ABOUT.md"')


@pytest.mark.asyncio
async def test_failed_about_write_does_not_complete_studio(db, monkeypatch):
    def denied(*args):
        raise PermissionError("read-only file")

    monkeypatch.setattr(studio, "write_context", denied)
    with pytest.raises(HTTPException) as error:
        await studio.studio_submit(
            StudioSubmit(username="default", profile=PROFILE, preferences=PREFS)
        )
    assert error.value.status_code == 503
    assert not (await studio.studio_state("default"))["exists"]


def test_partial_update_keeps_verified_background_but_not_another_candidate():
    previous = build_context(PROFILE, PREFS) + "\n## Projects\n\n- Verified independent project.\n"
    content = build_context({"fullName": "Test Candidate"}, PREFS, previous)
    assert "Verified independent project" in content
    assert "Built payment integrations" in content
    assert "test@example.com" in content
    other = build_context({"fullName": "Someone Else"}, PREFS, previous)
    assert "Verified independent project" not in other
    assert "Built payment integrations" not in other


def test_namespaces_and_unknowns():
    write_context("private candidate", "other")
    assert read_context() == ""
    assert read_context("other") == "private candidate"
    with pytest.raises(ValueError):
        context_path("../escape")
    content = build_context({"fullName": "Test Candidate"}, {"salary": {"mode": "auto"}})
    assert "Disability: Not provided" in content
    assert "Willing to relocate: Not provided" in content


def test_legacy_resume_graph_uses_shared_experience_and_education():
    write_context(build_context(PROFILE, PREFS))
    graph = _build_manual_graph("Test Candidate", "", "", "", "", "", "", "")
    assert graph.profile.email == PROFILE["email"]
    assert graph.experience[0].company == "Freelance"
    assert graph.experience[0].start_date is None  # a year alone does not invent January 1
    assert graph.education[0].institution == "Example University"
    assert {item.name for item in graph.skills} == {"Python", "FastAPI"}
    assert "salary" not in graph.to_search_text().lower()


@pytest.mark.asyncio
@pytest.mark.parametrize("low,high,expected", [(10000, 16000, 12100), (14000, 20000, 16100)])
async def test_auto_salary_changes_with_vacancy(monkeypatch, low, high, expected):
    quote = f"{low}–{high} PLN gross per month"
    monkeypatch.setattr(
        salary,
        "call_ai",
        AsyncMock(
            return_value=json.dumps(
                {
                    "low": low,
                    "high": high,
                    "currency": "PLN",
                    "period": "month",
                    "basis": "gross",
                    "evidence": quote,
                }
            )
        ),
    )
    value = await salary.expectation(
        "Expected salary PLN gross per month",
        {"salary": quote},
        Settings(context=build_context(PROFILE, PREFS)),
    )
    assert value["amount"] == expected
    assert value["evidence"] == quote
    assert value["mode"] == "auto"


@pytest.mark.asyncio
@pytest.mark.parametrize(
    "question",
    [
        "Expected annual salary USD gross",
        "Expected monthly salary PLN net B2B",
        "Previous salary",
        "Are your salary expectations negotiable?",
    ],
)
async def test_salary_never_converts_incompatible_units_or_answers_history(monkeypatch, question):
    quote = "10000–16000 PLN gross per month"
    monkeypatch.setattr(
        salary,
        "call_ai",
        AsyncMock(
            return_value=json.dumps(
                {
                    "low": 10000,
                    "high": 16000,
                    "currency": "PLN",
                    "period": "month",
                    "basis": "gross",
                    "evidence": quote,
                }
            )
        ),
    )
    assert await salary.expectation(question, {"salary": quote}, Settings()) is None


@pytest.mark.asyncio
async def test_no_budget_or_fabricated_band_never_invents_amount(monkeypatch):
    response = AsyncMock(
        return_value=json.dumps(
            {
                "low": 8000,
                "high": 10000,
                "currency": "PLN",
                "period": "month",
                "basis": "gross",
                "evidence": "10000–16000 PLN gross per month",
            }
        )
    )
    monkeypatch.setattr(salary, "call_ai", response)
    assert (
        await salary.expectation("Expected salary", {"salary": "Not published"}, Settings()) is None
    )
    response.assert_not_called()
    assert (
        await salary.expectation(
            "Expected salary", {"salary": "10000–16000 PLN gross per month"}, Settings()
        )
        is None
    )


@pytest.mark.asyncio
async def test_legacy_default_amount_migrates_to_auto(db):
    preferences = {**PREFS, "salary": {"amount": 120000, "currency": "PLN"}}
    await studio.studio_submit(
        StudioSubmit(username="default", profile=PROFILE, preferences=preferences)
    )
    assert studio_snapshot(read_context())["preferences"]["salary"] == {"mode": "auto"}
    assert "120000" not in read_context()


def test_current_decline_does_not_copy_historical_gender_and_keeps_permit_facts():
    previous = build_context(PROFILE, PREFS)
    previous = previous.replace("| Email |", "| Prefix / gender | Mr / Male |\n| Email |")
    previous = previous.replace(
        "## Work authorization\n",
        "## Work authorization\n\n- Citizenship: Belarus.\n"
        "- Residence: Permanent residence, valid to 2036.\n",
    )
    content = build_context({"fullName": "Test Candidate"}, PREFS, previous)
    assert "Mr / Male" not in content
    assert "Gender: Prefer not to disclose" in content
    assert "Citizenship: Belarus" in content
    assert "valid to 2036" in content
