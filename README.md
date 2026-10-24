<div align="center">

# CareerTwin AI

**Your AI-powered career operating system.**

Not a resume builder. Not a template gallery.  
An intelligent agent that learns your professional DNA,  
remembers everything, and builds career documents that actually work.

[![Python](https://img.shields.io/badge/Python-3.14+-3776AB?style=flat&logo=python&logoColor=white)](https://www.python.org/)
[![LangGraph](https://img.shields.io/badge/LangGraph-orchestration-blue)](https://langchain-ai.github.io/langgraph/)
[![License](https://img.shields.io/badge/License-Apache%202.0-green)](LICENSE)

</div>

---

## The Problem

Every "AI resume builder" does the same thing: you paste your resume, it rewrites it with buzzwords. That's not intelligence — that's a mail merge.

**The real problem:** your professional life is scattered across GitHub, LinkedIn, personal websites, portfolios, and your memory. No tool stitches it together.

## The Solution

CareerTwin AI builds a **Career Knowledge Graph** — a structured, queryable representation of everything you've ever done professionally. Then it uses that graph to generate tailored, visually polished, ATS-optimized documents for every opportunity.

```
Your scattered data     →    Career Knowledge Graph    →    Perfect CV
GitHub + LinkedIn +           Skills, projects,              Tailored to
Website + Portfolio           achievements, evidence         each job
```

---

## How It Works

### 1. Data Collection

CareerTwin connects to your professional profiles and extracts structured data:

| Source | What it extracts |
|---|---|
| **GitHub** | Repositories, languages, commits, stars, contribution graph |
| **LinkedIn** | Experience, skills, recommendations, certificates |
| **Personal Website** | Projects, blog posts, case studies, tech stack |
| **Portfolio** | Behance, Dribbble, HuggingFace, Kaggle projects |

### 2. Career Knowledge Graph

Not a flat text file. A structured graph with relationships:

```
Person
  └── Experience
        └── Project
              └── Technology
                    └── Achievement
                          └── Evidence
```

Real example:

```
Python
  └── used_in: AI Video Pipeline
        └── evidence: github.com/user/ai-pipeline
              └── achievement: 2.4k stars, 50k downloads
```

### 3. Intelligent Resume Assembly

```
Career Graph + Job Description
        ↓
Analyze relevance per experience
        ↓
Rank and select strongest evidence
        ↓
Assemble with Design DNA
        ↓
Render PDF/DOCX
```

### 4. Visual Quality Assurance

The system doesn't just generate text — it **looks at** the result:

1. **PDF → Image** — renders each page as a visual
2. **Vision Model** — analyzes layout, spacing, density, overflow
3. **Score & Issues** — `layout_score: 95`, issues list
4. **Repair Agent** — fixes problems automatically
5. **Re-render & Re-check** — loop until approved

---

## Architecture

```
                        CareerTwin AI
                              │
                 ┌────────────┴────────────┐
                 │   Personal Intelligence  │
                 │         Layer            │
                 └────────────┬────────────┘
                              │
            ┌─────────────────┼─────────────────┐
            │                 │                 │
         GitHub           LinkedIn          Website
            │                 │                 │
            └─────────────────┼─────────────────┘
                              │
                   Career Knowledge Graph
                              │
                         LangGraph
                              │
        ┌─────────┬───────────┼───────────┬─────────┐
        │         │           │           │         │
     Job Agent  Resume    ATS Agent   Design    Vision QA
               Builder                  Agent     Agent
        │         │           │           │         │
        └─────────┴───────────┼───────────┴─────────┘
                              │
                     PDF/DOCX Renderer
                              │
                        Final Document
```

### Agents

| Agent | Responsibility |
|---|---|
| **Personal Data Agent** | Collects data from all sources, builds the Knowledge Graph |
| **Resume Agent** | Selects relevant experience, assembles CV per job |
| **ATS Agent** | Validates against Applicant Tracking Systems |
| **Design Agent** | Applies design DNA from trend research |
| **Vision QA Agent** | Visually inspects rendered output |
| **Repair Agent** | Fixes issues found by Vision QA |
| **Cover Letter Agent** | Generates tailored cover letters |
| **Job Agent** | Matches opportunities to your profile |

---

## Design DNA

CareerTwin doesn't copy templates. It learns design principles:

```json
{
  "font": "Inter",
  "layout": "two-column",
  "spacing": "12px",
  "style": "minimal-modern",
  "accent_color": "#2563EB"
}
```

Sources analyzed: Dribbble, Behance, Canva, Resume.io, Overleaf templates.

---

## Roadmap

### Phase 1 — Foundation ✅
- [x] GitHub data collector
- [x] LinkedIn data collector  
- [x] Personal website parser
- [x] CV/resume parser
- [x] Career Knowledge Graph schema
- [x] Qdrant vector store integration
- [x] LangGraph agent orchestration
- [x] ATS compatibility checker

### Phase 2 — Intelligence ✅
- [x] Auto Resume Builder (per-job adaptation)
- [x] Cover Letter Generator
- [x] Job matching agent
- [x] Skill gap analysis
- [x] Career trajectory recommendations

### Phase 3 — Visual Quality ✅
- [x] PDF rendering pipeline
- [x] Vision QA with Qwen-VL / LLaVA
- [x] Design trend scraper (Dribbble, Behance)
- [x] Design DNA engine
- [x] Repair Agent loop
- [x] Multi-template support

### Phase 4 — Platform ✅
- [x] FastAPI backend with REST API
- [x] Angular 19 web dashboard
- [x] Job Search MCP integration (Greenhouse, Lever, Ashby, SmartRecruiters, Workday)
- [x] Application tracking workflow
- [x] Multi-language CV support

---

## Tech Stack

| Component | Technology |
|---|---|---|
| Backend | Python 3.14+, FastAPI |
| Frontend | Angular 19, Tailwind CSS v4 |
| Data Models | Pydantic v2 |
| Agent Framework | LangGraph |
| Vector Database | Qdrant |
| HTTP Client | httpx |
| Web Scraping | BeautifulSoup4 + lxml |
| PDF Parsing | pdfminer.six |
| DOCX Parsing | python-docx |
| GitHub API | PyGithub |
| MCP Adapters | Greenhouse, Lever, Ashby, SmartRecruiters, Workday |
| Containerization | Docker Compose |
| Package Manager | uv (Python), npm (frontend) |

---

## Getting Started

### Four-agent job search

Open **Agents** at `/agents` to run the application workflow:

1. **Search and rank:** Remote OK and Remotive public feeds, indexed results from
   the 25 remote boards plus LinkedIn, Glassdoor and ZipRecruiter, and company
   career boards through the existing ATS adapters. Import a link and its full
   description when a source requires login or is not indexed. The decision model evaluates
   skills, experience, preferences, and explicit eligibility restrictions.
2. **Prepare documents:** a text model selects evidence from your profile. The
   agent renders a text-extractable, single-column PDF of at most two pages and
   saves the cover letter, evidence, vacancy and profile snapshot. Claims and
   experience dates come from the source facts.
3. **Apply:** the browser agent uses Switchyard MCP in your signed-in browser,
   uploads the verified PDF, records its answers and checks the confirmation
   page after submitting. The decision model checks proposed answers against your profile.
   Missing answers, login and CAPTCHA pause the application for your input.
4. **Track:** SQLite keeps each operation and its exact answers. The fourth
   agent automatically updates `data/outputs/CareerTwin-applications.xlsx`.
   Download a fresh export in the UI, and record interviews, offers, hiring,
   rejection or withdrawal in the application details.

```bash
cp .env.example .env
cp ABOUT.example.md ABOUT.md  # only for a new profile; preserve your existing ABOUT.md
# Fill ABOUT.md with your real context.
# Run the Switchyard gateway and connect its browser extension.
./start.sh
```

Visit `http://localhost:4200/agents`. Your private `ABOUT.md` seeds the initial
settings and is ignored by Git. Later edits are saved in the Agents settings.
For index searches, run SearXNG at `SEARXNG_URL`, or configure `TAVILY_API_KEY`.
Company entries accept the ATS board identifier as `name` and the career page
as `url`; `ats_platform` can be `greenhouse`, `lever`, `ashby`,
`smartrecruiters` or `workday` through the settings API.

Decisions default to connected cloud models through Switchyard's `/v1/decisions`
API. Set **Decision model** in Agents to a model ID from Switchyard or `auto`.
Use **Load available models** to choose from the connected gateway. If automatic
routing runs out of available providers, select a specific working model.
The service evaluates finite choices; the four fit dimensions use five levels,
and their distributions produce a weighted fit score. Returned certainty is
an approximate model estimate, not calibrated accuracy or a hiring probability.
This uses hosted models without training or loading a model on your computer.

The [Unsloth decision model guide](https://unsloth.ai/docs/basics/train-your-own-decision-model-with-unsloth)
describes training an additional decision head and calibrating it on held-out
data. A prompt-based cloud model has a different implementation and does not
inherit those calibration guarantees.

Hosted Jev is optional: choose **TypeSafe Jev** in Agents settings. It uses the
native `/v1/systemone` API and requires `TYPESAFE_API_KEY`.
Text generation defaults to the Switchyard gateway (`SWITCHYARD_URL`,
`SWITCHYARD_MODEL`); set the same provider/model in Agents settings if you
change them. Browser tools use `SWITCHYARD_MCP_URL` and optional
`SWITCHYARD_API_KEY`. Site permissions are handled by the extension: open its
side panel on the application tab when it requests access.

Automatic CV preparation is enabled; automatic submission and scheduled searches
are off until enabled in settings. Explicit **Submit application** also starts
submission for a selected vacancy. A fit score is compatibility with documented
requirements, not a hiring probability. A CV cannot guarantee acceptance.
Scheduled searches require the backend to remain running. Use one backend worker
to serialize browser work. Restart recovery never repeats a potentially sent
application: **Check confirmation** reads the existing tab without submitting.
Close neither the application tab nor the browser while verification is pending.

The workbook contains applications, exact answers, full event history, document
paths with SHA256 hashes, saved per-application answers and complete snapshots.
Long values are split into numbered parts instead of silently truncated.
Files and SQLite data persist in `data/` locally or the Docker data volume.

API routes are under `/v1/automation`: settings, sources, status, run/stop,
vacancy import and actions, answers, outcomes, events and `export.xlsx`.

```bash
uv sync --extra dev --extra rendering
uv run pytest -m 'not integration'
cd web/frontend
npm ci
npm run build
```

```bash
# Clone the repo
git clone https://github.com/kravchenski/careertwin.git
cd careertwin

# Install dependencies
uv sync

# Set up environment (optional — public API used by default)
cp .env.example .env
# Edit .env and add your GITHUB_TOKEN for private repos and higher rate limits

# Collect from GitHub
uv run python main.py collect --github <username>

# Parse an existing resume
uv run python main.py collect --resume path/to/resume.pdf

# Check ATS compatibility
uv run python main.py check --resume path/to/resume.pdf

# View collected data
uv run python main.py info
```

---

## Running with Docker

The fastest way to run the full stack (Angular frontend + FastAPI backend + SearXNG) is Docker Compose.

```bash
# Copy the environment file and add your API keys
cp .env.example .env

# Build and start both services
docker compose up --build

# Or use the helper script
./start.sh --docker
```

- Frontend:  http://localhost:8080
- Backend API: http://localhost:8000
- SearXNG:    http://localhost:8888

The Nginx frontend proxies `/api/`, `/api/v1/`, `/outputs/`, and `/screenshots/` to the backend, so the SPA and API share one origin — no CORS or manual proxy config needed.

To run with a local model provider (LM Studio / Ollama), point the matching
`*_BASE_URL` variables at `host.docker.internal` in your `.env` (already the default).

### Development Mode

For hot-reload development, use the Docker override:

```bash
docker compose -f docker-compose.yml -f docker-compose.override.yml up --build
```

This enables backend hot-reload and exposes port `4200` for the Angular dev server with live reload.

Stop and remove containers:

```bash
docker compose down
```

Persist data (DB, generated outputs) in the `careertwin-data` named volume.

---

## Project Structure

```
careertwin/
├── web/
│   ├── api/              # FastAPI backend
│   │   ├── main.py          # Entry point
│   │   ├── routes/          # API routes (profile, jobs, resume, studio)
│   │   ├── jobs_mcp/        # MCP job adapters (GH, Lever, Ashby, SR, WD)
│   │   ├── services/        # Business logic layer
│   │   └── providers/       # LLM provider abstraction
│   └── frontend/         # Angular 19 SPA
│       └── src/app/pages/   # Dashboard, Jobs, Resume, Studio, Profile
├── src/
│   └── careertwin/
│       ├── agents/          # LangGraph agents (orchestrator, matcher, builder)
│       ├── ats/             # ATS compatibility checker
│       ├── collectors/      # GitHub, resume, website data collectors
│       ├── graph/           # Career Knowledge Graph schema
│       ├── platform/        # AI generation, job search, tracking
│       ├── rendering/       # PDF renderer, design DNA, repair
│       ├── storage/         # Qdrant vector store
│       ├── templates/       # Resume templates
│       └── vision/          # Vision QA agent
├── Dockerfile.backend
├── Dockerfile.frontend
├── docker-compose.yml
├── docker-compose.override.yml  # Dev mode with hot reload
├── start.sh                # Local dev launcher
├── Makefile                # Common dev commands
└── pyproject.toml
```

---

## Contributing

Contributions are welcome. Please open an issue first to discuss what you'd like to change.

```bash
# Fork the repo
# Create your branch
git checkout -b feature/amazing-feature

# Commit
git commit -m 'Add amazing feature'

# Push
git push origin feature/amazing-feature

# Open a Pull Request
```

---

## License

Distributed under the Apache License 2.0. See [LICENSE](LICENSE) for details.

---

<div align="center">

**Built with the belief that your career deserves more than a template.**

</div>


Studio is the candidate context editor. Completing the questionnaire writes `ABOUT.md` from all answers and the uploaded profile; search, ranking, documents, form filling and Excel use the same file. Download it from Studio or `/v1/studio/about`. Existing background sections are preserved when a partial profile belongs to the same named candidate. `CAREERTWIN_ABOUT_PATH` can select a different shared file. Completing Studio fails visibly if the file cannot be written.

Salary defaults to **Auto**. The application agent selects a separate amount inside the published salary range for each vacancy (35% into the band), matching currency, period and gross/net B2B basis. It never reuses another job's amount or silently converts incompatible units. With no usable advertised budget, negotiable wording is allowed where the form accepts it; a required numeric answer pauses for your input. Salary decisions and their evidence are recorded per application and included in Excel. An optional fixed amount requires explicit currency, pay period and basis.
