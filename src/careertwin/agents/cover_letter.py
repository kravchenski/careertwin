"""Cover Letter Generator agent — creates tailored cover letters."""

from __future__ import annotations

from dataclasses import dataclass

from careertwin.graph.schema import CareerGraph


@dataclass
class CoverLetter:
    greeting: str
    opening: str
    body: str
    closing: str
    signature: str

    def render(self) -> str:
        parts = [self.greeting, self.opening, self.body, self.closing, self.signature]
        return "\n\n".join(parts)


class CoverLetterGenerator:
    """Generates tailored cover letters based on career graph and job description."""

    def generate(
        self,
        graph: CareerGraph,
        job_title: str,
        company: str,
        job_description: str = "",
    ) -> CoverLetter:
        name = graph.profile.full_name or "Applicant"
        email = graph.profile.email or ""

        greeting = "Dear Hiring Manager,"

        opening = self._generate_opening(name, job_title, company)

        body = self._generate_body(graph, job_title, company, job_description)

        closing = self._generate_closing(job_title, company)

        signature = f"Sincerely,\n{name}"
        if email:
            signature += f"\n{email}"

        return CoverLetter(
            greeting=greeting,
            opening=opening,
            body=body,
            closing=closing,
            signature=signature,
        )

    def _generate_opening(self, name: str, job_title: str, company: str) -> str:
        return (
            f"I am writing to express my interest in the {job_title} position "
            f"at {company}. With my background in software development and "
            f"passion for building impactful applications, I believe I would "
            f"be a valuable addition to your team."
        )

    def _generate_body(
        self,
        graph: CareerGraph,
        job_title: str,
        company: str,
        job_description: str,
    ) -> str:
        paragraphs = []

        experience_summary = self._summarize_experience(graph)
        if experience_summary:
            paragraphs.append(experience_summary)

        skills_summary = self._summarize_skills(graph, job_description)
        if skills_summary:
            paragraphs.append(skills_summary)

        achievements = self._highlight_achievements(graph)
        if achievements:
            paragraphs.append(achievements)

        return "\n\n".join(paragraphs) if paragraphs else (
            "Throughout my career, I have developed strong technical skills "
            "and a commitment to delivering high-quality solutions."
        )

    def _summarize_experience(self, graph: CareerGraph) -> str:
        if not graph.experience:
            return ""

        exp = graph.experience[0]
        years = len(graph.experience)

        return (
            f"In my most recent role as {exp.position} at {exp.company}, "
            f"I have honed my skills in building scalable applications. "
            f"With {years} positions of professional experience, "
            f"I bring a proven track record of delivering results."
        )

    def _summarize_skills(self, graph: CareerGraph, job_description: str) -> str:
        if not graph.skills:
            return ""

        top_skills = [s.name for s in graph.skills[:5]]
        if len(top_skills) > 1:
            skills_text = ", ".join(top_skills[:-1]) + f" and {top_skills[-1]}"
        else:
            skills_text = top_skills[0] if top_skills else ""

        return (
            f"My technical expertise includes {skills_text}, "
            f"which aligns well with the requirements of this position."
        )

    def _highlight_achievements(self, graph: CareerGraph) -> str:
        achievements = []
        for exp in graph.experience:
            for ach in exp.achievements:
                if ach.metric:
                    achievements.append(f"{ach.title} ({ach.metric})")

        if not achievements:
            return ""

        ach_text = achievements[0]
        return (
            f"One of my key achievements includes {ach_text}, "
            f"demonstrating my ability to deliver measurable impact."
        )

    def _generate_closing(self, job_title: str, company: str) -> str:
        return (
            f"I am excited about the opportunity to contribute to {company} "
            f"as a {job_title}. I would welcome the chance to discuss how "
            f"my skills and experience align with your team's needs."
        )
