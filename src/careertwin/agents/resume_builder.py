"""Resume Builder agent — creates tailored resumes for specific job descriptions."""

from __future__ import annotations

from dataclasses import dataclass, field

from careertwin.graph.schema import (
    CareerGraph,
    Experience,
    Project,
    Skill,
)


@dataclass
class JobDescription:
    title: str
    company: str
    description: str
    required_skills: list[str] = field(default_factory=list)
    preferred_skills: list[str] = field(default_factory=list)
    responsibilities: list[str] = field(default_factory=list)


@dataclass
class ResumeSection:
    title: str
    content: str
    priority: int = 0


class ResumeBuilder:
    """Builds tailored resumes based on career graph and job description."""

    def build(self, graph: CareerGraph, job: JobDescription) -> CareerGraph:
        job = self._normalize_job(job, graph)
        adapted = CareerGraph(profile=graph.profile)
        adapted.profile.summary = self._adapt_summary(graph, job)

        adapted.experience = self._adapt_experience(graph, job)
        adapted.projects = self._adapt_projects(graph, job)
        adapted.skills = self._adapt_skills(graph, job)
        adapted.technologies = graph.technologies
        adapted.education = graph.education

        return adapted

    def _normalize_job(self, job: JobDescription, graph: CareerGraph) -> JobDescription:
        required = list(job.required_skills)
        preferred = list(job.preferred_skills)

        if not required:
            required = self._extract_skills(job.description, graph)

        return JobDescription(
            title=job.title,
            company=job.company,
            description=job.description,
            required_skills=required,
            preferred_skills=preferred,
            responsibilities=list(job.responsibilities),
        )

    def _extract_skills(self, description: str, graph: CareerGraph) -> list[str]:
        lower = description.lower()
        known = {
            "python",
            "javascript",
            "typescript",
            "java",
            "go",
            "rust",
            "ruby",
            "php",
            "react",
            "vue",
            "angular",
            "django",
            "flask",
            "fastapi",
            "spring",
            "docker",
            "kubernetes",
            "aws",
            "gcp",
            "azure",
            "postgresql",
            "mysql",
            "redis",
            "mongodb",
            "graphql",
            "sql",
            "git",
            "ci/cd",
            "langchain",
            "langgraph",
            "openai",
            "anthropic",
            "llm",
            "rag",
        }
        known.update(skill.name.lower() for skill in graph.skills)
        known.update(tech.name.lower() for tech in graph.all_technologies())

        found = []
        for skill in sorted(known):
            if skill and skill in lower:
                found.append(skill)
        return found[:12]

    def _adapt_summary(self, graph: CareerGraph, job: JobDescription) -> str | None:
        base_summary = (graph.profile.summary or "").strip()
        headline = (graph.profile.headline or "").strip()
        matching_skills = [
            skill.name
            for skill in graph.skills
            if skill.name.lower() in {s.lower() for s in job.required_skills[:6]}
        ]

        if not (base_summary or headline or matching_skills):
            return graph.profile.summary

        parts: list[str] = []
        if headline:
            parts.append(headline)
        elif base_summary:
            parts.append(base_summary)

        if matching_skills:
            skills_text = ", ".join(matching_skills[:5])
            parts.append(f"Relevant strengths for this role include {skills_text}.")

        summary = " ".join(parts).strip()
        return summary or graph.profile.summary

    def _adapt_experience(self, graph: CareerGraph, job: JobDescription) -> list[Experience]:
        scored = []
        for exp in graph.experience:
            score = self._score_experience(exp, job)
            scored.append((score, exp))

        scored.sort(key=lambda x: -x[0])

        result = []
        for score, exp in scored[:5]:
            adapted_exp = Experience(
                company=exp.company,
                position=exp.position,
                description=self._tailor_description(exp.description, job),
                location=exp.location,
                start_date=exp.start_date,
                end_date=exp.end_date,
                technologies=self._pick_relevant_tech(exp.technologies, job),
                achievements=self._pick_relevant_achievements(exp.achievements, job),
            )
            result.append(adapted_exp)

        return result

    def _score_experience(self, exp: Experience, job: JobDescription) -> float:
        score = 0.0
        text = f"{exp.position} {exp.description}".lower()

        for skill in job.required_skills:
            if skill.lower() in text:
                score += 10

        for skill in job.preferred_skills:
            if skill.lower() in text:
                score += 5

        for resp in job.responsibilities:
            keywords = resp.lower().split()
            for kw in keywords:
                if len(kw) > 3 and kw in text:
                    score += 1

        return score

    def _tailor_description(self, description: str, job: JobDescription) -> str:
        if not description:
            return description

        lines = [l for l in description.strip().split("\n") if l.strip()]
        if len(lines) <= 6:
            return "\n".join(lines)

        def score(line: str) -> int:
            lower = line.lower()
            s = 0
            for skill in job.required_skills:
                if skill.lower() in lower:
                    s += 10
            for skill in job.preferred_skills:
                if skill.lower() in lower:
                    s += 5
            return s

        def is_bullet(line: str) -> bool:
            return line.strip().startswith(("•", "-", "*"))

        headers = [(i, l) for i, l in enumerate(lines) if not is_bullet(l)]
        bullets = [(i, l) for i, l in enumerate(lines) if is_bullet(l)]
        budget = max(1, 6 - len(headers))
        keep_bullets = sorted(bullets, key=lambda x: -score(x[1]))[:budget]
        keep_indices = {i for i, _ in headers} | {i for i, _ in keep_bullets}
        return "\n".join(l for i, l in enumerate(lines) if i in keep_indices)

    def _pick_relevant_tech(self, technologies: list, job: JobDescription) -> list:
        relevant = []
        job_skills = {s.lower() for s in job.required_skills + job.preferred_skills}

        for tech in technologies:
            if tech.name.lower() in job_skills:
                relevant.append(tech)

        return relevant or technologies[:5]

    def _pick_relevant_achievements(self, achievements: list, job: JobDescription) -> list:
        relevant = []
        for ach in achievements:
            text = f"{ach.title} {ach.description} {ach.metric or ''}".lower()
            for skill in job.required_skills + job.preferred_skills:
                if skill.lower() in text:
                    relevant.append(ach)
                    break

        return relevant or achievements[:3]

    def _adapt_projects(self, graph: CareerGraph, job: JobDescription) -> list[Project]:
        scored = []
        for proj in graph.projects:
            score = self._score_project(proj, job)
            scored.append((score, proj))

        scored.sort(key=lambda x: -x[0])
        return [proj for _, proj in scored[:3]]

    def _score_project(self, proj: Project, job: JobDescription) -> float:
        score = 0.0
        text = f"{proj.name} {proj.description}".lower()

        for skill in job.required_skills:
            if skill.lower() in text:
                score += 10

        for tech in proj.technologies:
            if tech.name.lower() in {s.lower() for s in job.required_skills}:
                score += 5

        return score

    def _adapt_skills(self, graph: CareerGraph, job: JobDescription) -> list[Skill]:
        job_skills = {s.lower() for s in job.required_skills + job.preferred_skills}
        relevant = []
        other = []

        for skill in graph.skills:
            if skill.name.lower() in job_skills:
                relevant.append(skill)
            else:
                other.append(skill)

        relevant.sort(key=lambda s: s.name)
        other.sort(key=lambda s: s.name)

        return relevant + other[:10]
