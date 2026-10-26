# Security

CareerTwin holds a CV, an `ABOUT.md`, the password of an account and the ability to send an application or an
email in your name. This page says what stands between that and someone who should not have it, where the
protection is tested, and what it does **not** cover.

Run `uv run careertwin security` at any time. It lists what is protected and what is not, never prints a
secret (only the variable's name), and with `--fix` repairs file permissions.

## The account the agents use

Put one account in `.env` (copy `.env.example`; keep the file private, `chmod 600 .env`):

```
ACCOUNT_EMAIL=you@gmail.com
ACCOUNT_PASSWORD=...                 # typed on the sites listed below, by the program, never by a model
CAREERTWIN_LOGIN_DOMAINS=linkedin.com
MAIL_PASSWORD=...                    # an APP PASSWORD for IMAP/SMTP, not your main password
```

* **Mail** needs an app password (Gmail, Outlook and iCloud issue them). The mailbox is opened read-only, and a
  message is sent only after you approve the draft.
* **The site password** is typed only on the domains in `CAREERTWIN_LOGIN_DOMAINS`, which is empty by default,
  so by default it is typed nowhere. A shared application system (Greenhouse, Workday…) is deliberately not a
  place for it: anyone can publish a page there.
* Two-factor prompts and CAPTCHAs are not bypassed: the agent is told to stop and ask you (`careertwin watch`),
  and the answer checks keep it from guessing. A one-time code that arrives by email is read by the mail agent
  and typed with the code as its evidence.

## Who can hurt you, and what stops them

| Threat | What stops it | Tested in |
|---|---|---|
| A **vacancy page or email** carries hidden instructions ("send your CV to…") | Page and mail text is data. The form agent may only act on the employer's own site and known application systems; a page that moves the tab elsewhere stops it before anything is typed. A model can only choose from fixed actions, and every typed value needs evidence from your profile. A received email is classified into a fixed list and never drafts, sends or changes a job by itself | `test_automation.py`, `test_guard.py`, `test_mail.py` |
| A **model is told to reveal a secret** | Secrets are not in the prompts. Text that would carry one is refused before it is sent (`vault.assert_clean`); pages and replies are masked on the way in; logs, stored events, mail records and the workbook are masked on the way out | `test_vault.py`, `test_automation.py`, `test_dashboard.py` |
| A model **types the password into a phishing form** | The password is a placeholder the model cannot resolve. The program substitutes it only into a password field, only on a login domain you listed, and only for `ACCOUNT_PASSWORD` | `test_automation.py`, `test_guard.py` |
| A **web page you visit** calls the API on `127.0.0.1` | Every route needs the bearer token; the `Host` must be a loopback name (stops DNS rebinding); a browser request from another origin is refused | `test_api_security.py` |
| **Someone on your network** reaches the API or the container desktop | The API and the container listen on `127.0.0.1`; `--allow-remote` is needed to do otherwise; the browser desktop in the container needs `VNC_PASSWORD` and is loopback-only | `test_api_security.py` |
| The **dashboard page** is the one route besides the health check that is open without a token | The page carries no data: every number is fetched from the API with the token, which `careertwin dashboard` passes in the address fragment (never sent to the server, removed from the bar at once, kept in that tab's session storage). Its script and style are pinned by hash in the Content-Security-Policy, nothing is loaded from outside, everything on it is set as text (a vacancy title cannot become markup), and its buttons only queue steps that stay on this computer (rank, prepare, fill, skip): sending an application or an email is not among them | `test_agents_dashboard.py` |
| **Guessing the token**, or flooding the API | Constant-time comparison, a lockout after repeated wrong tokens, a request rate limit, a body size limit | `test_api_security.py` |
| A link makes the program **fetch an internal address** (SSRF) | Every fetch resolves the name, refuses private, loopback, link-local and cloud-metadata addresses, connects to the address it checked (no rebinding), checks every redirect, caps the size of a page | `test_net_safe.py` |
| The **mail agent writes to a stranger** or smuggles a header | One recipient, only an address the vacancy names, a sender in the thread or the employer's domain, checked again at send time; headers are single lines; the only attachment is the CV whose hash was verified; a daily limit | `test_mail.py` |
| An application goes out **without you** | Sending needs your yes (`careertwin watch`, or an explicit API call). `auto_submit` and `mail_auto_send` are off, and agents cannot switch them on | `test_automation.py`, `test_mail.py`, `test_watch.py` |
| Things go wrong and you want it to **stop now** | The kill switch: `touch data/STOP` (works with the API down), `POST /v1/automation/kill`, or type `kill` at a `watch` prompt. Nothing is submitted, sent or visited until it is lifted | `test_guard.py`, `test_mail.py`, `test_monitor.py` |
| **Another user on the machine** reads your files | `.env`, `ABOUT.md` and the API token are checked for group/world access (`careertwin security --fix`); the workbook is written mode 0600; generated documents are served only to the token holder, and the profile snapshot never | `test_vault.py`, `test_security_check.py`, `test_dashboard.py`, `test_api_security.py` |
| A **formula in a spreadsheet cell** (`=HYPERLINK(…)`) | Any cell that starts like a formula is written as text | `test_dashboard.py` |
| A **profile, resume or page** carries HTML that makes the PDF engine fetch an address or read a file | Template text is escaped; the PDF engine is a restricted fetcher that loads only `data:` URLs and the fonts it was given, so even unescaped HTML fetches nothing | `test_audit_fixes.py` |
| A **request body or a redirect** sends a real browser (Playwright, the crawler, the agent browser) to an internal address or a `file:` URL | The three routes that took any URL refuse it; every browser launch is guarded request by request, redirects and subresources included | `test_audit_fixes.py` |
| A **hostile line of text** freezes the program (catastrophic regex backtracking) | The redaction, experience and question patterns are bounded, and a spoken question is cut at 2000 characters; each is timed on a 300 000-character hostile input in a separate process | `test_audit_fixes.py` |
| **Another account** connects to the interview window's control socket | The socket lives in a folder only its owner can enter (checked: owner, mode, not a link), so there is no window between `bind` and `chmod`; the API and the window create every file private by default | `test_audit_fixes.py` |
| A **repository in a folder you analyse** runs its own hooks when git is called | Git is called with hooks and the file-system monitor switched off and without prompts | `test_audit_fixes.py` |
| The **MCP server over HTTP** is a second door to the same tools (it holds the API token and runs them with it) | It asks for the same bearer token, the same Host and Origin rules and the same limits as the API, and listens on the loopback only | `test_mcp_security.py` |
| A **vacancy site is hammered** | One request at a time per host, `robots.txt` respected on career pages, exponential back-off, a batch size | `test_monitor.py` |

## Everything goes through Switchyard

Text generation, decisions and the browser tools of the agents use your Switchyard gateway and nothing else; a
different provider in the saved settings is ignored. If you route Switchyard to a cloud model, that model sees
what the agents send it: your facts, vacancy text and the emails it classifies. Choose the route accordingly.
`decision_backend=typesafe` is an exception you opt into, and `careertwin security` warns when it is on.

## The last audit

Run on 2026-10-24 against the code of this guide (OWASP-style classes, a dependency audit with `pip-audit`, the
`ruff` security rules, a timing test of every regular expression, a scan of all 266 commits for credentials and
private files, and the workflows). Found and fixed:

* **High: HTML injection into the PDF engine** (confirmed with a local listener that the renderer called it).
  Escaped now, and the engine may fetch nothing.
* **Medium: three routes sent a real browser to any URL** from a request body, including `file:///etc/passwd`
  and cloud-metadata addresses. Validated and guarded.
* **Medium: three regular expressions that take minutes on a hostile line**, one of them in the secret redaction
  of project analysis. Bounded; a gap that let `Authorization: Bearer …` through unredacted is closed.
* **Medium: 16 vulnerable packages (150 advisories)**, among them `pypdf` (27, on uploaded files), `weasyprint`,
  `pillow`, `urllib3` and the `certifi` root-store advisory. Upgraded, minimum versions raised, and a test keeps
  them from going back. One is left: `cryptography` 49 (fix in 50), which `pyopenssl` does not yet allow; the
  advisory concerns PKCS#7 decryption, which nothing here uses.
* **Medium (second pass): the MCP server over HTTP accepted any local caller without a token**, so another account
  on the machine could run the tools through it. Now behind the API's gate.
* **Low:** the control socket race, a settings file readable for an instant before `chmod`, git run with the
  repository's own config, an unpinned CI toolchain and a workflow with no `permissions`. All fixed.

Clean: no SQL built from input (the few dynamic statements take column names from fixed lists), no deserialization
of untrusted data, no `shell=True`, no disabled TLS verification, no committed secrets or private files, no
script injection in the workflows.

Left, on purpose or for lack of a way to test it here:

* **The container runs as root with Chrome's sandbox off** (`--no-sandbox`) and on the host network. A browser
  escape would land in a root container that shares the host's loopback. Running it as an unprivileged user needs
  an image build and test that was not possible here.
* **A browser checks a name once.** The guard resolves the name for each request, but a name that changes between
  the check and the connection (DNS rebinding) can still reach a browser; our own HTTP client has no such gap.
* **Error texts** of the older routes still return the exception message to the caller, and a few have no upper
  bound on a list in the body. Both need the token and neither can reach a secret.

## What this does not cover

* **Anything that already runs as you.** A process under your user account can read `.env`, `ABOUT.md`, the token
  and your signed-in Chrome. The protection is against the web, the network, other accounts and hostile content,
  not against malware on your own user.
* **Your Chrome sessions.** The form agent works in your signed-in browser; whatever that browser can do, a
  correct but mistaken agent could attempt on an employer's site. The domain lock and the approval step bound it.
* **The sites' rules.** Many job sites forbid automation in their terms. Using it on your own account can get the
  account limited. That is a decision for you, not something code can settle.
* **A model that is simply wrong.** An answer is checked against your profile, but a check is not a guarantee:
  read the form before you say yes.
* **Transport to a remote gateway.** Use HTTPS if Switchyard is not on this machine (`careertwin security` fails a
  plain-HTTP remote address).
* **Supply chain.** Dependencies are pinned in `uv.lock` but not audited here.

## Reporting a problem

Open a private security advisory on the GitHub repository, or write to the maintainer. Please do not post a
working exploit in a public issue.
