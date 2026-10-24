"""The Windows setup program, the macOS app and the release workflow that makes them."""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

import pytest

from careertwin.interview import audio, screenshot

ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = (ROOT / ".github/workflows/release.yml").read_text()
ISS = (ROOT / "packaging/windows/installer.iss").read_text()
MAC_SCRIPT = (ROOT / "scripts/build_macos.sh").read_text()


# ---- the app on a Mac -------------------------------------------------------


def test_listening_is_off_by_default_only_on_a_mac():
    code = "import sys; sys.platform = {!r}; from careertwin.interview.settings import InterviewSettings; print(InterviewSettings().listen)"
    for platform, expected in (("darwin", "False"), ("linux", "True")):  # win32 cannot be faked in a subprocess
        result = subprocess.run([sys.executable, "-c", code.format(platform)], capture_output=True, text=True, cwd=ROOT)
        assert result.stdout.strip() == expected, (platform, result.stderr)


def test_call_audio_on_a_mac_says_plainly_that_it_is_not_there_yet(monkeypatch):
    monkeypatch.setattr(audio.sys, "platform", "darwin")
    with pytest.raises(audio.AudioError, match="not available on macOS yet") as caught:
        audio.default_sources()
    assert "pipewire" not in str(caught.value).lower()
    with pytest.raises(audio.AudioError, match="macOS"):
        audio.list_sources()


def test_a_mac_takes_screenshots_with_its_own_tool(monkeypatch, tmp_path):
    monkeypatch.setattr(screenshot, "_windows", lambda: False)
    monkeypatch.delenv("WAYLAND_DISPLAY", raising=False)
    monkeypatch.setattr(
        screenshot.shutil, "which", lambda tool: "/usr/sbin/screencapture" if tool == "screencapture" else None
    )
    assert screenshot.available_tools() == ["screencapture"]
    seen = []

    def run(command, **kwargs):
        seen.append(command)
        Path(command[-1]).write_bytes(b"\x89PNG-data")
        return subprocess.CompletedProcess(command, 0, stdout="", stderr="")

    monkeypatch.setattr(screenshot.subprocess, "run", run)
    assert screenshot.capture_png() == b"\x89PNG-data"
    assert seen[0][:2] == ["screencapture", "-x"]


def test_screencapture_is_not_offered_on_wayland(monkeypatch):
    monkeypatch.setattr(screenshot, "_windows", lambda: False)
    monkeypatch.setenv("WAYLAND_DISPLAY", "wayland-1")
    monkeypatch.setattr(screenshot.shutil, "which", lambda tool: f"/usr/bin/{tool}")
    assert "screencapture" not in screenshot.available_tools()


# ---- the macOS build ----------------------------------------------------------


def test_the_mac_script_is_valid_shell_and_executable():
    assert subprocess.run(["bash", "-n", str(ROOT / "scripts/build_macos.sh")]).returncode == 0
    assert (ROOT / "scripts/build_macos.sh").stat().st_mode & 0o111


def test_the_mac_app_has_what_macos_needs_to_start_it():
    for key in ("CFBundleExecutable", "CFBundleIdentifier", "CFBundleIconFile", "NSHighResolutionCapable",
                "NSMicrophoneUsageDescription", "LSMinimumSystemVersion"):
        assert f"<key>{key}</key>" in MAC_SCRIPT
    assert "codesign --force --deep --sign -" in MAC_SCRIPT and "hdiutil create" in MAC_SCRIPT
    assert "for attempt in 1 2 3 4 5" in MAC_SCRIPT  # hdiutil is flaky on the runners
    assert 'ln -s /Applications' in MAC_SCRIPT and "iconutil -c icns" in MAC_SCRIPT
    assert "Application Support/CareerTwin Interview" in MAC_SCRIPT  # data survives replacing the app
    assert (ROOT / "packaging/macos/README-macOS.txt").read_text().count("Right-click") == 1


def test_the_mac_icon_source_exists_and_the_dmg_name_matches_the_workflow():
    assert (ROOT / "packaging/careertwin-interview.png").is_file()
    assert "CareerTwin-Interview-$VERSION-macos-$ARCH.dmg" in MAC_SCRIPT
    assert "dist/macos/*.dmg" in WORKFLOW


# ---- the Windows setup --------------------------------------------------------


def test_the_setup_installs_per_user_and_starts_the_program_the_way_the_launcher_does():
    assert "PrivilegesRequired=lowest" in ISS and "ArchitecturesAllowed=x64compatible" in ISS
    assert re.search(r"AppId=\{\{[0-9A-F-]{36}\}", ISS)
    launcher = (ROOT / "packaging/windows/Start CareerTwin Interview.bat").read_text()
    assert r"python\pythonw.exe" in launcher and "start.pyw" in launcher
    assert r'Filename: "{app}\python\pythonw.exe"; Parameters: """{app}\start.pyw"""' in ISS
    assert (ROOT / "packaging/windows/start.pyw").is_file() and (ROOT / "packaging/windows/careertwin-interview.ico").is_file()


def test_an_update_and_an_uninstall_leave_the_users_data_alone():
    assert 'Excludes: "data\\*,__pycache__"' in ISS
    uninstall = ISS.split("[UninstallDelete]")[1]
    assert '{app}\\python"' in uninstall and '{app}\\site-packages"' in uninstall
    assert "{app}\\data" not in uninstall and "{app}\\*" not in uninstall


def test_the_setup_file_name_matches_what_the_workflow_looks_for():
    name = re.search(r"OutputBaseFilename=(\S+)", ISS)[1]
    assert name == "CareerTwin-Interview-{#AppVersion}-windows-x64-setup"
    assert "*-setup.exe" in WORKFLOW


# ---- the workflow ----------------------------------------------------------------


def test_every_installer_is_built_checked_and_published():
    import yaml

    jobs = yaml.safe_load(WORKFLOW)["jobs"]
    assert list(jobs) == ["windows", "windows-installer", "macos", "publish"]
    assert jobs["windows-installer"]["runs-on"] == "windows-latest" and jobs["macos"]["runs-on"].startswith("macos-")
    assert jobs["publish"]["needs"] == ["windows", "windows-installer", "macos"]
    assert jobs["publish"]["if"] == "startsWith(github.ref, 'refs/tags/v')"
    assert jobs["publish"]["permissions"] == {"contents": "write"}
    steps = {job: [step.get("name", "") for step in data["steps"]] for job, data in jobs.items()}
    assert "Install it, start it, uninstall it" in steps["windows-installer"]
    assert "Open the dmg and start the app inside it" in steps["macos"]


def test_a_change_to_any_packaging_file_runs_the_build_without_publishing():
    import yaml

    triggers = yaml.safe_load(WORKFLOW)[True]
    assert {"scripts/build_windows.sh", "scripts/build_macos.sh", "packaging/**"} <= set(triggers["pull_request"]["paths"])
    assert triggers["push"]["tags"] == ["v*"] and "workflow_dispatch" in triggers


def test_the_mac_and_windows_checks_start_the_same_window_the_user_will_see():
    for needle in ("OverlayWindow(Bridge())", "faster_whisper", "QT_QPA_PLATFORM", "offscreen"):
        assert WORKFLOW.count(needle) >= 2, needle
    assert "--help" in WORKFLOW and "last-run.log" in WORKFLOW
    assert WORKFLOW.count("CAREERTWIN_DATA_DIR") >= 1  # the dmg is read-only: the check must say where the data goes


def test_the_release_notes_do_not_promise_what_was_not_tried():
    notes = WORKFLOW.split('notes="')[1].split('"\n')[0]
    assert "not signed by Apple" in notes and "Call audio is not on the Mac yet" in notes
    assert "expected to work" in notes and "SHA256SUMS.txt" in notes
