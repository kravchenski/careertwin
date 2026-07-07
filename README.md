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

### Phase 2 — Intelligence
- [x] Auto Resume Builder (per-job adaptation)
- [x] Cover Letter Generator
- [x] Job matching agent
- [x] Skill gap analysis
- [x] Career trajectory recommendations

### Phase 3 — Visual Quality
- [ ] PDF rendering pipeline
- [ ] Vision QA with Qwen-VL / LLaVA
- [ ] Design trend scraper (Dribbble, Behance)
- [ ] Design DNA engine
- [ ] Repair Agent loop
- [ ] Multi-template support

### Phase 4 — Platform
- [ ] Job Search MCP integration
- [ ] Application tracking workflow
- [ ] Web dashboard
- [ ] API for third-party integrations
- [ ] Multi-language CV support

---

## Tech Stack

| Component | Technology |
|---|---|
| Language | Python 3.14+ |
| Data Models | Pydantic v2 |
| Agent Framework | LangGraph |
| Vector Database | Qdrant |
| HTTP Client | httpx |
| Web Scraping | BeautifulSoup4 + lxml |
| PDF Parsing | pdfminer.six |
| DOCX Parsing | python-docx |
| GitHub API | PyGithub |
| Package Manager | uv |

---

## Getting Started

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
