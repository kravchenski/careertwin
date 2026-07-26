"""Tests for the end-to-end application package builder."""

from careertwin.agents.application_package import ApplicationPackage, ApplicationPackageBuilder
from careertwin.agents.resume_builder import JobDescription


class TestApplicationPackageBuilder:
    def test_build_writes_expected_files(self, sample_graph, tmp_path):
        builder = ApplicationPackageBuilder()
        job = JobDescription(
            title="Backend Engineer",
            company="Acme",
            description="Python Django PostgreSQL Docker APIs",
        )

        result = builder.build(sample_graph, job, output_dir=tmp_path / "package")

        assert isinstance(result, ApplicationPackage)
        assert (result.output_dir / "resume.json").exists()
        assert (result.output_dir / "resume.txt").exists()
        assert (result.output_dir / "cover_letter.txt").exists()
        assert (result.output_dir / "job_match.json").exists()
        assert (result.output_dir / "skill_gap.json").exists()
        assert (result.output_dir / "ats_report.json").exists()
        assert (result.output_dir / "resume.pdf").exists()

    def test_build_infers_job_skills_from_description(self, sample_graph, tmp_path):
        builder = ApplicationPackageBuilder()
        job = JobDescription(
            title="Frontend Engineer",
            company="Acme",
            description="React TypeScript frontend engineer building product UI",
        )

        result = builder.build(sample_graph, job, output_dir=tmp_path / "package")

        skill_names = [skill.name.lower() for skill in result.adapted_graph.skills]
        assert "react" in skill_names
