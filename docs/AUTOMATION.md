# Application automation and the HTTP API

Four agents run the application workflow. You start and steer them through the
HTTP API: directly with `curl`, through the tools in [AGENTS.md](AGENTS.md), or
from your own code. Interactive docs are at `http://127.0.0.1:8000/docs` while
the API runs.

## The four agents

1. **Search and rank.** Remote OK and Remotive public feeds, indexed results from
   the 25 remote boards plus LinkedIn, Glassdoor and ZipRecruiter, and company
   career boards through the ATS adapters. Import a link with its full description
   when a source requires login or is not indexed. The decision model evaluates
   skills, experience, preferences and explicit eligibility restrictions.
2. **Prepare documents.** A text model selects evidence from your profile. The agent
   renders a text-extractable, single-column PDF of at most two pages and saves the
   cover letter, evidence, vacancy and profile snapshot. Claims and experience dates
   come from the source facts.
3. **Apply.** The browser agent uses Switchyard's MCP browser tools in your signed-in
   Chrome, uploads the verified PDF, records its answers and checks the confirmation
   page after submitting. The decision model checks proposed answers against your
   profile. Missing answers, login and CAPTCHA pause the application for your input.
4. **Track.** SQLite keeps each operation and its exact answers. The fourth agent
   updates `data/outputs/CareerTwin-applications.xlsx`.

## Set it up

```bash
cp .env.example .env
cp ABOUT.example.md ABOUT.md     # only for a new profile; keep an existing ABOUT.md
# fill ABOUT.md with your real context
switchyard                       # gateway running, browser extension connected
uv run careertwin serve
```

`ABOUT.md` is private and git-ignored. It seeds the first settings; later changes go
through `PUT /v1/automation/settings` or the `update_search_settings` tool.
For index searches run SearXNG at `SEARXNG_URL`, or set `TAVILY_API_KEY`. Company
entries accept the ATS board identifier as `name` and the career page as `url`;
`ats_platform` can be `greenhouse`, `lever`, `ashby`, `smartrecruiters` or `workday`.

## A vacancy's life

```
new ──rank──► priority | stretch | needs_info | skip
priority ──prepare──► prepared ──fill──► form_ready | awaiting_answers
form_ready ──submit──► submitted | submission_uncertain ──verify──► submitted
submitted ──outcome──► interview | offer | hired | rejected | withdrawn
```

Steps run in the background; read the state with `GET /v1/automation/jobs/{id}`.

## Routes

All under `/v1/automation` unless noted.

| Route | Purpose |
|---|---|
| `GET/PUT /settings` | Queries, sources, preferences, thresholds, limits, auto flags |
| `GET /sources`, `GET /decision-models` | Known sources; models your gateway offers |
| `GET /status`, `POST /run`, `POST /stop` | Agent state; start or cancel a search cycle |
| `GET/POST /jobs`, `GET /jobs/{id}` | The pipeline; import a vacancy; detail with events and files |
| `POST /jobs/{id}/actions` | `rank`, `prepare`, `fill`, `submit`, `resume`, `verify` |
| `POST /jobs/{id}/answers` | Save your answer to a form question |
| `POST /jobs/{id}/outcome` | `interview`, `offer`, `hired`, `rejected`, `withdrawn` |
| `GET /events`, `GET /export.xlsx` | Event log; a fresh workbook |
| `/v1/interview/suggest`, `/v1/interview/practice` | Interview answer hints and practice questions |
| `/v1/jobs/*`, `/v1/companies/*` | Search, scan and scrape; company list |
| `/v1/profile/*`, `/v1/studio/*`, `/v1/resume/*`, `/v1/applications` | Profile, Studio context, CV tailoring, manual tracking |

```bash
curl -s localhost:8000/v1/automation/jobs | jq '.[].state'
curl -s -X POST localhost:8000/v1/automation/run
curl -s localhost:8000/v1/automation/export.xlsx -o applications.xlsx
```

## Models

Decisions go to Switchyard's `/v1/decisions` API. Set `decision_model` in the settings
to a model id from `GET /v1/automation/decision-models` or to `auto`. If automatic
routing runs out of providers, pick one working model. The service evaluates finite
choices; the four fit dimensions use five levels and their distributions produce a
weighted fit score. The returned certainty is an approximate model estimate, not
calibrated accuracy or a hiring probability. Hosted models are used; nothing is trained
or loaded on your computer.

The [Unsloth decision model guide](https://unsloth.ai/docs/basics/train-your-own-decision-model-with-unsloth)
describes training a decision head and calibrating it on held-out data. A prompt-based
cloud model does not inherit those guarantees.

Hosted Jev is optional: set `decision_backend` to `typesafe` and `TYPESAFE_API_KEY`.
Text generation uses `SWITCHYARD_URL` and `SWITCHYARD_MODEL`; browser tools use
`SWITCHYARD_MCP_URL` and the optional `SWITCHYARD_API_KEY`. Site permissions are
handled by the extension: open its side panel on the application tab when it asks for
access.

## Behaviour to know

- Automatic CV preparation is on. Automatic submission and scheduled searches are off
  until you enable them in the settings. An explicit submit starts submission for that
  vacancy.
- A fit score is compatibility with documented requirements, not a hiring probability.
  A CV cannot guarantee acceptance.
- Scheduled searches need the API to keep running. Use one worker so browser work is
  serialized.
- Restart recovery never repeats an application that may have been sent. `verify` reads
  the existing tab without submitting. Keep the application tab and the browser open
  while verification is pending.
- The workbook holds applications, exact answers, the full event history, document paths
  with SHA256 hashes, saved per-application answers and complete snapshots. Long values
  are split into numbered parts instead of being truncated. Files and SQLite data live
  in `data/` or the Docker data volume.
