#!/usr/bin/env python3
"""A scripted, OpenAI-compatible stand-in for Switchyard, used only to record the demo.

It answers `/v1/chat/completions` with a fixed plan so the recording is
reproducible: tool calls for the agent, streamed hints for the interview coach.
The CareerTwin API, the tools and the interview pipeline around it are real.
"""

from __future__ import annotations

import argparse
import json
import re
import time

import uvicorn
from fastapi import FastAPI, Request
from fastapi.responses import JSONResponse, StreamingResponse

app = FastAPI()

TASK_URL = "https://example.com/jobs/python-developer"

INCIDENT = (
    "POINTS:\n"
    "- Situation: order API p95 latency doubled after a release\n"
    "- Action: traced it to a query without an index, added the index\n"
    "- Result: normal within the hour, plus a latency alert\n"
    "ANSWER:\n"
    "After a release at Northwind our order API got twice as slow. I traced it to a query on "
    "customer id that had no index, added one, and we were back to normal within the hour. "
    "Then I added a latency alert so we would catch the next one sooner."
)
KUBERNETES = (
    "POINTS:\n"
    "- No direct experience on record: Kubernetes was coursework only\n"
    "- Closest real work: Docker deployments, GitHub Actions in CI\n"
    "- Say you learn fast and name what you would study first\n"
    "ANSWER:\n"
    "I have not run Kubernetes in production. I worked with it on a lab cluster at university. "
    "At Northwind I deployed everything with Docker and automated releases in GitHub Actions, "
    "so I know the problems it solves, and I would be glad to learn it properly on the job."
)
GENERIC = (
    "POINTS:\n- Answer from your real experience\nANSWER:\n"
    "I would answer from the work described in my profile."
)


def agent_step(messages: list[dict]) -> dict:
    last_user = max(i for i, m in enumerate(messages) if m["role"] == "user")
    results = [m for m in messages[last_user:] if m["role"] == "tool"]
    step = len(results)
    plan = [
        ("get_status", {}),
        ("get_candidate_context", {"max_chars": 400}),
        (
            "import_vacancy",
            {
                "url": TASK_URL,
                "title": "Python Developer (Remote)",
                "company": "Example Corp",
                "description": "Build APIs with Python and FastAPI on PostgreSQL. Docker, CI.",
            },
        ),
        ("list_vacancies", {}),
    ]
    if step < len(plan):
        name, arguments = plan[step]
    elif step == len(plan):
        job_id = json.loads(results[2]["content"])["id"]
        name, arguments = "submit_application", {"job_id": job_id, "confirm": True}
    else:
        return {
            "role": "assistant",
            "content": (
                "I imported the Python Developer role at Example Corp; it is in the pipeline "
                "as `new`. I did not apply: sending an application needs your approval, and "
                "nothing was sent. Next I can rank it against your profile and prepare a CV."
            ),
        }
    return {
        "role": "assistant",
        "content": None,
        "tool_calls": [
            {
                "id": f"call_{step}",
                "type": "function",
                "function": {"name": name, "arguments": json.dumps(arguments)},
            }
        ],
    }


def coach_reply(messages: list[dict]) -> str:
    prompt = messages[-1]["content"]
    question = (re.search(r"INTERVIEWER'S QUESTION: (.*)", prompt) or [None, ""])[1].lower()
    if "kubernetes" in question:
        return KUBERNETES
    if "debug" in question or "incident" in question or "production" in question:
        return INCIDENT
    return GENERIC


@app.get("/v1/models")
async def models():
    return {"data": [{"id": "demo"}]}


@app.post("/v1/chat/completions")
async def chat(request: Request):
    body = await request.json()
    if body.get("tools"):
        return JSONResponse({"choices": [{"message": agent_step(body["messages"])}]})
    text = coach_reply(body["messages"])
    if not body.get("stream"):
        return JSONResponse({"choices": [{"message": {"role": "assistant", "content": text}}]})

    def events():
        for start in range(0, len(text), 14):
            time.sleep(0.02)
            chunk = {"choices": [{"delta": {"content": text[start : start + 14]}}]}
            yield f"data: {json.dumps(chunk)}\n\n"
        yield "data: [DONE]\n\n"

    return StreamingResponse(events(), media_type="text/event-stream")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=3299)
    uvicorn.run(app, host="127.0.0.1", port=parser.parse_args().port, log_level="warning")
