"""CV/Resume parser — extracts structured data from PDF and DOCX."""

from __future__ import annotations

import re
from pathlib import Path
from typing import Optional

from careertwin.graph.schema import (
    CareerGraph,
    Education,
    Experience,
    Profile,
    Skill,
    SkillCategory,
    SourceType,
    Evidence,
)


class ResumeParser:
    """Parses existing CV/resume files into CareerGraph."""

    def parse(self, file_path: str | Path) -> CareerGraph:
        path = Path(file_path)
        if path.suffix == ".pdf":
            return self._parse_pdf(path)
        elif path.suffix in (".docx", ".doc"):
            return self._parse_docx(path)
        elif path.suffix == ".txt":
            return self._parse_text(path)
        else:
            raise ValueError(f"Unsupported format: {path.suffix}")

    def _parse_pdf(self, path: Path) -> CareerGraph:
        text = self._extract_pdf_text(path)
        return self._parse_text_content(text, str(path))

    def _parse_docx(self, path: Path) -> CareerGraph:
        from docx import Document

        doc = Document(str(path))
        paragraphs = [p.text for p in doc.paragraphs if p.text.strip()]
        text = "\n".join(paragraphs)

        for table in doc.tables:
            for row in table.rows:
                cells = [cell.text.strip() for cell in row.cells if cell.text.strip()]
                if cells:
                    text += "\n" + " | ".join(cells)

        return self._parse_text_content(text, str(path))

    def _parse_text(self, path: Path) -> CareerGraph:
        text = path.read_text(encoding="utf-8")
        return self._parse_text_content(text, str(path))

    def _extract_pdf_text(self, path: Path) -> str:
        from pdfminer.high_level import extract_text
        return extract_text(str(path))

    def _parse_text_content(self, text: str, source: str) -> CareerGraph:
        lines = [line.strip() for line in text.split("\n") if line.strip()]

        profile = self._extract_profile(lines)
        sections = self._split_sections(lines)
        experience = self._extract_experience(sections)
        education = self._extract_education(sections)
        skills = self._extract_skills(sections)

        return CareerGraph(
            profile=profile,
            experience=experience,
            education=education,
            skills=skills,
            evidence=[Evidence(source=SourceType.RESUME, url=source)],
        )

    def _extract_profile(self, lines: list[str]) -> Profile:
        name = ""
        email = ""
        phone = ""
        location = ""
        website = ""
        linkedin = ""

        for line in lines[:10]:
            if not name and not any(c in line for c in ["@", "http", "•", "|"]):
                if len(line.split()) <= 4 and not line[0].isdigit():
                    name = line
                    continue

            email_match = re.search(r"[\w.-]+@[\w.-]+\.\w+", line)
            if email_match:
                email = email_match.group()

            phone_match = re.search(r"[\+]?[\d\s\-\(\)]{7,}", line)
            if phone_match and not phone:
                phone = phone_match.group().strip()

            if "linkedin.com" in line.lower():
                linkedin = line
            if "github.com" in line.lower():
                website = line

            if not location and any(w in line.lower() for w in ["city", "remote", "country"]):
                location = line

        return Profile(
            full_name=name,
            email=email,
            phone=phone,
            location=location,
            website=website,
            linkedin=linkedin,
        )

    def _split_sections(self, lines: list[str]) -> dict[str, list[str]]:
        section_headers = [
            "experience", "work", "employment", "career",
            "education", "academic",
            "skills", "technologies", "tech stack",
            "projects", "portfolio",
            "certifications", "awards", "summary", "about",
        ]

        sections: dict[str, list[str]] = {"other": []}
        current = "other"

        for line in lines:
            lower = line.lower().rstrip(":")
            if any(h in lower for h in section_headers) and len(line) < 50:
                current = lower.split(":")[0].strip()
                sections[current] = []
            else:
                sections[current].append(line)

        return sections

    def _extract_experience(self, sections: dict[str, list[str]]) -> list[Experience]:
        exp_keys = ["experience", "work", "employment", "career"]
        lines = []
        for key in exp_keys:
            if key in sections:
                lines = sections[key]
                break

        if not lines:
            return []

        experiences = []
        current_exp: Optional[dict] = None

        for line in lines:
            if self._is_date_range(line):
                if current_exp:
                    dates = self._parse_date_range(line)
                    current_exp["start_date"] = dates[0]
                    current_exp["end_date"] = dates[1] if len(dates) > 1 else None
                    experiences.append(current_exp)
                current_exp = {"company": "", "position": "", "description": "", "location": ""}
            elif current_exp is None:
                current_exp = {"company": "", "position": line, "description": "", "location": ""}
            elif not current_exp["company"]:
                current_exp["company"] = line
            elif not current_exp["position"]:
                current_exp["position"] = line
            else:
                current_exp["description"] += line + "\n"

        if current_exp and current_exp.get("company"):
            experiences.append(current_exp)

        return [
            Experience(
                company=e["company"],
                position=e["position"],
                description=e["description"].strip(),
                start_date=e.get("start_date"),
                end_date=e.get("end_date"),
                evidence=[Evidence(source=SourceType.RESUME, description="Parsed from resume")],
            )
            for e in experiences
        ]

    def _extract_education(self, sections: dict[str, list[str]]) -> list[Education]:
        edu_keys = ["education", "academic"]
        lines = []
        for key in edu_keys:
            if key in sections:
                lines = sections[key]
                break

        educations = []
        for line in lines:
            parts = [p.strip() for p in re.split(r"[•|,]", line) if p.strip()]
            if parts:
                educations.append(Education(
                    institution=parts[0] if len(parts) > 0 else "",
                    degree=parts[1] if len(parts) > 1 else "",
                    field=parts[2] if len(parts) > 2 else None,
                    evidence=[Evidence(source=SourceType.RESUME, description="Parsed from resume")],
                ))

        return educations

    def _extract_skills(self, sections: dict[str, list[str]]) -> list[Skill]:
        skill_keys = ["skills", "technologies", "tech stack", "tools"]
        raw = []
        for key in skill_keys:
            if key in sections:
                raw = sections[key]
                break

        skills = []
        seen: set[str] = set()

        for line in raw:
            items = re.split(r"[•,|/]", line)
            for item in items:
                name = item.strip()
                if name and name.lower() not in seen:
                    seen.add(name.lower())
                    category = self._classify_skill(name)
                    skills.append(Skill(
                        name=name,
                        category=category,
                        evidence=[Evidence(source=SourceType.RESUME, description="Parsed from resume")],
                    ))

        return skills

    def _classify_skill(self, name: str) -> SkillCategory:
        languages = {"python", "javascript", "typescript", "java", "c++", "c#", "go", "rust", "ruby", "php", "swift", "kotlin"}
        frameworks = {"react", "vue", "angular", "django", "flask", "fastapi", "spring", "rails", "nextjs", "nuxt"}

        lower = name.lower()
        if lower in languages:
            return SkillCategory.LANGUAGE
        if lower in frameworks:
            return SkillCategory.FRAMEWORK
        return SkillCategory.TOOL

    def _is_date_range(self, line: str) -> bool:
        patterns = [
            r"\d{4}\s*[-–—]\s*(?:\d{4}|present|now|current)",
            r"(?:jan|feb|mar|apr|may|jun|jul|aug|sep|oct|nov|dec)\w*\s+\d{4}",
        ]
        return any(re.search(p, line.lower()) for p in patterns)

    def _parse_date_range(self, line: str) -> list:
        from datetime import date

        years = re.findall(r"\d{4}", line)
        dates = []
        for y in years[:2]:
            try:
                dates.append(date(int(y), 1, 1))
            except ValueError:
                pass
        return dates
