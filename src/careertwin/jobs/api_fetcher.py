"""Direct API job fetchers for companies with public career APIs."""

from __future__ import annotations

import html as _html
import logging
import re
from typing import Any

import httpx

from careertwin.net import safe_async_client

logger = logging.getLogger("careertwin.api")


def _strip_html(text: str) -> str:
    """Remove HTML tags and decode entities."""
    text = _html.unescape(text)
    text = re.sub(r"<[^>]+>", " ", text)
    text = re.sub(r"\s+", " ", text).strip()
    return text


async def fetch_epam_jobs(query: str = "", limit: int = 50) -> list[dict[str, Any]]:
    """Fetch jobs from EPAM careers API."""
    url = "https://careers.epam.com/api/jobs/v2/search/careers-i18n"
    params = {
        "from": 0,
        "lang": "en",
        "q": query,
        "size": min(limit, 100),
        "sortBy": "relevance",
        "websiteLocale": "en-us",
    }
    try:
        async with safe_async_client(timeout=15) as client:
            resp = await client.get(url, params=params)
            data = resp.json()
            jobs = data.get("data", {}).get("jobs", [])
            results = []
            for j in jobs:
                cities = j.get("city", [])
                location = ", ".join([c.get("name", "") for c in cities[:2]]) if cities else ""
                country = ""
                if cities and cities[0].get("country"):
                    country = cities[0]["country"].get("name", "")
                if country and country not in location:
                    location = f"{location}, {country}" if location else country

                vacancy_type = j.get("vacancy_type", "")
                work_type_map = {"Remote": "remote", "Hybrid": "hybrid", "Onsite": "onsite", "On-site": "onsite"}
                work_type = work_type_map.get(vacancy_type, vacancy_type.lower() if vacancy_type else "")

                uid = j.get("uid", "")
                seo = j.get("seo", {})
                job_url = f"https://careers.epam.com{seo.get('url', '')}" if seo.get("url") else f"https://careers.epam.com/en/vacancy/{uid}"

                results.append({
                    "company": "EPAM",
                    "title": j.get("name", ""),
                    "url": job_url,
                    "location": location,
                    "source": "epam_api",
                    "description": _strip_html(j.get("description", "")),
                    "skills": j.get("skills", []),
                    "seniority": j.get("seniority", ""),
                    "employment_type": work_type,
                })
            return results
    except Exception as e:
        logger.warning("EPAM API error: %s", e)
        return []


async def fetch_epam_job_full(url: str) -> str:
    """Fetch full EPAM job description via Playwright (on-demand)."""
    return await _fetch_epam_job_description(url)


async def _fetch_epam_job_description(url: str) -> str:
    """Fetch full job description from EPAM job page via Playwright."""
    try:
        from playwright.async_api import async_playwright
        async with async_playwright() as p:
            browser = await p.chromium.launch(headless=True, args=["--no-sandbox", "--disable-dev-shm-usage"])
            page = await browser.new_page()
            try:
                await page.goto(url, wait_until="networkidle", timeout=20000)
                await page.wait_for_timeout(2000)
                desc = await page.evaluate("""() => {
                    // EPAM specific selectors
                    const selectors = [
                        '.job-description__content',
                        '.vacancy-detail__description',
                        '[class*="job-description"]',
                        '[class*="vacancy-detail"]',
                        'article',
                        '.content-body',
                    ];
                    for (const sel of selectors) {
                        const el = document.querySelector(sel);
                        if (el && el.textContent.trim().length > 100) {
                            return el.textContent.trim();
                        }
                    }
                    // Fallback: find the main text block after the title
                    const allText = document.body.innerText;
                    const lines = allText.split('\\n').filter(l => l.trim().length > 20);
                    // Skip navigation lines, find content
                    const contentStart = lines.findIndex(l =>
                        l.includes('Responsibilities') || l.includes('Requirements') ||
                        l.includes('Description') || l.includes('About the role') ||
                        l.includes('What you will') || l.includes('We are seeking')
                    );
                    if (contentStart >= 0) {
                        return lines.slice(contentStart, contentStart + 80).join('\\n');
                    }
                    return '';
                }""")
                return _strip_html(desc)[:3000] if desc else ""
            except Exception:
                return ""
            finally:
                await browser.close()
    except Exception:
        return ""


async def fetch_jobs_from_api(company: str, query: str = "", limit: int = 50) -> list[dict[str, Any]]:
    """Route to the correct API fetcher based on company name."""
    lower = company.lower().replace(" ", "")
    if lower in ("epam", "epamsystems"):
        return await fetch_epam_jobs(query, limit)
    return []
