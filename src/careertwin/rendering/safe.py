"""WeasyPrint that loads nothing it was not given.

A CV is built from text written by others (a GitHub profile, an uploaded resume, a vacancy). Escaping it is the
first defence; this is the second: whatever reaches the HTML, the PDF engine may not fetch an address, read a local
file or follow a stylesheet. Only `data:` URLs and the files named by the caller (the page itself, fonts the user
configured) are allowed.
"""

from __future__ import annotations

import os
from collections.abc import Iterable
from typing import Any
from urllib.parse import unquote, urlsplit


class ResourceBlocked(ValueError):
    pass


def fetcher(allowed_files: Iterable[str] = ()) -> Any:
    """A WeasyPrint `URLFetcher` that serves `data:` URLs and the named files and refuses everything else."""
    from weasyprint import URLFetcher

    allowed = {os.path.realpath(path) for path in allowed_files}

    class Restricted(URLFetcher):
        def fetch(self, url: str, headers: Any = None) -> Any:
            if url.startswith("file:"):
                if os.path.realpath(unquote(urlsplit(url).path)) not in allowed:
                    raise ResourceBlocked(f"The document may not read {url[:80]}")
            elif not url.startswith("data:"):
                raise ResourceBlocked(f"The document may not load {url[:80]}")
            return super().fetch(url, headers)

    return Restricted(allowed_protocols={"data", "file"}, allow_redirects=False)


def html(*, string: str | None = None, filename: str | None = None, files: Iterable[str] = ()) -> Any:
    """`weasyprint.HTML` with the fetcher above; `files` are the local files it may read (fonts).

    A document given by `filename` is read here and handed over as text, so the engine never needs to be
    allowed to open files to find it."""
    from weasyprint import HTML

    if filename is not None:
        with open(filename, encoding="utf-8") as stream:
            string = stream.read()
    return HTML(string=string, url_fetcher=fetcher(files))
