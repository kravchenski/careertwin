# Contributing to CareerTwin AI

## Getting Started

1. Fork the repository.
2. Create a branch: `git checkout -b feature/your-feature`
3. Install dependencies: `uv sync`
4. Make your changes.
5. Run tests: `uv run pytest tests/ -v`
6. Run linting: `uv run ruff check src/ tests/ web/api/`
7. Commit: `git commit -m "feat: your change"`
8. Push: `git push origin feature/your-feature`
9. Open a Pull Request.

## Development

```bash
# Start both services locally
make dev

# Start backend API only
make dev-backend

# Start frontend only
make dev-frontend

# Run with Docker (production mode)
make dev-docker

# Docker with hot reload
docker compose -f docker-compose.yml -f docker-compose.override.yml up
```

## Code Style

- Python: PEP 8, type hints required, formatted by Ruff
- TypeScript/HTML: Angular style guide, Tailwind utility classes
- No inline comments in source files
- Conventional commits: `feat:`, `fix:`, `refactor:`, `chore:`, `docs:`, `test:`

## Project Structure

- `web/api/` — FastAPI backend with per-feature route modules
- `web/frontend/` — Angular 19 SPA with standalone components
- `src/careertwin/agents/` — LangGraph agent definitions
- `src/careertwin/ats/` — ATS validation logic
