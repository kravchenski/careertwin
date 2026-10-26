# Hosting the agents on AWS

The agents run inside the API (`careertwin serve`), so hosting them means running the API on a machine that stays
on, and watching it from your own terminal with `careertwin watch`. The API carries your CV and can send
applications, so **nothing is exposed to the internet**: everything listens on the server's loopback and you
reach it through an SSH tunnel.

```
 your laptop                                   one AWS server (only port 22 is open, to your IP)
 ───────────                                   ─────────────────────────────────────────────────
 careertwin watch ──► 127.0.0.1:8000 ══ ssh ══► 127.0.0.1:8000  CareerTwin API + agents (docker)
 (notifications here)                          127.0.0.1:3260  Switchyard (the models)
                                               127.0.0.1:8888  SearXNG (web search)
```

## What runs on the server and what does not

| Agent | On the server | Why |
| --- | --- | --- |
| Company-site monitor | yes | plain HTTP to career boards, on a timer |
| Mail (read replies, drafts) | yes | IMAP and SMTP with an app password |
| Resume builder, Excel export | yes | files in the data volume |
| Search, ranking, preparing CVs | yes | needs a model: Switchyard on the same server |
| Form filler | yes, with the headless browser of step 8 | it drives a Chrome through Switchyard's extension; on a server that Chrome is headless and holds no sign-in of yours |
| Desktop notifications | on your laptop | `careertwin watch` sends them where it runs |

Switchyard's browser-session providers (the signed-in web chats) cannot sign in on a server. Give the server a
Switchyard with API-key providers only (Groq, NVIDIA, OpenRouter and so on).

## 1. The server

* Ubuntu 24.04 LTS on x86_64, 2 vCPU and **4 GB of memory** (2 GB works only with swap), a 30 GB disk: the image holds
  Chromium, Playwright and the PDF libraries and is several gigabytes.
* Security group: one inbound rule, **SSH (22) from your own IP only** (`x.x.x.x/32`). No rule for 8000, 6080, 8888 or 3260.
* A key pair for SSH. Keep the `.pem` out of every repository and `chmod 400` it.

```bash
sudo apt-get update && sudo apt-get install -y docker.io docker-compose-v2 rsync
sudo usermod -aG docker "$USER"      # log out and in once
```

## 2. Copy the code

From the repository on your laptop:

```bash
SSH_KEY=~/.ssh/aws.pem scripts/deploy-remote.sh ubuntu@SERVER_IP
```

It copies the code with `rsync` (never `.env`, `ABOUT.md`, `data/` or a PDF), then stops and tells you that `.env`
and `ABOUT.md` are missing. That is the next step.

## 3. Put the secrets there yourself

```bash
scp -i ~/.ssh/aws.pem ABOUT.md ubuntu@SERVER_IP:careertwin/ABOUT.md
ssh -i ~/.ssh/aws.pem ubuntu@SERVER_IP 'umask 077; cat > careertwin/.env' <<EOF
VNC_PASSWORD=$(python -c "import secrets; print(secrets.token_urlsafe(24))")
CAREERTWIN_API_TOKEN=$(python -c "import secrets; print(secrets.token_urlsafe(32))")
SWITCHYARD_URL=http://127.0.0.1:3260
SWITCHYARD_API_KEY=
MAIL_DRY_RUN=1
EOF
```

Keep that token: it is what `careertwin watch` signs in with. Add `ACCOUNT_EMAIL`, `ACCOUNT_PASSWORD`,
`CAREERTWIN_LOGIN_DOMAINS` and the `MAIL_*` variables from `.env.example` only when you want those agents, and leave
`MAIL_DRY_RUN=1` until you have seen a few drafts that look right.

## 4. A model: Switchyard on the same server

The container uses the host network, so the default `SWITCHYARD_URL=http://127.0.0.1:3260` reaches a Switchyard that
publishes its port on the server's loopback (its own `docker-compose.yml` does). If one is already running on this
server, you are done. Without a gateway the agents start but cannot rank or prepare anything.

## 5. Start it

```bash
SSH_KEY=~/.ssh/aws.pem scripts/deploy-remote.sh ubuntu@SERVER_IP
```

The same command updates it later. Check it on the server:

```bash
docker compose ps                       # backend: healthy
docker compose exec backend python -m careertwin security
```

## 6. Watch it from your laptop

```bash
ssh -i ~/.ssh/aws.pem -N -L 8000:127.0.0.1:8000 ubuntu@SERVER_IP      # keep this open
export CAREERTWIN_API_URL=http://127.0.0.1:8000
export CAREERTWIN_API_TOKEN=...       # the value you put in the server's .env
careertwin watch
```

`careertwin security` rejects a plain-HTTP address that is not the loopback; the tunnel makes it one.

## 7. Turn on the timers

Both are off or need a mailbox by default, and an agent cannot switch them on, so you do it once. The settings are
replaced as a whole, so read, change, write:

```bash
H="Authorization: Bearer $CAREERTWIN_API_TOKEN"; U=$CAREERTWIN_API_URL/v1/automation/settings
curl -s -H "$H" $U | jq '.monitor_scheduled=true | .monitor_interval_minutes=360' \
  | curl -s -X PUT -H "$H" -H 'content-type: application/json' -d @- $U
```

The monitor visits the companies you added with `watch_company` (or `monitor_known_companies: true` for the bundled
list). The mail agent checks the inbox every `mail_check_minutes` once a mailbox is configured.

## 8. A browser on the server (the form filler)

The form filler drives a Chrome through Switchyard's browser extension. A server has no screen and nobody to click the
extension's "Allow Switchyard to act on this site?" prompt, so the server runs a **headless build** of the extension
that acts without asking. Build it next to the gateway and start Chromium with it:

```bash
git clone https://github.com/kravchenski/switchyard-extension.git ~/switchyard-extension
cd ~/switchyard-extension && bun install
SWITCHYARD_GATEWAY_URL=http://127.0.0.1:3260 bun run build --headless    # writes dist-headless/
cd ~/careertwin && scripts/headless-browser.sh ~/switchyard-extension/dist-headless &
```

`SWITCHYARD_MCP_URL` already points at `http://127.0.0.1:3260/mcp`, so the form filler needs nothing else. If the
gateway has an API key, build with `SWITCHYARD_API_KEY=...` too. **That build carries the key: it stays on the server.**

What this changes and what it does not:

* The extension no longer asks before it clicks or types on a site. A page that talks a model into clicking has nothing
  between it and whatever the browser profile holds, so the profile is empty: no sign-in of yours, no saved password.
  CareerTwin's own limits stay: a form is filled only on the employer's site and the application systems, a password is
  typed only on the domains in `CAREERTWIN_LOGIN_DOMAINS`, and nothing is sent without your yes.
* There is no signed-in browser of yours. Sites that need an account (Workday asks for one per employer) work when you
  created the account yourself and set `ACCOUNT_EMAIL`, `ACCOUNT_PASSWORD` and `CAREERTWIN_LOGIN_DOMAINS`; a CAPTCHA or a
  code sent by mail pauses the application for you.
* Only one browser is the gateway's browser at a time: a second extension connection replaces the first. A laptop Chrome
  with the extension and the server's browser cannot share one gateway.
* Checked: the form filler in a headless Chromium with this build typed a name, a phone and an e-mail into a public form
  with no prompt and asked for the answers it had no facts for. It was not run against an employer's site on a server.

In a container running as root, Chromium needs `CHROMIUM_FLAGS=--no-sandbox`, which turns off its sandbox: do that only
in a container that holds nothing else.

## Keeping it

* **Update:** the same `scripts/deploy-remote.sh` command.
* **Back up** the data volume (vacancies, drafts, the API token): 
  `docker run --rm -v careertwin_careertwin-data:/data -v "$PWD":/backup alpine tar czf /backup/careertwin-data.tgz -C /data .`
* **One place at a time:** the pipeline state is a SQLite file in that volume. Copying your laptop's
  `data/careertwin.db` there starts the server from your history, and from then on the laptop's copy is stale.
* **Cost:** an instance bills for every hour it runs; stop it from the console when you do not need the agents.

## What is not hardened

The container runs as root with Chrome's sandbox off and on the host network (see [SECURITY.md](SECURITY.md)). On a
single-purpose server with port 22 closed to everyone but you that is acceptable; do not put anything else on it, and
do not reuse the key pair elsewhere.
