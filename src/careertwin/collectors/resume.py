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
        text = self._preprocess_text(text)
        return self._parse_text_content(text, str(path))

    def _extract_pdf_text(self, path: Path) -> str:
        try:
            from pypdf import PdfReader
            reader = PdfReader(str(path))
            text = ""
            for page in reader.pages:
                text += page.extract_text() + "\n"
            text = self._clean_pypdf_text(text)
            return text
        except Exception:
            from pdfminer.high_level import extract_text
            from pdfminer.layout import LAParams
            laparams = LAParams(
                line_margin=0.5,
                word_margin=0.1,
                char_margin=2.0,
                boxes_flow=0.5,
                detect_vertical=True,
            )
            return extract_text(str(path), laparams=laparams)

    def _clean_pypdf_text(self, text: str) -> str:
        """Clean pypdf output which often has spaces between characters."""
        import re
        text = re.sub(r"([a-zA-Z])\s([a-zA-Z])\s", r"\1\2 ", text)
        text = re.sub(r"([a-zA-Z])\s([a-zA-Z])$", r"\1\2", text, flags=re.MULTILINE)
        text = re.sub(r"(\w)\s{2,}(\w)", r"\1 \2", text)
        return text

    def _preprocess_text(self, text: str) -> str:
        """Add line breaks before section headers and improve formatting."""
        section_headers = [
            "SUMMARY", "EXPERIENCE", "EDUCATION", "SKILLS", "PROJECTS",
            "CONTACT", "PROFILE", "LANGUAGES", "CERTIFICATIONS", "AWARDS",
            "COURSES", "CONTRIBUTIONS", "SELECTED PROJECTS", "FREELANCE EXPERIENCE",
            "HACKATHONS", "SKILLS PROFILE", "FIT", "AI PLATFORM",
        ]
        
        for header in section_headers:
            text = re.sub(rf"(\w)({header})", r"\1\n\2", text)
            text = re.sub(rf"({header})(\w)", r"\1\n\2", text)
        
        text = re.sub(r"([a-z])([A-Z][a-z])", r"\1\n\2", text)
        text = re.sub(r"([.!?:])([A-Z])", r"\1\n\2", text)
        text = re.sub(r"(\d{4}\s*[-–—]\s*(?:\d{4}|Present|current))", r"\n\1", text)
        text = re.sub(r"(?:^|\n)([A-Z][a-z]+(?:\s+[A-Z][a-z]+)*)\s*[-–—]\s*", r"\n\1 - ", text)
        
        return text

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
        
        profile = self._extract_profile_from_text(text)
        experience = self._extract_experience_from_text(text)
        education = self._extract_education_from_text(text)
        skills = self._extract_skills_from_text(text)

        return CareerGraph(
            profile=profile,
            experience=experience,
            education=education,
            skills=skills,
            evidence=[Evidence(source=SourceType.RESUME, url=source)],
        )

    def _extract_profile_from_text(self, text: str) -> Profile:
        import re
        
        email = ""
        phone = ""
        github = ""
        linkedin = ""
        location = ""
        name = ""
        
        email_match = re.search(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}", text)
        if email_match:
            email = email_match.group()
        
        phone_match = re.search(r"\+?\d{10,15}", text)
        if phone_match:
            phone = phone_match.group()
        
        github_match = re.search(r"github\.com/([a-zA-Z0-9_-]+)", text)
        if github_match:
            github = f"https://github.com/{github_match.group(1)}"
        
        linkedin_match = re.search(r"linkedin\.com/in/([a-zA-Z0-9_-]+)", text)
        if linkedin_match:
            linkedin = f"https://linkedin.com/in/{linkedin_match.group(1)}"
        
        location_match = re.search(r"([A-Z][a-z]+(?:\s+[A-Z][a-z]+)*),\s*([A-Z][a-z]+(?:\s+[A-Z][a-z]+)*)", text)
        if location_match:
            location = location_match.group(0)
        
        name_match = re.search(r"^([A-Z][a-z]+\s+[A-Z][a-z]+)", text, re.MULTILINE)
        if name_match:
            name = name_match.group(1)
        
        return Profile(
            full_name=name,
            email=email,
            phone=phone,
            location=location,
            github=github,
            linkedin=linkedin,
        )

    def _extract_experience_from_text(self, text: str) -> list[Experience]:
        import re
        from datetime import date
        
        experiences = []
        
        patterns = [
            r"([A-Z][^@]*?)\s*[-–—]\s*([A-Z][^@]*?)\s*[-–—]\s*(\d{4})\s*[-–—]\s*(?:Present|current|\d{4})",
            r"([A-Z][a-zA-Z\s]+(?:Engineer|Developer|Manager|Director|Lead|Senior|Junior|Intern))\s*[-–—]\s*([A-Z][a-zA-Z\s]+)",
        ]
        
        for pattern in patterns:
            matches = re.finditer(pattern, text)
            for match in matches:
                groups = match.groups()
                if len(groups) >= 2:
                    exp = Experience(
                        position=groups[0].strip(),
                        company=groups[1].strip() if len(groups) > 1 else "",
                        description=groups[2].strip() if len(groups) > 2 else "",
                    )
                    experiences.append(exp)
        
        return experiences

    def _extract_education_from_text(self, text: str) -> list[Education]:
        import re
        
        educations = []
        
        patterns = [
            r"(?:University|College|School|Institute)[^,\n]*(?:,|\s)[^,\n]*(?:\d{4})",
            r"([A-Z][a-zA-Z\s]+(?:University|College|School|Institute))\s*[-–—]\s*([A-Z][a-zA-Z\s]+)",
        ]
        
        for pattern in patterns:
            matches = re.finditer(pattern, text)
            for match in matches:
                groups = match.groups()
                if groups:
                    edu = Education(
                        institution=groups[0].strip() if len(groups) > 0 else "",
                        degree=groups[1].strip() if len(groups) > 1 else "",
                    )
                    educations.append(edu)
        
        return educations

    def _extract_skills_from_text(self, text: str) -> list[Skill]:
        import re
        
        skills = []
        seen: set[str] = set()
        
        known_skills = {
            "python": "language", "javascript": "language", "typescript": "language",
            "java": "language", "go": "language", "rust": "language", "ruby": "language",
            "php": "language", "swift": "language", "kotlin": "language", "c": "language",
            "c++": "language", "c#": "language", "sql": "language",
            "react": "framework", "vue": "framework", "angular": "framework",
            "django": "framework", "flask": "framework", "fastapi": "framework",
            "spring": "framework", "rails": "framework", "nextjs": "framework",
            "nodejs": "tool", "docker": "tool", "kubernetes": "tool", "aws": "tool",
            "gcp": "tool", "azure": "tool", "git": "tool", "postgresql": "tool",
            "mysql": "tool", "redis": "tool", "mongodb": "tool",
        }
        
        skill_patterns = [
            r"(?:Skills?|Technologies?|Tech Stack|Languages?)[:\s]*([^\n]+)",
            r"(?:Proficient in|Experience with|Knowledge of)[:\s]*([^\n]+)",
        ]
        
        for pattern in skill_patterns:
            matches = re.finditer(pattern, text, re.IGNORECASE)
            for match in matches:
                skill_text = match.group(1) if match.lastindex else match.group(0)
                items = re.split(r"[,;|/]", skill_text)
                for item in items:
                    name = item.strip()
                    if name and name.lower() not in seen and len(name) < 50:
                        seen.add(name.lower())
                        category = known_skills.get(name.lower(), "tool")
                        skills.append(Skill(
                            name=name,
                            category=SkillCategory(category),
                        ))
        
        return skills

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
            # Handle pipe-separated format: "Position | Company | 2021 - Present"
            if "|" in line and self._is_date_range(line):
                parts = [p.strip() for p in line.split("|")]
                if len(parts) >= 2:
                    dates = self._parse_date_range(line)
                    exp: dict = {
                        "position": parts[0],
                        "company": parts[1],
                        "description": "",
                        "location": "",
                        "start_date": dates[0] if dates else None,
                        "end_date": dates[1] if len(dates) > 1 else None,
                    }
                    experiences.append(exp)
                    current_exp = exp
                    continue

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
