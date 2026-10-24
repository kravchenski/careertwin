"""Terminal front end for the agent runner."""

from __future__ import annotations

import sys

from careertwin.agentic.client import ApiError
from careertwin.agentic.runner import AgentRunner, gateway_model, gateway_url


async def run_agent(
    task: str | None, *, model: str | None = None, max_steps: int = 25, yes: bool = False
) -> None:
    runner = AgentRunner(
        model=model,
        max_steps=max_steps,
        confirm=(lambda _prompt: True) if yes else _ask,
    )
    print(f"CareerTwin agent via {gateway_url()} · model {runner.model or gateway_model()}")
    try:
        if task:
            print(await runner.ask(task))
            return
        while True:
            try:
                line = input("\nyou › ").strip()
            except (EOFError, KeyboardInterrupt):
                print()
                return
            if line in {"", "exit", "quit"}:
                return
            print(await runner.ask(line))
    except ApiError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1) from exc


def _ask(prompt: str) -> bool:
    if not sys.stdin.isatty():
        return False
    return input(f"{prompt} [y/N] ").strip().lower() in {"y", "yes"}
