"""CareerTwin API — resume tailoring, cover letter, and job discovery services."""

from __future__ import annotations

import html as _html
import logging
import re
import shutil
from pathlib import Path
from typing import Any
from uuid import uuid4

from careertwin.agents.cover_letter import CoverLetterGenerator
from careertwin.agents.job_matcher import JobMatcher
from careertwin.agents.resume_builder import JobDescription, ResumeBuilder
from careertwin.agents.skill_gap import SkillGapAnalyzer
from careertwin.ats.checker import ATSChecker
from careertwin.graph.schema import CareerGraph, Experience, Skill, SkillCategory
from careertwin.jobs.scraper import (
    _language_from_signals,
    _skills_from_text,
    detect_company_profile,
    scrape_job_url,
)
from careertwin.llm import call_ai, parse_json_object
from careertwin.profile.context import read_context
from careertwin.rendering import PdfRenderer, RenderConfig
from careertwin.settings import LANGUAGE_LABELS, OUTPUT_DIR, TEMPLATE_MAP, _agent_result

logger = logging.getLogger("careertwin.api")




def _apply_llm_resume_payload(graph: CareerGraph, payload: dict[str, Any]) -> CareerGraph:
    summary = payload.get("summary")
    if isinstance(summary, str) and summary.strip():
        graph.profile.summary = summary.strip()

    skill_names = payload.get("skills")
    if isinstance(skill_names, list) and skill_names:
        graph.skills = [
            Skill(name=str(name).strip(), category=SkillCategory.TOOL)
            for name in skill_names
            if str(name).strip()
        ]

    rewritten = payload.get("experience")
    if isinstance(rewritten, list):
        new_experience: list[Experience] = []
        for idx, item in enumerate(rewritten):
            if idx >= len(graph.experience) or not isinstance(item, dict):
                break
            original = graph.experience[idx]
            bullets = item.get("bullets") or []
            description = "\n".join(
                f"- {str(bullet).strip()}" for bullet in bullets if str(bullet).strip()
            )
            if not description:
                description = original.description
            new_experience.append(
                Experience(
                    company=original.company,
                    position=original.position,
                    description=description,
                    location=original.location,
                    start_date=original.start_date,
                    end_date=original.end_date,
                    projects=original.projects,
                    technologies=original.technologies,
                    achievements=original.achievements,
                    evidence=original.evidence,
                )
            )
        if new_experience:
            graph.experience = new_experience + graph.experience[len(new_experience):]
    return graph




def _build_cover_letter_html(
    full_name: str,
    email: str,
    phone: str,
    company: str,
    job_title: str,
    language_label: str,
    body: str,
) -> str:
    body_html = "".join(
        f"<p>{_html.escape(paragraph.strip())}</p>"
        for paragraph in body.split("\n\n")
        if paragraph.strip()
    )
    contact_bits = []
    if email:
        contact_bits.append(f'<a href="mailto:{_html.escape(email, quote=True)}">{_html.escape(email)}</a>')
    if phone:
        tel = re.sub(r"[^0-9+]", "", phone)
        contact_bits.append(f'<a href="tel:{tel}">{_html.escape(phone)}</a>')
    contact_line = " · ".join(contact_bits)
    role_line = " · ".join(_html.escape(p) for p in (company, job_title, language_label) if p)
    meta_lines = "".join(f"<p>{line}</p>" for line in (contact_line, role_line) if line)
    return f"""<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <style>
      @page {{ size: A4; margin: 22mm 18mm; }}
      body {{
        font-family: Georgia, 'Times New Roman', serif;
        color: #1f2937;
        line-height: 1.62;
        font-size: 11.5pt;
      }}
      .letter {{
        max-width: 700px;
        margin: 0 auto;
      }}
      .meta {{
        margin-bottom: 22px;
        border-bottom: 1px solid #d1d5db;
        padding-bottom: 12px;
      }}
      .meta h1 {{
        font-size: 22pt;
        margin: 0 0 6px;
        color: #0f172a;
      }}
      .meta p {{
        margin: 0;
        color: #475569;
      }}
      .label {{
        margin-top: 14px;
        font-size: 9pt;
        text-transform: uppercase;
        letter-spacing: 0.14em;
        color: #64748b;
      }}
      .body p {{
        margin: 0 0 14px;
      }}
    </style>
  </head>
  <body>
    <main class="letter">
      <header class="meta">
        <h1>{_html.escape(full_name)}</h1>
        {meta_lines}
      </header>
      <section class="body">{body_html}</section>
    </main>
  </body>
</html>"""


def _render_cover_letter_pdf(
    output_path: Path,
    full_name: str,
    email: str,
    company: str,
    job_title: str,
    language_label: str,
    body: str,
    phone: str = "",
) -> None:
    html = _build_cover_letter_html(full_name, email, phone, company, job_title, language_label, body)
    try:
        from careertwin.rendering.safe import html as safe_html

        safe_html(string=html).write_pdf(str(output_path))
    except Exception:
        output_path.write_text(body, encoding="utf-8")




def _format_profile_for_resume(user_resume: dict[str, Any] | None, full_name: str) -> str:
    """Flatten the candidate's raw profile data into one text blob for the
    markdown resume prompt, preserving whatever structure they already wrote
    instead of forcing it into rigid schema fields."""
    parts = [f"Name: {full_name}"]
    if not user_resume:
        return "\n".join(parts)
    if user_resume.get("summary"):
        parts.append(f"\nBackground summary:\n{user_resume['summary']}")
    if user_resume.get("experience"):
        parts.append("\nExperience (each entry may itself list several distinct clients/projects):")
        for exp in user_resume["experience"]:
            header = f"{exp.get('title', '')} at {exp.get('company', '')} ({exp.get('duration', '')})".strip()
            parts.append(f"\n{header}")
            if exp.get("description"):
                parts.append(exp["description"])
    if user_resume.get("education"):
        parts.append("\nEducation:")
        for edu in user_resume["education"]:
            degree_line = edu.get("degree", "")
            if edu.get("field"):
                degree_line = f"{degree_line} in {edu['field']}".strip()
            parts.append(f"- {degree_line} — {edu.get('school', '')}")
    if user_resume.get("languages"):
        parts.append("\nLanguages: " + ", ".join(user_resume["languages"]))
    return "\n".join(parts)


async def generate_markdown_resume(
    user_resume: dict[str, Any] | None,
    full_name: str,
    skills: list[str],
    job_title: str,
    company: str,
    job_description: str,
    language: str = "English",
) -> str:
    """Ask the model for the candidate's resume, tailored to this job, as
    clean Markdown — no JSON schema to fit content into or drop it from."""
    profile_text = _format_profile_for_resume(user_resume, full_name)
    skills_text = ", ".join(skills)

    system_prompt = f"""You are a senior technical recruiter writing a candidate's resume tailored to a \
specific job, as clean Markdown. Output ONLY the resume body in {language} — no preamble, no explanation, \
no code fences.

Formatting rules:
- "## SECTION NAME" for section headers, in ALL CAPS.
- "- " for bullet points.
- The SKILLS section is the one exception to bullet points: write it as a single comma-separated \
line grouped by category if useful (e.g. "Core: Python, Go · Frontend: React, Angular · \
Backend: FastAPI, REST"), never one skill per bullet — a resume must not waste a full line per skill. \
Group strictly by what the skill actually is (e.g. backend frameworks and frontend frameworks are \
different categories — never group FastAPI with React/Angular just because all three are "frameworks"). \
Never put spoken/human languages (Russian, English, etc.) in SKILLS — those belong only in the separate \
LANGUAGES section. If the candidate's skill list contains a bare vendor/company name for an API or \
platform they used, append the word "API" after it instead of leaving it bare — for example the skill \
"Anthropic" becomes "Anthropic API" in the output. Do not wrap it in any brackets, quotes, or other \
punctuation — output the plain two-word phrase exactly like that.
- PROFILE is also bullet-free: write it as one short plain paragraph (2-4 sentences), never as a bulleted \
list item.
- "**Company/Client — Role**  Dates" as a bold sub-header line directly before that entry's bullets. Use \
exactly this "**...**" bold syntax for every entry header — never a markdown heading ("#", "##", "###") for \
anything other than the ALL-CAPS "## SECTION NAME" section headers themselves.
- Standard section order, only including a section when the candidate's background actually supports it: \
PROFILE, SKILLS, EXPERIENCE (call it FREELANCE EXPERIENCE if the work described is freelance/client work), \
SELECTED PROJECTS, EDUCATION, COURSES & CERTIFICATES, LANGUAGES.

Content rules:
- Never invent employers, dates, projects, or numbers that aren't in the candidate's background below.
- The candidate's background often lists several distinct clients/projects inside one entry — give each of \
them its own bold sub-header + bullets. Do not merge them into one blob or drop any of them.
- Quantify wherever the source material gives you a number.
- Mirror keywords from the job description wherever the candidate genuinely has that skill.
- Lead bullets with what the candidate did and the result, not soft traits.
- Copy proper nouns and product/technology names byte-for-byte from the candidate's background — \
"FastAPI", "PostgreSQL", "bePaid" must never gain an internal space ("Fast API", "Postgre SQL", \
"be Paid") or otherwise be altered.
- Every bullet must be a single complete sentence. Never cut a bullet off mid-clause — if you are unsure \
how a sentence ends, shorten it to a complete thought instead of trailing off.

ATS (applicant tracking system) rules — this resume is parsed by keyword-matching software before any \
human reads it, so:
- Reuse the job description's own wording for a required skill, not a paraphrase — if the posting says \
"CI/CD pipelines" and the candidate has that experience, write "CI/CD pipelines", not "automated deployment \
workflows".
- The first time an acronym the job description uses appears (e.g. "CI/CD", "ML", "SLA"), and the candidate's \
background supports it, write it once with the expansion so both forms are scannable — "CI/CD (Continuous \
Integration/Continuous Deployment)" — but only ever with real content already present in the candidate's \
background, never invented.
- Do not use tables, multi-column layouts, or non-standard bullet symbols (arrows, emoji, box-drawing \
characters) anywhere — plain "- " bullets and headings only, so text extraction stays in correct reading \
order.

Job: {job_title} at {company}
Job description: {job_description[:1500]}"""

    user_msg = f"Candidate background:\n{profile_text}\n\nSkills: {skills_text}\n\nWrite the tailored resume markdown now."

    for attempt in range(2):
        raw = await call_ai(system_prompt + "\nUse ABOUT.md as authoritative candidate facts. Do not put salary, demographics or consent in a resume.", "SHARED ABOUT.md:\n" + read_context() + "\n\n" + user_msg, provider="lmstudio", model="", max_tokens=6000)
        if raw.strip():
            return raw.strip()
    return ""


def _inline_markdown(text: str) -> str:
    import re as _re

    text = _html.escape(text)
    text = _re.sub(r"\*\*(.+?)\*\*", r"<strong>\1</strong>", text)
    return text


_KNOWN_SECTION_NAMES = {
    "PROFILE", "SUMMARY", "SKILLS", "EXPERIENCE", "FREELANCE EXPERIENCE",
    "WORK EXPERIENCE", "SELECTED PROJECTS", "PROJECTS", "EDUCATION",
    "COURSES & CERTIFICATES", "CERTIFICATES", "LANGUAGES",
}


def _bare_section_header(line: str) -> str | None:
    """Some model outputs drop the "## " prefix for the first (PROFILE) or
    last (LANGUAGES) section, rendering it as a plain paragraph instead of a
    styled heading. Recognize a known section name on its own line — plain
    or bolded — as a header even without the markdown prefix."""
    stripped = line.strip().strip("*").strip().rstrip(":")
    if stripped and stripped.upper() == stripped and stripped in _KNOWN_SECTION_NAMES:
        return stripped
    return None


def _markdown_to_resume_html(markdown_text: str) -> str:
    parts: list[str] = []
    in_list = False
    for raw_line in markdown_text.split("\n"):
        line = raw_line.rstrip()
        if not line.strip():
            continue
        bare_header = _bare_section_header(line)
        heading_match = re.match(r"#{1,6}\s+(.*)", line.strip())
        if line.startswith("## ") or bare_header:
            if in_list:
                parts.append("</ul>")
                in_list = False
            title = bare_header or line[3:].strip()
            parts.append(f"<h2>{_html.escape(title)}</h2>")
        elif heading_match:
            if in_list:
                parts.append("</ul>")
                in_list = False
            parts.append(f'<p class="entry-header"><strong>{_inline_markdown(heading_match.group(1).strip())}</strong></p>')
        elif line.strip().startswith(("- ", "* ")):
            if not in_list:
                parts.append("<ul>")
                in_list = True
            parts.append(f"<li>{_inline_markdown(line.strip()[2:].strip())}</li>")
        else:
            if in_list:
                parts.append("</ul>")
                in_list = False
            css_class = ' class="entry-header"' if line.strip().startswith("**") else ""
            parts.append(f"<p{css_class}>{_inline_markdown(line.strip())}</p>")
    if in_list:
        parts.append("</ul>")
    return "\n".join(parts)


def _build_markdown_resume_html(
    full_name: str,
    title: str,
    email: str,
    phone: str,
    location: str,
    github: str,
    linkedin: str,
    body_html: str,
) -> str:
    def _link(text: str, href: str) -> str:
        return f'<a href="{_html.escape(href, quote=True)}" style="color:#52525b;">{_html.escape(text)}</a>'

    contact_parts = []
    if email:
        contact_parts.append(_link(email, f"mailto:{email}"))
    if phone:
        contact_parts.append(_link(phone, f"tel:{re.sub(r'[^0-9+]', '', phone)}"))
    if location:
        contact_parts.append(_html.escape(location))
    if github:
        href = github if github.startswith("http") else f"https://github.com/{github.lstrip('/')}"
        contact_parts.append(_link(github, href))
    if linkedin:
        href = linkedin if linkedin.startswith("http") else f"https://www.linkedin.com/in/{linkedin.lstrip('/')}"
        contact_parts.append(_link(linkedin, href))
    contact_line = "&nbsp;&nbsp;·&nbsp;&nbsp;".join(contact_parts)
    return f"""<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <style>
      @page {{ size: A4; margin: 16mm 14mm; }}
      body {{
        font-family: 'Helvetica Neue', Arial, sans-serif;
        color: #18181b;
        font-size: 9.5pt;
        line-height: 1.45;
      }}
      h1 {{ font-size: 19pt; margin: 0; font-weight: 800; letter-spacing: 0.01em; }}
      .cv-title {{ color: #be185d; font-weight: 700; font-size: 10.5pt; margin: 2px 0 6px; }}
      .cv-contact {{ color: #52525b; font-size: 8.3pt; margin: 0 0 10px; }}
      hr {{ border: none; border-top: 2px solid #18181b; margin: 0 0 12px; }}
      h2 {{
        color: #be185d; font-size: 9.5pt; letter-spacing: 0.05em; text-transform: uppercase;
        border-bottom: 1px solid #e4e4e7; padding-bottom: 3px; margin: 13px 0 6px;
        break-after: avoid-page; page-break-after: avoid;
      }}
      h2:first-of-type {{ margin-top: 0; }}
      p {{ margin: 2px 0 6px; }}
      /* A section/entry header stranded alone at the bottom of a page, with
         its content pushed to the next, reads as broken — keep the header
         glued to whatever comes right after it. */
      .entry-header {{ break-after: avoid-page; page-break-after: avoid; }}
      ul {{ margin: 0 0 8px; padding-left: 15px; break-inside: avoid-page; page-break-inside: avoid; }}
      li {{ margin-bottom: 3px; }}
      strong {{ font-weight: 700; }}
    </style>
  </head>
  <body>
    <h1>{_html.escape(full_name.upper())}</h1>
    <p class="cv-title">{_html.escape(title)}</p>
    <p class="cv-contact">{contact_line}</p>
    <hr />
    {body_html}
  </body>
</html>"""


def _verify_pdf_layout(pdf_path: Path) -> list[str]:
    """Catch the visual defects a compiled PDF can have that the HTML/CSS
    alone never reveals — pages that overflow further than a single tailored
    resume should, or (belt-and-suspenders on top of the CSS avoid-break
    rules) a section/entry header stranded alone at the very bottom of a
    page with its content pushed onto the next one."""
    issues: list[str] = []
    try:
        from pdfminer.high_level import extract_pages
        from pdfminer.layout import LTTextContainer

        pages = list(extract_pages(str(pdf_path)))
        if len(pages) > 2:
            issues.append(f"CV compiled to {len(pages)} pages — a single tailored resume should fit 1-2.")

        for page_num, page in enumerate(pages, start=1):
            if page_num == len(pages):
                continue
            for elem in page:
                if not isinstance(elem, LTTextContainer):
                    continue
                text = elem.get_text().strip()
                if not text or text.upper() != text or len(text) > 40:
                    continue
                if elem.y0 < page.height * 0.08:
                    issues.append(f"Page {page_num}: header '{text}' sits at the very bottom of the page — likely orphaned from its content.")
    except Exception as e:
        issues.append(f"Layout verification failed to run: {e}")
    return issues


def _check_ats_pdf_text(pdf_path: Path, email: str, phone: str, job_description: str) -> dict[str, Any]:
    """Extract the *compiled* PDF's text layer — the same thing an ATS parser
    reads — and check it the way a parser would, not the way the source
    markdown looks. A PDF can render perfectly and still extract as garbage
    (icon glyphs where contact info should be, columns interleaved out of
    reading order), and separately, this is the only place that can measure
    real keyword coverage against what a parser actually sees."""
    result: dict[str, Any] = {"contact_found": False, "keyword_coverage": 1.0, "missing_keywords": []}
    try:
        from pdfminer.high_level import extract_text

        text = extract_text(str(pdf_path))
        if not text.strip():
            result["error"] = "PDF text layer is empty"
            return result

        email_ok = bool(email) and email.lower() in text.lower()
        phone_digits = re.sub(r"[^0-9]", "", phone or "")
        phone_ok = bool(phone_digits) and phone_digits in re.sub(r"[^0-9]", "", text)
        result["contact_found"] = email_ok or phone_ok

        stopwords = {
            "about", "would", "their", "which", "there", "where", "should",
            "these", "those", "while", "being", "other", "using", "within",
        }
        text_lower = text.lower()
        job_words = {
            w for w in re.findall(r"[a-zA-Z][a-zA-Z+/#.-]{4,}", (job_description or "").lower())
            if w not in stopwords
        }
        if job_words:
            present = {w for w in job_words if w in text_lower}
            result["keyword_coverage"] = round(len(present) / len(job_words), 2)
            result["missing_keywords"] = sorted(job_words - present)[:10]
    except Exception as e:
        result["error"] = str(e)
    return result


async def render_markdown_resume_pdf(
    output_path: Path,
    user_resume: dict[str, Any] | None,
    full_name: str,
    title: str,
    email: str,
    phone: str,
    location: str,
    github: str,
    linkedin: str,
    skills: list[str],
    job_title: str,
    company: str,
    job_description: str,
    language: str = "English",
) -> None:
    markdown_text = await generate_markdown_resume(
        user_resume, full_name, skills, job_title, company, job_description, language
    )
    if not markdown_text.strip():
        raise RuntimeError("AI provider returned no resume content (timed out or unavailable)")
    body_html = _markdown_to_resume_html(markdown_text)
    html = _build_markdown_resume_html(full_name, title, email, phone, location, github, linkedin, body_html)
    try:
        from careertwin.rendering.safe import html as safe_html

        safe_html(string=html).write_pdf(str(output_path))
    except Exception:
        output_path.write_text(markdown_text, encoding="utf-8")
        return

    layout_issues = _verify_pdf_layout(output_path)
    if layout_issues:
        logger.warning("CV layout check (%s at %s): %s", job_title, company, "; ".join(layout_issues))

    ats_result = _check_ats_pdf_text(output_path, email, phone, job_description)
    if not ats_result.get("contact_found"):
        logger.warning("CV ATS check (%s at %s): contact info not found in compiled PDF text layer", job_title, company)
    if ats_result.get("keyword_coverage", 1.0) < 0.3:
        logger.warning(
            "CV ATS check (%s at %s): low keyword coverage %.0f%%, missing: %s",
            job_title, company, ats_result["keyword_coverage"] * 100, ats_result.get("missing_keywords"),
        )




def _template_name(name: str) -> str:
    return TEMPLATE_MAP.get(name, "modern")


def _language_from_signals_wrapper(text: str, domain_hint: str = "") -> tuple[str, str]:
    return _language_from_signals(text, domain_hint)




def _job_search_query_from_graph(graph: CareerGraph) -> str:
    skill_terms = [skill.name for skill in graph.skills[:8]]
    headline = graph.profile.headline or ""
    summary = graph.profile.summary or ""

    role_hint = ""
    if summary:
        lower = summary.lower()
        for role in [
            "backend", "frontend", "fullstack", "full-stack", "full stack",
            "devops", "data engineer", "machine learning", "ml engineer",
            "mobile", "ios", "android", "qa", "security", "cloud",
            "python", "react", "typescript",
        ]:
            if role in lower:
                role_hint = role
                break

    if headline:
        return f"{headline} {' '.join(skill_terms[:3])}".strip()
    if role_hint:
        return f"{role_hint} {' '.join(skill_terms[:4])}".strip()
    return " ".join(skill_terms[:5]) or "software engineer python"




async def tailor_resume_package(
    graph: CareerGraph,
    job_title: str,
    company_name: str,
    job_description: str,
    job_url: str,
    template: str,
    provider: str,
    model: str,
    requested_language: str,
) -> dict[str, Any]:
    diagnostics: dict[str, Any] = {}
    scraped = await scrape_job_url(job_url) if job_url else {
        "title": job_title,
        "description": job_description,
        "skills": _skills_from_text(job_description),
        "company": company_name,
        "url": job_url,
    }
    diagnostics["job_scraper"] = _agent_result(
        "success" if (scraped.get("title") or scraped.get("description")) else "warning",
        "Job description scraped" if job_url else "Using provided job description only",
        title=scraped.get("title", ""),
        skills=len(scraped.get("skills", [])),
    )
    title = job_title or scraped.get("title") or "Target Role"
    company = company_name or scraped.get("company") or "Target Company"
    description = job_description or scraped.get("description") or ""
    required_skills = scraped.get("skills") or _skills_from_text(description)

    company_profile = await detect_company_profile(
        company_name=company,
        job_url=job_url,
        job_description=description,
        provider=provider,
        model=model,
    )
    diagnostics["company_research"] = _agent_result(
        "success" if company_profile.get("company_summary") or company_profile.get("reasoning") else "warning",
        company_profile.get("reasoning", "No company reasoning available"),
        language=company_profile.get("language", "en"),
    )
    language = requested_language if requested_language not in ("", "auto") else company_profile["language"]
    language_name = LANGUAGE_LABELS.get(language, "English")

    builder = ResumeBuilder()
    adapted = builder.build(
        graph,
        JobDescription(
            title=title,
            company=company,
            description=description,
            required_skills=required_skills,
        ),
    )
    diagnostics["resume_builder"] = _agent_result(
        "success",
        "Resume builder selected relevant sections",
        experience=len(adapted.experience),
        skills=len(adapted.skills),
        projects=len(adapted.projects),
    )

    llm_resume_prompt = (
        f"Rewrite the resume for the job below in {language_name}. "
        "Do not invent employers, dates, or achievements. "
        "Return JSON only with keys summary, skills, experience.\n\n"
        f"SHARED CANDIDATE FACTS (ABOUT.md):\n{read_context()}\n\n"
        f"BASE RESUME:\n{adapted.to_search_text()}\n\n"
        f"JOB TITLE: {title}\n"
        f"COMPANY: {company}\n"
        f"JOB DESCRIPTION:\n{description[:3500]}\n\n"
        "JSON schema:\n"
        "{\n"
        '  "summary": "2-4 lines",\n'
        '  "skills": ["skill1", "skill2"],\n'
        '  "experience": [\n'
        '    {"company": "...", "position": "...", "bullets": ["...", "..."]}\n'
        "  ]\n"
        "}"
    )
    llm_resume_payload = parse_json_object(
        await call_ai(
            system_prompt=(
                "You are a senior recruiter and resume writer. "
                "Improve relevance and ATS fit but never fabricate experience."
            ),
            user_prompt=llm_resume_prompt,
            provider=provider,
            model=model,
        )
    )
    if llm_resume_payload:
        adapted = _apply_llm_resume_payload(adapted, llm_resume_payload)
        diagnostics["resume_rewriter"] = _agent_result(
            "success",
            "LLM resume rewrite applied",
            keys=",".join(sorted(llm_resume_payload.keys())),
        )
    else:
        diagnostics["resume_rewriter"] = _agent_result(
            "warning",
            "LLM resume rewrite unavailable, using deterministic builder output",
        )

    cover_prompt = (
        f"Write a professional cover letter in {language_name}. "
        "Use the company context, be specific, and keep it human. "
        "Do not invent facts.\n\n"
        f"SHARED CANDIDATE FACTS (ABOUT.md):\n{read_context()}\n\n"
        f"CANDIDATE:\n{adapted.to_search_text()}\n\n"
        f"COMPANY RESEARCH:\n{company_profile.get('company_summary', '')}\n\n"
        f"JOB TITLE: {title}\n"
        f"COMPANY: {company}\n"
        f"JOB DESCRIPTION:\n{description[:3000]}"
    )
    cover_letter = await call_ai(
        system_prompt="You are an expert cover letter writer. Return plain text only.",
        user_prompt=cover_prompt,
        provider=provider,
        model=model,
    )
    if not cover_letter.strip():
        cover_letter = CoverLetterGenerator().generate(
            graph=adapted,
            job_title=title,
            company=company,
            job_description=description,
        ).render()
        diagnostics["cover_letter"] = _agent_result(
            "warning",
            "LLM cover letter unavailable, using deterministic fallback generator",
        )
    else:
        diagnostics["cover_letter"] = _agent_result(
            "success",
            "LLM cover letter generated",
            length=len(cover_letter),
        )

    ats_checker = ATSChecker()
    ats_report = ats_checker.check(adapted, adapted.to_search_text())
    diagnostics["ats_checker"] = _agent_result(
        "success",
        f"ATS score {ats_report.score}",
    )
    matcher = JobMatcher()
    job_match = matcher.match(adapted, description)
    diagnostics["job_matcher"] = _agent_result(
        "success",
        f"Job match score {job_match.match_score}",
    )
    skill_gap = SkillGapAnalyzer().analyze(adapted, description, target_role=title)
    diagnostics["skill_gap"] = _agent_result(
        "success",
        f"Readiness score {skill_gap.overall_readiness}",
    )

    package_id = uuid4().hex[:10]
    pdf_path = OUTPUT_DIR / f"{package_id}.pdf"
    html_path = OUTPUT_DIR / f"{package_id}.html"
    cover_letter_pdf_path = OUTPUT_DIR / f"{package_id}-cover-letter.pdf"
    renderer = PdfRenderer()
    render_result = renderer.render(
        adapted,
        RenderConfig(
            template=_template_name(template),
            output_path=str(pdf_path),
        ),
    )
    try:
        shutil.copyfile(render_result.html_path, html_path)
    except Exception:
        html_path.write_text("<html><body><p>Preview unavailable</p></body></html>", encoding="utf-8")

    _render_cover_letter_pdf(
        output_path=cover_letter_pdf_path,
        full_name=adapted.profile.full_name,
        email=adapted.profile.email or "",
        phone=adapted.profile.phone or "",
        company=company,
        job_title=title,
        language_label=language_name,
        body=cover_letter,
    )

    return {
        "success": True,
        "package_id": package_id,
        "pdf_url": f"/outputs/{pdf_path.name}",
        "html_url": f"/outputs/{html_path.name}",
        "cover_letter_pdf_url": f"/outputs/{cover_letter_pdf_path.name}",
        "ats_score": ats_report.score,
        "job_match_score": job_match.match_score,
        "readiness_score": skill_gap.overall_readiness,
        "language": language,
        "language_label": LANGUAGE_LABELS.get(language, "English"),
        "company_research": company_profile,
        "job": {
            "title": title,
            "company": company,
            "description": description,
            "skills": required_skills,
            "url": job_url,
        },
        "resume": {
            "full_name": adapted.profile.full_name,
            "summary": adapted.profile.summary,
            "text": adapted.to_search_text(),
            "skills": [skill.name for skill in adapted.skills],
            "projects": [
                {
                    "name": project.name,
                    "description": project.description,
                    "url": project.url,
                }
                for project in adapted.projects
            ],
            "experience": [
                {
                    "company": exp.company,
                    "position": exp.position,
                    "description": exp.description,
                }
                for exp in adapted.experience
            ],
        },
        "cover_letter": cover_letter,
        "agent_diagnostics": diagnostics,
        "reports": {
            "ats_passed": ats_report.is_passing,
            "ats_issues": [issue.message for issue in ats_report.issues],
            "missing_skills": skill_gap.required_skills[:8],
            "skill_gaps": [gap.skill for gap in skill_gap.gaps],
            "match_recommendation": job_match.recommendation,
        },
    }




async def discover_jobs_from_graph(
    graph: CareerGraph,
    limit: int,
    sources: list[str],
) -> dict[str, Any]:
    from careertwin.jobs.scraper import _web_search_jobs

    matcher = JobMatcher()
    query = _job_search_query_from_graph(graph)
    discovered = await _web_search_jobs(query=query, limit=max(limit * 3, limit), sources=sources)

    jobs: list[dict[str, Any]] = []
    search_diagnostics = _agent_result(
        "warning",
        "No vacancies discovered from the selected sources",
        query=query,
        sources=sources,
    )

    for raw_job in discovered:
        try:
            details = await scrape_job_url(raw_job.get("url", ""))
            description = details.get("description", "") or raw_job.get("title", "")
            title = details.get("title") or raw_job.get("title", "")
            company = raw_job.get("company") or details.get("company", "")
            match = matcher.match(graph, description)
            jobs.append(
                {
                    "source": raw_job.get("source", "web"),
                    "url": raw_job.get("url", ""),
                    "title": title,
                    "company": company,
                    "description": description[:1200],
                    "match_score": match.match_score,
                    "matching_skills": match.matching_skills[:8],
                    "missing_skills": match.missing_skills[:8],
                }
            )
        except Exception:
            continue

    jobs.sort(key=lambda item: item.get("match_score", 0), reverse=True)
    jobs = jobs[:limit]

    if jobs:
        search_diagnostics = _agent_result(
            "success",
            f"Discovered and ranked {len(jobs)} matching vacancies",
            query=query,
            sources=sources,
        )

    return {
        "success": True,
        "query": query,
        "total": len(jobs),
        "jobs": jobs,
        "agent_diagnostics": {
            "job_discovery": search_diagnostics,
        },
    }


async def discover_and_tailor_jobs(
    graph: CareerGraph,
    provider: str,
    model: str,
    template: str,
    requested_language: str,
    limit: int,
    sources: list[str],
) -> dict[str, Any]:
    query = _job_search_query_from_graph(graph)
    discovered_payload = await discover_jobs_from_graph(
        graph=graph,
        limit=max(limit * 2, limit),
        sources=sources,
    )
    discovered = discovered_payload.get("jobs", [])

    packages = []
    for raw_job in discovered[:limit]:
        try:
            package = await tailor_resume_package(
                graph=graph,
                job_title=raw_job.get("title", ""),
                company_name=raw_job.get("company") or raw_job.get("source", "").title(),
                job_description=raw_job.get("description", ""),
                job_url=raw_job["url"],
                template=template,
                provider=provider,
                model=model,
                requested_language=requested_language,
            )
            package["source"] = raw_job.get("source", "web")
            package["initial_match_score"] = raw_job.get("match_score", 0)
            packages.append(package)
        except Exception:
            continue

    return {
        "success": True,
        "query": query,
        "jobs_found": len(discovered),
        "jobs_processed": len(packages),
        "sources": sources,
        "packages": packages,
        "agent_diagnostics": discovered_payload.get("agent_diagnostics", {}),
    }
