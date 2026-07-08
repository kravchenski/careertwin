#!/usr/bin/env python3
"""Checks every company's `careers` URL and website domain in
web/api/companies.json for reachability.

Deliberately dependency-light (httpx + stdlib only) and framework-agnostic so
any AI coding agent (Claude Code, OpenCode, MimoCode, Pi, etc.) can invoke it
directly through its shell tool without reimplementing the HTTP-checking
logic itself — the agent's job is to run this, read the report, and decide
what (if anything) to fix in companies.json.

Usage:
    python3 scripts/check_company_links.py [--limit N] [--concurrency N]
        [--field careers|domain|both] [--out report.json]

Exit code is non-zero if any broken link was found (useful for CI or for an
agent to branch on).
"""

from __future__ import annotations

import argparse
import asyncio
import json
import sys
from pathlib import Path

import httpx

ROOT = Path(__file__).resolve().parent.parent
COMPANIES_FILE = ROOT / "web" / "api" / "companies.json"
TIMEOUT = 12.0
USER_AGENT = "Mozilla/5.0 (compatible; CareerTwinLinkChecker/1.0)"


async def check_url(client: httpx.AsyncClient, url: str) -> dict:
    try:
        resp = await client.get(url, follow_redirects=True, headers={"User-Agent": USER_AGENT})
        final_url = str(resp.url)
        return {
            "url": url,
            "final_url": final_url if final_url != url else None,
            "status": resp.status_code,
            "ok": resp.status_code < 400,
            "error": None,
        }
    except httpx.TimeoutException:
        return {"url": url, "final_url": None, "status": None, "ok": False, "error": "timeout"}
    except httpx.HTTPError as exc:
        return {"url": url, "final_url": None, "status": None, "ok": False, "error": type(exc).__name__}
    except Exception as exc:  # noqa: BLE001 — best-effort link check, any failure means "broken"
        return {"url": url, "final_url": None, "status": None, "ok": False, "error": str(exc)}


async def run(companies: list[dict], fields: set[str], concurrency: int) -> list[dict]:
    sem = asyncio.Semaphore(concurrency)
    results: list[dict] = []

    async with httpx.AsyncClient(timeout=TIMEOUT) as client:
        async def worker(company: dict) -> None:
            async with sem:
                entry: dict = {"name": company.get("name")}
                if "careers" in fields and company.get("careers"):
                    entry["careers"] = await check_url(client, company["careers"])
                if "domain" in fields and company.get("domain"):
                    entry["website"] = await check_url(client, f"https://{company['domain']}")
                results.append(entry)

        await asyncio.gather(*(worker(c) for c in companies))
    return results


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--limit", type=int, default=None, help="Only check the first N companies")
    parser.add_argument("--concurrency", type=int, default=15)
    parser.add_argument("--field", choices=["careers", "domain", "both"], default="both")
    parser.add_argument("--out", type=Path, default=None, help="Write full JSON results here")
    args = parser.parse_args()

    companies = json.loads(COMPANIES_FILE.read_text(encoding="utf-8"))
    if args.limit:
        companies = companies[: args.limit]
    fields = {"careers", "domain"} if args.field == "both" else {args.field}

    results = asyncio.run(run(companies, fields, args.concurrency))

    broken = []
    for r in results:
        for field in ("careers", "website"):
            if field in r and not r[field]["ok"]:
                broken.append((r["name"], field, r[field]))

    print(f"Checked {len(results)} companies.")
    if broken:
        print(f"\n{len(broken)} broken link(s):\n")
        for name, field, info in broken:
            detail = info.get("error") or f"HTTP {info.get('status')}"
            print(f"  - {name} [{field}]: {info['url']}  ->  {detail}")
    else:
        print("All links OK.")

    if args.out:
        args.out.write_text(json.dumps(results, indent=2), encoding="utf-8")
        print(f"\nFull results written to {args.out}")

    sys.exit(1 if broken else 0)


if __name__ == "__main__":
    main()
