"""Job Matching agent — matches career profile to job descriptions."""

from __future__ import annotations

from dataclasses import dataclass, field

from careertwin.graph.schema import CareerGraph


@dataclass
class JobMatch:
    title: str
    company: str
    match_score: float
    matching_skills: list[str] = field(default_factory=list)
    missing_skills: list[str] = field(default_factory=list)
    matching_experience: list[str] = field(default_factory=list)
    recommendation: str = ""


class JobMatcher:
    """Matches career graph against job descriptions."""

    SKILL_TAXONOMY: list[str] = [
        "python", "javascript", "typescript", "java", "go", "golang", "rust", "ruby", "php", "c++", "c#", "scala", "kotlin", "swift",
        "react", "vue", "vue.js", "angular", "svelte", "next.js", "nextjs", "nuxt", "tailwind", "css", "html",
        "django", "flask", "fastapi", "spring", "spring boot", "express", "node.js", "nodejs", "rails", "laravel", "graphql",
        "machine learning", "deep learning", "nlp", "llm", "rag", "pytorch", "tensorflow", "scikit-learn", "pandas", "numpy",
        "langchain", "langgraph", "openai", "anthropic", "hugging face", "transformers",
        "data engineering", "spark", "airflow", "dbt", "snowflake", "bigquery", "redshift",
        "data science", "statistics", "etl", "data pipeline",
        "docker", "kubernetes", "k8s", "aws", "gcp", "azure", "terraform", "ansible", "jenkins", "github actions",
        "ci/cd", "linux", "nginx", "microservices",
        "postgresql", "postgres", "mysql", "redis", "mongodb", "elasticsearch", "sql", "nosql", "sqlite",
        "git", "jira", "confluence", "figma", "slack",
        "product management", "agile", "scrum", "stakeholder", "roadmap", "strategy", "analytics",
        "a/b testing", "metrics", "kpi", "saas", "b2b", "b2c",
        "fintech", "healthcare", "e-commerce", "ecommerce", "cybersecurity", "cloud",
        "api", "rest api", "webhook", "microservice",
    ]

    ROLE_CLUSTERS: dict[str, list[str]] = {
        "engineering": ["engineer", "developer", "swe", "sde", "backend", "frontend", "fullstack", "full-stack", "infrastructure", "platform", "devops", "sre"],
        "data": ["data scientist", "data engineer", "data analyst", "ml engineer", "machine learning", "ai engineer", "mlops"],
        "product": ["product manager", "product owner", "pm ", "program manager"],
        "design": ["designer", "ux", "ui", "figma"],
        "marketing": ["marketing", "growth", "seo", "content", "social media"],
        "sales": ["sales", "account executive", "business development", "bdr", "sdr"],
        "operations": ["operations", "hr", "recruiter", "recruiting", "finance", "accounting"],
    }

    def match(self, graph: CareerGraph, job_description: str) -> JobMatch:
        job_skills = self._extract_skills_from_text(job_description)
        job_keywords = self._extract_keywords(job_description)

        profile_skills = {s.name.lower(): s for s in graph.skills}
        profile_text = graph.to_search_text().lower()
        profile_techs = {t.name.lower() for t in graph.all_technologies()} if hasattr(graph, "all_technologies") else set()

        profile_words = set(profile_text.split())
        profile_words.update(profile_skills.keys())
        profile_words.update(profile_techs)

        matching_skills = []
        missing_skills = []

        for skill in job_skills:
            if skill.lower() in profile_skills or skill.lower() in profile_techs:
                matching_skills.append(skill)
            else:
                missing_skills.append(skill)

        matching_experience = self._find_matching_experience(graph, job_keywords)

        desc_lower = job_description.lower()

        skill_score = len(matching_skills) / max(len(job_skills), 1) * 40

        exp_score = min(len(matching_experience) * 8, 25)

        keyword_score = self._keyword_match_score(profile_text, job_keywords) * 20

        role_score = self._role_cluster_score(desc_lower, profile_text) * 15

        match_score = min(skill_score + exp_score + keyword_score + role_score, 100)

        if match_score == 0 and keyword_score > 0:
            match_score = max(keyword_score * 10, 5)

        recommendation = self._generate_recommendation(
            match_score, matching_skills, missing_skills
        )

        return JobMatch(
            title=self._extract_job_title(job_description),
            company=self._extract_company(job_description),
            match_score=round(match_score, 1),
            matching_skills=matching_skills,
            missing_skills=missing_skills,
            matching_experience=matching_experience,
            recommendation=recommendation,
        )

    def _extract_skills_from_text(self, text: str) -> list[str]:
        lower = text.lower()
        tokens = {w.strip(".,;:()[]{}<>/\\\"'").lower() for w in text.split()}

        found: list[str] = []
        for skill in self.SKILL_TAXONOMY:
            if " " in skill or "." in skill:
                if skill in lower:
                    found.append(skill)
            else:
                if skill in tokens:
                    found.append(skill)

        return found

    def _extract_keywords(self, text: str) -> list[str]:
        stop_words = {
            "the", "a", "an", "and", "or", "but", "in", "on", "at", "to", "for",
            "of", "with", "by", "from", "is", "are", "was", "were", "be", "been",
            "have", "has", "had", "do", "does", "did", "will", "would", "could",
            "should", "may", "might", "must", "shall", "can", "need", "dare",
            "ought", "used", "this", "that", "these", "those", "i", "you", "he",
            "she", "it", "we", "they", "me", "him", "her", "us", "them", "my",
            "your", "his", "its", "our", "their", "what", "which", "who", "whom",
            "about", "into", "through", "during", "before", "after", "above",
            "below", "between", "same", "such", "than", "too", "very", "just",
            "also", "over", "only", "own", "some", "then", "here", "when", "where",
            "how", "all", "each", "both", "few", "more", "most", "other", "some",
            "not", "no", "nor", "now", "well", "back", "even", "still", "new",
        }

        words = text.lower().split()
        keywords = []
        for word in words:
            clean = "".join(c for c in word if c.isalnum())
            if len(clean) > 3 and clean not in stop_words:
                keywords.append(clean)

        return list(set(keywords))[:80]

    def _find_matching_experience(self, graph: CareerGraph, keywords: list[str]) -> list[str]:
        matches = []
        for exp in graph.experience:
            text = f"{exp.position} {exp.description}".lower()
            score = sum(1 for kw in keywords if kw in text)
            if score >= 2:
                matches.append(f"{exp.position} at {exp.company}")

        return matches[:3]

    def _keyword_match_score(self, profile_text: str, keywords: list[str]) -> float:
        if not keywords:
            return 0
        matches = sum(1 for kw in keywords if kw in profile_text)
        return matches / len(keywords)

    def _role_cluster_score(self, job_text: str, profile_text: str) -> float:
        """Check if the job role matches the user's career direction."""
        best = 0.0
        for cluster, keywords in self.ROLE_CLUSTERS.items():
            job_hits = sum(1 for kw in keywords if kw in job_text)
            profile_hits = sum(1 for kw in keywords if kw in profile_text)
            if job_hits > 0 and profile_hits > 0:
                score = min(job_hits, profile_hits) / max(len(keywords), 1)
                best = max(best, score)
        return best

    def _extract_job_title(self, text: str) -> str:
        lines = text.strip().split("\n")
        for line in lines[:3]:
            line = line.strip()
            if 5 < len(line) < 100 and not line.endswith(":"):
                return line
        return "Position"

    def _extract_company(self, text: str) -> str:
        lower = text.lower()
        for pattern in ["at ", "company: ", "organization: "]:
            idx = lower.find(pattern)
            if idx >= 0:
                rest = text[idx + len(pattern):]
                end = rest.find("\n")
                if end > 0:
                    return rest[:end].strip()
                return rest.strip()[:50]
        return "Company"

    def _generate_recommendation(
        self, score: float, matching: list[str], missing: list[str]
    ) -> str:
        if score >= 80:
            return "Strong match — apply directly"
        elif score >= 60:
            return "Good match — highlight relevant experience"
        elif score >= 40:
            return "Partial match — consider upskilling in: " + ", ".join(missing[:3])
        else:
            return "Weak match — significant skill gaps to address"
