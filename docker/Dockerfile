# syntax=docker/dockerfile:1

# ── Stage 1: Python package builder ───────────────────────────────────────────
# Compiles packages that need build-essential (e.g. lxml, weasyprint deps).
# The compiled venv is then copied into the lean runtime image.
FROM python:3.14-slim AS python-deps

RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential \
        libffi-dev \
        libcairo2-dev \
        libpango1.0-dev \
        libgdk-pixbuf-2.0-dev \
    && rm -rf /var/lib/apt/lists/*

RUN pip install --no-cache-dir uv

COPY pyproject.toml uv.lock README.md /build/
WORKDIR /build

RUN uv venv /opt/venv && \
    UV_PROJECT_ENVIRONMENT=/opt/venv uv sync \
        --frozen \
        --extra rendering \
        --no-dev \
        --no-install-project

# ── Stage 2: Runtime ──────────────────────────────────────────────────────────
FROM python:3.14-slim AS runtime

# System runtime libs — no build tools
RUN apt-get update && apt-get install -y --no-install-recommends \
        # WeasyPrint PDF renderer
        libpango-1.0-0 libpangocairo-1.0-0 libgdk-pixbuf-2.0-0 \
        libcairo2 libgobject-2.0-0 shared-mime-info \
        # pdf2image
        poppler-utils \
        # Process manager
        supervisor \
        # Bootstrap script for Node.js setup
        curl \
        # PDF fonts
        fonts-liberation \
        # Chromium runtime (agent-browser)
        libasound2 libatk-bridge2.0-0 libatk1.0-0 libcups2 \
        libdrm2 libgbm1 libgtk-3-0 libnspr4 libnss3 \
        libxcomposite1 libxdamage1 libxrandr2 xdg-utils \
        # Headless display stack
        xvfb x11vnc novnc websockify \
    && rm -rf /var/lib/apt/lists/*

# Node.js 22 LTS
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && rm -rf /var/lib/apt/lists/*

# agent-browser CLI (provides headless Chrome control)
RUN npm install -g agent-browser && npm cache clean --force

# Playwright Chromium (best-effort; failures are non-fatal)
RUN npx playwright install chromium --with-deps 2>/dev/null || \
    ( npx playwright install-deps chromium 2>/dev/null && \
      npx playwright install chromium 2>/dev/null ) || true

# ── Copy compiled Python environment from build stage ─────────────────────────
COPY --from=python-deps /opt/venv /opt/venv

# crawl4ai's own Playwright browser install (best-effort; failures are non-fatal)
RUN /opt/venv/bin/crawl4ai-setup 2>/dev/null || true

WORKDIR /app
COPY src/ src/
COPY docker/supervisord.conf /etc/supervisor/careertwin.conf

RUN mkdir -p data /tmp/.X11-unix /tmp/runtime-root \
    && chmod 700 /tmp/runtime-root

ENV PATH="/opt/venv/bin:$PATH" \
    PYTHONPATH="/app/src" \
    PYTHONUNBUFFERED=1 \
    AGENT_BROWSER_BIN=/usr/bin/agent-browser \
    CHROME_FLAGS="--no-sandbox --disable-gpu --disable-dev-shm-usage --disable-setuid-sandbox" \
    DISPLAY=:13 \
    XDG_RUNTIME_DIR=/tmp/runtime-root

EXPOSE 8000 6080

CMD ["supervisord", "-c", "/etc/supervisor/careertwin.conf"]
