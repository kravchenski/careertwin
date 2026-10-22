"""The Linux install files stay consistent with the app."""

from __future__ import annotations

import os
import re
import subprocess
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
APP_ID = "careertwin-interview"


def test_desktop_entry_matches_the_app_id_and_icon():
    text = (ROOT / "packaging" / f"{APP_ID}.desktop").read_text()
    fields = dict(line.split("=", 1) for line in text.splitlines() if "=" in line)
    assert fields["Exec"] == f"@BIN@/{APP_ID}" and fields["Icon"] == APP_ID
    assert fields["StartupWMClass"] == APP_ID and fields["Terminal"] == "false"
    assert fields["Categories"].count(";") == 1
    overlay = (ROOT / "src" / "careertwin" / "interview" / "overlay.py").read_text()
    assert f'setDesktopFileName("{APP_ID}")' in overlay and f'setApplicationName("{APP_ID}")' in overlay


def test_the_icon_is_valid_svg_with_a_square_viewbox():
    root = ET.parse(ROOT / "packaging" / f"{APP_ID}.svg").getroot()
    assert root.tag.endswith("svg")
    assert re.fullmatch(r"0 0 (\d+) \1", root.attrib["viewBox"])


def test_scripts_are_executable_and_parse():
    for name in ("install.sh", "uninstall.sh"):
        script = ROOT / "scripts" / name
        assert os.access(script, os.X_OK)
        assert subprocess.run(["bash", "-n", str(script)], capture_output=True).returncode == 0


def test_the_installed_commands_exist_in_pyproject():
    pyproject = (ROOT / "pyproject.toml").read_text()
    for command in ("careertwin", "careertwin-mcp", APP_ID):
        assert re.search(rf'^{re.escape(command)} = "', pyproject, re.M)
    assert "[interview,rendering]" in (ROOT / "scripts" / "install.sh").read_text()
