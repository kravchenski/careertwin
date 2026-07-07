"""CareerTwin AI — entry point."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from careertwin.agents.orchestrator import CareerTwinOrchestrator
from careertwin.graph.schema import CareerGraph


def main():
    parser = argparse.ArgumentParser(description="CareerTwin AI — Career Operating System")
    sub = parser.add_subparsers(dest="command")

    collect = sub.add_parser("collect", help="Collect career data from sources")
    collect.add_argument("--github", help="GitHub username")
    collect.add_argument("--resume", help="Path to resume file (PDF/DOCX/TXT)")
    collect.add_argument("--website", help="Personal website URL")
    collect.add_argument("--linkedin-export", help="LinkedIn data export path (ZIP/CSV)")

    check = sub.add_parser("check", help="Run ATS compatibility check")
    check.add_argument("--resume", help="Path to resume file to check")

    info = sub.add_parser("info", help="Show collected career data")

    args = parser.parse_args()
    orchestrator = CareerTwinOrchestrator()

    if args.command == "collect":
        graph = orchestrator.run_collection(
            github=args.github,
            resume_path=args.resume,
        )
        output = Path("career_graph.json")
        output.write_text(json.dumps(json.loads(graph.model_dump_json()), indent=2))
        print(f"\nCareer graph saved to {output}")

    elif args.command == "check":
        if args.resume:
            from careertwin.collectors.resume import ResumeParser
            parser = ResumeParser()
            graph = parser.parse(args.resume)
            orchestrator.graph = graph

        report = orchestrator.check_ats()
        print(report.summary())

    elif args.command == "info":
        graph = orchestrator.graph
        print(f"Name: {graph.profile.full_name}")
        print(f"Experiences: {len(graph.experience)}")
        print(f"Projects: {len(graph.projects)}")
        print(f"Skills: {len(graph.skills)}")
        print(f"Technologies: {len(graph.technologies)}")

    else:
        parser.print_help()


if __name__ == "__main__":
    main()
