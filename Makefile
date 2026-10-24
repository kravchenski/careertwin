.PHONY: dev api mcp agent interview install uninstall windows macos test lint lint-fix docker clean

dev: api

api:
	uv run uvicorn careertwin.api.app:app --reload --host 127.0.0.1 --port 8000

mcp:
	uv run careertwin mcp --transport http --port 8765

agent:
	uv run careertwin agent

interview:
	uv run --extra interview careertwin-interview

install:
	./scripts/install.sh

uninstall:
	./scripts/uninstall.sh

windows:
	./scripts/build_windows.sh

macos:
	./scripts/build_macos.sh

test:
	uv run pytest tests/ -v --tb=short -m "not integration"

lint:
	uv run ruff check src/ tests/

lint-fix:
	uv run ruff check --fix src/ tests/

docker:
	docker compose up --build

clean:
	find . -type d -name __pycache__ -not -path "./.venv/*" -exec rm -rf {} +
	rm -rf .ruff_cache .pytest_cache
