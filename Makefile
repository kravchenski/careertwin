.PHONY: all dev dev-backend dev-frontend dev-docker test lint build clean

all: dev

dev:
	@bash start.sh

dev-backend:
	cd web/api && uv run uvicorn main:app --reload --host 0.0.0.0 --port 8000

dev-frontend:
	cd web/frontend && npm run start -- --host 0.0.0.0 --port 4200

dev-docker:
	docker compose up --build

test:
	uv run pytest tests/ -v --tb=short

lint:
	uv run ruff check src/ tests/ web/api/

lint-fix:
	uv run ruff check --fix src/ tests/ web/api/

typecheck:
	uv run mypy src/ web/api/ --ignore-missing-imports || true

build:
	docker compose build

clean:
	rm -rf web/frontend/node_modules web/frontend/.angular
	find . -type d -name __pycache__ -exec rm -rf {} +
	find . -type d -name .ruff_cache -exec rm -rf {} +
	find . -type d -name .pytest_cache -exec rm -rf {} +
