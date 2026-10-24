"""CareerTwin API — FastAPI application entry point."""

from __future__ import annotations

import logging
from contextlib import asynccontextmanager
from typing import Any

from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware

from careertwin.api.routes import (
    applications,
    automation,
    boards,
    companies,
    files,
    interview,
    jobs,
    profile,
    providers,
    resume,
    studio,
)
from careertwin.api.security import SecurityMiddleware, cors_origins, docs_enabled, server_token, token_file
from careertwin.db import db_conn, init_db, migrate_db
from careertwin.vault import install_log_redaction, private_umask

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    datefmt="%Y-%m-%d %H:%M:%S",
)
install_log_redaction()
private_umask()
logger = logging.getLogger("careertwin.api")


@asynccontextmanager
async def lifespan(app: FastAPI):
    server_token()  # creates data/api-token on the first start
    logger.info("API token is in %s (or CAREERTWIN_API_TOKEN)", token_file())
    init_db()
    migrate_db()
    from careertwin.jobs.companies import save_known_companies_to_db
    save_known_companies_to_db()
    automation.engine.start()
    try:
        yield
    finally:
        await automation.engine.close()


# The interactive docs are off unless CAREERTWIN_API_DOCS=1: they cannot send the token.
_DOCS = {} if docs_enabled() else {"docs_url": None, "redoc_url": None, "openapi_url": None}
app = FastAPI(title="CareerTwin API", version="2.0.0", lifespan=lifespan, **_DOCS)

app.add_middleware(SecurityMiddleware)
app.add_middleware(
    CORSMiddleware,
    allow_origins=cors_origins(),
    allow_credentials=False,
    allow_methods=["GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"],
    allow_headers=["Content-Type", "Authorization"],
)

app.include_router(files.router)
app.include_router(profile.router)
app.include_router(resume.router)
app.include_router(jobs.router)
app.include_router(providers.router)
app.include_router(studio.router)
app.include_router(companies.router)
app.include_router(boards.router)
app.include_router(applications.router)
app.include_router(automation.router)
app.include_router(interview.router)


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
