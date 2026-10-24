# Architecture

CareerTwin is one Python package, `careertwin`, with three front doors onto the same core.

```
                 ┌──────────────────────── front doors ────────────────────────┐
 agents (MCP)    │  agentic/      tool catalogue · MCP server · Switchyard loop   │
 HTTP clients    │  api/          FastAPI app · routes · request models           │
 you, live       │  interview/    audio · Whisper · coach · Qt overlay            │
                 └───────────────┬───────────────────────────┬──────────────────┘
                                 ▼                           ▼
                  automation/  five agents + guard  services/  profile/  jobs/
                  monitor · résumé · filler ·       CV tailoring · ABOUT.md ·
                  mail · Excel                      vacancy discovery
                                 │                           │
                                 ▼                           ▼
              settings · db (SQLite) · llm (providers) · browser · net      document library
              data/ outputs, cache, transcripts                             (agents, ats, rendering, …)
```

## Layers and rules

1. **Front doors depend downward only.** `agentic/` talks to the API over HTTP and never imports
   `automation/`, so an agent sees exactly what any HTTP client sees.
2. **`api/routes/` are thin.** They validate with `api/schemas.py` and delegate to `automation/`,
   `services/`, `jobs/` or `interview/`.
3. **`automation/` owns the application workflow** and its state in SQLite (`Store`). The `Engine`
   runs one asyncio task per vacancy and recovers interrupted work on restart without re-sending an application.
4. **`jobs/` finds vacancies.** `boards/` has one adapter per ATS (Greenhouse, Lever, Ashby,
   SmartRecruiters, Workday); `companies.json` is the curated company list.
5. **`profile/` is the single source of facts.** `context.py` reads and writes `ABOUT.md`; documents,
   form answers and interview hints are generated from it and nowhere else.
6. **Configuration lives in `settings.py`** (paths and provider table) and `.env`, loaded once by
   `careertwin/__init__.py`. Real environment variables win over `.env`.
7. **No module changes `sys.path`.** All imports are absolute from `careertwin`.
8. **Secrets and outbound traffic have one door each.** `vault.py` knows which environment variables are
   secrets and masks them in logs, events and files and refuses them in a prompt; `net.py` is the only way a
   foreign URL is fetched (public addresses only, redirects checked); `api/security.py` is the only gate in front
   of the API; `automation/guard.py` holds the limits no model output can argue with. [SECURITY.md](SECURITY.md)
   lists what each one stops.

## How an agent drives a vacancy

```
agent ──tool call──► agentic/tools.py ──HTTP──► api/routes/automation.py ──► Engine.enqueue()
                                                                              │ asyncio task
        wait_for_vacancy polls GET /jobs/{id} ◄── Store (SQLite) ◄── Ranker · DocumentAgent · BrowserAgent
```

- `rank` calls the decision model through Switchyard `/v1/decisions`.
- `prepare` calls the text model, renders the PDF with the document library and stores hashes.
- `fill` and `submit` drive Chrome through Switchyard's `/mcp` browser tools.
- Every step writes an event; `ExcelAgent` mirrors the pipeline into the workbook.

## The interview assistant

```
parec (sink monitor) ─► 20 ms frames ─► UtteranceSegmenter ─► WhisperTranscriber (local)
                                                                   │ text
                          is_question? ─► debounce ─► StreamingCoach ─► Switchyard (SSE)
                                                                   │ tokens
                                       Qt overlay ◄── Bridge signals ◄── InterviewSession callbacks
```

`InterviewSession` knows nothing about Qt; the overlay is one consumer of its callbacks, and the
`/v1/interview/*` routes are another consumer of the same coach and brief.

## State and files

| Where | What |
|---|---|
| `data/careertwin.db` | Profiles, studio sessions, companies, collected jobs, applications, and the automation jobs, events, answers and files |
| `data/outputs/` | Generated CVs, cover letters, `CareerTwin-applications.xlsx`; served only to the token holder |
| `data/api-token`, `data/STOP` | The API token (mode 0600); the kill switch while the file exists |
| `data/cache/`, `data/screenshots/` | Job-source cache, browser screenshots |
| `data/interviews/` | Interview transcripts (text only) |
| `ABOUT.md` | Your private candidate context; git-ignored |

`CAREERTWIN_DATA_DIR` moves all of `data/`.

## Extending it

- **A new tool for agents:** add an async function with `@tool("summary")` in `agentic/tools.py`.
- **A new ATS:** subclass `BaseSourceAdapter` in `jobs/boards/sources/` and register it in `boards/__init__.py`.
- **A new job source:** add its domain to `SOURCES` in `automation/sources.py`.
- **A new CV template:** drop an HTML file into `templates/html/` and add it to `templates/__init__.py`.

## Tests

`uv run pytest -m "not integration"` runs without network or real applications. Tests build their own
SQLite files, fake Switchyard with `httpx.MockTransport`, and set `CAREERTWIN_NO_DOTENV` so a local
`.env` cannot change results.
