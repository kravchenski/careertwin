"""Entry point of the portable Windows package: keeps data next to the program and logs crashes."""

import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent
DATA = ROOT / "data"
DATA.mkdir(exist_ok=True)
os.environ.setdefault("CAREERTWIN_DATA_DIR", str(DATA))
os.chdir(ROOT)  # a `.env` placed here is read too

if sys.stdout is None or sys.stderr is None:  # pythonw has no console: keep the output in a file
    sys.stdout = sys.stderr = open(DATA / "last-run.log", "w", encoding="utf-8", buffering=1)
else:  # the console's code page cannot print the arrows and accents the messages contain
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

from careertwin.interview.app import main  # noqa: E402

main()
