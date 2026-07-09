"""Application package builder — produces tailored resume artifacts for one job."""

from __future__ import annotations

import json
from dataclasses import asdict, dataclass
from pathlib import Path

from careertwin.agents.cover_letter import CoverLetter, CoverLetterGenerator
from careertwin.agents.job_matcher import JobMatch, JobMatcher
from careertwin.agents.resume_builder import JobDescription, ResumeBuilder
from careertwin.agents.skill_gap import SkillGapAnalyzer, SkillGapReport
from careertwin.ats.checker import ATSChecker, ATSReport
from careertwin.graph.schema import CareerGraph
from careertwin.rendering import PdfRenderer, RenderConfig, RenderResult
from careertwin.vision import QAReport, VisionQAAgent


@dataclass
class ApplicationPackage:
    output_dir: Path
    adapted_graph: CareerGraph
    job_match: JobMatch
    skill_gap: SkillGapReport
    ats_report: ATSReport
    cover_letter: CoverLetter
    render_result: RenderResult
    qa_report: QAReport | None = None


class ApplicationPackageBuilder:
    """Build a job-specific application package from a master career graph."""

    def __init__(
        self,
        resume_builder: ResumeBuilder | None = None,
        matcher: JobMatcher | None = None,
        gap_analyzer: SkillGapAnalyzer | None = None,
        cover_letter_generator: CoverLetterGenerator | None = None,
        ats_checker: ATSChecker | None = None,
        renderer: PdfRenderer | None = None,
        qa_agent: VisionQAAgent | None = None,
    ):
        self.resume_builder = resume_builder or ResumeBuilder()
        self.matcher = matcher or JobMatcher()
        self.gap_analyzer = gap_analyzer or SkillGapAnalyzer()
        self.cover_letter_generator = cover_letter_generator or CoverLetterGenerator()
        self.ats_checker = ats_checker or ATSChecker()
        self.renderer = renderer or PdfRenderer()
        self.qa_agent = qa_agent or VisionQAAgent()

    def build(
        self,
        graph: CareerGraph,
        job: JobDescription,
        output_dir: str | Path = "output/application",
        template: str = "modern",
        run_qa: bool = True,
    ) -> ApplicationPackage:
        output_path = Path(output_dir)
        output_path.mkdir(parents=True, exist_ok=True)

        adapted = self.resume_builder.build(graph, job)
        match = self.matcher.match(adapted, job.description)
        gap = self.gap_analyzer.analyze(adapted, job.description, target_role=job.title)
        ats = self.ats_checker.check(adapted, adapted.to_search_text())
        letter = self.cover_letter_generator.generate(
            graph=adapted,
            job_title=job.title,
            company=job.company,
            job_description=job.description,
        )

        render_result = self.renderer.render(
            adapted,
            RenderConfig(
                template=template,
                output_path=str(output_path / "resume.pdf"),
            ),
        )

        qa_report = self.qa_agent.check_pdf(render_result.pdf_path) if run_qa else None

        self._write_outputs(
            output_path=output_path,
            adapted=adapted,
            match=match,
            gap=gap,
            ats=ats,
            letter=letter,
            qa_report=qa_report,
            render_result=render_result,
            job=job,
        )

        return ApplicationPackage(
            output_dir=output_path,
            adapted_graph=adapted,
            job_match=match,
            skill_gap=gap,
            ats_report=ats,
            cover_letter=letter,
            render_result=render_result,
            qa_report=qa_report,
        )

    def _write_outputs(
        self,
        output_path: Path,
        adapted: CareerGraph,
        match: JobMatch,
        gap: SkillGapReport,
        ats: ATSReport,
        letter: CoverLetter,
        qa_report: QAReport | None,
        render_result: RenderResult,
        job: JobDescription,
    ) -> None:
        (output_path / "resume.json").write_text(
            json.dumps(json.loads(adapted.model_dump_json()), indent=2),
            encoding="utf-8",
        )
        (output_path / "resume.txt").write_text(adapted.to_search_text(), encoding="utf-8")
        (output_path / "cover_letter.txt").write_text(letter.render(), encoding="utf-8")
        (output_path / "job.json").write_text(json.dumps(asdict(job), indent=2), encoding="utf-8")
        (output_path / "job_match.json").write_text(
            json.dumps(asdict(match), indent=2),
            encoding="utf-8",
        )
        (output_path / "skill_gap.json").write_text(
            json.dumps(asdict(gap), indent=2),
            encoding="utf-8",
        )
        (output_path / "ats_report.json").write_text(
            json.dumps(asdict(ats), indent=2),
            encoding="utf-8",
        )
        (output_path / "render_result.json").write_text(
            json.dumps(asdict(render_result), indent=2),
            encoding="utf-8",
        )
        if qa_report:
            (output_path / "qa_report.json").write_text(
                json.dumps(asdict(qa_report), indent=2),
                encoding="utf-8",
            )
            (output_path / "qa_report.txt").write_text(qa_report.to_text(), encoding="utf-8")
