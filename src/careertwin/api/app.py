"""CareerTwin API — FastAPI application entry point."""

from __future__ import annotations

import logging
import os
from contextlib import asynccontextmanager
from typing import Any

from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from fastapi.staticfiles import StaticFiles

from careertwin.api.routes import (
    applications,
    automation,
    boards,
    companies,
    jobs,
    profile,
    providers,
    resume,
    studio,
)
from careertwin.db import db_conn, init_db, migrate_db
from careertwin.settings import OUTPUT_DIR, SCREENSHOT_DIR

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    datefmt="%Y-%m-%d %H:%M:%S",
)
logger = logging.getLogger("careertwin.api")


@asynccontextmanager
async def lifespan(app: FastAPI):
    init_db()
    migrate_db()
    from careertwin.jobs.companies import save_known_companies_to_db
    save_known_companies_to_db()
    automation.engine.start()
    try:
        yield
    finally:
        await automation.engine.close()


app = FastAPI(title="CareerTwin API", version="2.0.0", lifespan=lifespan)

_CORS_ORIGINS = [
    o.strip()
    for o in os.getenv(
        "CORS_ORIGINS",
        "",
    ).split(",")
    if o.strip()
]

app.add_middleware(
    CORSMiddleware,
    allow_origins=_CORS_ORIGINS,
    allow_credentials=False,
    allow_methods=["GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"],
    allow_headers=["Content-Type", "Authorization"],
)

app.mount("/outputs", StaticFiles(directory=str(OUTPUT_DIR)), name="outputs")
app.mount("/screenshots", StaticFiles(directory=str(SCREENSHOT_DIR)), name="screenshots")

app.include_router(profile.router)
app.include_router(resume.router)
app.include_router(jobs.router)
app.include_router(providers.router)
app.include_router(studio.router)
app.include_router(companies.router)
app.include_router(boards.router)
app.include_router(applications.router)
app.include_router(automation.router)


@app.get("/")
async def root() -> dict[str, Any]:
    return {"name": "CareerTwin API", "version": "2.0.0", "status": "ok"}


@app.get("/v1/health")
async def health() -> dict[str, Any]:
    db_ok = False
    try:
        with db_conn() as conn:
            conn.execute("SELECT 1")
            db_ok = True
    except Exception:
        pass
    return {"status": "ok", "database": "connected" if db_ok else "disconnected", "version": "2.0.0"}
