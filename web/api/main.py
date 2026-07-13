"""CareerTwin API — FastAPI application entry point."""

from __future__ import annotations

import logging
import os
import sys
from contextlib import asynccontextmanager
from pathlib import Path
from typing import Any

import config

from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from fastapi.staticfiles import StaticFiles

from config import OUTPUT_DIR
from database import _init_db, _migrate_db
from routes import profile, resume, jobs, providers, studio, companies, jobs_mcp

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    datefmt="%Y-%m-%d %H:%M:%S",
)
logger = logging.getLogger("careertwin.api")


@asynccontextmanager
async def lifespan(app: FastAPI):
    _init_db()
    _migrate_db()
    from company_collector import save_known_companies_to_db
    save_known_companies_to_db()
    yield


app = FastAPI(title="CareerTwin API", version="2.0.0", lifespan=lifespan)

_CORS_ORIGINS = [
    o.strip()
    for o in os.getenv(
        "CORS_ORIGINS",
        "http://localhost:4200,http://localhost:8080",
    ).split(",")
    if o.strip()
]

app.add_middleware(
    CORSMiddleware,
    allow_origins=_CORS_ORIGINS,
    allow_credentials=False,
    allow_methods=["GET", "POST", "PATCH", "DELETE", "OPTIONS"],
    allow_headers=["Content-Type", "Authorization"],
)

app.mount("/outputs", StaticFiles(directory=str(OUTPUT_DIR)), name="outputs")
app.mount(
    "/screenshots",
    StaticFiles(directory=str(Path(__file__).resolve().parent / "screenshots")),
    name="screenshots",
)

app.include_router(profile.router)
app.include_router(resume.router)
app.include_router(jobs.router)
app.include_router(providers.router)
app.include_router(studio.router)
app.include_router(companies.router)
app.include_router(jobs_mcp.router)


@app.get("/")
async def root() -> dict[str, Any]:
    return {"name": "CareerTwin API", "version": "2.0.0", "status": "ok"}


@app.get("/v1/health")
async def health() -> dict[str, str]:
    return {"status": "ok"}
