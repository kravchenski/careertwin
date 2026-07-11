"""Skill Gap Analyzer — identifies missing skills and recommends learning paths."""

from __future__ import annotations

from dataclasses import dataclass, field

from careertwin.graph.schema import CareerGraph


@dataclass
class SkillGap:
    skill: str
    importance: str
    current_level: str
    recommended_resources: list[str] = field(default_factory=list)


@dataclass
class SkillGapReport:
    target_role: str
    current_skills: list[str]
    required_skills: list[str]
    gaps: list[SkillGap]
    overall_readiness: float
    recommendations: list[str] = field(default_factory=list)


class SkillGapAnalyzer:
    """Analyzes skill gaps between current profile and target role."""

    SKILL_RESOURCES = {
        "python": ["Python.org tutorial", "Automate the Boring Stuff"],
        "javascript": ["MDN Web Docs", "JavaScript.info"],
        "typescript": ["TypeScript Handbook", "Total TypeScript"],
        "react": ["React.dev docs", "Full Stack Open"],
        "docker": ["Docker docs", "Docker for Beginners"],
        "kubernetes": ["Kubernetes docs", "CKAD preparation"],
        "aws": ["AWS Training", "A Cloud Guru"],
        "gcp": ["Google Cloud Training", "Cloud Skills Boost"],
        "machine learning": ["Coursera ML Specialization", "Fast.ai"],
        "deep learning": ["Deep Learning Specialization", "PyTorch tutorials"],
        "nlp": ["Hugging Face course", "Stanford CS224n"],
        "llm": ["LLM University", "LangChain docs"],
        "langchain": ["LangChain docs", "LangChain YouTube"],
        "fastapi": ["FastAPI docs", "FastAPI tutorial"],
        "django": ["Django docs", "Django for Beginners"],
        "postgresql": ["PostgreSQL docs", "Use The Index, Luke"],
        "graphql": ["GraphQL docs", "How to GraphQL"],
    }

    def analyze(
        self, graph: CareerGraph, job_description: str, target_role: str = ""
    ) -> SkillGapReport:
        current_skills = {s.name.lower() for s in graph.skills}

        for exp in graph.experience:
            for tech in exp.technologies:
                current_skills.add(tech.name.lower())

        required = self._extract_required_skills(job_description)
        preferred = [
            s for s in self._extract_preferred_skills(job_description)
            if s not in required
        ]

        gaps = []
        for skill in required:
            if skill.lower() not in current_skills:
                gaps.append(SkillGap(
                    skill=skill,
                    importance="critical",
                    current_level="none",
                    recommended_resources=self._get_resources(skill),
                ))

        for skill in preferred:
            if skill.lower() not in current_skills:
                gaps.append(SkillGap(
                    skill=skill,
                    importance="important",
                    current_level="none",
                    recommended_resources=self._get_resources(skill),
                ))

        total = len(required) + len(preferred)
        matched = total - len(gaps)
        readiness = (matched / max(total, 1)) * 100

        recommendations = self._generate_recommendations(gaps, readiness)

        return SkillGapReport(
            target_role=target_role or self._extract_role_from_desc(job_description),
            current_skills=sorted(current_skills),
            required_skills=required + preferred,
            gaps=gaps,
            overall_readiness=round(readiness, 1),
            recommendations=recommendations,
        )

    def _extract_required_skills(self, text: str) -> list[str]:
        known = list(self.SKILL_RESOURCES.keys())
        lower = text.lower()
        tokens = {w.strip(".,;:()[]{}<>/\\").lower() for w in text.split()}
        found: list[str] = []

        for skill in known:
            if " " in skill:
                if skill in lower:
                    found.append(skill)
            else:
                if skill in tokens:
                    found.append(skill)

        return found

    def _extract_preferred_skills(self, text: str) -> list[str]:
        known = [
            "graphql", "redis", "mongodb", "elasticsearch", "kafka",
            "terraform", "ansible", "ci/cd", "jenkins", "github actions",
        ]
        found = []
        lower = text.lower()

        for skill in known:
            if skill in lower:
                found.append(skill)

        return found

    def _get_resources(self, skill: str) -> list[str]:
        return self.SKILL_RESOURCES.get(skill.lower(), [f"{skill} official documentation"])

    def _extract_role_from_desc(self, text: str) -> str:
        lines = text.strip().split("\n")
        if lines:
            return lines[0][:80]
        return "Target Role"

    def _generate_recommendations(self, gaps: list[SkillGap], readiness: float) -> list[str]:
        recs = []

        if readiness >= 80:
            recs.append("You are well-qualified for this role")
        elif readiness >= 50:
            critical = [g.skill for g in gaps if g.importance == "critical"]
            if critical:
                recs.append(f"Focus on learning: {', '.join(critical[:3])}")
        else:
            critical = [g.skill for g in gaps if g.importance == "critical"]
            if critical:
                recs.append(f"Priority skills to learn: {', '.join(critical[:3])}")

        if len(gaps) > 5:
            recs.append("Consider a structured learning path rather than individual courses")

        return recs
