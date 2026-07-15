"""CareerTwin API — resume routes."""

from __future__ import annotations

from typing import Any

from fastapi import APIRouter, File, Form, UploadFile

from config import LANGUAGE_LABELS, TEMPLATE_MAP
from graph_builder import _build_manual_graph, build_career_graph
from resume_service import discover_and_tailor_jobs, tailor_resume_package

router = APIRouter()


@router.post("/v1/resume/tailor")
async def tailor_resume(
    mode: str = Form("manual"),
    resume_file: UploadFile | None = File(None),
    fullName: str = Form(""),
    email: str = Form(""),
    phone: str = Form(""),
    location: str = Form(""),
    github: str = Form(""),
    linkedin: str = Form(""),
    website: str = Form(""),
    summary: str = Form(""),
    jobUrl: str = Form(""),
    jobTitle: str = Form(""),
    companyName: str = Form(""),
    jobDescription: str = Form(""),
    template: str = Form("clean-slate"),
    provider: str = Form("lmstudio"),
    model: str = Form(""),
    language: str = Form("auto"),
) -> dict[str, Any]:
    graph, build_diagnostics = await build_career_graph(
        mode=mode,
        resume_file=resume_file,
        full_name=fullName,
        email=email,
        phone=phone,
        location=location,
        github=github,
        linkedin=linkedin,
        website=website,
        summary_text=summary,
    )
    result = await tailor_resume_package(
        graph=graph,
        job_title=jobTitle,
        company_name=companyName,
        job_description=jobDescription,
        job_url=jobUrl,
        template=template,
        provider=provider,
        model=model,
        requested_language=language,
    )
    result["agent_diagnostics"] = {**build_diagnostics, **result.get("agent_diagnostics", {})}
    return result


@router.post("/v1/resume/auto-tailor")
async def auto_tailor_resume(
    mode: str = Form("manual"),
    resume_file: UploadFile | None = File(None),
    fullName: str = Form(""),
    email: str = Form(""),
    phone: str = Form(""),
    location: str = Form(""),
    github: str = Form(""),
    linkedin: str = Form(""),
    website: str = Form(""),
    summary: str = Form(""),
    template: str = Form("clean-slate"),
    provider: str = Form("lmstudio"),
    model: str = Form(""),
    language: str = Form("auto"),
    limit: int = Form(3),
    sources: str = Form("linkedin,justjoin,nofluffjobs,remoteok"),
) -> dict[str, Any]:
    graph, build_diagnostics = await build_career_graph(
        mode=mode,
        resume_file=resume_file,
        full_name=fullName,
        email=email,
        phone=phone,
        location=location,
        github=github,
        linkedin=linkedin,
        website=website,
        summary_text=summary,
    )
    search_sources = [item.strip().lower() for item in sources.split(",") if item.strip()]
    result = await discover_and_tailor_jobs(
        graph=graph,
        provider=provider,
        model=model,
        template=template,
        requested_language=language,
        limit=max(1, min(limit, 5)),
        sources=search_sources,
    )
    result["agent_diagnostics"] = {
        **build_diagnostics,
        **result.get("agent_diagnostics", {}),
    }
    return result


@router.post("/v1/resume/generate")
async def generate_resume_legacy(
    payload: dict[str, Any],
) -> dict[str, Any]:
    graph = _build_manual_graph(
        full_name=payload.get("fullName", ""),
        email=payload.get("email", ""),
        phone=payload.get("phone", ""),
        location=payload.get("location", ""),
        github=payload.get("github", ""),
        linkedin=payload.get("linkedin", ""),
        website=payload.get("website", ""),
        summary_text=payload.get("summary", "") or payload.get("notes", ""),
    )
    return await tailor_resume_package(
        graph=graph,
        job_title=payload.get("jobTitle", ""),
        company_name=payload.get("companyName", ""),
        job_description=payload.get("jobDescription", ""),
        job_url=payload.get("jobUrl", ""),
        template=payload.get("template", "clean-slate"),
        provider=payload.get("provider", "lmstudio"),
        model=payload.get("model", ""),
        requested_language=payload.get("language", "auto"),
    )


@router.get("/v1/templates")
async def list_templates() -> dict[str, Any]:
    return {
        "templates": [
            {"id": "clean-slate", "name": "Clean Slate"},
            {"id": "graphite", "name": "Graphite"},
            {"id": "signal", "name": "Signal"},
            {"id": "north", "name": "North"},
            {"id": "baseline", "name": "Baseline"},
        ]
    }


@router.get("/v1/languages")
async def list_languages() -> dict[str, Any]:
    return {
        "languages": [
            {"code": code, "name": label}
            for code, label in LANGUAGE_LABELS.items()
        ]
    }
