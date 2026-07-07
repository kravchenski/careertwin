"""CV/Resume parser — extracts structured data from PDF and DOCX."""

from __future__ import annotations

import re
from pathlib import Path
from typing import Optional

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
        texts: list[str] = []
        try:
            from pdfminer.high_level import extract_text
            miner_text = extract_text(str(path))
            if miner_text:
                texts.append(("pdfminer", miner_text))
        except Exception:
            pass
        try:
            from pypdf import PdfReader
            reader = PdfReader(str(path))
            raw = ""
            for page in reader.pages:
                raw += page.extract_text() + "\n"
            cleaned = self._clean_pypdf_text(raw)
            if cleaned:
                texts.append(("pypdf", cleaned))
        except Exception:
            pass
        if not texts:
            raise ValueError(f"Could not extract text from {path}")
        # Prefer pdfminer for multi-column PDFs, fall back to longer text
        for name, text in texts:
            if name == "pdfminer":
                return text
        return texts[0][1]

    def _clean_pypdf_text(self, text: str) -> str:
        text = re.sub(r"(\w)\s(\w)\s(\w)\s", r"\1\2\3 ", text)
        text = re.sub(r"(\w)\s(\w)\s", r"\1\2 ", text)
        text = re.sub(r"(\w)\s(\w)$", r"\1\2", text, flags=re.MULTILINE)
        text = re.sub(r"(\w)\s{2,}(\w)", r"\1 \2", text)
        return text

    def _preprocess_text(self, text: str) -> str:
        # Fix merged words from pdfminer (e.g. "MaksimKrauchyk" -> "Maksim Krauchyk")
        text = re.sub(r"([a-z])([A-Z])", r"\1 \2", text)
        text = re.sub(r"([A-Z])([A-Z][a-z])", r"\1 \2", text)

        section_headers = [
            "SUMMARY", "EXPERIENCE", "EDUCATION", "SKILLS", "PROJECTS",
            "CONTACT", "PROFILE", "LANGUAGES", "CERTIFICATIONS", "AWARDS",
            "COURSES", "CONTRIBUTIONS", "SELECTED PROJECTS", "FREELANCE EXPERIENCE",
            "HACKATHONS", "SKILLS PROFILE", "AI PLATFORM FIT",
        ]
        for header in section_headers:
            text = re.sub(rf"(\w)({header})", r"\1\n\2", text)
            text = re.sub(rf"({header})(\w)", r"\1\n\2", text)
        text = re.sub(r"([.!?:])([A-Z])", r"\1\n\2", text)
        text = re.sub(r"(\d{4}\s*[-–—]\s*(?:\d{4}|Present|current))", r"\n\1", text)

        # Add line breaks before email, phone, and URLs
        text = re.sub(r"(\+?\d{9,})", r"\n\1", text)
        text = re.sub(r"([\w.+-]+@[\w.-]+\.\w{2,})", r"\n\1", text)
        text = re.sub(r"(github\.com/)", r"\n\1", text)
        text = re.sub(r"(linkedin\.com/)", r"\n\1", text)

        # Add line breaks before "(Commercial Client)" and "(Telegram)" patterns
        text = re.sub(r"(\([^)]+\))\s*([A-Z])", r"\1\n\2", text)

        # Add line breaks before known section markers
        text = re.sub(r"(Designed, developed)", r"\n\1", text)
        text = re.sub(r"(Developed a)", r"\n\1", text)
        text = re.sub(r"(Managed the)", r"\n\1", text)

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

    def _parse_text_content(self, text: str, source: str) -> CareerGraph:
        lines = [line.strip() for line in text.split("\n") if line.strip()]
        profile = self._extract_profile(lines)
        sections = self._split_sections(lines)
        experience = self._extract_experience(sections)
        education = self._extract_education(sections)
        skills = self._extract_skills(sections)
        if len(skills) < 3:
            skills = self._extract_skills_from_text(text) or skills
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
        github = ""
        linkedin = ""

        # Join first few lines for easier parsing
        text = " ".join(lines[:15])

        # Extract email (look for @ ... .domain pattern, not phone numbers)
        email_match = re.search(r"[a-zA-Z][\w.+-]+@[\w.-]+\.\w{2,}", text)
        if email_match:
            email = email_match.group()

        # Extract phone (international format with +)
        phone_match = re.search(r"\+\d[\d\s\-\(\)]{6,}", text)
        if phone_match:
            phone = re.sub(r"\s", "", phone_match.group()).strip()

        # Extract LinkedIn - check all lines
        for line in lines:
            linkedin_match = re.search(r"linkedin\.com/in/([\w-]+)", line)
            if linkedin_match:
                linkedin = f"https://www.linkedin.com/in/{linkedin_match.group(1)}"
                break

        # Extract GitHub - check all lines
        for line in lines:
            github_match = re.search(r"github\.com/([a-zA-Z0-9_-]+?)(?:https?|$|\s)", line)
            if github_match:
                github = f"https://github.com/{github_match.group(1)}"
                break

        # Extract name (first 2-4 word line that looks like a name)
        for line in lines[:8]:
            line = line.strip()
            if not line or "@" in line or "http" in line or "|" in line:
                continue
            if any(h in line.upper() for h in [
                "SKILLS", "PROFILE", "EXPERIENCE", "EDUCATION",
                "PROJECTS", "PYTHON", "JAVASCRIPT", "LANGUAGES",
                "COURSES", "CONTRIBUTIONS", "HACKATHONS",
            ]):
                continue
            words = line.split()
            if 2 <= len(words) <= 4 and all(w[0].isupper() for w in words if w):
                name = line
                break

        # Fallback: extract name from the beginning of first line
        if not name and lines:
            first_line = lines[0]
            name_match = re.match(r"^([A-Z][a-z]+\s+[A-Z][a-z]+)", first_line)
            if name_match:
                name = name_match.group(1)

        # Extract location - look for "City, Country" in first line
        if lines:
            first_line = lines[0]
            # Try to find "City, Country" pattern - must be preceded by name or start of line
            location_match = re.search(
                r"(?:^|\s)([A-Za-zżźćńółęąśŻŹĆŃÓŁĘĄŚ]{2,}),\s+([A-Za-zżźćńółęąśŻŹĆŃÓŁĘĄŚ]{2,})",
                first_line,
            )
            if location_match:
                loc = location_match.group().strip()
                if len(loc) < 40:
                    location = loc

        return Profile(
            full_name=name,
            email=email or None,
            phone=phone or None,
            location=location or None,
            github=github or None,
            linkedin=linkedin or None,
        )
        if location_match:
            location = location_match.group()

        return Profile(
            full_name=name,
            email=email or None,
            phone=phone or None,
            location=location or None,
            github=github or None,
            linkedin=linkedin or None,
        )

    def _split_sections(self, lines: list[str]) -> dict[str, list[str]]:
        section_headers = [
            "experience", "work", "employment", "career", "freelance experience",
            "education", "academic",
            "skills", "technologies", "tech stack", "skills profile",
            "projects", "portfolio", "selected projects",
            "certifications", "awards", "summary", "about",
            "languages", "courses", "contributions", "hackathons",
            "ai platform fit",
        ]
        sections: dict[str, list[str]] = {"other": []}
        current = "other"
        for line in lines:
            lower = line.lower().rstrip(":")
            # Check if line contains a section header (even if mixed with other text)
            matched = False
            for header in section_headers:
                if header in lower:
                    # If the header is at the start or the line is short, treat as section header
                    idx = lower.find(header)
                    if idx == 0 or len(line) < 60:
                        current = header
                        if current not in sections:
                            sections[current] = []
                        # If there's text after the header on the same line, add it
                        rest = line[idx + len(header):].strip()
                        if rest:
                            sections[current].append(rest)
                        matched = True
                        break
                    else:
                        # Header is in the middle of a longer line - split it
                        before = line[:idx].strip()
                        after = line[idx + len(header):].strip()
                        if before:
                            sections[current].append(before)
                        current = header
                        if current not in sections:
                            sections[current] = []
                        if after:
                            sections[current].append(after)
                        matched = True
                        break
            if not matched:
                sections[current].append(line)
        return sections

    def _extract_experience(self, sections: dict[str, list[str]]) -> list[Experience]:
        exp_keys = ["experience", "work", "employment", "career", "freelance experience"]
        lines = []
        for key in exp_keys:
            if key in sections:
                lines.extend(sections[key])
        if not lines:
            return []

        experiences = []
        current_exp: Optional[dict] = None

        for line in lines:
            if "|" in line and self._is_date_range(line):
                parts = [p.strip() for p in line.split("|")]
                if len(parts) >= 2:
                    dates = self._parse_date_range(line)
                    exp: dict = {
                        "position": parts[0], "company": parts[1],
                        "description": "", "location": "",
                        "start_date": dates[0] if dates else None,
                        "end_date": dates[1] if len(dates) > 1 else None,
                    }
                    experiences.append(exp)
                    current_exp = exp
                    continue
            if self._is_date_range(line):
                dates = self._parse_date_range(line)
                if current_exp and current_exp not in experiences:
                    current_exp["start_date"] = dates[0] if dates else None
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

        if current_exp and current_exp.get("company") and current_exp not in experiences:
            experiences.append(current_exp)

        return [
            Experience(
                company=e["company"], position=e["position"],
                description=e["description"].strip(),
                start_date=e.get("start_date"), end_date=e.get("end_date"),
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
        skill_keys = [
            "skills", "technologies", "tech stack", "tools",
            "skills profile", "ai platform fit",
        ]
        raw = []
        for key in skill_keys:
            if key in sections:
                raw.extend(sections[key])
                break
        skills = []
        seen: set[str] = set()
        for line in raw:
            # Handle "Languages:" and "AI/Data:" prefixes
            line = re.sub(
                r"^(?:Languages|AI/Data|Technologies|Tools|Frameworks):\s*",
                "", line, flags=re.IGNORECASE,
            )
            items = re.split(r"[•,|/]", line)
            for item in items:
                name = item.strip()
                # Clean up common extraction artifacts
                name = re.sub(r"\s+", " ", name)
                if name and len(name) > 1 and name.lower() not in seen:
                    seen.add(name.lower())
                    category = self._classify_skill(name)
                    skills.append(Skill(
                        name=name, category=category,
                        evidence=[Evidence(
                            source=SourceType.RESUME,
                            description="Parsed from resume",
                        )],
                    ))
        return skills

    def _classify_skill(self, name: str) -> SkillCategory:
        languages = {
            "python", "javascript", "typescript", "java", "c++", "c#",
            "go", "rust", "ruby", "php", "swift", "kotlin", "sql", "graphql",
        }
        frameworks = {
            "react", "vue", "angular", "django", "flask", "fastapi",
            "spring", "rails", "nextjs", "nuxt", "langchain", "langgraph",
        }
        lower = name.lower().strip()
        if lower in languages:
            return SkillCategory.LANGUAGE
        if lower in frameworks:
            return SkillCategory.FRAMEWORK
        return SkillCategory.TOOL

    def _extract_skills_from_text(self, text: str) -> list[Skill]:
        known_skills = [
            "python", "javascript", "typescript", "java", "go", "rust", "ruby", "php",
            "react", "vue", "angular", "django", "flask", "fastapi", "spring",
            "docker", "kubernetes", "aws", "gcp", "azure", "postgresql", "mysql",
            "redis", "mongodb", "graphql", "rest", "sql", "git", "ci/cd",
            "langchain", "langgraph", "aiogram", "aiogram",
            "openai", "anthropic", "llm", "rag",
        ]
        lower = text.lower()
        skills = []
        seen: set[str] = set()
        for skill in known_skills:
            if skill in lower and skill not in seen:
                seen.add(skill)
                category = self._classify_skill(skill)
                skills.append(Skill(
                    name=skill.title() if skill.islower() else skill,
                    category=category,
                    evidence=[Evidence(
                        source=SourceType.RESUME,
                        description="Extracted from text",
                    )],
                ))
        return skills

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
