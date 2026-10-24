"""CareerTwin API — company collector via LinkedIn search and URL extraction."""

from __future__ import annotations

import json
import logging
from typing import Any

from careertwin.browser import (
    _agent_browser_available,
    _agent_browser_eval,
    _agent_browser_open,
    _agent_browser_run,
    _agent_browser_session,
)
from careertwin.db import db_conn
from careertwin.settings import COMPANIES_FILE

logger = logging.getLogger("careertwin.api")

_COMPANIES_FILE = COMPANIES_FILE
_companies_cache: list[dict[str, str]] | None = None

_INTERNSHIP_URLS: dict[str, str] = {
    "google": "https://www.google.com/about/careers/applications/students/",
    "microsoft": "https://careers.microsoft.com/students/us/en",
    "amazon": "https://www.amazon.jobs/en/teams/internships-for-students",
    "amazon-web-services": "https://www.amazon.jobs/en/teams/internships-for-students",
    "aws": "https://www.amazon.jobs/en/teams/internships-for-students",
    "ibm": "https://www.ibm.com/careers/students/",
    "salesforce": "https://www.salesforce.com/company/careers/university-recruiting/",
    "jpmorgan": "https://careers.jpmorgan.com/global/en/students",
    "goldmansachs": "https://www.goldmansachs.com/careers/students/",
    "morgan-stanley": "https://www.morganstanley.com/people/students-graduates",
    "sap": "https://jobs.sap.com/students",
    "cisco": "https://jobs.cisco.com/jobs/students",
    "dell": "https://jobs.dell.com/students",
    "mastercard": "https://careers.mastercard.com/us/en/students",
    "walmart": "https://careers.walmart.com/students",
    "spotify": "https://www.lifeatspotify.com/students",
    "pepsico": "https://www.pepsicojobs.com/students",
    "procter-gamble": "https://www.pgcareers.com/students",
}


def _internship_url(name: str) -> str:
    """Verified student/intern page if known, else empty (internships are covered by the
    regular careers page for most companies — no separate "Interns" link should show)."""
    return _INTERNSHIP_URLS.get(name.lower(), "")


def _load_companies() -> list[dict[str, str]]:
    """Load companies from companies.json (cached in memory)."""
    global _companies_cache
    if _companies_cache is not None:
        return _companies_cache
    try:
        with open(_COMPANIES_FILE, "r") as f:
            _companies_cache = json.load(f)
    except Exception as exc:
        logger.warning("Failed to load companies.json: %s", exc)
        _companies_cache = []
    return _companies_cache


def _companies_to_dict(companies: list[dict]) -> dict[str, dict]:
    """Convert companies list to dict keyed by lowercase name."""
    result = {}
    for c in companies:
        key = c.get("name", "").lower().replace(" ", "-")
        result[key] = c
    return result


def save_known_companies_to_db() -> int:
    """Sync companies.json into the database. Returns count of new companies saved."""
    companies = _load_companies()
    saved = 0
    for c in companies:
        name = c.get("name", "")
        if not name:
            continue
        domain = c.get("domain", "")
        ats_platform = c.get("ats_platform", "")
        careers_url = c.get("careers", "")
        github = c.get("github", f"https://github.com/{name.lower().replace(' ', '')}")
        urls = {
            "website": f"https://www.{domain}" if domain else "",
            "careers": careers_url,
            "internships": _internship_url(name),
            "github": github,
            "learning": f"https://learn.{domain}" if domain else "",
            "linkedin": f"https://www.linkedin.com/company/{name.lower().replace(' ', '')}",
            "glassdoor": f"https://www.glassdoor.com/Reviews/{name.replace(' ', '-')}-reviews-SRCH_KE0,{len(name)}.htm",
            "wellfound": f"https://wellfound.com/company/{name.lower().replace(' ', '')}",
        }
        try:
            with db_conn() as conn:
                row = conn.execute("SELECT id FROM companies WHERE name = ?", (name,)).fetchone()
                if row:
                    conn.execute(
                        "UPDATE companies SET github=?, careers=?, website=?, linkedin=?, glassdoor=?, wellfound=?, internships=?, learning=?, ats_platform=? WHERE name=?",
                        (urls["github"], urls["careers"], urls["website"], urls["linkedin"], urls["glassdoor"], urls["wellfound"], urls["internships"], urls["learning"], ats_platform, name),
                    )
                    conn.commit()
                    continue
        except Exception:
            continue

        save_company_to_db({
            "name": name,
            "description": "",
            "industry": "",
            "employee_count": "",
            "ats_platform": ats_platform,
            **urls,
        })
        saved += 1
    return saved


def search_companies(query: str, limit: int = 20) -> list[dict[str, Any]]:
    """Search LinkedIn for companies using agent-browser."""
    if not _agent_browser_available():
        logger.warning("agent-browser not available for company search")
        return []

    session = _agent_browser_session("company-search")
    search_url = (
        "https://www.linkedin.com/search/results/companies/"
        f"?keywords={query}&origin=FACETED_SEARCH&spellCorrectionEnabled=true"
    )

    open_ok, _stdout, _stderr = _agent_browser_open(search_url, session=session, wait_ms=4000)
    if not open_ok:
        logger.warning("Failed to open LinkedIn company search: %s", _stderr)
        return []

    _agent_browser_run(["wait", "3000"], session=session, timeout=10)

    companies: list[dict[str, Any]] = []
    seen_names: set[str] = set()

    for _scroll_round in range(5):
        raw = _agent_browser_eval(
            r"""
            (() => {
                const cards = document.querySelectorAll(
                    '.entity-result__item, .reusable-search__result-container, [data-view-name="search-result"]'
                );
                const results = [];
                cards.forEach(card => {
                    const nameEl = card.querySelector(
                        '.entity-result__title-text a, .app-aware-link span[aria-hidden="true"], .reusable-entity-lockup__title'
                    );
                    const descEl = card.querySelector(
                        '.entity-result__primary-subtitle, .reusable-entity-lockup__subtitle, .entity-result__caption'
                    );
                    const linkEl = card.querySelector('a[href*="/company/"]');
                    const imgEl = card.querySelector('img');
                    if (nameEl) {
                        results.push({
                            name: nameEl.textContent.trim(),
                            description: descEl ? descEl.textContent.trim() : '',
                            url: linkEl ? linkEl.href : '',
                            logo: imgEl ? imgEl.src : ''
                        });
                    }
                });
                return JSON.stringify(results);
            })()
            """,
            session=session,
            timeout=15,
        )

        if raw:
            try:
                items = json.loads(raw)
                for item in items:
                    if not isinstance(item, dict):
                        continue
                    name = item.get("name", "").strip()
                    if not name or name.lower() in seen_names:
                        continue
                    seen_names.add(name.lower())
                    companies.append({
                        "name": name,
                        "description": item.get("description", ""),
                        "linkedin_url": item.get("url", ""),
                        "logo": item.get("logo", ""),
                    })
                    if len(companies) >= limit:
                        break
            except json.JSONDecodeError:
                pass

        if len(companies) >= limit:
            break

        _agent_browser_run(["scroll", "window", "1500"], session=session, timeout=10)
        _agent_browser_run(["wait", "2000"], session=session, timeout=10)

    _agent_browser_run(["close"], session=session, timeout=5)
    logger.info("Found %d companies for query '%s'", len(companies), query)
    return companies[:limit]


def extract_company_urls(company_name: str, linkedin_url: str = "") -> dict[str, Any]:
    """Extract company URLs using known data from companies.json."""
    lower_name = company_name.lower().replace(" ", "").replace(".", "").replace(",", "")
    companies = _load_companies()
    known = _companies_to_dict(companies)

    urls: dict[str, Any] = {
        "website": "",
        "careers": "",
        "internships": "",
        "certifications": "",
        "learning": "",
        "github": "",
        "linkedin": linkedin_url,
        "glassdoor": "",
        "wellfound": "",
    }

    if lower_name in known:
        info = known[lower_name]
        domain = info.get("domain", "")
        urls["website"] = f"https://www.{domain}" if domain else ""
        urls["careers"] = info.get("careers", "")
        urls["internships"] = _internship_url(lower_name)
        urls["github"] = info.get("github", f"https://github.com/{lower_name}")
        urls["learning"] = f"https://learn.{domain}" if domain else ""
    else:
        urls["github"] = f"https://github.com/{lower_name}"

    urls["glassdoor"] = f"https://www.glassdoor.com/Reviews/{company_name.replace(' ', '-')}-reviews-SRCH_KE0,{len(company_name)}.htm"
    urls["wellfound"] = f"https://wellfound.com/company/{lower_name}"
    urls["linkedin"] = linkedin_url or f"https://www.linkedin.com/company/{lower_name}"

    return urls


def save_company_to_db(company: dict[str, Any]) -> int | None:
    """Save a company to the database. Returns the company ID."""
    try:
        with db_conn() as conn:
            cursor = conn.execute(
                """INSERT OR IGNORE INTO companies
                   (name, description, industry, employee_count, website, careers,
                    internships, certifications, learning, github, linkedin, glassdoor,
                    wellfound, linkedin_raw, ats_platform, cached_at)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))""",
                (
                    company.get("name", ""),
                    company.get("description", ""),
                    company.get("industry", ""),
                    company.get("employee_count", ""),
                    company.get("website", ""),
                    company.get("careers", ""),
                    company.get("internships", ""),
                    company.get("certifications", ""),
                    company.get("learning", ""),
                    company.get("github", ""),
                    company.get("linkedin", ""),
                    company.get("glassdoor", ""),
                    company.get("wellfound", ""),
                    json.dumps(company.get("linkedin_raw", {})),
                    company.get("ats_platform", ""),
                ),
            )
            conn.commit()
            return cursor.lastrowid
    except Exception as exc:
        logger.warning("Failed to save company %s: %s", company.get("name"), exc)
        return None


def company_exists_in_db(name: str) -> bool:
    """Check if a company already exists in the database."""
    try:
        with db_conn() as conn:
            row = conn.execute(
                "SELECT id FROM companies WHERE name = ?", (name,)
            ).fetchone()
            return row is not None
    except Exception:
        return False


def get_all_companies() -> list[dict[str, Any]]:
    """Get all companies from the database."""
    try:
        with db_conn() as conn:
            conn.row_factory = __import__("sqlite3").Row
            rows = conn.execute(
                "SELECT * FROM companies ORDER BY name"
            ).fetchall()
            return [dict(row) for row in rows]
    except Exception as exc:
        logger.warning("Failed to get companies: %s", exc)
        return []


def get_company_by_name(name: str) -> dict[str, Any] | None:
    """Get a single company by name."""
    try:
        with db_conn() as conn:
            conn.row_factory = __import__("sqlite3").Row
            row = conn.execute(
                "SELECT * FROM companies WHERE name = ?", (name,)
            ).fetchone()
            return dict(row) if row else None
    except Exception:
        return None


def collect_companies(query: str = "software engineer", limit: int = 50) -> dict[str, Any]:
    """Auto-collect IT companies: search LinkedIn, extract URLs, save to DB."""
    raw_companies = search_companies(query, limit=limit * 2)

    saved = 0
    skipped = 0
    results: list[dict[str, Any]] = []

    for raw in raw_companies:
        name = raw.get("name", "")
        if not name:
            continue

        if company_exists_in_db(name):
            skipped += 1
            existing = get_company_by_name(name)
            if existing:
                results.append(existing)
            continue

        urls = extract_company_urls(name, raw.get("linkedin_url", ""))

        company_data = {
            "name": name,
            "description": raw.get("description", ""),
            "industry": "",
            "employee_count": "",
            **urls,
            "linkedin_raw": raw,
        }

        cid = save_company_to_db(company_data)
        if cid:
            company_data["id"] = cid
            results.append(company_data)
            saved += 1

        if saved + skipped >= limit:
            break

    logger.info("Company collection done: %d saved, %d skipped (cached)", saved, skipped)
    return {
        "total": len(results),
        "saved": saved,
        "skipped": skipped,
        "companies": results,
    }
