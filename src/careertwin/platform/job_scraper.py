"""Job scraper — extracts job details from URLs."""

from __future__ import annotations

import re
from dataclasses import dataclass, field


@dataclass
class JobDetails:
    title: str = ""
    company: str = ""
    location: str = ""
    description: str = ""
    requirements: list[str] = field(default_factory=list)
    skills: list[str] = field(default_factory=list)
    salary: str = ""
    url: str = ""
    remote: bool = False


class JobScraper:
    """Scrapes job details from various sources."""

    def scrape_url(self, url: str) -> JobDetails:
        """Scrape job details from a URL."""
        import httpx

        try:
            response = httpx.get(url, timeout=15.0, follow_redirects=True, headers={
                "User-Agent": "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36"
            })
            response.raise_for_status()
            html = response.text
            return self._parse_html(html, url)
        except Exception:
            return JobDetails(url=url)

    def parse_description(self, text: str) -> JobDetails:
        """Parse a job description from raw text."""
        details = JobDetails(description=text)
        details.skills = self._extract_skills(text)
        details.requirements = self._extract_requirements(text)
        return details

    def _parse_html(self, html: str, url: str) -> JobDetails:
        """Parse job details from HTML content."""
        from bs4 import BeautifulSoup

        soup = BeautifulSoup(html, "html.parser")
        details = JobDetails(url=url)

        title_tag = soup.find("h1") or soup.find("title")
        if title_tag:
            details.title = title_tag.get_text(strip=True)[:100]

        meta_desc = soup.find("meta", attrs={"name": "description"})
        if meta_desc:
            details.description = meta_desc.get("content", "")

        if not details.description:
            article = soup.find("article") or soup.find("main") or soup.find("div", class_=re.compile(r"job|description|content"))
            if article:
                details.description = article.get_text(separator="\n", strip=True)[:3000]

        details.skills = self._extract_skills(details.description)
        details.requirements = self._extract_requirements(details.description)

        return details

    def _extract_skills(self, text: str) -> list[str]:
        """Extract skills from job description."""
        common_skills = [
            "React", "TypeScript", "JavaScript", "Python", "Node.js", "Go", "Rust",
            "AWS", "GCP", "Azure", "Docker", "Kubernetes", "GraphQL", "REST",
            "PostgreSQL", "MongoDB", "Redis", "Git", "CI/CD", "Agile", "Scrum",
            "HTML", "CSS", "Tailwind", "Vue", "Angular", "Svelte", "Next.js",
            "FastAPI", "Django", "Flask", "Express", "NestJS", "Rails",
            "Figma", "Adobe", "Sketch", "Prototyping", "User Research",
            "Machine Learning", "Deep Learning", "NLP", "Computer Vision",
            "System Design", "Microservices", "Event-Driven", "Message Queues",
        ]
        found = []
        text_lower = text.lower()
        for skill in common_skills:
            if skill.lower() in text_lower:
                found.append(skill)
        return found[:15]

    def _extract_requirements(self, text: str) -> list[str]:
        """Extract requirements from job description."""
        requirements = []
        lines = text.split("\n")
        for line in lines:
            line = line.strip()
            if not line:
                continue
            if any(marker in line for marker in ["•", "-", "*", "✓", "Requirements", "Qualifications", "What you"]):
                clean = re.sub(r"^[•\-*✓]\s*", "", line).strip()
                if clean and len(clean) > 10:
                    requirements.append(clean[:200])
        return requirements[:10]
