"""CareerTwin API — agent-browser helpers, LLM browser controller, LinkedIn browser flows."""

from __future__ import annotations

import logging
import os
import re
import shutil
import subprocess
from pathlib import Path
from typing import Any

from careertwin.llm import call_ai, parse_json_object
from careertwin.net import check_public_url
from careertwin.settings import AGENT_BROWSER_LOCAL_BIN, DATA_DIR, SCREENSHOT_DIR

logger = logging.getLogger("careertwin.api")





def _agent_browser_bin() -> str | None:
    env_bin = os.getenv("AGENT_BROWSER_BIN", "").strip()
    if env_bin:
        if os.path.exists(env_bin):
            return env_bin
        return None
    if AGENT_BROWSER_LOCAL_BIN.exists():
        return str(AGENT_BROWSER_LOCAL_BIN)
    return shutil.which("agent-browser")


def _agent_browser_available() -> bool:
    return _agent_browser_bin() is not None


def _agent_browser_session(name: str) -> str:
    safe = re.sub(r"[^a-zA-Z0-9_-]+", "-", name).strip("-").lower()
    return f"careertwin-{safe or 'default'}"




def _agent_browser_run(
    args: list[str],
    *,
    session: str,
    timeout: int = 30,
    headed: bool = False,
    json_output: bool = False,
) -> tuple[bool, str, str]:
    bin_path = _agent_browser_bin()
    if not bin_path:
        return False, "", "agent-browser binary not found"

    run_cwd = str(DATA_DIR / "browser")
    Path(run_cwd).mkdir(parents=True, exist_ok=True)

    os.environ.setdefault("CHROME_FLAGS", "--no-sandbox --disable-gpu --disable-dev-shm-usage")

    cmd = [bin_path, "--session", session]
    if headed:
        cmd.append("--headed")
    if json_output:
        cmd.append("--json")
    cmd.extend(args)

    try:
        proc = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=timeout,
            cwd=run_cwd,
        )
    except Exception as exc:
        return False, "", str(exc)

    stdout = (proc.stdout or "").strip()
    stderr = (proc.stderr or "").strip()
    return proc.returncode == 0, stdout, stderr




def _agent_browser_open(url: str, *, session: str, wait_ms: int = 0) -> tuple[bool, str, str]:
    try:
        check_public_url(url)  # a real browser would happily open 169.254.169.254 or file:///etc/passwd
    except ValueError as exc:
        return False, "", f"blocked: {exc}"
    ok, stdout, stderr = _agent_browser_run(["open", url], session=session, timeout=45)
    if ok and wait_ms > 0:
        _agent_browser_run(["wait", str(wait_ms)], session=session, timeout=max(10, wait_ms // 1000 + 5))
    return ok, stdout, stderr


def _agent_browser_read(*, session: str, url: str | None = None, max_output: int = 12000) -> str:
    args = ["read"]
    if url:
        args.append(url)
    ok, stdout, _stderr = _agent_browser_run(
        ["--max-output", str(max_output), *args],
        session=session,
        timeout=45,
        headed=False,
    )
    return stdout if ok else ""


def _agent_browser_snapshot(*, session: str, interactive: bool = True) -> str:
    args = ["snapshot"]
    if interactive:
        args.append("-i")
    ok, stdout, _stderr = _agent_browser_run(args, session=session, timeout=30, headed=False)
    return stdout if ok else ""


def _agent_browser_eval(expression: str, *, session: str, timeout: int = 30) -> str:
    ok, stdout, _stderr = _agent_browser_run(
        ["eval", expression],
        session=session,
        timeout=timeout,
        headed=False,
    )
    return stdout if ok else ""


def _agent_browser_get(what: str, *, session: str) -> str:
    ok, stdout, _stderr = _agent_browser_run(["get", what], session=session, timeout=15, headed=False)
    return stdout if ok else ""


def _agent_browser_screenshot(session: str, step_name: str, import_id: str) -> str:
    """Take a screenshot and return the filename."""
    import_id = re.sub(r"[^a-zA-Z0-9_-]", "", import_id)[:64]
    filename = f"{import_id}_{step_name}.png"
    filepath = SCREENSHOT_DIR / filename
    ok, _stdout, _stderr = _agent_browser_run(
        ["screenshot", str(filepath)],
        session=session,
        timeout=15,
        headed=False,
    )
    return filename if ok and filepath.exists() else ""


def _agent_browser_browser_session(session: str, detail: str, status: str = "ready") -> dict[str, Any]:
    return {
        "status": status,
        "detail": detail,
        "provider": "agent-browser",
        "session": session,
    }


def _agent_browser_text_arg(text: str, max_length: int = 4000) -> str:
    return text.replace("\r\n", "\n").replace("\r", "\n").strip()[:max_length]


def _run_agent_browser_action(
    action: str,
    *,
    session: str,
    target: str = "",
    value: str = "",
) -> tuple[bool, str, str]:
    normalized = action.strip().lower()
    if normalized == "click" and target:
        return _agent_browser_run(["click", target], session=session, timeout=20)
    if normalized == "fill" and target:
        return _agent_browser_run(["fill", target, _agent_browser_text_arg(value)], session=session, timeout=25)
    if normalized == "press" and value:
        return _agent_browser_run(["press", value], session=session, timeout=15)
    if normalized == "wait":
        wait_value = value or target or "2000"
        return _agent_browser_run(["wait", wait_value], session=session, timeout=20)
    if normalized == "scroll" and value:
        pieces = value.split()
        if len(pieces) == 1:
            return _agent_browser_run(["scroll", pieces[0]], session=session, timeout=15)
        return _agent_browser_run(["scroll", pieces[0], pieces[1]], session=session, timeout=15)
    return False, "", f"Unsupported action: {action}"




async def _llm_browser_controller(
    *,
    session: str,
    job_url: str,
    full_name: str,
    email: str,
    phone: str,
    cover_letter: str,
    provider: str,
    model: str,
    auto_submit: bool,
) -> dict[str, Any]:
    trace: list[dict[str, Any]] = []
    max_steps = 8
    last_snapshot = ""

    for step in range(1, max_steps + 1):
        snapshot = _agent_browser_snapshot(session=session, interactive=True)[:12000]
        read_text = _agent_browser_read(session=session, max_output=8000)[:8000]
        current_url = _agent_browser_get("url", session=session)
        current_title = _agent_browser_get("title", session=session)
        forms_found = _agent_browser_run(["get", "count", "form"], session=session, timeout=10, headed=False)[1].strip() or "0"
        inputs_found = _agent_browser_run(["get", "count", "input, textarea, [contenteditable=\"true\"]"], session=session, timeout=10, headed=False)[1].strip() or "0"

        prompt = (
            "You control a visible browser using a constrained action set.\n"
            "Goal: open the job application flow, fill relevant fields using the candidate context, "
            "and stop before final submission unless auto_submit=true.\n"
            "Return JSON only with keys: status, reason, action, target, value.\n"
            "Allowed status: continue, done, needs_user.\n"
            "Allowed action: click, fill, press, wait, scroll, finish.\n"
            "Use snapshot refs like @e12 whenever possible. Never invent refs.\n"
            "If the page asks for login, captcha, OTP, file upload, or unknown questions, return needs_user.\n"
            f"auto_submit={str(auto_submit).lower()}\n\n"
            "Candidate context:\n"
            f"- Full name: {full_name}\n"
            f"- Email: {email}\n"
            f"- Phone: {phone}\n"
            f"- Cover letter: {cover_letter[:1500]}\n\n"
            "Current page:\n"
            f"- URL: {current_url}\n"
            f"- Title: {current_title}\n"
            f"- Forms found: {forms_found}\n"
            f"- Inputs found: {inputs_found}\n\n"
            "Snapshot:\n"
            f"{snapshot}\n\n"
            "Readable page text excerpt:\n"
            f"{read_text}\n\n"
            "Rules:\n"
            "- Prefer clicking an Apply/Easy Apply button before filling.\n"
            "- Fill name/email/phone/message fields when clearly visible.\n"
            "- Do not click final Submit/Apply unless auto_submit=true and all relevant fields are filled.\n"
            "- If already ready for human review, return {\"status\":\"done\",\"action\":\"finish\",...}.\n"
        )
        response = await call_ai(
            system_prompt="You are a browser control agent. Return strict JSON only.",
            user_prompt=prompt,
            provider=provider,
            model=model,
            max_tokens=320,
            temperature=0.1,
        )
        decision = parse_json_object(response) or {}
        status = str(decision.get("status", "")).strip().lower()
        action = str(decision.get("action", "")).strip().lower()
        target = str(decision.get("target", "")).strip()
        value = str(decision.get("value", "")).strip()
        reason = str(decision.get("reason", "")).strip() or "No reason provided."

        trace.append(
            {
                "step": step,
                "url": current_url,
                "title": current_title,
                "status": status or "invalid",
                "action": action or "none",
                "target": target,
                "value_preview": value[:140],
                "reason": reason,
            }
        )

        if status == "needs_user":
            return {
                "trace": trace,
                "status": "needs_user",
                "message": reason,
                "forms_found": int(forms_found) if forms_found.isdigit() else 0,
                "inputs_found": int(inputs_found) if inputs_found.isdigit() else 0,
            }

        if status == "done" or action == "finish":
            return {
                "trace": trace,
                "status": "done",
                "message": reason or "Agent reached a manual review point.",
                "forms_found": int(forms_found) if forms_found.isdigit() else 0,
                "inputs_found": int(inputs_found) if inputs_found.isdigit() else 0,
            }

        if status != "continue" or action not in {"click", "fill", "press", "wait", "scroll"}:
            break

        if snapshot == last_snapshot and action in {"wait", "scroll"} and step >= 2:
            trace.append(
                {
                    "step": step,
                    "status": "stalled",
                    "action": action,
                    "target": target,
                    "value_preview": value[:140],
                    "reason": "Agent is not making progress on the page.",
                }
            )
            break

        ok, stdout, stderr = _run_agent_browser_action(
            action,
            session=session,
            target=target,
            value=value,
        )
        trace[-1]["command_ok"] = ok
        trace[-1]["command_output"] = (stdout or stderr)[:200]
        if not ok:
            break

        if action in {"click", "press"}:
            _agent_browser_run(["wait", "2500"], session=session, timeout=10)
        elif action == "fill":
            _agent_browser_run(["wait", "600"], session=session, timeout=10)

        last_snapshot = snapshot

    fallback_fields: list[str] = []
    fallback_plan = [
        ("full_name", full_name, ['input[name*="name" i]', 'input[autocomplete="name"]']),
        ("email", email, ['input[type="email"]', 'input[name*="email" i]']),
        ("phone", phone, ['input[type="tel"]', 'input[name*="phone" i]']),
        ("cover_letter", cover_letter, ['textarea', '[contenteditable="true"]']),
    ]
    for label, candidate_value, selectors in fallback_plan:
        if not candidate_value.strip():
            continue
        for selector in selectors:
            ok, _stdout, _stderr = _agent_browser_run(["fill", selector, candidate_value], session=session, timeout=15)
            if ok:
                fallback_fields.append(label)
                break

    trace.append(
        {
            "step": len(trace) + 1,
            "status": "fallback",
            "action": "fill",
            "target": "deterministic-selectors",
            "value_preview": ", ".join(fallback_fields),
            "reason": "LLM browser control returned invalid or stalled output, deterministic fallback applied.",
        }
    )
    return {
        "trace": trace,
        "status": "done",
        "message": "Visible browser session prepared. Deterministic fallback filled the obvious fields for manual review.",
        "forms_found": 0,
        "inputs_found": 0,
    }

