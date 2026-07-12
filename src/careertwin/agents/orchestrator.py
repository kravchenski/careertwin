"""LangGraph agent orchestration — manages the CareerTwin workflow."""

from __future__ import annotations

import logging
from typing import Annotated, Optional, TypedDict

from langchain_core.messages import SystemMessage
from langchain_core.tools import tool
from langgraph.graph import END, StateGraph
from langgraph.graph.message import add_messages

from careertwin.ats.checker import ATSChecker, ATSReport
from careertwin.collectors.github import GitHubCollector
from careertwin.collectors.resume import ResumeParser
from careertwin.collectors.website import WebsiteCollector
from careertwin.graph.schema import CareerGraph

logger = logging.getLogger("careertwin.orchestrator")


class CareerState(TypedDict):
    messages: Annotated[list, add_messages]
    graph: Optional[CareerGraph]
    job_description: Optional[str]
    ats_report: Optional[ATSReport]
    collected_sources: list[str]


def _merge_graphs(target: CareerGraph, source: CareerGraph) -> CareerGraph:
    existing_exp_ids = {e.id for e in target.experience}
    for exp in source.experience:
        if exp.id not in existing_exp_ids:
            target.experience.append(exp)

    existing_proj_ids = {p.id for p in target.projects}
    for proj in source.projects:
        if proj.id not in existing_proj_ids:
            target.projects.append(proj)

    existing_skill_ids = {s.id for s in target.skills}
    for skill in source.skills:
        if skill.id not in existing_skill_ids:
            target.skills.append(skill)

    existing_tech_names = {t.name.lower() for t in target.technologies}
    for tech in source.technologies:
        if tech.name.lower() not in existing_tech_names:
            target.technologies.append(tech)
            existing_tech_names.add(tech.name.lower())

    if source.profile.summary and not target.profile.summary:
        target.profile.summary = source.profile.summary
    if source.profile.email and not target.profile.email:
        target.profile.email = source.profile.email

    return target


@tool
def collect_github(username: str) -> str:
    """Collect professional data from a GitHub profile."""
    try:
        collector = GitHubCollector()
        result = collector.collect(username)
        return f"Collected {len(result.projects)} projects, {len(result.skills)} skills from GitHub"
    except Exception as e:
        return f"GitHub collection failed: {e}"


@tool
def collect_website(url: str) -> str:
    """Parse a personal website for professional data."""
    try:
        collector = WebsiteCollector()
        result = collector.collect(url)
        return f"Collected {len(result.projects)} projects from website"
    except Exception as e:
        return f"Website collection failed: {e}"


@tool
def parse_resume(file_path: str) -> str:
    """Parse an existing CV/resume file (PDF, DOCX, TXT)."""
    try:
        parser = ResumeParser()
        result = parser.parse(file_path)
        return f"Parsed resume: {len(result.experience)} experiences, {len(result.skills)} skills"
    except Exception as e:
        return f"Resume parsing failed: {e}"


@tool
def check_ats_compatibility() -> str:
    """Check the current career graph for ATS compatibility."""
    return "ATS check requested — run after data collection"


class CareerTwinOrchestrator:
    """LangGraph-based orchestrator for CareerTwin agents."""

    def __init__(self):
        self.graph = CareerGraph(
            profile={"full_name": "", "summary": ""},
        )
        self.ats_checker = ATSChecker()
        self._build_graph()

    def _build_graph(self):
        workflow = StateGraph(CareerState)

        workflow.add_node("collector", self._collector_node)
        workflow.add_node("merger", self._merger_node)
        workflow.add_node("ats_checker", self._ats_node)
        workflow.add_node("adviser", self._adviser_node)

        workflow.set_entry_point("collector")
        workflow.add_edge("collector", "merger")
        workflow.add_edge("merger", "ats_checker")
        workflow.add_edge("ats_checker", "adviser")
        workflow.add_edge("adviser", END)

        self.app = workflow.compile()

    def _collector_node(self, state: CareerState) -> dict:
        return {
            "messages": [SystemMessage(content="Data collection in progress...")],
        }

    def _merger_node(self, state: CareerState) -> dict:
        return {"collected_sources": state.get("collected_sources", [])}

    def _ats_node(self, state: CareerState) -> dict:
        graph = state.get("graph") or self.graph
        text = graph.to_search_text()
        report = self.ats_checker.check(graph, text)
        return {"ats_report": report}

    def _adviser_node(self, state: CareerState) -> dict:
        report = state.get("ats_report")
        if report:
            summary = report.summary()
        else:
            summary = "No ATS report available"
        return {
            "messages": [SystemMessage(content=f"Career analysis complete.\n{summary}")]
        }

    def run_collection(
        self, github: Optional[str] = None, resume_path: Optional[str] = None
    ) -> CareerGraph:
        if github:
            try:
                collector = GitHubCollector()
                gh_data = collector.collect(github)
                self.graph = _merge_graphs(self.graph, gh_data)
            except Exception as e:
                logger.warning("GitHub collection error: %s", e)

        if resume_path:
            try:
                parser = ResumeParser()
                cv_data = parser.parse(resume_path)
                self.graph = _merge_graphs(self.graph, cv_data)
            except Exception as e:
                logger.warning("Resume parsing error: %s", e)

        text = self.graph.to_search_text()
        report = self.ats_checker.check(self.graph, text)
        logger.info("ATS Report:\n%s", report.summary())

        return self.graph

    def check_ats(self) -> ATSReport:
        text = self.graph.to_search_text()
        return self.ats_checker.check(self.graph, text)
