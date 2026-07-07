"""ATS compatibility checker — validates CV against Applicant Tracking Systems."""

from __future__ import annotations

import re
from dataclasses import dataclass, field

from careertwin.graph.schema import CareerGraph, Skill


@dataclass
class ATSIssue:
    severity: str  # critical, warning, info
    category: str
    message: str
    fix_suggestion: str


@dataclass
class ATSReport:
    score: int  # 0-100
    issues: list[ATSIssue] = field(default_factory=list)
    passed_checks: list[str] = field(default_factory=list)

    @property
    def is_passing(self) -> bool:
        return self.score >= 70

    def summary(self) -> str:
        lines = [f"ATS Score: {self.score}/100"]
        if self.is_passing:
            lines.append("PASS")
        else:
            lines.append("NEEDS IMPROVEMENT")
        for issue in self.issues:
            lines.append(f"  [{issue.severity.upper()}] {issue.message}")
        return "\n".join(lines)


class ATSChecker:
    """Checks career documents for ATS compatibility."""

    # Common ATS-unfriendly patterns
    UNSAFE_PATTERNS = [
        (r"[│┃]", "vertical bars (tables)"),
        (r"[●○◆◇■□▲△]", "special characters/icons"),
        (r"\b(?:click here|here|this link)\b", "non-descriptive link text"),
    ]

    # Critical sections an ATS expects
    EXPECTED_SECTIONS = [
        "experience",
        "education",
        "skills",
        "summary",
    ]

    # Action verbs that ATS and recruiters look for
    STRONG_VERBS = {
        "achieved", "built", "created", "designed", "developed", "drove",
        "established", "implemented", "improved", "increased", "led",
        "managed", "optimized", "reduced", "launched", "delivered",
        "architected", "automated", "scaled", "migrated", "deployed",
    }

    def check(self, graph: CareerGraph, text: str) -> ATSReport:
        issues: list[ATSIssue] = []
        passed: list[str] = []

        self._check_sections(text, issues, passed)
        self._check_formatting(text, issues, passed)
        self._check_content(graph, text, issues, passed)
        self._check_keywords(graph, issues, passed)
        self._check_length(text, issues, passed)

        critical = sum(1 for i in issues if i.severity == "critical")
        warnings = sum(1 for i in issues if i.severity == "warning")

        score = 100
        score -= critical * 15
        score -= warnings * 5
        score = max(0, min(100, score))

        return ATSReport(score=score, issues=issues, passed_checks=passed)

    def _check_sections(self, text: str, issues: list[ATSIssue], passed: list[str]):
        lower = text.lower()
        for section in self.EXPECTED_SECTIONS:
            if section in lower:
                passed.append(f"Section found: {section}")
            else:
                issues.append(ATSIssue(
                    severity="critical",
                    category="sections",
                    message=f"Missing section: {section}",
                    fix_suggestion=f"Add a '{section.title()}' section to your resume",
                ))

    def _check_formatting(self, text: str, issues: list[ATSIssue], passed: list[str]):
        for pattern, name in self.UNSAFE_PATTERNS:
            matches = re.findall(pattern, text)
            if matches:
                issues.append(ATSIssue(
                    severity="warning",
                    category="formatting",
                    message=f"Found {name} ({len(matches)} instances)",
                    fix_suggestion=f"Replace {name} with plain text for better ATS parsing",
                ))
            else:
                passed.append(f"No {name} found")

        if re.search(r"\t", text):
            issues.append(ATSIssue(
                severity="warning",
                category="formatting",
                message="Tab characters detected",
                fix_suggestion="Use spaces instead of tabs",
            ))

    def _check_content(self, graph: CareerGraph, text: str, issues: list[ATSIssue], passed: list[str]):
        words = text.split()
        word_count = len(words)

        if word_count < 200:
            issues.append(ATSIssue(
                severity="warning",
                category="content",
                message=f"Resume too short ({word_count} words)",
                fix_suggestion="Aim for 400-800 words for a professional resume",
            ))
        elif word_count > 1500:
            issues.append(ATSIssue(
                severity="warning",
                category="content",
                message=f"Resume too long ({word_count} words)",
                fix_suggestion="Consider condensing to 1-2 pages",
            ))
        else:
            passed.append(f"Good length: {word_count} words")

        lines = text.split("\n")
        long_lines = [l for l in lines if len(l) > 120]
        if long_lines:
            issues.append(ATSIssue(
                severity="info",
                category="content",
                message=f"{len(long_lines)} lines exceed 120 characters",
                fix_suggestion="Keep lines under 80 characters for better readability",
            ))

    def _check_keywords(self, graph: CareerGraph, issues: list[ATSIssue], passed: list[str]):
        descriptions = []
        for exp in graph.experience:
            descriptions.append(exp.description.lower())
        all_text = " ".join(descriptions)

        found_verbs = [v for v in self.STRONG_VERBS if v in all_text]
        if len(found_verbs) < 3:
            issues.append(ATSIssue(
                severity="warning",
                category="keywords",
                message="Few action verbs found in experience descriptions",
                fix_suggestion="Start bullet points with strong verbs: Led, Built, Implemented, etc.",
            ))
        else:
            passed.append(f"Good action verbs: {', '.join(found_verbs[:5])}")

        has_numbers = bool(re.search(r"\d+", all_text))
        if not has_numbers:
            issues.append(ATSIssue(
                severity="info",
                category="keywords",
                message="No metrics/numbers found in experience",
                fix_suggestion="Add quantifiable achievements: 'Improved performance by 40%'",
            ))
        else:
            passed.append("Contains quantifiable metrics")

    def _check_length(self, text: str, issues: list[ATSIssue], passed: list[str]):
        lines = [l for l in text.split("\n") if l.strip()]
        if len(lines) > 80:
            issues.append(ATSIssue(
                severity="info",
                category="length",
                message=f"Resume has {len(lines)} non-empty lines",
                fix_suggestion="Aim for 40-60 lines for a clean, single-page layout",
            ))
