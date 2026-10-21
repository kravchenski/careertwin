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
        texts: list[tuple[str, str]] = []
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
                raw += (page.extract_text() or "") + "\n"
            cleaned = self._clean_pypdf_text(raw)
            if cleaned:
                texts.append(("pypdf", cleaned))
        except Exception:
            pass
        if not texts:
            raise ValueError(f"Could not extract text from {path}")
        return max(texts, key=lambda item: self._pdf_text_quality(item[1]))[1]

    def _clean_pypdf_text(self, text: str) -> str:
        text = re.sub(r"(\w)\s(\w)\s(\w)\s", r"\1\2\3 ", text)
        text = re.sub(r"(\w)\s(\w)\s", r"\1\2 ", text)
        text = re.sub(r"(\w)\s(\w)$", r"\1\2", text, flags=re.MULTILINE)
        text = re.sub(r"(\w)\s{2,}(\w)", r"\1 \2", text)
        text = re.sub(r"(?<=\b[A-Za-z]{2,})\s(?=[A-Za-z]{2,}\b)", "", text)
        text = re.sub(r"([a-z])([A-Z])", r"\1 \2", text)
        text = re.sub(r"\s{2,}", " ", text)
        return text

    def _pdf_text_quality(self, text: str) -> float:
        lines = [line.strip() for line in text.splitlines() if line.strip()]
        if not lines:
            return 0.0

        score = len(text)
        headers = ("SUMMARY", "EXPERIENCE", "EDUCATION", "SKILLS", "PROJECTS")
        score += sum(300 for line in lines if any(header in line.upper() for header in headers))

        fragmented = re.findall(r"(?:\b[a-zA-Z]\b[\s,.;:|/-]*){6,}", text)
        score -= len(fragmented) * 250

        long_words = re.findall(r"\b[a-zA-Z]{18,}\b", text)
        score -= len(long_words) * 40

        if "linkedin.com" in text.lower():
            score += 120
        if "github.com" in text.lower():
            score += 120

        return float(score)

    def _preprocess_text(self, text: str) -> str:
        """Fix common PDF extraction issues without breaking words."""
        text = re.sub(r"([a-z])([A-Z])", r"\1 \2", text)
        text = re.sub(r"([A-Z])([A-Z][a-z])", r"\1 \2", text)

        section_headers = [
            "SUMMARY", "EXPERIENCE", "EDUCATION", "SKILLS", "PROJECTS",
            "CONTACT", "PROFILE", "LANGUAGES", "CERTIFICATIONS", "AWARDS",
            "COURSES", "CONTRIBUTIONS", "SELECTED PROJECTS", "FREELANCE EXPERIENCE",
            "HACKATHONS", "SKILLS PROFILE", "AI PLATFORM FIT",
            "WORK HISTORY", "WORK EXPERIENCE", "PROFESSIONAL EXPERIENCE",
            "TECHNICAL SKILLS", "TECH STACK",
        ]
        for header in section_headers:
            text = re.sub(rf"(\s)({header})", r"\1\n\2", text)
            text = re.sub(rf"({header})(\s)", r"\1\n\2", text)

        text = re.sub(r"(\d{4}\s*[-–—]\s*(?:\d{4}|Present|current))", r"\n\1", text)

        text = re.sub(r"(\+?\d{9,})", r"\n\1", text)
        text = re.sub(r"([\w.+-]+@[\w.-]+\.\w{2,})", r"\n\1", text)
        text = re.sub(r"(github\.com/)", r"\n\1", text)
        text = re.sub(r"(linkedin\.com/)", r"\n\1", text)

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

        certificates = self._extract_certificates(sections)

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

        text = " ".join(lines[:15])

        email_match = re.search(r"[a-zA-Z][\w.+-]+@[\w.-]+\.\w{2,}", text)
        if email_match:
            email = email_match.group()

        phone_match = re.search(r"\+?\d[\d\s\-\(\)]{6,}", text)
        if phone_match:
            phone = re.sub(r"\s", "", phone_match.group()).strip()

        for line in lines:
            linkedin_match = re.search(r"linkedin\.com/in/([\w-]+)", line)
            if linkedin_match:
                linkedin = f"https://www.linkedin.com/in/{linkedin_match.group(1)}"
                break

        for line in lines:
            github_match = re.search(
                r"(?:https?://)?(?:www\.)?github\.com/([A-Za-z0-9_-]+)(?:/|$|\s)",
                line, flags=re.IGNORECASE,
            )
            if github_match:
                github = f"https://github.com/{github_match.group(1)}"
                break

        for line in lines[:5]:
            pipe_match = re.search(r"\|\s*([A-Za-zżźćńółęąśŻŹĆŃÓŁĘĄŚ]{2,}),\s*([A-Za-zżźćńółęąśŻŹĆŃÓŁĘĄŚ]{2,})", line)
            if pipe_match:
                location = pipe_match.group().strip().lstrip("|").strip()
                break
            end_match = re.search(r"([A-Za-zżźćńółęąśŻŹĆŃÓŁĘĄŚ]{2,}),\s*([A-Za-zżźćńółęąśŻŹĆŃÓŁĘĄŚ]{2,})\s*$", line)
            if end_match:
                loc = end_match.group().strip()
                if len(loc) < 40 and not any(c.isdigit() for c in loc):
                    location = loc
                    break

        for line in lines[:8]:
            line = line.strip()
            if not line or "@" in line or "http" in line or "|" in line:
                continue
            if any(h in line.upper() for h in [
                "SKILLS", "PROFILE", "EXPERIENCE", "EDUCATION",
                "PROJECTS", "PYTHON", "JAVASCRIPT", "LANGUAGES",
                "COURSES", "CONTRIBUTIONS", "HACKATHONS", "SUMMARY",
                "CERTIFICATIONS", "WORK", "EMPLOYMENT", "CAREER",
            ]):
                continue
            words = line.split()
            if 2 <= len(words) <= 5 and all(w[0].isupper() for w in words if w):
                name = line
                break

        if not name and lines:
            first_line = lines[0]
            name_match = re.match(r"^([A-Z][a-z]+\s+[A-Z][a-z]+)", first_line)
            if name_match:
                name = name_match.group(1)

        return Profile(
            full_name=name,
            email=email or None,
            phone=phone or None,
            location=location or None,
            github=github or None,
            linkedin=linkedin or None,
        )

    def _split_sections(self, lines: list[str]) -> dict[str, list[str]]:
        """Split resume lines into sections based on headers.
        
        Handles various CV formats with robust header detection.
        """
        section_map = {
            "experience": "experience",
            "work": "experience",
            "work history": "experience",
            "work experience": "experience",
            "professional experience": "experience",
            "employment": "experience",
            "career": "experience",
            "freelance experience": "experience",
            "education": "education",
            "academic": "education",
            "skills": "skills",
            "technologies": "skills",
            "tech stack": "skills",
            "technical skills": "skills",
            "skills profile": "skills",
            "projects": "projects",
            "portfolio": "projects",
            "selected projects": "projects",
            "certifications": "certificates",
            "certificates": "certificates",
            "courses": "certificates",
            "certifications & courses": "certificates",
            "certifications and courses": "certificates",
            "awards": "certificates",
            "summary": "summary",
            "about": "summary",
            "profile": "summary",
            "objective": "summary",
            "professional summary": "summary",
            "languages": "languages",
            "contributions": "other",
            "hackathons": "other",
            "ai platform fit": "other",
        }

        sections: dict[str, list[str]] = {"other": []}
        current = "other"
        section_order = []

        for line in lines:
            stripped = line.strip()
            lower = stripped.lower().rstrip(":")

            matched = False

            if lower in section_map:
                current = section_map[lower]
                if current not in sections:
                    sections[current] = []
                    section_order.append(current)
                matched = True
            else:
                for header, section in section_map.items():
                    if lower.startswith(header):
                        current = section
                        if current not in sections:
                            sections[current] = []
                            section_order.append(current)
                        rest = stripped[len(header):].strip().lstrip(":").strip()
                        if rest:
                            sections[current].append(rest)
                        matched = True
                        break

            if not matched:
                sections[current].append(stripped)

        for sec_name in section_order:
            if not sections[sec_name]:
                pass

        return sections

    def _extract_experience(self, sections: dict[str, list[str]]) -> list[Experience]:
        exp_keys = ["experience", "work", "work history", "work experience",
                     "professional experience", "employment", "career",
                     "freelance experience"]
        lines = []
        for key in exp_keys:
            if key in sections:
                lines.extend(sections[key])
        if not lines:
            return []

        experiences = []
        current_exp: Optional[dict] = None

        def _start_new(raw_line: str) -> dict:
            m = re.match(r"^(.+?)\s*[—–]\s*(.+)$", raw_line)
            if m:
                return {"company": m.group(1).strip(), "position": m.group(2).strip(),
                         "description": "", "location": ""}
            return {"company": "", "position": raw_line, "description": "", "location": ""}

        def _looks_like_new_header(raw_line: str) -> bool:
            s = raw_line.strip()
            return (
                bool(s)
                and len(s) <= 100
                and not s.endswith((".", ",", ";", ":"))
                and bool(re.search(r"[—–]", s))
            )

        for line in lines:
            stripped = line.strip()

            if stripped in ("•", "-", "*", "◦", "▪", "‣"):
                continue

            if line.lower().rstrip(":") in ("experience", "work history", "work experience",
                                              "professional experience", "employment"):
                continue

            line = re.sub(r"\s*\|\s*$", "", line)

            has_date = self._is_date_range(line)

            if has_date:
                dates = self._parse_date_range(line)
                if (
                    current_exp is not None
                    and current_exp.get("start_date") is None
                    and (current_exp.get("position") or current_exp.get("company"))
                ):
                    current_exp["start_date"] = dates[0] if dates else None
                    current_exp["end_date"] = dates[1] if len(dates) > 1 else None
                    continue
                if current_exp:
                    if current_exp.get("position") or current_exp.get("company"):
                        experiences.append(current_exp)
                date_text = re.sub(r"\d{4}\s*[-–—]\s*(?:\d{4}|present|now|current).*$", "", line, flags=re.IGNORECASE).strip()
                company = ""
                position = ""
                if "|" in date_text:
                    parts = [p.strip() for p in date_text.split("|")]
                    position = parts[0] if len(parts) > 0 else ""
                    company = parts[1] if len(parts) > 1 else ""
                current_exp = {
                    "company": company, "position": position,
                    "description": "", "location": "",
                    "start_date": dates[0] if dates else None,
                    "end_date": dates[1] if len(dates) > 1 else None,
                }
                continue

            if current_exp is None:
                if "|" in line:
                    parts = [p.strip() for p in line.split("|")]
                    if len(parts) >= 2 and parts[0] and parts[1]:
                        current_exp = {
                            "position": parts[0], "company": parts[1],
                            "description": "", "location": "",
                        }
                        continue
                current_exp = _start_new(line)
            elif current_exp["company"] and current_exp["position"]:
                if _looks_like_new_header(line):
                    experiences.append(current_exp)
                    current_exp = _start_new(line)
                    continue
                current_exp["description"] += line + "\n"
            elif not current_exp["company"]:
                if "|" in line:
                    parts = [p.strip() for p in line.split("|")]
                    current_exp["position"] = parts[0]
                    current_exp["company"] = parts[1] if len(parts) > 1 else ""
                else:
                    current_exp["company"] = line
            elif not current_exp["position"]:
                current_exp["position"] = line

        if current_exp and (current_exp.get("position") or current_exp.get("company")):
            experiences.append(current_exp)

        return [
            Experience(
                company=e["company"], position=e["position"],
                description=e["description"].strip(),
                start_date=e.get("start_date"), end_date=e.get("end_date"),
                evidence=[Evidence(source=SourceType.RESUME, description="Parsed from resume")],
            )
            for e in experiences
            if e.get("position") or e.get("company")
        ]

    def _extract_education(self, sections: dict[str, list[str]]) -> list[Education]:
        edu_keys = ["education", "academic"]
        lines = []
        for key in edu_keys:
            if key in sections:
                lines = sections[key]
                break
        if not lines:
            return []

        educations = []
        current = None

        for line in lines:
            if re.match(r"^\d{4}\s*[-–—]\s*\d{4}$", line.strip()):
                continue
            if re.match(r"^\d{4}$", line.strip()):
                continue
            if re.match(r"^gpa[:\s]", line.lower()):
                continue

            has_date = bool(re.search(r"\d{4}\s*[-–—]\s*(?:\d{4}|present)", line.lower()))
            looks_like_school = any(w in line.lower() for w in [
                "university", "institute", "college", "school", "academy",
                "polytechnic", "bachelor", "master", "phd", "bsc", "msc",
            ])

            if has_date or looks_like_school:
                if current and current.institution:
                    educations.append(current)
                if "|" in line:
                    parts = [p.strip() for p in line.split("|")]
                elif "—" in line or " – " in line:
                    parts = [p.strip() for p in re.split(r"[—–]", line)]
                else:
                    parts = [line.strip()]

                clean_parts = []
                for p in parts:
                    if re.match(r"^\d{4}\s*[-–—]", p):
                        continue
                    if p:
                        clean_parts.append(p)

                current = Education(
                    institution=clean_parts[0] if clean_parts else "",
                    degree=clean_parts[1] if len(clean_parts) > 1 else "",
                    field=clean_parts[2] if len(clean_parts) > 2 else None,
                    evidence=[Evidence(source=SourceType.RESUME, description="Parsed from resume")],
                )
            elif current:
                if not current.degree and not re.match(r"^\d{4}", line):
                    current.degree = line
                elif not current.field and not re.match(r"^\d{4}", line):
                    current.field = line

        if current and current.institution:
            educations.append(current)

        return educations

    def _extract_certificates(self, sections: dict[str, list[str]]) -> list[dict]:
        """Extract certificates/courses from certifications and courses sections."""
        cert_keys = ["certifications", "awards", "courses", "certificates",
                      "certifications & courses", "certifications and courses"]
        lines = []
        for key in cert_keys:
            if key in sections:
                lines.extend(sections[key])
        if not lines:
            return []

        certs = []
        current_name = ""
        current_issuer = ""

        for line in lines:
            if re.match(r"^\d{4}$", line.strip()):
                continue
            if re.match(r"^\d{4}\s*[-–—]", line.strip()):
                continue
            if line.lower().rstrip(":") in ("certifications", "courses", "certificates",
                                              "awards", "certifications & courses"):
                continue

            if "|" in line:
                parts = [p.strip() for p in line.split("|")]
                name = parts[0]
                issuer = parts[1] if len(parts) > 1 else ""
                if name and not re.match(r"^\d+$", name):
                    certs.append({"name": name, "issuer": issuer})
                    current_name = ""
                    current_issuer = ""
                continue

            m = re.match(r"^(.+?)\s*[—–-]\s*(.+)$", line)
            if m:
                name = m.group(1).strip()
                issuer = m.group(2).strip()
                if name and not re.match(r"^\d+$", name):
                    certs.append({"name": name, "issuer": issuer})
                    current_name = ""
                    current_issuer = ""
                continue

            if re.match(r"^\d+$", line.strip()):
                continue
            if not current_name:
                current_name = line
            elif not current_issuer:
                current_issuer = line
                certs.append({"name": current_name, "issuer": current_issuer})
                current_name = ""
                current_issuer = ""

        if current_name:
            certs.append({"name": current_name, "issuer": current_issuer})

        return certs

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
            line = re.sub(
                r"^(?:Languages|AI/Data|Technologies|Tools|Frameworks):\s*",
                "", line, flags=re.IGNORECASE,
            )
            items = re.split(r"[•,|/]", line)
            for item in items:
                name = item.strip()
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
            "langchain", "langgraph", "aiogram",
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
