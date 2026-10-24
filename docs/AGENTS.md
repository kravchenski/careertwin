# Agents on Switchyard

CareerTwin has no web UI. The HTTP API is the product, and agents drive it.
Any agent that talks to [Switchyard](https://github.com/kravchenski/switchyard),
your local OpenAI- and Anthropic-compatible gateway, can run the whole job
search: it finds vacancies, ranks them, prepares CVs, fills application forms
and tracks outcomes.

```
 Claude Code · Codex · OpenCode · pi · careertwin agent
              │  model calls                 │  tool calls
              ▼                              ▼
     Switchyard :3260              CareerTwin MCP server
   (25+ providers, `auto`)                   │  HTTP
              │                              ▼
              └── browser tools ──►   CareerTwin API :8000
                  (/mcp, Chrome)      search · rank · prepare · apply · track
```

Switchyard supplies the model and the signed-in browser. CareerTwin supplies
the job-search tools and keeps the state. They meet in the agent.

## 1. Start the pieces

```bash
switchyard                      # gateway on http://127.0.0.1:3260, providers connected
make api                        # CareerTwin API on http://127.0.0.1:8000
```

Fill `ABOUT.md` (see `ABOUT.example.md`) and set `SWITCHYARD_*` in `.env`.

## 2. Pick how the agent reaches the tools

### A. The built-in agent

```bash
uv run careertwin agent "Find strong Python matches and prepare CVs for the top two"
uv run careertwin agent            # interactive session
```

`careertwin agent` runs a tool-calling loop against Switchyard's
`/v1/chat/completions`. Model: `--model`, `SWITCHYARD_AGENT_MODEL`,
`SWITCHYARD_MODEL`, or `auto`. It asks before any action that contacts an
employer; `--yes` approves them for the whole session.

### B. Your coding agent, over MCP

Start Claude Code, Codex or OpenCode against Switchyard as usual
(`bun run setup:agents` in the Switchyard repo writes their configs), then add
the CareerTwin MCP server. Replace `/path/to/careertwin`.

| Agent | Command or config |
|---|---|
| Claude Code | `claude mcp add careertwin -- uv run --directory /path/to/careertwin careertwin-mcp` |
| Codex | `~/.codex/config.toml`: `[mcp_servers.careertwin]` with `command = "uv"` and `args = ["run", "--directory", "/path/to/careertwin", "careertwin-mcp"]` |
| OpenCode | `opencode.json`: `"mcp": { "careertwin": { "type": "local", "command": ["uv", "run", "--directory", "/path/to/careertwin", "careertwin-mcp"] } }` |
| Any HTTP MCP client | `uv run careertwin mcp --transport http --port 8765`, then `http://127.0.0.1:8765/mcp` |

The server reads `CAREERTWIN_API_URL` (default `http://127.0.0.1:8000`).
The HTTP transport binds to localhost; keep it there.

### C. Your own code

`careertwin.agentic.registry.openai_tool_specs()` returns the catalogue as
OpenAI function-calling specs and `get_tool(name).call(args)` runs one tool,
so any OpenAI-compatible client can use the same tools.

## Tools

| Tool | Changes state | What it does |
|---|---|---|
| `get_status` | no | API health and what the agents are doing now |
| `get_candidate_context` | no | Search settings and the start of the private `ABOUT` context |
| `list_job_sources` | no | Sources the search agent can use |
| `search_company_boards` | no | Search company career boards through their ATS |
| `fetch_job_description` | no | Full text of a vacancy page |
| `list_vacancies`, `get_vacancy`, `recent_events` | no | Read the pipeline and its history |
| `wait_for_vacancy` | no | Wait for a queued step to finish |
| `import_vacancy` | local | Add a vacancy to the pipeline |
| `update_search_settings` | local | Queries, sources, threshold, limits |
| `start_search_cycle`, `stop_agents` | local | Run or cancel the search agent |
| `process_vacancy` | local | Queue `rank`, `prepare`, `fill` or `verify` |
| `answer_question` | local | Save the candidate's answer to a form question |
| `record_outcome` | local | Interview, offer, hired, rejected, withdrawn |
| `prepare_interview_questions`, `suggest_interview_answer`, `review_cv_for_vacancy` | no | Practice questions, grounded answers and a CV-versus-vacancy review for a booked interview (see [INTERVIEW.md](INTERVIEW.md)) |
| `get_summary`, `what_needs_me` | no | The numbers of the search; everything waiting for the candidate |
| `list_monitored_companies` | no | The watched companies and their health |
| `watch_company`, `run_monitor` | local | Add a company to the monitor; visit the watched sites now |
| `check_mail`, `list_mail` | local / no | Read the inbox once and file replies to applications; list what was filed |
| `draft_email` | local | Prepare an application or follow-up email as a draft (nothing is sent) |
| `submit_application` | **employer** | Press submit on a prepared form |
| `send_email` | **employer** | Approve and send a drafted email |

## Safety

- **Submitting is opt-in three times.** `submit_application` needs
  `confirm=true`, the environment variable `CAREERTWIN_AGENT_ALLOW_SUBMIT=1`,
  and, in `careertwin agent`, your answer to a prompt.
- **Sending an email is opt-in the same three times.** `send_email` needs `confirm=true`,
  `CAREERTWIN_AGENT_ALLOW_MAIL=1` and your yes. A draft only goes to an address the vacancy names.
- **Auto-submit, automatic mail and scheduling cannot be switched on by an agent.**
  `update_search_settings` does not expose them.
- **A secret never reaches the model.** See [SECURITY.md](SECURITY.md).
- **Facts only.** Documents and form answers come from your `ABOUT` context.
  An agent saves only the answers you give it.
- **A fit score is not a hiring probability.** It measures compatibility with
  the written requirements.
- Browser steps run in your signed-in Chrome through Switchyard's extension.
  Login and CAPTCHA pause the application for you.

## Adding a tool

Write an async function in `src/careertwin/agentic/tools.py` and decorate it
with `@tool("one-line summary")`. Type hints become the JSON schema and the docstring becomes the
description. Use `mutating=True` when it changes local state and `outward=True`
when it contacts a third party. The MCP server, the built-in agent and the
tests pick it up automatically.
