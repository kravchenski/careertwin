"""The packaging of the Rust window (Windows setup and zip, macOS app, Linux tarball), the release workflow that
makes them, and what the Python window still does on a Mac."""

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
PACK = (ROOT / "scripts/package_overlay.sh").read_text()


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


# ---- the macOS and Linux packaging -----------------------------------------------


def test_the_packaging_script_is_valid_shell_and_executable():
    assert subprocess.run(["bash", "-n", str(ROOT / "scripts/package_overlay.sh")]).returncode == 0
    assert (ROOT / "scripts/package_overlay.sh").stat().st_mode & 0o111
    refused = subprocess.run([str(ROOT / "scripts/package_overlay.sh"), "plan9"], capture_output=True, text=True, env={"SKIP_BUILD": "1", "PATH": "/usr/bin:/bin"})
    assert refused.returncode != 0


def test_the_mac_app_has_what_macos_needs_to_start_it():
    for key in ("CFBundleExecutable", "CFBundleIdentifier", "CFBundleIconFile", "NSHighResolutionCapable",
                "NSMicrophoneUsageDescription", "LSMinimumSystemVersion"):
        assert f"<key>{key}</key>" in PACK
    assert "codesign --force --deep --sign -" in PACK and "hdiutil create" in PACK
    assert "for attempt in 1 2 3 4 5" in PACK  # hdiutil is flaky on the runners
    assert "ln -s /Applications" in PACK and "iconutil -c icns" in PACK
    assert "Application Support/CareerTwin Interview" in PACK  # data survives replacing the app
    assert (ROOT / "packaging/macos/README-macOS.txt").read_text().count("Right-click") == 1


def test_the_mac_icon_source_exists_and_the_file_names_match_the_workflow():
    assert (ROOT / "packaging/careertwin-interview.png").is_file()
    assert "CareerTwin-Interview-$LABEL-macos-$ARCH.dmg" in PACK and "dist/macos/*.dmg" in WORKFLOW
    assert "CareerTwin-Interview-$LABEL-linux-$ARCH.tar.gz" in PACK and "dist/linux/*.tar.gz" in WORKFLOW


def test_the_linux_tarball_has_the_window_the_ctl_command_and_an_installer():
    for name in ("careertwin-overlay", "careertwin-interview-ctl", "install.sh", "careertwin-interview.desktop", "README.txt"):
        assert name in PACK
    assert 'careertwin-overlay" ctl "$@"' in PACK
    assert (ROOT / "packaging/careertwin-interview.desktop").read_text().count("@BIN@") == 1


def test_the_old_python_packaging_is_gone():
    for name in ("scripts/build_windows.sh", "scripts/build_macos.sh", "packaging/windows/start.pyw",
                 "packaging/windows/Start CareerTwin Interview.bat"):
        assert not (ROOT / name).exists(), name


# ---- the Windows setup --------------------------------------------------------


def test_the_setup_installs_per_user_and_starts_the_program():
    assert "PrivilegesRequired=lowest" in ISS and "ArchitecturesAllowed=x64compatible" in ISS
    assert re.search(r"AppId=\{\{[0-9A-F-]{36}\}", ISS)
    assert '#define AppExe "careertwin-overlay.exe"' in ISS
    assert 'Filename: "{app}\\{#AppExe}"' in ISS and "pythonw" not in ISS
    assert (ROOT / "packaging/windows/careertwin-interview.ico").is_file()


def test_an_update_and_an_uninstall_leave_the_users_data_alone():
    assert 'Excludes: "data\\*"' in ISS and "uninsneveruninstall" in ISS
    assert "[UninstallDelete]" not in ISS  # the uninstaller removes only what Setup installed


def test_installing_over_the_python_build_removes_its_files_and_keeps_the_same_identity():
    install_delete = ISS.split("[InstallDelete]")[1].split("[Icons]")[0]
    assert '{app}\\python"' in install_delete and '{app}\\site-packages"' in install_delete
    assert "10B902B4-D15B-43B5-8AB0-B2BCA40C9DCA" in ISS  # the AppId of the earlier setup


def test_the_setup_file_name_matches_what_the_workflow_looks_for():
    name = re.search(r"OutputBaseFilename=(\S+)", ISS)[1]
    assert name == "CareerTwin-Interview-{#AppVersion}-windows-x64-setup"
    assert "*-setup.exe" in WORKFLOW


# ---- the workflow ----------------------------------------------------------------


def test_every_package_is_built_checked_and_published():
    import yaml

    jobs = yaml.safe_load(WORKFLOW)["jobs"]
    assert list(jobs) == ["windows", "macos", "linux", "publish"]
    assert jobs["windows"]["runs-on"] == "windows-latest" and jobs["macos"]["runs-on"].startswith("macos-")
    assert jobs["linux"]["runs-on"].startswith("ubuntu-")
    assert jobs["publish"]["needs"] == ["windows", "macos", "linux"]
    assert jobs["publish"]["if"] == "startsWith(github.ref, 'refs/tags/v')"
    assert jobs["publish"]["permissions"] == {"contents": "write"}
    steps = {job: [step.get("name", "") for step in data["steps"]] for job, data in jobs.items()}
    assert "Install it, check it, uninstall it" in steps["windows"]
    assert "The program reports its version" in steps["windows"]
    assert "Open the dmg and start the app inside it" in steps["macos"]
    assert "Unpack it and ask it for its version" in steps["linux"]


def test_the_tag_must_match_the_version_of_the_window_on_every_system():
    import yaml

    jobs = yaml.safe_load(WORKFLOW)["jobs"]
    for job in ("windows", "macos", "linux"):
        step = next(step for step in jobs[job]["steps"] if step.get("name") == "Tag matches the version of the window")
        assert step["if"] == "startsWith(github.ref, 'refs/tags/v')" and "overlay/Cargo.toml" in step["run"]
    assert WORKFLOW.count("does not match version") == 3


def test_a_change_to_any_packaging_file_runs_the_build_without_publishing():
    import yaml

    triggers = yaml.safe_load(WORKFLOW)[True]
    assert {"scripts/package_overlay.sh", "packaging/**"} <= set(triggers["pull_request"]["paths"])
    assert triggers["push"]["tags"] == ["v*"] and "workflow_dispatch" in triggers


def test_the_checks_start_the_program_the_user_will_run():
    assert WORKFLOW.count("--version") >= 3 and "careertwin-overlay.exe" in WORKFLOW
    assert "--help" in WORKFLOW and "codesign --verify" in WORKFLOW
    assert "LIBCLANG_PATH" in WORKFLOW  # whisper.cpp's bindings need libclang on Windows
    assert WORKFLOW.count("--locked") >= 1


def test_the_release_notes_do_not_promise_what_was_not_tried():
    notes = WORKFLOW.split('notes="')[1].split('"\n')[0]
    assert "not signed by Apple" in notes and "Call audio is not on the Mac yet" in notes
    assert "Nobody has tried the Windows and macOS builds by hand yet" in notes and "SHA256SUMS.txt" in notes


def test_the_overlay_workflow_compiles_and_lints_on_every_system():
    import yaml

    overlay = yaml.safe_load((ROOT / ".github/workflows/overlay.yml").read_text())["jobs"]["test"]
    assert overlay["strategy"]["matrix"]["os"] == ["ubuntu-latest", "windows-latest", "macos-15"]
    commands = " ".join(step.get("run", "") for step in overlay["steps"])
    assert "cargo clippy --all-targets -- -D warnings" in commands and "cargo test --locked" in commands


# ---- NixOS -------------------------------------------------------------------------------------------------------------


def test_the_flake_builds_the_rust_window_and_reads_its_version_and_lock_from_the_crate():
    flake = (ROOT / "flake.nix").read_text()
    package = (ROOT / "nix/package.nix").read_text()
    assert "careertwin-overlay = pkgs.callPackage ./nix/package.nix" in flake and "devShells" in flake
    assert "fromTOML (builtins.readFile ../overlay/Cargo.toml)" in package  # one version, the release's
    assert "cargoLock.lockFile = ../overlay/Cargo.lock" in package
    assert "mainProgram = \"careertwin-overlay\"" in package and "careertwin-interview-ctl" in package


def test_the_window_is_wrapped_with_the_libraries_it_opens_and_the_tools_it_starts():
    runtime = (ROOT / "nix/runtime.nix").read_text()
    package = (ROOT / "nix/package.nix").read_text()
    for needed in ("wayland", "libxkbcommon", "libGL", "pulseaudio", "grim", "xdg-utils", "/run/opengl-driver/lib"):
        assert needed in runtime, needed
    assert "--prefix LD_LIBRARY_PATH" in package and "--suffix PATH" in package  # the system's own tools come first


def test_the_nix_workflow_builds_the_package_and_runs_it():
    import yaml

    workflow = yaml.safe_load((ROOT / ".github/workflows/nix.yml").read_text())
    steps = [step.get("name", "") for step in workflow["jobs"]["build"]["steps"]]
    assert "Build the window" in steps and "It starts and says its version" in steps
    assert "nix build .#careertwin-overlay" in (ROOT / ".github/workflows/nix.yml").read_text()
