import asyncio
import httpx
import json


async def main():
    async with httpx.AsyncClient(timeout=15, follow_redirects=True) as client:
        resp = await client.get(
            "https://join.ventionteams.com/job-openings",
            headers={"User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36"},
        )
        html = resp.text

    from job_api_discovery import _SSR_STATE_MARKERS, _find_job_arrays
    decoder = json.JSONDecoder()
    for marker in _SSR_STATE_MARKERS:
        idx = html.find(marker)
        print("marker", repr(marker), "found at", idx)
        if idx < 0:
            continue
        obj, _ = decoder.raw_decode(html, idx + len(marker))
        matches = _find_job_arrays(obj)
        print("matches found:", len(matches))
        for path, arr in matches:
            print("  path:", path, "count:", len(arr), "sample keys:", list(arr[0].keys()) if arr else None)
            print("  sample item:", arr[0] if arr else None)


asyncio.run(main())
