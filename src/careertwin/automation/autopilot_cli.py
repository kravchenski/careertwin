"""`careertwin autopilot` and `careertwin answers`: the candidate's own switch and the answers kept for every form.

They work on the saved settings and answers directly, so they need neither the API nor its token; a running API
reads the settings again on its next tick (within a minute).
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from careertwin.automation import autopilot
from careertwin.automation.salary import SALARY_FIELD
from careertwin.automation.store import Store

MARK = {"ok": "ok     ", "warn": "warning", "blocker": "BLOCKER"}


def _store() -> Store:
    store = Store()
    store.initialize()
    return store


def _autopilot(args: argparse.Namespace, out=print) -> int:
    store = _store()
    settings = store.settings()
    if args.action == "off":
        store.save_settings(autopilot.disable(settings))
        out("Autopilot is off: nothing is sent without you, the search timer is stopped.")
        return 0
    checks = autopilot.readiness(store, settings)
    if args.action == "status":
        state = autopilot.report(store, settings, gateway=True)
        out(f"Autopilot: {'ON' if state['on'] else 'off'}   today {state['submitted_today']}/{state['daily_limit']}   "
            f"sent in all {state['submitted_total']}   fit threshold {state['threshold']:g}")
        for check in checks:
            out(f"  {MARK[check.status]}  {check.title}" + (f": {check.detail}" if check.detail else ""))
        for item in state["waiting_for_you"]:
            out(f"  waits for you ({item['state']}): {item['title']} at {item['company'] or '?'}" + (f" - {item['detail']}" if item["detail"] else ""))
        return 0
    for check in checks:
        out(f"  {MARK[check.status]}  {check.title}" + (f": {check.detail}" if check.detail else ""))
    if autopilot.blockers(checks):
        out("Autopilot stays off until the blockers are gone.")
        return 1
    new = autopilot.enable(settings, daily_limit=args.daily_limit, threshold=args.threshold)
    out(
        f"\nOn: the agents will search every {new.interval_hours} h, and for a vacancy with a fit of {new.threshold:g} or more they will "
        f"prepare the CV, fill the form, tick the terms and privacy boxes and send it, at most {new.daily_limit} a day.\n"
        "They never create an account, type a password off your login domains, solve a CAPTCHA, tick a marketing box, send mail or\n"
        "guess an answer; an application that needs any of that waits for you. `touch data/STOP` stops everything."
    )
    if not args.yes:
        if not sys.stdin.isatty() or input("Turn it on? [y/N] ").strip().lower() not in {"y", "yes"}:
            out("Not turned on. Pass --yes to confirm without a prompt.")
            return 1
    store.save_settings(new)
    store.event(None, "autopilot_on", {"daily_limit": new.daily_limit, "threshold": new.threshold})
    out("Autopilot is on.")
    return 0


def _answers(args: argparse.Namespace, out=print) -> int:
    store = _store()
    if args.action == "list":
        standing = store.standing()
        for question, answer in sorted(standing.items()):
            out(f"{question}\n    {answer}")
        if not standing:
            out("No standing answers. Add one with: careertwin answers add \"Question\" \"Answer\"")
        return 0
    if args.action == "add":
        if SALARY_FIELD.search(args.question):
            out("A salary is settled per vacancy, not for every form.")
            return 1
        store.answer(args.question, args.answer)
        out("Saved.")
        return 0
    if args.action == "remove":
        out("Removed." if store.forget(args.question) else "No such answer.")
        return 0
    data = json.loads(Path(args.file).read_text(encoding="utf-8"))
    if not isinstance(data, dict) or not all(isinstance(k, str) and isinstance(v, str) for k, v in data.items()):
        out('The file must be one JSON object of {"question": "answer"} pairs.')
        return 1
    skipped = [question for question in data if SALARY_FIELD.search(question)]
    for question, answer in data.items():
        if question not in skipped and question.strip() and answer.strip():
            store.answer(question.strip(), answer.strip())
    out(f"Saved {len(data) - len(skipped)} answers." + (f" Skipped (a salary is per vacancy): {', '.join(skipped)}" if skipped else ""))
    return 0


def main(argv: list[str] | None = None, out=print) -> int:
    parser = argparse.ArgumentParser(prog="careertwin autopilot")
    parser.add_argument("action", choices=["on", "off", "status"])
    parser.add_argument("--daily-limit", type=int, help=f"Applications sent per day (default {autopilot.DEFAULT_DAILY_LIMIT})")
    parser.add_argument("--threshold", type=float, help=f"Lowest fit that is sent (never below {autopilot.FLOOR:g})")
    parser.add_argument("--yes", action="store_true", help="Do not ask for confirmation")
    return _autopilot(parser.parse_args(argv), out)


def consent_main(argv: list[str] | None = None, out=print) -> int:
    """`careertwin consent on|off|status`: only the box ticking, without the rest of the autopilot."""
    parser = argparse.ArgumentParser(prog="careertwin consent")
    parser.add_argument("action", choices=["on", "off", "status"])
    action = parser.parse_args(argv).action
    store = _store()
    settings = store.settings()
    if action != "status":
        store.save_settings(settings.model_copy(update={"consent_auto": action == "on"}))
        settings = store.settings()
    out(
        "The filler ticks the box that confirms an application's terms or privacy notice (\"I agree to the Privacy Statement\")."
        if settings.consent_auto
        else "The filler ticks no box: you do it."
    )
    out("It never ticks advertising, marketing, newsletter, job alert, talent community or data sharing boxes.")
    return 0


def answers_main(argv: list[str] | None = None, out=print) -> int:
    parser = argparse.ArgumentParser(prog="careertwin answers")
    sub = parser.add_subparsers(dest="action", required=True)
    sub.add_parser("list", help="Show the answers kept for every form")
    add = sub.add_parser("add", help="Keep an answer for every form")
    add.add_argument("question")
    add.add_argument("answer")
    remove = sub.add_parser("remove", help="Forget one")
    remove.add_argument("question")
    load = sub.add_parser("import", help="Load many from a JSON file of question: answer pairs")
    load.add_argument("file")
    return _answers(parser.parse_args(argv), out)
