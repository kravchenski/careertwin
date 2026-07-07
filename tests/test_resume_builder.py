"""Tests for resume builder agent."""

from careertwin.agents.resume_builder import JobDescription, ResumeBuilder
from careertwin.graph.schema import CareerGraph, Profile


class TestResumeBuilder:
    def test_build_returns_career_graph(self, sample_graph: CareerGraph):
        builder = ResumeBuilder()
        job = JobDescription(
            title="Backend Engineer",
            company="Acme",
            description="Python, Django, PostgreSQL",
            required_skills=["python", "django", "postgresql"],
        )
        result = builder.build(sample_graph, job)

        assert isinstance(result, CareerGraph)
        assert result.profile == sample_graph.profile

    def test_build_adapts_experience(self, sample_graph: CareerGraph):
        builder = ResumeBuilder()
        job = JobDescription(
            title="Backend Engineer",
            company="Acme",
            description="Python Django PostgreSQL",
            required_skills=["python", "django"],
        )
        result = builder.build(sample_graph, job)

        assert len(result.experience) >= 1

    def test_build_adapts_skills(self, sample_graph: CareerGraph):
        builder = ResumeBuilder()
        job = JobDescription(
            title="Frontend Engineer",
            company="Acme",
            description="React TypeScript",
            required_skills=["react", "typescript"],
        )
        result = builder.build(sample_graph, job)

        skill_names = [s.name.lower() for s in result.skills]
        assert "react" in skill_names

    def test_build_limits_experience(self, sample_graph: CareerGraph):
        builder = ResumeBuilder()
        job = JobDescription(title="Role", company="Co", description="")
        result = builder.build(sample_graph, job)

        assert len(result.experience) <= 5

    def test_build_limits_projects(self, sample_graph: CareerGraph):
        builder = ResumeBuilder()
        job = JobDescription(title="Role", company="Co", description="")
        result = builder.build(sample_graph, job)

        assert len(result.projects) <= 3

    def test_build_empty_graph(self):
        graph = CareerGraph(profile=Profile(full_name="Test"))
        builder = ResumeBuilder()
        job = JobDescription(title="Role", company="Co", description="Python")

        result = builder.build(graph, job)
        assert result.profile.full_name == "Test"
        assert len(result.experience) == 0

    def test_build_preserves_education(self, sample_graph: CareerGraph):
        builder = ResumeBuilder()
        job = JobDescription(title="Role", company="Co", description="")
        result = builder.build(sample_graph, job)

        assert result.education == sample_graph.education
