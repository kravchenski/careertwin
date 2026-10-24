"""`careertwin watch`: the terminal where you control the process.

The agents run inside the API (`careertwin serve`). This program is the other end: it shows what they do
as it happens, asks you about everything that needs a human (a form ready to send, an email draft, a
reply to read, a question the form asked) and sends a desktop notification when something new waits.
Nothing is sent to an employer unless you answer yes here (or in the API with the token).
"""

from __future__ import annotations

import asyncio
import re
import shutil
import subprocess
import sys
from collections.abc import Callable
from typing import Any

from careertwin.agentic.client import ApiError, CareerTwinClient

SHOWN = {
    "monitor_found": lambda p: f"found: {p.get('title')} at {p.get('company')}",
    "monitor_closed": lambda p: f"closed on the career site: {p.get('title')} at {p.get('company')}",
    "monitor_summary": lambda p: f"monitor: {p.get('checked')} companies visited, {p.get('new')} new, {p.get('failed')} failed",
    "monitor_error": lambda p: f"monitor: {p.get('company') or 'error'}: {p.get('message')}",
    "ranked": lambda p: f"ranked: {p.get('category')} (fit {p.get('score')})",
    "prepared": lambda p: "CV and cover letter prepared",
    "form_ready": lambda p: "the form is filled and waits for you",
    "questions": lambda p: "the form asks: " + "; ".join(p.get("questions") or []),
    "domain_blocked": lambda p: f"stopped: the page left the employer's site ({p.get('url')})",
    "secret_blocked": lambda p: "stopped: the password may not be typed on this page",
    "submission_confirmed": lambda p: "application sent and confirmed",
    "submission_uncertain": lambda p: "sent, but no confirmation was seen: check it before doing anything else",
    "mail_received": lambda p: f"mail ({p.get('kind')}): {p.get('subject')}",
    "mail_drafted": lambda p: f"email drafted to {p.get('to')}",
    "mail_sent": lambda p: f"email sent to {p.get('to')}",
    "mail_error": lambda p: f"mail: {p.get('message')}",
    "error": lambda p: f"error in {p.get('operation')}: {p.get('message')}",
    "cycle_finished": lambda p: f"search finished: {p.get('found')} found",
    "user_outcome": lambda p: f"outcome recorded: {p.get('status')}",
}
URGENT = {"interview_invite", "submit", "email"}


def plain(text: Any, limit: int = 160) -> str:
    """Text from outside, safe for a terminal and a notification: no control characters, one line."""
    text = re.sub(r"[\x00-\x1f\x7f-\x9f]+", " ", str(text))
    return " ".join(text.split())[:limit]


def notify(title: str, body: str) -> None:
    """A desktop notification where one is available. Arguments go as a list, never through a shell."""
    title, body = plain(title, 80), plain(body, 200)
    try:
        if sys.platform == "darwin":
            script = 'display notification "{}" with title "{}"'.format(body.replace("\\", " ").replace('"', "'"), title.replace("\\", " ").replace('"', "'"))
            subprocess.run(["osascript", "-e", script], check=False, timeout=5, capture_output=True)
        elif shutil.which("notify-send"):
            subprocess.run(["notify-send", "--app-name=CareerTwin", "--", title, body], check=False, timeout=5, capture_output=True)
        else:
            sys.stdout.write("\a")
    except (OSError, subprocess.SubprocessError):
        pass


class Console:
    def say(self, text: str = "") -> None:
        print(text, flush=True)

    def ask(self, prompt: str) -> str:
        return input(prompt).strip()


class Watcher:
    def __init__(self, client: CareerTwinClient, console: Console | None = None, tell: Callable[[str, str], None] = notify):
        self.client, self.console, self.tell = client, console or Console(), tell
        self.last_event = 0
        self.known: set[str] = set()
        self.declined: set[str] = set()  # items answered "later" this session: not asked again until restart
        self.kill_requested = False

    def prompt(self, text: str) -> str:
        """A question to the user. Typing `kill` at any prompt stops the agents (handled by `attend`)."""
        reply = self.console.ask(text)
        if reply.strip().lower() == "kill":
            self.kill_requested = True
            return ""
        return reply

    @staticmethod
    def key(item: dict[str, Any]) -> str:
        return f'{item["kind"]}:{item.get("mail_id") or item.get("job_id")}'

    async def feed(self) -> None:
        events = await self.client.get("/v1/automation/events")
        fresh = [event for event in events if event["id"] > self.last_event]
        if self.last_event == 0 and fresh:  # on the first look show the recent tail, not the whole history
            fresh = fresh[-10:]
        for event in fresh:
            render = SHOWN.get(event["stage"])
            if render:
                try:
                    line = render(event["payload"] if isinstance(event["payload"], dict) else {})
                except (TypeError, KeyError):
                    continue
                stamp = event["created_at"][11:19]
                self.console.say(f"{stamp}  {plain(line, 200)}")
        if events:
            self.last_event = max(self.last_event, events[-1]["id"])

    async def status(self) -> str:
        numbers = await self.client.get("/v1/automation/summary")
        state = await self.client.get("/v1/automation/status")
        mail = numbers["mail"]
        stopped = "  KILL SWITCH ON" if state.get("stopped") else ""
        return (
            f"{numbers['jobs']} vacancies · applied {numbers['applied']} · replies {numbers['replied']} · today {numbers['submitted_today']} sent · "
            f"companies {numbers['monitor']['watching']} ({numbers['monitor']['failing']} failing) · mail in {mail['received']} out {mail['sent']}{stopped}"
        )

    async def attend(self) -> int:
        """Ask about each item that waits. Returns how many were answered."""
        items = await self.client.get("/v1/automation/attention")
        fresh = [item for item in items if self.key(item) not in self.known]
        if fresh:
            urgent = [item for item in fresh if item["kind"] in URGENT] or fresh
            self.tell("CareerTwin: waiting for you", f"{len(fresh)} new: {plain(urgent[0]['title'], 120)}")
        self.known |= {self.key(item) for item in items}
        answered = 0
        for item in items:
            if self.key(item) in self.declined:
                continue
            if await self.handle(item):
                answered += 1
            if self.kill_requested:
                self.kill_requested = False
                await self.client.post("/v1/automation/kill")
                self.console.say("KILL SWITCH ON: nothing will be sent. Lift it with DELETE /v1/automation/kill or by removing data/STOP.")
                break
        return answered

    async def handle(self, item: dict[str, Any]) -> bool:
        say, ask, kind = self.console.say, self.prompt, item["kind"]
        say(f"\n── {kind.upper().replace('_', ' ')} ── {plain(item['title'], 200)}")
        if item.get("job"):
            say(f"   about: {plain(item['job'], 200)}")
        if item.get("detail"):
            say(f"   {plain(item['detail'], 400)}")
        base = "/v1/automation"
        job_id, mail_id = item.get("job_id"), item.get("mail_id")
        try:
            if kind == "submit":
                await self.show_answers(job_id)
                reply = ask("Send this application?  [y] send  [s] skip the vacancy  [Enter] later  ").lower()
                if reply == "y":
                    await self.client.post(f"{base}/jobs/{job_id}/actions", {"operation": "submit"})
                    say("   submitting…")
                    return True
                if reply == "s":
                    await self.client.post(f"{base}/jobs/{job_id}/outcome", {"status": "withdrawn", "note": "skipped in the terminal"})
                    return True
            elif kind == "answer":
                for question in await self.questions(job_id) or [plain(item.get("detail") or "the form asked a question")]:
                    answer = ask(f"   {plain(question, 200)}\n   your answer (Enter = later): ")
                    if not answer:
                        self.declined.add(self.key(item))
                        return False
                    await self.client.post(f"{base}/jobs/{job_id}/answers", {"question": question, "answer": answer})
                await self.client.post(f"{base}/jobs/{job_id}/actions", {"operation": "fill"})
                say("   filling again…")
                return True
            elif kind == "verify":
                if ask("Check whether it was sent?  [y] verify  [Enter] later  ").lower() == "y":
                    await self.client.post(f"{base}/jobs/{job_id}/actions", {"operation": "verify"})
                    return True
            elif kind == "review":
                reply = ask("[r] resume  [s] skip the vacancy  [Enter] later  ").lower()
                if reply == "r":
                    await self.client.post(f"{base}/jobs/{job_id}/actions", {"operation": "resume"})
                    return True
                if reply == "s":
                    await self.client.post(f"{base}/jobs/{job_id}/outcome", {"status": "withdrawn", "note": "skipped in the terminal"})
                    return True
            elif kind == "email":
                draft = await self.client.get(f"{base}/mail/{mail_id}")
                say(f"   to: {plain(draft['to_addr'])}\n   subject: {plain(draft['subject'])}\n   ---\n" + "\n".join("   " + plain(line, 300) for line in draft["body"].splitlines()[:12]))
                if draft["attachment"]:
                    say("   attachment: the verified CV (PDF)")
                reply = ask("Send this email?  [y] send  [n] discard  [Enter] later  ").lower()
                if reply == "y":
                    await self.client.post(f"{base}/mail/{mail_id}/approve")
                    await self.client.post(f"{base}/mail/{mail_id}/send")
                    say("   sent")
                    return True
                if reply == "n":
                    await self.client.post(f"{base}/mail/{mail_id}/reject")
                    return True
            elif mail_id:  # a reply: interview_invite, rejection, needs_info
                reply = ask("[i] record interview  [r] record rejection  [h] mark read  [Enter] later  ").lower()
                outcome = {"i": "interview", "r": "rejected"}.get(reply)
                if outcome or reply == "h":
                    await self.client.post(f"{base}/mail/{mail_id}/handled", {"outcome": outcome})
                    return True
        except ApiError as exc:
            say(f"   not done: {plain(exc, 300)}")
        self.declined.add(self.key(item))
        return False

    async def questions(self, job_id: str) -> list[str]:
        detail = await self.client.get(f"/v1/automation/jobs/{job_id}")
        for event in reversed(detail["events"]):
            if event["stage"] == "questions":
                return [str(q) for q in event["payload"].get("questions", [])]
        return []

    async def show_answers(self, job_id: str) -> None:
        detail = await self.client.get(f"/v1/automation/jobs/{job_id}")
        rows = [e["payload"] for e in detail["events"] if e["stage"] == "browser_action" and e["payload"].get("field")]
        for row in rows[-12:]:
            self.console.say(f"   {plain(row['field'], 60)}: {plain(row['value'], 100)}")
        job = detail["job"]
        self.console.say(f"   {plain(job['vacancy']['url'], 200)}")
        if job.get("package"):
            self.console.say(f"   CV: {plain(job['package'].get('cv_path', ''), 200)}")

    async def run(self, *, once: bool = False, poll: float = 3.0, rounds: int | None = None) -> None:
        self.console.say("CareerTwin watch · Ctrl-C to leave · `kill` at a prompt stops the agents")
        count = 0
        while True:
            try:
                self.console.say(await self.status())
                await self.feed()
                if once:
                    for item in await self.client.get("/v1/automation/attention"):
                        self.console.say(f"waiting: [{item['kind']}] {plain(item['title'], 160)}")
                    return
                await self.attend()
            except ApiError as exc:
                self.console.say(f"cannot reach the API: {plain(exc, 200)}")
                if once:
                    raise SystemExit(1) from exc
            count += 1
            if rounds is not None and count >= rounds:
                return
            await asyncio.sleep(poll)


def main(argv: list[str] | None = None) -> None:
    once = "--once" in (argv or [])
    if not once and not sys.stdin.isatty():
        print("watch asks questions: run it in a terminal, or use --once to print what waits.", file=sys.stderr)
        raise SystemExit(2)
    try:
        asyncio.run(Watcher(CareerTwinClient()).run(once=once))
    except KeyboardInterrupt:
        print()
