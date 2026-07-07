"""Quick demo of CareerTwin capabilities."""

from careertwin.collectors.resume import ResumeParser
from careertwin.ats.checker import ATSChecker
from careertwin.graph.schema import CareerGraph


def demo_resume_parsing():
    print("=" * 60)
    print("RESUME PARSING DEMO")
    print("=" * 60)

    parser = ResumeParser()
    graph = parser.parse("test_resume.txt")

    print(f"\nName: {graph.profile.full_name}")
    print(f"Email: {graph.profile.email}")
    print(f"Location: {graph.profile.location}")

    print("\n--- Experience ---")
    for exp in graph.experience:
        print(f"  {exp.position} at {exp.company}")
        if exp.description:
            print(f"    {exp.description[:60]}...")

    print("\n--- Education ---")
    for edu in graph.education:
        print(f"  {edu.degree} - {edu.institution}")

    print("\n--- Skills ---")
    for skill in graph.skills:
        print(f"  - {skill.name} ({skill.category.value})")

    return graph


def demo_ats_check(graph: CareerGraph):
    print("\n" + "=" * 60)
    print("ATS COMPATIBILITY CHECK")
    print("=" * 60)

    checker = ATSChecker()
    text = graph.to_search_text()
    report = checker.check(graph, text)

    print(f"\nScore: {report.score}/100")
    print(f"Status: {'PASS' if report.is_passing else 'NEEDS IMPROVEMENT'}")

    if report.issues:
        print("\nIssues:")
        for issue in report.issues:
            icon = "!!!" if issue.severity == "critical" else "!" if issue.severity == "warning" else "i"
            print(f"  [{icon}] {issue.message}")
            print(f"      Fix: {issue.fix_suggestion}")

    if report.passed_checks:
        print("\nPassed:")
        for check in report.passed_checks:
            print(f"  [ok] {check}")


def main():
    graph = demo_resume_parsing()
    demo_ats_check(graph)

    print("\n" + "=" * 60)
    print("NEXT STEPS")
    print("=" * 60)
    print("""
To test with your own data:

1. Parse your resume:
   uv run python main.py collect --resume your_resume.pdf

2. Check ATS compatibility:
   uv run python main.py check --resume your_resume.pdf

3. Collect from GitHub (requires token):
   export GITHUB_TOKEN=ghp_xxx
   uv run python main.py collect --github your_username
""")


if __name__ == "__main__":
    main()
