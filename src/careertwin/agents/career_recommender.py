"""Career Trajectory Recommender — suggests career growth paths."""

from __future__ import annotations

from dataclasses import dataclass, field

from careertwin.graph.schema import CareerGraph


@dataclass
class CareerStep:
    role: str
    timeline: str
    skills_needed: list[str] = field(default_factory=list)
    description: str = ""


@dataclass
class CareerTrajectory:
    current_level: str
    suggested_paths: list[list[CareerStep]]
    summary: str = ""


class CareerRecommender:
    """Recommends career growth trajectories based on experience."""

    LEVELS = [
        "junior",
        "mid-level",
        "senior",
        "lead",
        "principal",
        "staff",
        "director",
        "vp",
        "cto",
    ]

    TRAJECTORIES = {
        "ic": [
            ("Senior Engineer", "1-2 years", ["system design", "mentoring"]),
            ("Staff Engineer", "2-3 years", ["architecture", "technical strategy"]),
            ("Principal Engineer", "3-5 years", ["industry expertise", "innovation"]),
        ],
        "management": [
            ("Tech Lead", "1-2 years", ["team leadership", "project management"]),
            ("Engineering Manager", "2-3 years", ["people management", "hiring"]),
            ("Director of Engineering", "3-5 years", ["strategy", "budgeting"]),
        ],
        "specialist": [
            ("Domain Expert", "1-2 years", ["deep knowledge", "research"]),
            ("Solutions Architect", "2-3 years", ["architecture", "client relations"]),
            ("Consultant", "3-5 years", ["business acumen", "communication"]),
        ],
    }

    def recommend(self, graph: CareerGraph) -> CareerTrajectory:
        current_level = self._assess_level(graph)
        years = self._estimate_years(graph)

        paths = []
        for path_type, steps in self.TRAJECTORIES.items():
            path = self._build_path(steps, current_level, years)
            if path:
                paths.append(path)

        summary = self._generate_summary(current_level, years, paths)

        return CareerTrajectory(
            current_level=current_level,
            suggested_paths=paths,
            summary=summary,
        )

    def _assess_level(self, graph: CareerGraph) -> str:
        years = self._estimate_years(graph)
        positions = [e.position.lower() for e in graph.experience]

        if any("lead" in p or "senior" in p or "principal" in p for p in positions):
            return "senior"
        elif any("junior" in p or "intern" in p or "trainee" in p for p in positions):
            return "junior"
        elif years >= 5:
            return "senior"
        elif years >= 2:
            return "mid-level"
        else:
            return "junior"

    def _estimate_years(self, graph: CareerGraph) -> int:
        if not graph.experience:
            return 0

        years = set()
        for exp in graph.experience:
            if exp.start_date:
                years.add(exp.start_date.year)
            if exp.end_date:
                years.add(exp.end_date.year)

        return max(years) - min(years) + 1 if len(years) > 1 else 1

    def _build_path(
        self, steps: list[tuple], current_level: str, current_years: int
    ) -> list[CareerStep]:
        level_idx = self.LEVELS.index(current_level) if current_level in self.LEVELS else 1
        path = []

        for role, timeline, skills in steps:
            role_lower = role.lower()
            for i, level in enumerate(self.LEVELS):
                if level in role_lower and i > level_idx:
                    step = CareerStep(
                        role=role,
                        timeline=timeline,
                        skills_needed=skills,
                        description=f"Typically requires {timeline} from current level",
                    )
                    path.append(step)
                    break

        return path[:3]

    def _generate_summary(self, level: str, years: int, paths: list) -> str:
        path_names = {
            0: "Individual Contributor",
            1: "Management",
            2: "Specialist/Consulting",
        }

        summaries = [
            f"You are currently at {level} level with approximately {years} years of experience.",
        ]

        for i, path in enumerate(paths):
            if path:
                final_role = path[-1].role
                path_name = path_names.get(i, "alternative track")
                summaries.append(
                    f"Path {i + 1}: Progress to {final_role} via {path_name}"
                )

        return " ".join(summaries)
