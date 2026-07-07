"""Repair Agent — fixes issues found by Vision QA in an iterative loop."""

from __future__ import annotations

from dataclasses import dataclass, field

from careertwin.graph.schema import CareerGraph
from careertwin.rendering import PdfRenderer, RenderConfig
from careertwin.templates import list_templates
from careertwin.vision import QAIssue, QAReport, VisionQAAgent


@dataclass
class RepairAction:
    issue: QAIssue
    fix_applied: str
    success: bool = False


@dataclass
class RepairResult:
    original_score: float
    final_score: float
    iterations: int
    actions: list[RepairAction] = field(default_factory=list)
    report: QAReport | None = None
    template_used: str = ""

    @property
    def improved(self) -> bool:
        return self.final_score > self.original_score


class RepairAgent:
    """Iteratively fixes resume issues found by QA checks."""

    MAX_ITERATIONS = 3
    MIN_SCORE = 80

    TEMPLATE_FALLBACK_ORDER = ["modern", "ats", "classic", "minimal", "executive", "sidebar"]

    def __init__(self, renderer: PdfRenderer | None = None, qa_agent: VisionQAAgent | None = None):
        self.renderer = renderer or PdfRenderer()
        self.qa_agent = qa_agent or VisionQAAgent()

    def repair(self, graph: CareerGraph, config: RenderConfig | None = None) -> RepairResult:
        config = config or RenderConfig()
        actions: list[RepairAction] = []
        last_score = 0.0
        template_used = config.template

        for iteration in range(self.MAX_ITERATIONS):
            result = self.renderer.render(graph, config)
            report = self.qa_agent.check_pdf(result.pdf_path)

            if iteration == 0:
                last_score = report.score

            if report.score >= self.MIN_SCORE:
                return RepairResult(
                    original_score=last_score,
                    final_score=report.score,
                    iterations=iteration + 1,
                    actions=actions,
                    report=report,
                    template_used=template_used,
                )

            for issue in report.issues:
                action = self._fix_issue(graph, issue)
                if action:
                    actions.append(action)

            template_switch = self._try_template_switch(config, report)
            if template_switch:
                actions.append(template_switch)
                template_used = config.template

        final_result = self.renderer.render(graph, config)
        final_report = self.qa_agent.check_pdf(final_result.pdf_path)

        return RepairResult(
            original_score=last_score,
            final_score=final_report.score,
            iterations=self.MAX_ITERATIONS,
            actions=actions,
            report=final_report,
            template_used=template_used,
        )

    def _fix_issue(self, graph: CareerGraph, issue: QAIssue) -> RepairAction | None:
        if issue.category == "content" and "placeholder" in issue.message.lower():
            return RepairAction(
                issue=issue, fix_applied="Flagged placeholder content", success=True,
            )

        if issue.category == "content" and "missing" in issue.message.lower():
            return self._fix_missing_content(graph, issue)

        if issue.category == "layout":
            return RepairAction(
                issue=issue, fix_applied="Layout adjustment needed in template", success=False,
            )

        if issue.category == "ats":
            return RepairAction(
                issue=issue, fix_applied="ATS issue flagged for manual review", success=False,
            )

        return None

    def _try_template_switch(self, config: RenderConfig, report: QAReport) -> RepairAction | None:
        """Try switching to a better template when layout/ATS issues are detected."""
        has_layout_issues = any(i.category == "layout" for i in report.issues)
        has_ats_issues = any(i.category == "ats" for i in report.issues)

        if not (has_layout_issues or has_ats_issues):
            return None

        available = set(list_templates())
        current_idx = 0
        for i, name in enumerate(self.TEMPLATE_FALLBACK_ORDER):
            if name == config.template:
                current_idx = i
                break

        for i in range(current_idx + 1, len(self.TEMPLATE_FALLBACK_ORDER)):
            candidate = self.TEMPLATE_FALLBACK_ORDER[i]
            if candidate in available and candidate != config.template:
                old_template = config.template
                config.template = candidate
                return RepairAction(
                    issue=QAIssue(
                        category="layout" if has_layout_issues else "ats",
                        severity="warning",
                        message=f"Switching template from '{old_template}' to '{candidate}'",
                    ),
                    fix_applied=f"Template switch: {old_template} -> {candidate}",
                    success=True,
                )

        return None

    def _fix_missing_content(self, graph: CareerGraph, issue: QAIssue) -> RepairAction:
        message = issue.message.lower()

        if "email" in message and not graph.profile.email:
            return RepairAction(
                issue=issue,
                fix_applied="Email missing — please add to profile",
                success=False,
            )
        if "phone" in message and not graph.profile.phone:
            return RepairAction(
                issue=issue,
                fix_applied="Phone missing — please add to profile",
                success=False,
            )
        if "summary" in message and not graph.profile.summary:
            skills_text = ", ".join(s.name for s in graph.skills[:3])
            graph.profile.summary = f"Experienced professional with expertise in {skills_text}."
            return RepairAction(
                issue=issue,
                fix_applied="Auto-generated summary from skills",
                success=True,
            )

        return RepairAction(issue=issue, fix_applied="Could not auto-fix", success=False)

    def _fix_truncated_content(self, graph: CareerGraph, issue: QAIssue) -> RepairAction:
        message = issue.message.lower()

        if "experience" in message or "description" in message:
            for exp in graph.experience:
                if len(exp.description) > 500:
                    exp.description = exp.description[:497] + "..."
                    return RepairAction(
                        issue=issue,
                        fix_applied="Truncated long experience description",
                        success=True,
                    )

        if "summary" in message and graph.profile.summary:
            if len(graph.profile.summary) > 300:
                graph.profile.summary = graph.profile.summary[:297] + "..."
                return RepairAction(
                    issue=issue,
                    fix_applied="Truncated long summary",
                    success=True,
                )

        return RepairAction(issue=issue, fix_applied="Could not auto-fix truncation", success=False)

    def _fix_layout_issue(
        self, graph: CareerGraph, issue: QAIssue, config: RenderConfig
    ) -> RepairAction:
        message = issue.message.lower()

        if "overflow" in message or "cut off" in message:
            return RepairAction(
                issue=issue,
                fix_applied="Layout overflow — consider reducing content or margins",
                success=False,
            )

        if "overlap" in message:
            return RepairAction(
                issue=issue,
                fix_applied="Overlapping elements — check z-index and positioning",
                success=False,
            )

        if "alignment" in message:
            return RepairAction(
                issue=issue,
                fix_applied="Alignment issue — verify CSS flexbox/grid settings",
                success=False,
            )

        return RepairAction(
            issue=issue, fix_applied="Layout issue flagged for review", success=False,
        )

    def _fix_typography_issue(
        self, graph: CareerGraph, issue: QAIssue, config: RenderConfig
    ) -> RepairAction:
        message = issue.message.lower()

        if "font" in message and "inconsistent" in message:
            return RepairAction(
                issue=issue,
                fix_applied="Inconsistent fonts — standardize font-family declarations",
                success=False,
            )

        if "size" in message and ("small" in message or "large" in message):
            return RepairAction(
                issue=issue,
                fix_applied="Font size issue — adjust font-size in CSS",
                success=False,
            )

        if "readability" in message:
            return RepairAction(
                issue=issue,
                fix_applied="Readability issue — increase font size or line-height",
                success=False,
            )

        return RepairAction(
            issue=issue, fix_applied="Typography issue flagged for review", success=False,
        )

    def _fix_spacing_issue(
        self, graph: CareerGraph, issue: QAIssue, config: RenderConfig
    ) -> RepairAction:
        message = issue.message.lower()

        if "margin" in message:
            return RepairAction(
                issue=issue,
                fix_applied="Margin issue — adjust margin values in CSS",
                success=False,
            )

        if "padding" in message:
            return RepairAction(
                issue=issue,
                fix_applied="Padding issue — adjust padding values in CSS",
                success=False,
            )

        if "gap" in message or "spacing" in message:
            return RepairAction(
                issue=issue,
                fix_applied="Spacing issue — adjust gap or line-height",
                success=False,
            )

        return RepairAction(
            issue=issue, fix_applied="Spacing issue flagged for review", success=False,
        )

    def _fix_ats_issue(self, graph: CareerGraph, issue: QAIssue) -> RepairAction:
        message = issue.message.lower()

        if "table" in message:
            return RepairAction(
                issue=issue,
                fix_applied="Table layout detected — convert to CSS-based layout",
                success=False,
            )

        if "section" in message:
            return RepairAction(
                issue=issue,
                fix_applied="Non-standard section name — use common ATS keywords",
                success=False,
            )

        if "format" in message:
            return RepairAction(
                issue=issue,
                fix_applied="Format issue — ensure consistent date and list formatting",
                success=False,
            )

        return RepairAction(
            issue=issue, fix_applied="ATS issue flagged for manual review", success=False,
        )
