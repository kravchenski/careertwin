"""CareerTwin API — company routes."""

from __future__ import annotations

import logging
from typing import Any

from fastapi import APIRouter

from careertwin.api.schemas import CompanyCollectRequest, CompanySearchRequest
from careertwin.jobs.companies import (
    collect_companies,
    extract_company_urls,
    get_all_companies,
    get_company_by_name,
    save_company_to_db,
    save_known_companies_to_db,
    search_companies,
)

logger = logging.getLogger("careertwin.api")

router = APIRouter()


@router.post("/v1/companies/search")
async def api_search_companies(req: CompanySearchRequest) -> dict[str, Any]:
    """Search LinkedIn for companies using agent-browser."""
    companies = search_companies(req.query, limit=req.limit)
    return {
        "success": True,
        "query": req.query,
        "total": len(companies),
        "companies": companies,
    }


@router.post("/v1/companies/collect")
async def api_collect_companies(req: CompanyCollectRequest) -> dict[str, Any]:
    """Auto-collect IT companies from LinkedIn, extract URLs, save to DB."""
    result = collect_companies(query=req.query, limit=req.limit)
    return {
        "success": True,
        **result,
    }


@router.get("/v1/companies")
async def api_list_companies() -> dict[str, Any]:
    """List all companies from the database."""
    companies = get_all_companies()
    return {
        "success": True,
        "total": len(companies),
        "companies": companies,
    }


@router.post("/v1/companies/add")
async def api_add_company(body: dict[str, Any]) -> dict[str, Any]:
    """Add a company manually with URL extraction."""
    name = body.get("name", "").strip()
    if not name:
        return {"success": False, "error": "Company name is required"}

    existing = get_company_by_name(name)
    if existing:
        return {"success": True, "company": existing, "cached": True}

    linkedin_url = body.get("linkedin_url", "")
    urls = extract_company_urls(name, linkedin_url)

    company_data = {
        "name": name,
        "description": body.get("description", ""),
        "industry": body.get("industry", ""),
        "employee_count": body.get("employee_count", ""),
        **urls,
    }

    cid = save_company_to_db(company_data)
    if cid:
        company_data["id"] = cid
        return {"success": True, "company": company_data, "cached": False}

    return {"success": False, "error": "Failed to save company"}


@router.get("/v1/companies/{company_name}")
async def api_get_company(company_name: str) -> dict[str, Any]:
    """Get a single company by name."""
    company = get_company_by_name(company_name)
    if company:
        return {"success": True, "company": company}
    return {"success": False, "error": "Company not found"}


@router.post("/v1/companies/sync")
async def api_sync_known_companies() -> dict[str, Any]:
    """Sync companies.json into the database."""
    saved = save_known_companies_to_db()
    companies = get_all_companies()
    return {
        "success": True,
        "newly_synced": saved,
        "total": len(companies),
    }
