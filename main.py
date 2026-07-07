"""CareerTwin AI — entry point."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from careertwin.agents.career_recommender import CareerRecommender
from careertwin.agents.cover_letter import CoverLetterGenerator
from careertwin.agents.job_matcher import JobMatcher
from careertwin.agents.orchestrator import CareerTwinOrchestrator
from careertwin.agents.resume_builder import JobDescription, ResumeBuilder
from careertwin.agents.skill_gap import SkillGapAnalyzer
from careertwin.collectors.resume import ResumeParser
from careertwin.graph.schema import CareerGraph


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
    _ = build.add_argument("--job", help="Path to job description file")
    _ = build.add_argument("--title", help="Job title")
    _ = build.add_argument("--company", help="Company name")
    _ = build.add_argument("--description", help="Job description text")

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

    args = parser.parse_args()
    command: str = args.command or ""

    orchestrator = CareerTwinOrchestrator()

    # Auto-load saved career graph if it exists
    graph_path = Path("career_graph.json")
    if graph_path.exists() and command not in ("collect", ""):
        try:
            data = json.loads(graph_path.read_text())
            orchestrator.graph = CareerGraph.model_validate(data)
        except Exception:
            pass

    if command == "collect":
        _handle_collect(args, orchestrator)
    elif command == "check":
        _handle_check(args, orchestrator)
    elif command == "info":
        _handle_info(orchestrator)
    elif command == "build":
        _handle_build(args, orchestrator)
    elif command == "cover":
        _handle_cover(args, orchestrator)
    elif command == "match":
        _handle_match(args, orchestrator)
    elif command == "gaps":
        _handle_gaps(args, orchestrator)
    elif command == "trajectory":
        _handle_trajectory(args, orchestrator)
    else:
        parser.print_help()


if __name__ == "__main__":
    main()
