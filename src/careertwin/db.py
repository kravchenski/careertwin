"""SQLite connection, schema creation and migrations."""

from __future__ import annotations

import contextlib
import json
import sqlite3
from typing import Any

from careertwin.settings import DB_PATH


def init_db() -> None:
    with contextlib.closing(sqlite3.connect(str(DB_PATH))) as conn:
        conn.execute("""
            CREATE TABLE IF NOT EXISTS user_profiles (
                id TEXT PRIMARY KEY,
                username TEXT UNIQUE NOT NULL,
                full_name TEXT,
                email TEXT,
                phone TEXT,
                location TEXT,
                github TEXT,
                linkedin TEXT,
                website TEXT,
                summary TEXT,
                skills TEXT DEFAULT '[]',
                created_at TEXT DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT DEFAULT CURRENT_TIMESTAMP
            )
        """)
        conn.execute("""
            CREATE TABLE IF NOT EXISTS studio_sessions (
                id TEXT PRIMARY KEY,
                username TEXT UNIQUE NOT NULL,
                first_name TEXT,
                last_name TEXT,
                linkedin_url TEXT,
                linkedin_email TEXT,
                github TEXT,
                website TEXT,
                preferences TEXT DEFAULT '{}',
                profile TEXT DEFAULT '{}',
                screenshots TEXT DEFAULT '[]',
                completed_at TEXT,
                created_at TEXT DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT DEFAULT CURRENT_TIMESTAMP
            )
        """)
        conn.execute("""
            CREATE TABLE IF NOT EXISTS companies (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT UNIQUE NOT NULL,
                description TEXT DEFAULT '',
                industry TEXT DEFAULT '',
                employee_count TEXT DEFAULT '',
                website TEXT DEFAULT '',
                careers TEXT DEFAULT '',
                internships TEXT DEFAULT '',
                certifications TEXT DEFAULT '',
                learning TEXT DEFAULT '',
                github TEXT DEFAULT '',
                linkedin TEXT DEFAULT '',
                glassdoor TEXT DEFAULT '',
                wellfound TEXT DEFAULT '',
                linkedin_raw TEXT DEFAULT '{}',
                ats_platform TEXT DEFAULT '',
                cached_at TEXT DEFAULT CURRENT_TIMESTAMP,
                created_at TEXT DEFAULT CURRENT_TIMESTAMP
            )
        """)
        conn.execute("""
            CREATE TABLE IF NOT EXISTS collected_jobs (
                id TEXT PRIMARY KEY,
                source TEXT,
                url TEXT,
                title TEXT,
                company TEXT,
                location TEXT,
                description TEXT,
                match_score REAL DEFAULT 0,
                matching_skills TEXT DEFAULT '[]',
                missing_skills TEXT DEFAULT '[]',
                collected_at TEXT,
                status TEXT DEFAULT 'new'
            )
        """)
        conn.execute("""
            CREATE TABLE IF NOT EXISTS discovered_job_apis (
                company TEXT PRIMARY KEY,
                url TEXT NOT NULL,
                method TEXT NOT NULL,
                request_headers TEXT NOT NULL DEFAULT '{}',
                post_data TEXT,
                jobs_path TEXT NOT NULL DEFAULT '[]',
                requires_session INTEGER NOT NULL DEFAULT 0,
                fail_count INTEGER NOT NULL DEFAULT 0,
                discovered_at TEXT DEFAULT CURRENT_TIMESTAMP,
                last_used_at TEXT DEFAULT CURRENT_TIMESTAMP
            )
        """)
        conn.execute("""
            CREATE TABLE IF NOT EXISTS job_applications (
                id TEXT PRIMARY KEY,
                username TEXT NOT NULL,
                company TEXT NOT NULL,
                position TEXT NOT NULL,
                domain TEXT DEFAULT '',
                job_url TEXT DEFAULT '',
                status TEXT NOT NULL DEFAULT 'applied',
                applied_at TEXT,
                phone_screen_at TEXT,
                technical_at TEXT,
                offer_at TEXT,
                notes TEXT NOT NULL DEFAULT '[]',
                created_at TEXT DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT DEFAULT CURRENT_TIMESTAMP
            )
        """)
        conn.commit()


def migrate_db() -> None:
    """Add missing columns to existing tables (atomic — single commit)."""
    with contextlib.closing(sqlite3.connect(str(DB_PATH))) as conn:
        cols = {row[1] for row in conn.execute("PRAGMA table_info(studio_sessions)").fetchall()}
        needed = []
        if "screenshots" not in cols:
            needed.append("ALTER TABLE studio_sessions ADD COLUMN screenshots TEXT DEFAULT '[]'")
        if "completed_at" not in cols:
            needed.append("ALTER TABLE studio_sessions ADD COLUMN completed_at TEXT")
        if "first_name" not in cols:
            needed.append("ALTER TABLE studio_sessions ADD COLUMN first_name TEXT")
        if "last_name" not in cols:
            needed.append("ALTER TABLE studio_sessions ADD COLUMN last_name TEXT")
        for stmt in needed:
            conn.execute(stmt)

        comp_cols = {row[1] for row in conn.execute("PRAGMA table_info(companies)").fetchall()}
        if "ats_platform" not in comp_cols:
            conn.execute("ALTER TABLE companies ADD COLUMN ats_platform TEXT DEFAULT ''")

        if needed or "ats_platform" not in comp_cols:
            conn.commit()




def db_conn() -> contextlib.AbstractContextManager[sqlite3.Connection]:
    return contextlib.closing(sqlite3.connect(str(DB_PATH)))


def row_to_dict(row: sqlite3.Row | None) -> dict[str, Any] | None:
    if row is None:
        return None
    data = dict(row)
    if "skills" in data and isinstance(data["skills"], str):
        try:
            data["skills"] = json.loads(data["skills"])
        except (json.JSONDecodeError, TypeError):
            data["skills"] = []
    for json_col in ("preferences", "profile", "screenshots"):
        if json_col in data and isinstance(data[json_col], str):
            try:
                data[json_col] = json.loads(data[json_col])
            except (json.JSONDecodeError, TypeError):
                data[json_col] = {} if json_col != "screenshots" else []
    return data
