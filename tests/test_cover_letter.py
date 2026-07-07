"""Tests for cover letter generator agent."""

from careertwin.agents.cover_letter import CoverLetter, CoverLetterGenerator
from careertwin.graph.schema import CareerGraph, Profile


class TestCoverLetterGenerator:
    def test_generate_returns_cover_letter(self, sample_graph: CareerGraph):
        generator = CoverLetterGenerator()
        letter = generator.generate(
            graph=sample_graph,
            job_title="Software Engineer",
            company="Acme",
        )

        assert isinstance(letter, CoverLetter)
        assert "Dear Hiring Manager" in letter.greeting
        assert letter.signature

    def test_render_output(self, sample_graph: CareerGraph):
        generator = CoverLetterGenerator()
        letter = generator.generate(
            graph=sample_graph,
            job_title="Software Engineer",
            company="Acme",
        )
        rendered = letter.render()

        assert "Dear Hiring Manager" in rendered
        assert "Software Engineer" in rendered
        assert "Acme" in rendered
        assert "John Doe" in rendered

    def test_generate_with_email(self, sample_graph: CareerGraph):
        generator = CoverLetterGenerator()
        letter = generator.generate(
            graph=sample_graph,
            job_title="Role",
            company="Co",
        )

        assert "john@example.com" in letter.signature

    def test_generate_without_email(self):
        graph = CareerGraph(profile=Profile(full_name="No Email"))
        generator = CoverLetterGenerator()
        letter = generator.generate(
            graph=graph,
            job_title="Role",
            company="Co",
        )

        assert "No Email" in letter.signature
        assert "@" not in letter.signature

    def test_generate_with_job_description(self, sample_graph: CareerGraph):
        generator = CoverLetterGenerator()
        letter = generator.generate(
            graph=sample_graph,
            job_title="Python Developer",
            company="TechCorp",
            job_description="We need Python and Django experience",
        )

        assert "Python Developer" in letter.render()
        assert "TechCorp" in letter.render()

    def test_body_contains_experience(self, sample_graph: CareerGraph):
        generator = CoverLetterGenerator()
        letter = generator.generate(
            graph=sample_graph,
            job_title="Role",
            company="Co",
        )

        assert "experience" in letter.body.lower()

    def test_body_contains_skills(self, sample_graph: CareerGraph):
        generator = CoverLetterGenerator()
        letter = generator.generate(
            graph=sample_graph,
            job_title="Role",
            company="Co",
        )

        assert "Python" in letter.body
