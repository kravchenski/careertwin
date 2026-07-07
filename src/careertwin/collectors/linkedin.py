"""LinkedIn data collector — extracts experience, skills, recommendations."""

from __future__ import annotations

import os
from typing import Optional

import httpx
from bs4 import BeautifulSoup

from careertwin.graph.schema import (
    CareerGraph,
    Education,
    Evidence,
    Experience,
    Profile,
    Skill,
    SkillCategory,
    SourceType,
)


class LinkedInCollector:
    """Collects professional data from LinkedIn profile.

    Note: LinkedIn limits public data access. For full data, provide
    an exported LinkedIn data archive (Settings > Data Privacy > Get a copy).
    """

    def __init__(self, cookie: Optional[str] = None):
        self.cookie = cookie or os.getenv("LINKEDIN_COOKIE")

    def collect_from_export(self, export_path: str) -> CareerGraph:
        """Parse LinkedIn data export (ZIP or CSV files)."""
        import csv
        import zipfile
        from pathlib import Path

        path = Path(export_path)
        if path.suffix == ".zip":
            return self._parse_zip_export(path)
        return self._parse_csv_export(path)

    def _parse_zip_export(self, zip_path: Path) -> CareerGraph:
        import csv
        import zipfile
        from io import TextIOWrapper

        experience = []
        education = []
        skills = []

        with zipfile.ZipFile(zip_path) as zf:
            for name in zf.namelist():
                lower = name.lower()
                with zf.open(name) as f:
                    reader = csv.DictReader(TextIOWrapper(f, encoding="utf-8"))

                    if "position" in lower or "experience" in lower:
                        for row in reader:
                            experience.append(self._row_to_experience(row))
                    elif "education" in lower:
                        for row in reader:
                            education.append(self._row_to_education(row))
                    elif "skill" in lower:
                        for row in reader:
                            skills.append(self._row_to_skill(row))

        profile = Profile(
            full_name="LinkedIn User",
            summary="",
        )

        return CareerGraph(
            profile=profile,
            experience=experience,
            education=education,
            skills=skills,
        )

    def _parse_csv_export(self, csv_path: Path) -> CareerGraph:
        import csv

        experience = []
        education = []
        skills = []

        with open(csv_path, encoding="utf-8") as f:
            reader = csv.DictReader(f)
            fieldnames = [fn.lower() for fn in (reader.fieldnames or [])]

            if any("position" in fn or "experience" in fn for fn in fieldnames):
                for row in reader:
                    experience.append(self._row_to_experience(row))
            elif "education" in "".join(fieldnames):
                for row in reader:
                    education.append(self._row_to_education(row))
            elif "skill" in "".join(fieldnames):
                for row in reader:
                    skills.append(self._row_to_skill(row))

        return CareerGraph(
            profile=Profile(full_name="LinkedIn User"),
            experience=experience,
            education=education,
            skills=skills,
        )

    def _row_to_experience(self, row: dict) -> Experience:
        evidence = Evidence(
            source=SourceType.LINKEDIN,
            description="LinkedIn experience",
            raw_data=row,
        )
        return Experience(
            company=row.get("Company", row.get("company", "")),
            position=row.get("Title", row.get("position", row.get("Role", ""))),
            description=row.get("Description", row.get("description", "")),
            location=row.get("Location", row.get("location", "")),
            evidence=[evidence],
        )

    def _row_to_education(self, row: dict) -> Education:
        evidence = Evidence(
            source=SourceType.LINKEDIN,
            description="LinkedIn education",
            raw_data=row,
        )
        return Education(
            institution=row.get("School", row.get("school", row.get("Institution", ""))),
            degree=row.get("Degree", row.get("degree", "")),
            field=row.get("Field of Study", row.get("field", "")),
            evidence=[evidence],
        )

    def _row_to_skill(self, row: dict) -> Skill:
        name = row.get("Skill", row.get("skill", row.get("Name", "")))
        return Skill(
            name=name,
            category=SkillCategory.DOMAIN,
            evidence=[Evidence(source=SourceType.LINKEDIN, description=f"LinkedIn skill: {name}")],
        )

    def collect_from_profile(self, linkedin_url: str) -> CareerGraph:
        """Scrape public LinkedIn profile (limited data)."""
        headers = {
            "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36"
        }
        cookies = {"li_at": self.cookie} if self.cookie else {}

        resp = httpx.get(linkedin_url, headers=headers, cookies=cookies, follow_redirects=True)
        resp.raise_for_status()

        soup = BeautifulSoup(resp.text, "lxml")

        name_tag = soup.select_one("h1")
        headline_tag = soup.select_one(".text-body-medium")

        profile = Profile(
            full_name=name_tag.get_text(strip=True) if name_tag else "Unknown",
            headline=headline_tag.get_text(strip=True) if headline_tag else "",
            linkedin=linkedin_url,
        )

        return CareerGraph(profile=profile)
