"""Tests for Resume parser."""

from pathlib import Path

from careertwin.collectors.resume import ResumeParser


class TestResumeParser:
    def test_parse_sample_resume(self, sample_resume_path: Path):
        parser = ResumeParser()
        graph = parser.parse(sample_resume_path)

        assert graph.profile.full_name == "John Doe"
        assert graph.profile.email == "john@example.com"

    def test_parse_experience(self, sample_resume_path: Path):
        parser = ResumeParser()
        graph = parser.parse(sample_resume_path)

        assert len(graph.experience) >= 2
        companies = [e.company for e in graph.experience]
        assert "Google" in companies
        assert "Stripe" in companies

    def test_parse_positions(self, sample_resume_path: Path):
        parser = ResumeParser()
        graph = parser.parse(sample_resume_path)

        positions = [e.position for e in graph.experience]
        assert "Senior Software Engineer" in positions
        assert "Software Engineer" in positions

    def test_parse_education(self, sample_resume_path: Path):
        parser = ResumeParser()
        graph = parser.parse(sample_resume_path)

        assert len(graph.education) >= 1
        assert graph.education[0].institution == "MIT"

    def test_parse_skills(self, sample_resume_path: Path):
        parser = ResumeParser()
        graph = parser.parse(sample_resume_path)

        skill_names = [s.name for s in graph.skills]
        assert "Python" in skill_names
        assert "Go" in skill_names
        assert "React" in skill_names

    def test_skill_categories(self, sample_resume_path: Path):
        parser = ResumeParser()
        graph = parser.parse(sample_resume_path)

        skills_by_name = {s.name: s for s in graph.skills}
        assert skills_by_name["Python"].category.value == "language"
        assert skills_by_name["React"].category.value == "framework"

    def test_parse_minimal_resume(self, minimal_resume_path: Path):
        parser = ResumeParser()
        graph = parser.parse(minimal_resume_path)

        assert len(graph.experience) >= 1
        assert graph.experience[0].company == "Startup Inc"

    def test_to_search_text_after_parse(self, sample_resume_path: Path):
        parser = ResumeParser()
        graph = parser.parse(sample_resume_path)
        text = graph.to_search_text()

        assert "EXPERIENCE" in text
        assert "SKILLS" in text
        assert "Google" in text
