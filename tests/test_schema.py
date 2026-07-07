"""Tests for Career Knowledge Graph schema."""

from datetime import date

from careertwin.graph.schema import (
    CareerGraph,
    Evidence,
    Experience,
    Profile,
    SourceType,
    Technology,
)


class TestProfile:
    def test_create_profile(self):
        profile = Profile(full_name="John Doe", email="john@example.com")
        assert profile.full_name == "John Doe"
        assert profile.email == "john@example.com"
        assert profile.id  # auto-generated

    def test_optional_fields(self):
        profile = Profile(full_name="Jane")
        assert profile.headline is None
        assert profile.email is None


class TestTechnology:
    def test_create_technology(self):
        tech = Technology(name="Python", category="language")
        assert tech.name == "Python"
        assert tech.category == "language"

    def test_technology_with_evidence(self):
        evidence = [Evidence(source=SourceType.GITHUB, url="https://github.com/test")]
        tech = Technology(name="React", category="framework", evidence=evidence)
        assert len(tech.evidence) == 1


class TestExperience:
    def test_create_experience(self):
        exp = Experience(company="Google", position="Engineer")
        assert exp.company == "Google"
        assert exp.position == "Engineer"
        assert exp.technologies == []

    def test_experience_with_dates(self):
        exp = Experience(
            company="Stripe",
            position="Senior Engineer",
            start_date=date(2020, 1, 1),
            end_date=date(2023, 12, 31),
        )
        assert exp.start_date == date(2020, 1, 1)


class TestCareerGraph:
    def test_empty_graph(self):
        graph = CareerGraph(profile=Profile(full_name="Empty"))
        assert graph.experience == []
        assert graph.skills == []
        assert graph.projects == []

    def test_all_technologies(self, sample_graph: CareerGraph):
        techs = sample_graph.all_technologies()
        names = [t.name for t in techs]
        assert "Python" in names
        assert "Go" in names
        assert "Ruby" in names

    def test_all_technologies_dedup(self, sample_graph: CareerGraph):
        techs = sample_graph.all_technologies()
        names = [t.name for t in techs]
        assert len(names) == len(set(names))

    def test_all_achievements(self, sample_graph: CareerGraph):
        achievements = sample_graph.all_achievements()
        assert len(achievements) > 0
        assert any("40%" in a.metric for a in achievements if a.metric)

    def test_to_search_text(self, sample_graph: CareerGraph):
        text = sample_graph.to_search_text()
        assert "SUMMARY" in text
        assert "EXPERIENCE" in text
        assert "SKILLS" in text
        assert "Google" in text
        assert "Python" in text

    def test_to_search_text_empty(self):
        graph = CareerGraph(profile=Profile(full_name="Empty"))
        text = graph.to_search_text()
        assert text == ""
