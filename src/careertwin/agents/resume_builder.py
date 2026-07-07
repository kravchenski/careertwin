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
        adapted = CareerGraph(profile=graph.profile)

        adapted.experience = self._adapt_experience(graph, job)
        adapted.projects = self._adapt_projects(graph, job)
        adapted.skills = self._adapt_skills(graph, job)
        adapted.technologies = graph.technologies
        adapted.education = graph.education

        return adapted

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

        lines = description.strip().split("\n")
        scored_lines = []

        for line in lines:
            line_score = 0
            lower = line.lower()
            for skill in job.required_skills:
                if skill.lower() in lower:
                    line_score += 10
            for skill in job.preferred_skills:
                if skill.lower() in lower:
                    line_score += 5
            scored_lines.append((line_score, line))

        scored_lines.sort(key=lambda x: -x[0])
        return "\n".join(line for _, line in scored_lines[:5])

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
