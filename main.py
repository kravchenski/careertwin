"""CareerTwin AI — entry point."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from careertwin.agents.orchestrator import CareerTwinOrchestrator
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


def main() -> None:
    parser = argparse.ArgumentParser(description="CareerTwin AI — Career Operating System")
    sub = parser.add_subparsers(dest="command")

    collect = sub.add_parser("collect", help="Collect career data from sources")
    _ = collect.add_argument("--github", help="GitHub username")
    _ = collect.add_argument("--resume", help="Path to resume file (PDF/DOCX/TXT)")
    _ = collect.add_argument("--website", help="Personal website URL")
    _ = collect.add_argument("--linkedin-export", help="LinkedIn data export path (ZIP/CSV)")

    check = sub.add_parser("check", help="Run ATS compatibility check")
    _ = check.add_argument("--resume", help="Path to resume file to check")

    _ = sub.add_parser("info", help="Show collected career data")

    args = parser.parse_args()
    command: str = args.command or ""

    orchestrator = CareerTwinOrchestrator()

    if command == "collect":
        _handle_collect(args, orchestrator)
    elif command == "check":
        _handle_check(args, orchestrator)
    elif command == "info":
        _handle_info(orchestrator)
    else:
        parser.print_help()


if __name__ == "__main__":
    main()
