# The document library

The library under `src/careertwin/{collectors,graph,agents,ats,rendering,vision,templates,storage}`
turns your background into CVs and cover letters. The application agent calls it for the
CV step, and it has its own CLI.

```
GitHub · CV file · website ──► collectors ──► Career graph ──► agents ──► template ──► PDF
                                                  │                                    │
                                          ATS checker                         vision QA ► repair loop
```

## Career graph

`graph/schema.py` holds the Pydantic model: profile, experience, education, projects,
skills, technologies, achievements and the evidence behind each claim. `to_search_text()` flattens it for
matching. `storage/qdrant.py` can keep graphs in a Qdrant collection.

| Collector | Reads |
|---|---|
| `collectors/github.py` | Repositories, languages, stars (PyGithub; `GITHUB_TOKEN` for private repos and limits) |
| `collectors/resume.py` | PDF, DOCX or text CVs, with a fallback section parser |
| `collectors/website.py` | A personal site: projects and stack |

## Agents

| Module | Job |
|---|---|
| `agents/resume_builder.py` | Picks the experience and skills that matter for a job description |
| `agents/cover_letter.py` | Writes a cover letter from the same evidence |
| `agents/job_matcher.py` | Scores a vacancy against the graph |
| `agents/skill_gap.py` | Lists missing skills for a target role |
| `agents/career_recommender.py` | Suggests career trajectories |
| `agents/application_package.py` | CV, cover letter, ATS report and visual QA for one job |
| `agents/orchestrator.py` | A small LangGraph pipeline: collect, merge, ATS check, advise |

## Rendering and quality

- `templates/html/` has nine Jinja templates: ats, classic, creative, executive, minimal,
  modern, senior, sidebar, technical. `rendering/` turns one into a PDF with WeasyPrint
  (`uv sync --extra rendering`).
- `ats/checker.py` checks sections, formatting, content, keywords and length, and reports what passed and what to fix.
- `vision/` renders pages to images and asks a vision model (GPT-4V, Qwen-VL or LLaVA) for
  a layout score and issues; `rendering/repair.py` applies fixes and re-checks.
- `rendering/design_dna.py` and `trends.py` describe a template's design (font, layout,
  spacing, accent) and can fetch design references.

## Command line

```bash
uv run careertwin collect --github <user> --resume cv.pdf   # builds career_graph.json
uv run careertwin info
uv run careertwin check --resume cv.pdf                     # ATS report
uv run careertwin build --job job.txt                       # tailored CV for a job
uv run careertwin tailor --job job.txt                      # CV, cover letter, ATS and QA
uv run careertwin cover | match | gaps | trajectory
uv run careertwin templates
uv run careertwin render -t modern                          # PDF
uv run careertwin qa --pdf out.pdf                          # visual QA
uv run careertwin repair -t modern
```

Run any command with `-h` for its options. The same library is exposed over HTTP at
`/v1/resume/*` and `/v1/templates`.
