"""CareerTwin AI — entry point."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from careertwin.agents.application_package import ApplicationPackageBuilder
from careertwin.agents.career_recommender import CareerRecommender
from careertwin.agents.cover_letter import CoverLetterGenerator
from careertwin.agents.job_matcher import JobMatcher
from careertwin.agents.orchestrator import CareerTwinOrchestrator
from careertwin.agents.resume_builder import JobDescription, ResumeBuilder
from careertwin.agents.skill_gap import SkillGapAnalyzer
from careertwin.collectors.resume import ResumeParser
from careertwin.graph.schema import CareerGraph
from careertwin.rendering import PdfRenderer, RenderConfig
from careertwin.rendering.design_dna import DesignDNAEngine
from careertwin.rendering.repair import RepairAgent
from careertwin.rendering.trends import DesignTrendScraper
from careertwin.templates import list_templates
from careertwin.vision import VisionQAAgent


def _handle_collect(args: argparse.Namespace, orchestrator: CareerTwinOrchestrator) -> None:
    github: str = args.github or ""
    resume_path: str = args.resume or ""
    graph: CareerGraph = orchestrator.run_collection(
        github=github,
        resume_path=resume_path,
    )
    output = Path("career_graph.json")
    output.write_text(json.dumps(json.loads(graph.model_dump_json()), indent=2))
    print(f"\nCareer graph saved to {output}")


def _handle_check(args: argparse.Namespace, orchestrator: CareerTwinOrchestrator) -> None:
    resume_path: str = args.resume or ""
    if resume_path:
        cv_parser = ResumeParser()
        graph: CareerGraph = cv_parser.parse(resume_path)
        orchestrator.graph = graph

    report = orchestrator.check_ats()
    print(report.summary())


def _handle_info(orchestrator: CareerTwinOrchestrator) -> None:
    graph: CareerGraph = orchestrator.graph
    name: str = graph.profile.full_name
    exp_count: int = len(graph.experience)
    proj_count: int = len(graph.projects)
    skill_count: int = len(graph.skills)
    tech_count: int = len(graph.technologies)
    print(f"Name: {name}")
    print(f"Experiences: {exp_count}")
    print(f"Projects: {proj_count}")
    print(f"Skills: {skill_count}")
    print(f"Technologies: {tech_count}")


def _handle_build(args: argparse.Namespace, orchestrator: CareerTwinOrchestrator) -> None:
    job_text = Path(args.job).read_text() if args.job else args.description or ""
    job = JobDescription(
        title=args.title or "Position",
        company=args.company or "Company",
        description=job_text,
    )

    builder = ResumeBuilder()
    adapted = builder.build(orchestrator.graph, job)

    output = Path("adapted_resume.json")
    output.write_text(json.dumps(json.loads(adapted.model_dump_json()), indent=2))
    print(f"\nAdapted resume saved to {output}")
    print(f"Experiences: {len(adapted.experience)}")
    print(f"Skills: {len(adapted.skills)}")


def _load_graph_for_args(args: argparse.Namespace, orchestrator: CareerTwinOrchestrator) -> CareerGraph:
    if getattr(args, "resume", None):
        parser = ResumeParser()
        orchestrator.graph = parser.parse(args.resume)
    return orchestrator.graph


def _handle_tailor(args: argparse.Namespace, orchestrator: CareerTwinOrchestrator) -> None:
    graph = _load_graph_for_args(args, orchestrator)
    job_text = Path(args.job).read_text() if args.job else args.description or ""
    job = JobDescription(
        title=args.title or "Position",
        company=args.company or "Company",
        description=job_text,
    )

    builder = ApplicationPackageBuilder()
    package = builder.build(
        graph=graph,
        job=job,
        output_dir=args.output_dir or "output/application",
        template=args.template or "modern",
        run_qa=not args.skip_qa,
    )

    print(f"\nApplication package saved to {package.output_dir}")
    print(f"Match Score: {package.job_match.match_score}/100")
    print(f"ATS Score: {package.ats_report.score}/100")
    print(f"Readiness: {package.skill_gap.overall_readiness}%")
    print(f"Resume PDF: {package.render_result.pdf_path}")
    print(f"Cover Letter: {package.output_dir / 'cover_letter.txt'}")
    if package.qa_report:
        print(f"Visual QA: {package.qa_report.score}/100")


def _handle_cover(args: argparse.Namespace, orchestrator: CareerTwinOrchestrator) -> None:
    job_text = Path(args.job).read_text() if args.job else ""

    generator = CoverLetterGenerator()
    letter = generator.generate(
        graph=orchestrator.graph,
        job_title=args.title or "Position",
        company=args.company or "Company",
        job_description=job_text,
    )

    output = Path("cover_letter.txt")
    output.write_text(letter.render())
    print(f"\nCover letter saved to {output}")
    print(letter.render())


def _handle_match(args: argparse.Namespace, orchestrator: CareerTwinOrchestrator) -> None:
    job_text = Path(args.job).read_text() if args.job else args.description or ""

    matcher = JobMatcher()
    match = matcher.match(orchestrator.graph, job_text)

    print(f"\nJob: {match.title}")
    print(f"Company: {match.company}")
    print(f"Match Score: {match.match_score}/100")
    print(f"\nMatching Skills: {', '.join(match.matching_skills) or 'None'}")
    print(f"Missing Skills: {', '.join(match.missing_skills) or 'None'}")
    print(f"Matching Experience: {', '.join(match.matching_experience) or 'None'}")
    print(f"\nRecommendation: {match.recommendation}")


def _handle_gaps(args: argparse.Namespace, orchestrator: CareerTwinOrchestrator) -> None:
    job_text = Path(args.job).read_text() if args.job else args.description or ""

    analyzer = SkillGapAnalyzer()
    report = analyzer.analyze(
        graph=orchestrator.graph,
        job_description=job_text,
        target_role=args.title or "",
    )

    print(f"\nTarget Role: {report.target_role}")
    print(f"Readiness: {report.overall_readiness}%")
    print(f"\nCurrent Skills ({len(report.current_skills)}): {', '.join(report.current_skills[:10])}")
    print(f"\nRequired Skills ({len(report.required_skills)}): {', '.join(report.required_skills)}")

    if report.gaps:
        print(f"\nSkill Gaps ({len(report.gaps)}):")
        for gap in report.gaps:
            print(f"  - {gap.skill} [{gap.importance}]")
            if gap.recommended_resources:
                print(f"    Resources: {', '.join(gap.recommended_resources[:2])}")

    if report.recommendations:
        print("\nRecommendations:")
        for rec in report.recommendations:
            print(f"  - {rec}")


def _handle_trajectory(args: argparse.Namespace, orchestrator: CareerTwinOrchestrator) -> None:
    recommender = CareerRecommender()
    trajectory = recommender.recommend(orchestrator.graph)

    print(f"\nCurrent Level: {trajectory.current_level}")
    print(f"\n{trajectory.summary}")

    if trajectory.suggested_paths:
        print("\nSuggested Career Paths:")
        for i, path in enumerate(trajectory.suggested_paths, 1):
            print(f"\n  Path {i}:")
            for step in path:
                print(f"    → {step.role} ({step.timeline})")
                print(f"      Skills needed: {', '.join(step.skills_needed)}")


def _handle_render(args: argparse.Namespace, orchestrator: CareerTwinOrchestrator) -> None:
    renderer = PdfRenderer()
    config = RenderConfig(
        template=args.template or "modern",
        output_path=args.output or "resume.pdf",
    )

    result = renderer.render(orchestrator.graph, config)
    print(f"\nPDF rendered: {result.pdf_path}")
    print(f"Template: {result.template}")
    print(f"Pages: {result.page_count}")
    if result.html_path:
        print(f"HTML preview: {result.html_path}")


def _handle_qa(args: argparse.Namespace, orchestrator: CareerTwinOrchestrator) -> None:
    qa = VisionQAAgent()

    if args.pdf:
        report = qa.check_pdf(args.pdf)
    elif args.html:
        report = qa.check_html(args.html)
    else:
        renderer = PdfRenderer()
        config = RenderConfig(template=args.template or "modern")
        result = renderer.render(orchestrator.graph, config)
        report = qa.check_pdf(result.pdf_path)

    print(report.to_text())


def _handle_repair(args: argparse.Namespace, orchestrator: CareerTwinOrchestrator) -> None:
    agent = RepairAgent()
    config = RenderConfig(
        template=args.template or "modern",
        output_path=args.output or "resume.pdf",
    )

    print("Running repair loop...")
    result = agent.repair(orchestrator.graph, config)

    print(f"\nIterations: {result.iterations}")
    print(f"Score: {result.original_score:.0f} → {result.final_score:.0f}")
    print(f"Improved: {'Yes' if result.improved else 'No'}")

    if result.actions:
        print("\nActions taken:")
        for action in result.actions:
            status = "OK" if action.success else "SKIP"
            print(f"  [{status}] {action.fix_applied}")

    if result.report:
        print(f"\nFinal report:\n{result.report.to_text()}")


def _handle_trends(args: argparse.Namespace) -> None:
    scraper = DesignTrendScraper()
    trends = scraper.get_trends()

    print(f"\nDesign Trends ({len(trends)}):\n")
    for trend in trends:
        print(f"  {trend.name} (popularity: {trend.popularity:.0%})")
        print(f"    Colors: {', '.join(trend.colors)}")
        print(f"    Fonts: {', '.join(trend.fonts)}")
        print(f"    Tags: {', '.join(trend.style_tags)}")
        print()


def _handle_templates() -> None:
    templates = list_templates()
    print(f"\nAvailable Templates ({len(templates)}):\n")
    for t in templates:
        print(f"  - {t}")


def _handle_dna(args: argparse.Namespace, orchestrator: CareerTwinOrchestrator) -> None:
    engine = DesignDNAEngine()
    renderer = PdfRenderer()

    html = renderer.render_to_html(orchestrator.graph, args.template or "modern")
    dna = engine.analyze_html(html)

    print(f"\nDesign DNA for template '{args.template or 'modern'}':\n")
    print(f"  Style: {dna.style}")
    print(f"  Layout: {dna.layout}")
    print(f"  Spacing: {dna.spacing}")
    print(f"  Section style: {dna.section_style}")
    print(f"  Tag style: {dna.tag_style}")
    print(f"  Colors: {', '.join(dna.colors[:5])}")
    print(f"  Fonts: {', '.join(dna.fonts[:3])}")

    if dna.scores:
        print("\n  Scores:")
        for key, val in dna.scores.items():
            print(f"    {key}: {val:.0f}/100")


def main() -> None:
    parser = argparse.ArgumentParser(description="CareerTwin AI — Career Operating System")
    sub = parser.add_subparsers(dest="command")

    collect = sub.add_parser("collect", help="Collect career data from sources")
    _ = collect.add_argument("--github", help="GitHub username")
    _ = collect.add_argument("--resume", help="Path to resume file (PDF/DOCX/TXT)")

    check = sub.add_parser("check", help="Run ATS compatibility check")
    _ = check.add_argument("--resume", help="Path to resume file to check")

    _ = sub.add_parser("info", help="Show collected career data")

    build = sub.add_parser("build", help="Build tailored resume for a job")
    _ = build.add_argument("--resume", help="Path to resume file (overrides saved graph)")
    _ = build.add_argument("--job", help="Path to job description file")
    _ = build.add_argument("--title", help="Job title")
    _ = build.add_argument("--company", help="Company name")
    _ = build.add_argument("--description", help="Job description text")

    tailor = sub.add_parser("tailor", help="Generate a full application package for one job")
    _ = tailor.add_argument("--resume", help="Path to resume file (PDF/DOCX/TXT)")
    _ = tailor.add_argument("--job", help="Path to job description file")
    _ = tailor.add_argument("--title", help="Job title")
    _ = tailor.add_argument("--company", help="Company name")
    _ = tailor.add_argument("--description", help="Job description text")
    _ = tailor.add_argument("--template", "-t", help="Render template name")
    _ = tailor.add_argument("--output-dir", help="Directory for generated artifacts")
    _ = tailor.add_argument("--skip-qa", action="store_true", help="Skip visual QA step")

    cover = sub.add_parser("cover", help="Generate cover letter")
    _ = cover.add_argument("--job", help="Path to job description file")
    _ = cover.add_argument("--title", help="Job title")
    _ = cover.add_argument("--company", help="Company name")

    match = sub.add_parser("match", help="Match profile to job description")
    _ = match.add_argument("--job", help="Path to job description file")
    _ = match.add_argument("--description", help="Job description text")

    gaps = sub.add_parser("gaps", help="Analyze skill gaps for a role")
    _ = gaps.add_argument("--job", help="Path to job description file")
    _ = gaps.add_argument("--description", help="Job description text")
    _ = gaps.add_argument("--title", help="Target role title")

    _ = sub.add_parser("trajectory", help="Get career trajectory recommendations")

    render = sub.add_parser("render", help="Render resume to PDF")
    _ = render.add_argument("--template", "-t", help="Template name (modern, classic, minimal)")
    _ = render.add_argument("--output", "-o", help="Output PDF path")

    qa = sub.add_parser("qa", help="Run visual QA check on resume")
    _ = qa.add_argument("--pdf", help="Path to PDF file to check")
    _ = qa.add_argument("--html", help="Path to HTML file to check")
    _ = qa.add_argument("--template", "-t", help="Template to render and check")

    repair = sub.add_parser("repair", help="Auto-repair resume issues")
    _ = repair.add_argument("--template", "-t", help="Template name")
    _ = repair.add_argument("--output", "-o", help="Output PDF path")

    _ = sub.add_parser("trends", help="Show design trends")

    _ = sub.add_parser("templates", help="List available templates")

    dna = sub.add_parser("dna", help="Analyze design DNA of a template")
    _ = dna.add_argument("--template", "-t", help="Template to analyze")

    serve = sub.add_parser("serve", help="Run the HTTP API")
    _ = serve.add_argument("--host", default="127.0.0.1")
    _ = serve.add_argument("--port", type=int, default=8000)
    _ = serve.add_argument("--reload", action="store_true")
    _ = serve.add_argument(
        "--allow-remote",
        action="store_true",
        help="Listen on a non-loopback address (the API has no TLS: put a reverse proxy in front)",
    )

    mcp = sub.add_parser("mcp", help="Run the MCP server for agents")
    _ = mcp.add_argument("--transport", choices=["stdio", "http"], default="stdio")
    _ = mcp.add_argument("--host", default="127.0.0.1")
    _ = mcp.add_argument("--port", type=int, default=8765)

    agent = sub.add_parser("agent", help="Run the job-search agent on a Switchyard gateway")
    _ = agent.add_argument("task", nargs="?", help="What to do; omit for an interactive session")
    _ = agent.add_argument("--model", help="Gateway model id (default: SWITCHYARD_AGENT_MODEL)")
    _ = agent.add_argument("--max-steps", type=int, default=25)
    _ = agent.add_argument("--yes", action="store_true", help="Approve outward actions without asking")

    args = parser.parse_args()
    command: str = args.command or ""

    if command in {"serve", "mcp", "agent"}:
        _run_service(command, args)
        return

    orchestrator = CareerTwinOrchestrator()

    _try_load_saved_graph(orchestrator, command)

    if command == "collect":
        _handle_collect(args, orchestrator)
    elif command == "check":
        _handle_check(args, orchestrator)
    elif command == "info":
        _handle_info(orchestrator)
    elif command == "build":
        _load_graph_for_args(args, orchestrator)
        _handle_build(args, orchestrator)
    elif command == "tailor":
        _handle_tailor(args, orchestrator)
    elif command == "cover":
        _handle_cover(args, orchestrator)
    elif command == "match":
        _handle_match(args, orchestrator)
    elif command == "gaps":
        _handle_gaps(args, orchestrator)
    elif command == "trajectory":
        _handle_trajectory(args, orchestrator)
    elif command == "render":
        _handle_render(args, orchestrator)
    elif command == "qa":
        _handle_qa(args, orchestrator)
    elif command == "repair":
        _handle_repair(args, orchestrator)
    elif command == "trends":
        _handle_trends(args)
    elif command == "templates":
        _handle_templates()
    elif command == "dna":
        _handle_dna(args, orchestrator)
    else:
        parser.print_help()


def _run_service(command: str, args: argparse.Namespace) -> None:
    if command == "serve":
        import uvicorn

        if args.host not in {"127.0.0.1", "localhost", "::1"} and not args.allow_remote:
            raise SystemExit(
                f"Refusing to listen on {args.host}: the API carries your CV and can send applications. "
                "Use 127.0.0.1, or pass --allow-remote behind a TLS reverse proxy."
            )
        uvicorn.run(
            "careertwin.api.app:app", host=args.host, port=args.port, reload=args.reload
        )
    elif command == "mcp":
        from careertwin.agentic.mcp_server import main as mcp_main

        mcp_main(["--transport", args.transport, "--host", args.host, "--port", str(args.port)])
    else:
        import asyncio

        from careertwin.agentic.cli import run_agent

        asyncio.run(run_agent(args.task, model=args.model, max_steps=args.max_steps, yes=args.yes))


def _try_load_saved_graph(orchestrator: CareerTwinOrchestrator, command: str) -> None:
    graph_path = Path("career_graph.json")
    if graph_path.exists() and command not in ("collect", ""):
        try:
            data = json.loads(graph_path.read_text())
            orchestrator.graph = CareerGraph.model_validate(data)
        except Exception:
            pass


if __name__ == "__main__":
    main()
