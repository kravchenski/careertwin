"""Generated documents, served only to a caller with the token and only from the outputs folder."""

from __future__ import annotations

from pathlib import Path, PurePosixPath

from fastapi import APIRouter, HTTPException
from fastapi.responses import FileResponse

from careertwin.settings import OUTPUT_DIR

router = APIRouter(tags=["files"])

SERVED = {
    ".pdf": "application/pdf",
    ".docx": "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    ".xlsx": "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    ".html": "application/octet-stream",  # downloaded, never rendered next to the API
    ".txt": "text/plain; charset=utf-8",
    ".md": "text/plain; charset=utf-8",
}
# Kept on disk for the audit trail, never handed out: the profile snapshot is the whole ABOUT.md.
WITHHELD = {"profile-snapshot.md"}


def locate(path: str, root: Path | None = None) -> Path:
    """The file under the outputs folder that `path` names, or HTTPException(404)."""
    base = (root or OUTPUT_DIR).resolve()
    parts = PurePosixPath(path).parts
    if not parts or "\\" in path or "\0" in path or any(part in {"..", "."} or part.startswith(".") for part in parts):
        raise HTTPException(404, "Not found")
    target = (base / Path(*parts)).resolve()
    if not target.is_relative_to(base) or not target.is_file():
        raise HTTPException(404, "Not found")
    if target.suffix.lower() not in SERVED or target.name in WITHHELD:
        raise HTTPException(404, "Not found")
    return target


@router.get("/outputs/{path:path}")
async def download(path: str) -> FileResponse:
    target = locate(path)
    return FileResponse(target, media_type=SERVED[target.suffix.lower()], filename=target.name)
