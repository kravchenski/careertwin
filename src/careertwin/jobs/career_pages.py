"""Universal career page scraper — works for any company via Playwright."""

from __future__ import annotations

import logging
import re
from typing import Any
from urllib.parse import urlparse

logger = logging.getLogger("careertwin.api")

_JOB_WORDS = [
    "engineer", "developer", "manager", "designer", "analyst",
    "scientist", "architect", "lead", "head", "director",
    "senior", "junior", "staff", "principal", "intern",
    "associate", "specialist", "consultant", "coordinator",
    "product", "software", "frontend", "backend", "devops",
    "data", "research", "marketing", "sales", "finance",
    "operations", "account", "business", "strategy", "security",
    "platform", "infrastructure", "cloud", "machine learning",
    "full stack", "fullstack", "mobile", "qa", "test",
]

_NAV_SKIP = {
    "home", "about", "contact", "blog", "news", "privacy", "terms",
    "cookie", "login", "sign in", "sign up", "careers", "jobs",
    "our culture", "benefits", "what we do", "how we hire",
    "learn more", "view all", "see all", "explore", "products",
    "solutions", "pricing", "get started", "start free", "join us",
}

_SKIP_EXT = (".svg", ".png", ".jpg", ".jpeg", ".gif", ".webp", ".pdf")


def _is_job_link(text: str, url: str) -> bool:
    """Check if a link looks like a job listing."""
    lower = text.lower().strip()
    if len(lower) < 5 or lower in _NAV_SKIP:
        return False
    if any(url.lower().endswith(ext) for ext in _SKIP_EXT):
        return False
    if url.startswith(("mailto:", "tel:", "javascript:")):
        return False

    parsed = urlparse(url)
    path = parsed.path.lower()

    if any(ats in url for ats in ["greenhouse.io", "lever.co", "ashbyhq.com", "myworkdayjobs.com", "smartrecruiters.com"]):
        return True

    job_paths = ["/jobs/", "/job/", "/positions/", "/openings/", "/vacancies/", "/requisition/", "/listing/"]
    if any(p in path for p in job_paths) and path not in ("/careers/", "/jobs/", "/job/"):
        return True

    if re.search(r"[0-9a-f]{8}-[0-9a-f]{4}-", path):
        return True

    if any(w in lower for w in _JOB_WORDS) and len(lower) >= 8:
        return True

    return False


async def scrape_career_page(url: str, company_name: str = "") -> list[dict[str, Any]]:
    """Scrape a career page for job listings using Playwright."""
    from playwright.async_api import async_playwright

    jobs = []
    try:
        async with async_playwright() as p:
            browser = await p.chromium.launch(
                headless=True,
                args=["--no-sandbox", "--disable-dev-shm-usage"]
            )
            context = await browser.new_context(
                user_agent="Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
                viewport={"width": 1280, "height": 800},
            )
            page = await context.new_page()

            try:
                await page.goto(url, wait_until="networkidle", timeout=25000)
                await page.wait_for_timeout(4000)

                for _ in range(5):
                    await page.evaluate("window.scrollBy(0, window.innerHeight)")
                    await page.wait_for_timeout(500)

                raw_jobs = await page.evaluate("""() => {
                    const results = [];
                    const seenUrls = new Set();
                    const links = document.querySelectorAll('a[href]');

                    const navSkip = ['home','about','contact','blog','news','privacy','terms',
                        'cookie','login','sign in','sign up','careers','jobs','our culture',
                        'benefits','what we do','how we hire','learn more','view all','see all',
                        'explore','products','solutions','pricing','get started','start free','join us',
                        'follow us','share','download','sign up for','subscribe','newsletter',
                        'cookie policy','privacy policy','terms of use','accessibility',
                        'all countries','all cities','all categories','all levels',
                        'clear filters','filter by','sort by','search','country','city',
                        'category','level','department','clear all'];
                    const jobWords = ['engineer','developer','manager','designer','analyst',
                        'scientist','architect','lead','head','director','senior','junior',
                        'staff','principal','intern','associate','specialist','consultant',
                        'product','software','frontend','backend','devops','data','research',
                        'marketing','sales','finance','operations','account','business','strategy',
                        'security','platform','infrastructure','cloud','machine learning',
                        'full stack','fullstack','mobile','qa','test','scrum','agile'];
                    const skipDomains = ['facebook.com','twitter.com','linkedin.com/company','instagram.com','youtube.com','tiktok.com'];

                    for (const link of links) {
                        const href = link.href;
                        const text = (link.textContent || '').replace(/\\s+/g, ' ').trim();
                        if (!href || !text || text.length < 5) continue;
                        if (seenUrls.has(href)) continue;
                        if (href.endsWith('.svg') || href.endsWith('.png') || href.endsWith('.jpg') || href.endsWith('.pdf')) continue;
                        if (href.startsWith('mailto:') || href.startsWith('tel:') || href.startsWith('javascript:')) continue;
                        if (skipDomains.some(d => href.includes(d))) continue;

                        const lower = text.toLowerCase();
                        if (navSkip.includes(lower) || lower.length < 5) continue;

                        // Skip filter/option elements (usually short labels)
                        if (text.length < 20 && !jobWords.some(w => lower.includes(w))) continue;

                        let isJob = false;

                        // ATS links
                        if (/greenhouse\\.io|lever\\.co|ashbyhq\\.com|myworkdayjobs\\.com|smartrecruiters\\.com/.test(href)) {
                            isJob = true;
                        }

                        // Job path patterns
                        const path = new URL(href).pathname.toLowerCase();
                        if (['/jobs/','/job/','/positions/','/openings/','/vacancies/','/requisition/','/listing/'].some(p => path.includes(p))) {
                            if (path !== '/careers/' && path !== '/jobs/' && path !== '/job/') isJob = true;
                        }

                        // UUID in path
                        if (/[0-9a-f]{8}-[0-9a-f]{4}-/.test(path)) isJob = true;

                        // Job words in text — require longer text to avoid filter labels
                        if (!isJob && jobWords.some(w => lower.includes(w)) && text.length >= 15) isJob = true;

                        if (isJob) {
                            seenUrls.add(href);
                            const parts = text.split(/[•|–]/);
                            const title = parts[0].trim().substring(0, 160);
                            const location = parts.length > 1 ? parts[1].trim().substring(0, 100) : '';
                            results.push({ title, url: href, location });
                        }
                    }
                    return results;
                }""")

                company = company_name or urlparse(url).netloc.replace("www.", "")
                for j in raw_jobs:
                    jobs.append({
                        "company": company,
                        "title": j["title"],
                        "url": j["url"],
                        "location": j["location"],
                        "source": "playwright",
                        "description": "",
                    })

            except Exception as e:
                logger.warning("Playwright scrape error for %s: %s", url, e)
            finally:
                await browser.close()

    except Exception as e:
        logger.warning("Playwright launch error: %s", e)

    return jobs
