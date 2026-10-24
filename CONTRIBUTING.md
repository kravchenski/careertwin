# Contributing to CareerTwin AI

## Getting Started

1. Fork the repository.
2. Create a branch: `git checkout -b feature/your-feature`
3. Install dependencies: `uv sync --extra dev --extra rendering`
4. Make your changes.
5. Run tests: `uv run pytest tests/ -m "not integration"`
6. Run linting: `uv run ruff check src/ tests/`
7. Commit: `git commit -m "feat: your change"`
8. Push: `git push origin feature/your-feature`
9. Open a Pull Request.

## Development

```bash
make api       # FastAPI backend with reload on http://127.0.0.1:8000
make test      # unit tests
make lint      # ruff
make docker    # backend + SearXNG in containers
```

## Code Style

- Python: PEP 8, type hints required, formatted by Ruff
- Absolute imports from the `careertwin` package; no `sys.path` changes
- No inline comments in source files
- Conventional commits: `feat:`, `fix:`, `refactor:`, `chore:`, `docs:`, `test:`

## Project Structure

All code lives in the `careertwin` package under `src/`:

- `api/` — FastAPI app (`app.py`), request models (`schemas.py`), one module per feature in `routes/`
- `automation/` — four-agent application workflow: search and rank, prepare documents, apply, track
- `jobs/` — job discovery: ATS adapters (`boards/`), scanners, career-page scrapers, `data/companies.json`
- `profile/` — candidate context (`ABOUT.md`), career graph building, profile names
- `services/` — resume tailoring and cover-letter service used by the API
- `agents/`, `ats/`, `collectors/`, `graph/`, `rendering/`, `storage/`, `templates/`, `vision/` — the document-building library
- `settings.py`, `db.py`, `llm.py`, `browser.py`, `net.py` — configuration, SQLite, LLM providers, agent-browser helpers, URL safety
- `cli.py` — the `careertwin` command
