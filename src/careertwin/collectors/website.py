"""Personal website parser — extracts projects, about, blog, stack."""

from __future__ import annotations

from urllib.parse import urljoin

import httpx

from careertwin.net import safe_client
from bs4 import BeautifulSoup

from careertwin.graph.schema import (
    CareerGraph,
    Evidence,
    Project,
    SourceType,
    Technology,
)


class WebsiteCollector:
    """Parses personal websites for professional data."""

    def __init__(self):
        self.client = safe_client(
            timeout=15,
            follow_redirects=True,
            headers={"User-Agent": "CareerTwin/0.1 (career-data-collection)"},
        )

    def collect(self, url: str) -> CareerGraph:
        html = self._fetch(url)
        soup = BeautifulSoup(html, "lxml")

        projects = self._extract_projects(soup, url)
        summary = self._extract_about(soup)
        technologies = self._extract_stack(soup)

        return CareerGraph(
            profile={
                "full_name": "",
                "summary": summary,
                "website": url,
            },
            projects=projects,
            technologies=technologies,
        )

    def _fetch(self, url: str) -> str:
        resp = self.client.get(url)
        resp.raise_for_status()
        return resp.text

    def _extract_about(self, soup: BeautifulSoup) -> str:
        for selector in ["#about", ".about", "[data-section='about']", "section.about"]:
            el = soup.select_one(selector)
            if el:
                return el.get_text(separator="\n", strip=True)

        for h in soup.find_all(["h1", "h2"]):
            text = h.get_text(strip=True).lower()
            if "about" in text or "hi" in text or "hello" in text:
                next_el = h.find_next(["p", "div"])
                if next_el:
                    return next_el.get_text(separator="\n", strip=True)

        first_p = soup.find("p")
        if first_p:
            return first_p.get_text(strip=True)

        return ""

    def _extract_projects(self, soup: BeautifulSoup, base_url: str) -> list[Project]:
        projects = []
        seen_names: set[str] = set()

        for card in soup.select(
            "[class*='project'], [class*='card'], [data-section='projects'] > *"
        ):
            name_el = card.find(["h2", "h3", "h4", "a"])
            if not name_el:
                continue

            name = name_el.get_text(strip=True)
            if name.lower() in seen_names or len(name) < 2:
                continue
            seen_names.add(name.lower())

            desc_el = card.find(["p", "span", "div"])
            description = desc_el.get_text(strip=True) if desc_el else ""

            link_el = card.find("a", href=True)
            url = urljoin(base_url, link_el["href"]) if link_el else None

            tech_els = card.select("[class*='tag'], [class*='tech'], [class*='badge']")
            technologies = [
                Technology(name=t.get_text(strip=True), category="tool")
                for t in tech_els
                if t.get_text(strip=True)
            ]

            projects.append(Project(
                name=name,
                description=description,
                url=url,
                technologies=technologies,
                evidence=[Evidence(source=SourceType.WEBSITE, url=base_url)],
            ))

        return projects

    def _extract_stack(self, soup: BeautifulSoup) -> list[Technology]:
        technologies = []
        seen: set[str] = set()

        for section in soup.find_all(["section", "div"]):
            section_text = section.get_text(strip=True).lower()
            if any(kw in section_text for kw in ["stack", "tech", "tools", "technologies"]):
                for el in section.select("li, span, a, [class*='tag']"):
                    name = el.get_text(strip=True)
                    if name and name.lower() not in seen and len(name) < 50:
                        seen.add(name.lower())
                        technologies.append(Technology(name=name, category="tool"))

        return technologies

    def close(self):
        self.client.close()
