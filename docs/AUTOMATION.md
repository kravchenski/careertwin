# Application automation and the HTTP API

Five agents run the application workflow. You steer them from a terminal
(`uv run careertwin watch`), through the HTTP API (`curl`, the tools in
[AGENTS.md](AGENTS.md) or your own code) or by chatting with the built-in agent. Every API route but
`/v1/health` needs the API token (`Authorization: Bearer $(cat data/api-token)`, see
[SECURITY.md](SECURITY.md)). Interactive docs at `/docs` are off unless you start the API with
`CAREERTWIN_API_DOCS=1`.

All text generation, decisions and browser work go through your Switchyard gateway.

## The five agents

1. **Monitor.** Watches company career sites. The watch list is the `companies` of the settings (and, with
   `monitor_known_companies`, the 246 bundled employers). Each visit reads the open vacancies from the company's
   application system (Greenhouse, Lever, Ashby, SmartRecruiters, Workday) or, failing that, its career page,
   compares them with the last visit and files what is new and relevant into the pipeline. Vacancies that leave
   the site are closed in the pipeline. It is polite: one request at a time per host, `robots.txt` respected,
   exponential back-off on a failing site, and an empty page from a company that had vacancies counts as a broken
   page, not as a mass closing. The board feeds and indexed searches of the old search agent still run in a
   search cycle.
2. **Résumé builder.** A text model selects evidence from your profile. The agent renders a text-extractable,
   single-column PDF of at most two pages and saves the cover letter, evidence, vacancy and profile snapshot.
   Claims and experience dates come from the source facts.
3. **Filler.** The browser agent uses Switchyard's MCP browser tools in your signed-in Chrome, uploads the
   verified PDF, records its answers and checks the confirmation page after submitting. It stays on the
   employer's own site and known application systems, signs in with your account only on the domains you list,
   and asks the mail agent for a one-time code when a site mails one. Missing answers, CAPTCHA and anything it
   cannot support from your profile pause the application for your input.
4. **Mail.** Reads the mailbox (read-only) for replies to your applications and files them: interview invitations,
   rejections, requests for information. Drafts an application email or a follow-up for a vacancy that gives an
   address; a draft goes out only after you approve it, to one address the vacancy names, with the verified CV.
5. **Excel.** `data/outputs/CareerTwin-applications.xlsx`: a dashboard (funnel, reply rate, days to a reply, a chart
   of the last 14 days), the applications, exact answers, the history, a mail log and the health of every watched
   company. Written private to you, with secrets masked.

Around them sits a deterministic **guard** that no model output can argue with: the domain lock, the password
rule, the daily limits, the approval step and the kill switch (`touch data/STOP`, or `POST /v1/automation/kill`).

## Set it up

```bash
cp .env.example .env && chmod 600 .env
cp ABOUT.example.md ABOUT.md     # only for a new profile; keep an existing ABOUT.md
# fill ABOUT.md with your real context; add the account and mail settings to .env if you want them
switchyard                       # gateway running, browser extension connected
uv run careertwin serve          # the agents and the API
uv run careertwin watch          # in another terminal: the feed, the questions, the notifications
uv run careertwin security       # what is protected and what is not
```

`ABOUT.md` is private and git-ignored. It seeds the first settings; later changes go
through `PUT /v1/automation/settings` or the `update_search_settings` tool.
For index searches run SearXNG at `SEARXNG_URL`, or set `TAVILY_API_KEY`. Company
entries accept the ATS board identifier as `name` and the career page as `url`;
`ats_platform` can be `greenhouse`, `lever`, `ashby`, `smartrecruiters` or `workday`.

## The terminal

`careertwin watch` prints what the agents do as it happens and asks you about everything that needs a human:

| Waiting | You are asked |
|---|---|
| A form is filled | Send this application? (the recorded answers, the link and the CV are shown first) |
| An email draft | Send this email? (the whole draft is shown) |
| A reply arrived | Record an interview or a rejection, or mark it read |
| The form asked a question | Your answer; then the form is filled again |
| A submission is uncertain | Check whether it was sent |

Anything but `y` sends nothing. Typing `kill` at any prompt turns the kill switch on. New items also raise a
desktop notification (`notify-send` on Linux, `osascript` on macOS). `careertwin watch --once` prints the status
and what waits without asking.

## A vacancy's life

```
monitor/search ──► new ──rank──► priority | stretch | needs_info | skip
priority ──prepare──► prepared ──fill──► form_ready | awaiting_answers
form_ready ──submit──► submitted | submission_uncertain ──verify──► submitted
prepared ──email draft, approved, sent──► submitted
submitted ──outcome──► interview | offer | hired | rejected | withdrawn
new | priority | prepared | form_ready ──vacancy left the career site──► closed
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
| `GET /summary`, `GET /attention` | The numbers of the search; everything that waits for you |
| `POST /kill`, `DELETE /kill` | The kill switch on and off |
| `GET /monitor`, `POST /monitor/run`, `POST /monitor/companies`, `DELETE /monitor/companies/{name}` | The watch list and its health; visit now; add or remove a company |
| `GET /mail`, `GET /mail/status`, `POST /mail/check` | The filed messages; whether mail works; read the inbox now |
| `POST /jobs/{id}/mail`, `POST /mail/{id}/approve`, `/reject`, `/send`, `/handled` | Draft an application or follow-up; approve, discard or send it; mark a reply dealt with (optionally recording its outcome) |
| `/v1/interview/suggest`, `/v1/interview/practice` | Interview answer hints and practice questions |
| `/v1/jobs/*`, `/v1/companies/*` | Search, scan and scrape; company list |
| `/v1/profile/*`, `/v1/studio/*`, `/v1/resume/*`, `/v1/applications` | Profile, Studio context, CV tailoring, manual tracking |

```bash
AUTH="Authorization: Bearer $(cat data/api-token)"
curl -s -H "$AUTH" localhost:8000/v1/automation/jobs | jq '.[].state'
curl -s -H "$AUTH" -X POST localhost:8000/v1/automation/run
curl -s -H "$AUTH" localhost:8000/v1/automation/export.xlsx -o applications.xlsx
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

- Automatic CV preparation is on. Automatic submission, `mail_auto_send`, the scheduled search and the
  scheduled monitor are off until you enable them in the settings (the inbox check is on when mail is set
  up). An explicit submit starts submission for that vacancy.
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
