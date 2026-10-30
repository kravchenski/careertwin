"""Releases are automatic: the version comes from the commit titles, and the workflow only wires it up."""

from __future__ import annotations

import importlib.util
import re
import subprocess
import tomllib
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("next_release", ROOT / "scripts" / "next_release.py")
next_release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(next_release)

LOCK = '''version = 1

[[package]]
name = "anyio"
version = "4.15.1"

[[package]]
name = "careertwin"
version = "0.1.2"
source = { editable = "." }

[[package]]
name = "httpx"
version = "0.28.1"
'''


CARGO_LOCK = '''version = 4

[[package]]
name = "anyhow"
version = "1.0.104"

[[package]]
name = "careertwin-overlay"
version = "0.1.2"
dependencies = [
 "anyhow",
]
'''


def run(root, *args):
    return subprocess.run(["git", "-C", str(root), *args], capture_output=True, text=True, check=True).stdout


@pytest.fixture
def repo(tmp_path):
    root = tmp_path / "repo"
    root.mkdir()
    run(root, "init", "-q", "-b", "master")
    run(root, "config", "user.name", "t")
    run(root, "config", "user.email", "t@example.com")
    run(root, "config", "commit.gpgsign", "false")
    (root / "pyproject.toml").write_text('[project]\nname = "careertwin"\nversion = "0.1.2"\ndependencies = ["httpx>=0.27"]\n')
    (root / "uv.lock").write_text(LOCK)
    run(root, "add", ".")
    commit(root, "chore: start")
    run(root, "tag", "v0.1.2")
    return root


def commit(root, subject, body=""):
    run(root, "commit", "-q", "--allow-empty", "-m", subject + (f"\n\n{body}" if body else ""))


def version_of(root):
    return next_release.decide(root)[0]


@pytest.mark.parametrize(
    ("subjects", "expected"),
    [
        (["feat: a new thing"], (0, 2, 0)),
        (["fix: a bug"], (0, 1, 3)),
        (["perf: faster"], (0, 1, 3)),
        (["fix: a bug", "feat: a thing", "fix: another"], (0, 2, 0)),  # the largest change wins, whatever the order
        (["feat(api): scoped"], (0, 2, 0)),
        (["feat!: drops a route"], (0, 2, 0)),  # breaking is a minor release while the major version is 0
        (["fix(api)!: changes a reply"], (0, 2, 0)),
        (["docs: words"], None),
        (["chore: tidy", "test: more", "ci: pin", "style: format", "refactor: move", "build: pack"], None),
        (["docs: words", "fix: a bug"], (0, 1, 3)),
        (["Update README", "wip"], None),  # titles that are not conventional release nothing
        (["feature: not a type", "fixes: not a type"], None),
        ([], None),
    ],
)
def test_the_version_follows_the_commit_titles(repo, subjects, expected):
    for subject in subjects:
        commit(repo, subject)
    assert version_of(repo) == expected


def test_a_breaking_change_in_the_body_counts(repo):
    commit(repo, "fix: tighten", "BREAKING CHANGE: the token is required")
    assert version_of(repo) == (0, 2, 0)


def test_after_version_one_a_breaking_change_is_a_major_release(repo):
    run(repo, "tag", "v1.4.2")
    commit(repo, "feat: small")
    assert version_of(repo) == (1, 5, 0)
    commit(repo, "feat!: big")
    assert version_of(repo) == (2, 0, 0)


def test_merge_commits_are_not_read_only_the_commits_inside_them(repo):
    run(repo, "switch", "-q", "-c", "topic")
    commit(repo, "feat: on the branch")
    run(repo, "switch", "-q", "master")
    run(repo, "merge", "-q", "--no-ff", "topic", "-m", "Merge pull request #9 from x/topic\n\nfix: the title of the pull request")
    assert version_of(repo) == (0, 2, 0)  # the feat inside; the merge's own title is not a feat either way


def test_a_prerelease_tag_is_not_a_base(repo):
    commit(repo, "feat: a thing")
    run(repo, "tag", "v0.9.0-rc1")
    run(repo, "tag", "not-a-version")
    assert version_of(repo) == (0, 2, 0)


def test_the_highest_tag_is_the_base_not_the_newest(repo):
    commit(repo, "chore: x")
    run(repo, "tag", "v0.10.0")
    commit(repo, "fix: y")
    run(repo, "tag", "v0.9.5")
    commit(repo, "fix: z")
    assert version_of(repo) == (0, 10, 1)


def test_a_commit_can_say_skip_and_a_release_commit_never_releases(repo):
    commit(repo, "feat: a thing [skip release]")
    version, reason = next_release.decide(repo)
    assert version is None and "skip release" in reason
    commit(repo, "feat: another")
    assert version_of(repo) == (0, 2, 0)
    commit(repo, "chore: release v0.2.0")
    version, reason = next_release.decide(repo)
    assert version is None and "release commit" in reason


def test_a_version_raised_by_hand_is_released_as_it_is(repo):
    (repo / "pyproject.toml").write_text(next_release.VERSION.sub('version = "0.3.0"', (repo / "pyproject.toml").read_text()))
    commit(repo, "chore: raise the version")
    version, reason = next_release.decide(repo)
    assert version == (0, 3, 0) and "by hand" in reason
    next_release.main(["--root", str(repo), "--apply"])
    assert 'version = "0.3.0"' in (repo / "pyproject.toml").read_text()  # nothing to rewrite
    assert 'name = "careertwin"\nversion = "0.1.2"' in (repo / "uv.lock").read_text()  # the lock follows only a bump


def test_without_any_tag_the_declared_version_is_the_first_release(tmp_path):
    root = tmp_path / "fresh"
    root.mkdir()
    run(root, "init", "-q", "-b", "master")
    run(root, "config", "user.name", "t")
    run(root, "config", "user.email", "t@example.com")
    (root / "pyproject.toml").write_text('[project]\nversion = "0.4.1"\n')
    run(root, "add", ".")
    run(root, "commit", "-q", "-m", "docs: start")
    assert next_release.decide(root) == ((0, 4, 1), "no release yet: releasing the declared version 0.4.1")


def test_applying_writes_the_version_into_the_project_and_the_lock_and_nothing_else(repo, capsys):
    commit(repo, "feat: a thing")
    assert next_release.main(["--root", str(repo), "--apply"]) == 0
    assert capsys.readouterr().out.splitlines()[0] == "version=0.2.0"
    project = tomllib.loads((repo / "pyproject.toml").read_text())
    assert project["project"]["version"] == "0.2.0" and project["project"]["dependencies"] == ["httpx>=0.27"]
    lock = (repo / "uv.lock").read_text()
    assert 'name = "careertwin"\nversion = "0.2.0"' in lock
    assert 'name = "anyio"\nversion = "4.15.1"' in lock and 'name = "httpx"\nversion = "0.28.1"' in lock
    assert tomllib.loads(lock)["package"][1]["version"] == "0.2.0"


def test_the_rust_window_gets_the_same_version(repo, capsys):
    overlay = repo / "overlay"
    overlay.mkdir()
    (overlay / "Cargo.toml").write_text(
        '[package]\nname = "careertwin-overlay"\nversion = "0.1.2"\n\n[dependencies]\nanyhow = { version = "1.0.104" }\n'
    )
    (overlay / "Cargo.lock").write_text(CARGO_LOCK)
    commit(repo, "feat: a thing")
    assert next_release.main(["--root", str(repo), "--apply"]) == 0
    manifest = tomllib.loads((overlay / "Cargo.toml").read_text())
    assert manifest["package"]["version"] == "0.2.0" and manifest["dependencies"]["anyhow"]["version"] == "1.0.104"
    lock = (overlay / "Cargo.lock").read_text()
    assert 'name = "careertwin-overlay"\nversion = "0.2.0"' in lock and 'name = "anyhow"\nversion = "1.0.104"' in lock
    assert [package["version"] for package in tomllib.loads(lock)["package"]] == ["1.0.104", "0.2.0"]


def test_a_repository_without_the_rust_window_still_releases(repo, capsys):
    commit(repo, "fix: a thing")
    assert next_release.main(["--root", str(repo), "--apply"]) == 0
    assert capsys.readouterr().out.splitlines()[0] == "version=0.1.3" and not (repo / "overlay").exists()


def test_without_apply_nothing_changes(repo, capsys):
    commit(repo, "feat: a thing")
    before = (repo / "pyproject.toml").read_text(), (repo / "uv.lock").read_text()
    next_release.main(["--root", str(repo)])
    assert capsys.readouterr().out.splitlines()[0] == "version=0.2.0"
    assert before == ((repo / "pyproject.toml").read_text(), (repo / "uv.lock").read_text())


def test_the_whole_sequence_the_workflow_runs_releases_once(repo, capsys):
    """decide -> commit the bump -> tag, then the next push finds nothing left to release."""
    commit(repo, "feat: a thing")
    commit(repo, "fix: a bug")
    next_release.main(["--root", str(repo), "--apply"])
    version = capsys.readouterr().out.splitlines()[0].split("=")[1]
    assert version == "0.2.0"
    run(repo, "commit", "-q", "-am", f"chore: release v{version}")
    run(repo, "tag", f"v{version}")
    assert next_release.decide(repo)[0] is None  # the release commit itself
    commit(repo, "docs: and a note")
    assert next_release.decide(repo)[0] is None  # only a doc since the tag
    commit(repo, "fix: one more")
    assert next_release.decide(repo)[0] == (0, 2, 1)


def test_the_output_is_one_line_per_key_so_it_can_go_to_github_output(repo, capsys):
    commit(repo, "feat: a thing\n\nwith a body\nover lines")
    next_release.main(["--root", str(repo)])
    lines = capsys.readouterr().out.splitlines()
    assert len(lines) == 2 and re.fullmatch(r"version=\d+\.\d+\.\d+", lines[0]) and lines[1].startswith("reason=")
    commit(repo, "docs: only docs")
    run(repo, "tag", "v0.2.0")
    next_release.main(["--root", str(repo)])
    assert capsys.readouterr().out.splitlines()[0] == "version="


# ---- the workflow that runs it -----------------------------------------------------------------


@pytest.fixture(scope="module")
def workflow():
    import yaml

    text = (ROOT / ".github" / "workflows" / "autorelease.yml").read_text()
    return text, yaml.safe_load(text)


def test_the_workflow_runs_on_master_only_and_is_serialised(workflow):
    text, data = workflow
    assert data[True] == {"push": {"branches": ["master"]}}
    assert data["concurrency"] == {"group": "auto-release", "cancel-in-progress": False}


def test_the_workflow_cannot_loop_and_can_be_switched_off(workflow):
    _, data = workflow
    condition = data["jobs"]["release"]["if"]
    assert "vars.AUTO_RELEASE != 'false'" in condition and "chore: release v" in condition


def test_the_workflow_is_read_only_except_for_the_one_job_that_must_write(workflow):
    _, data = workflow
    assert data["permissions"] == {"contents": "read"}
    assert data["jobs"]["release"]["permissions"] == {"contents": "write", "actions": "write"}
    assert list(data["jobs"]) == ["release"]


def test_the_tag_is_what_starts_the_build_and_the_version_goes_through_the_environment(workflow):
    text, data = workflow
    step = next(s for s in data["jobs"]["release"]["steps"] if s.get("name", "").startswith("Commit the version"))
    assert 'gh workflow run release.yml --ref "v$VERSION"' in step["run"]
    assert "${{" not in step["run"], "no expression is pasted into the script: values come in as environment variables"
    assert step["env"]["VERSION"] == "${{ steps.next.outputs.version }}"
    release = (ROOT / ".github" / "workflows" / "release.yml").read_text()
    assert "workflow_dispatch:" in release and "startsWith(github.ref, 'refs/tags/v')" in release  # a run on a tag publishes


def test_a_pull_request_cannot_reach_the_release_job(workflow):
    _, data = workflow
    assert set(data[True]) == {"push"}


def test_the_script_is_standard_library_only(workflow):
    source = (ROOT / "scripts" / "next_release.py").read_text()
    imports = set(re.findall(r"^(?:from|import) (\w+)", source, re.M))
    assert imports <= {"__future__", "argparse", "re", "subprocess", "sys", "pathlib"}  # the runner has no project environment


def test_the_documentation_describes_the_automatic_release():
    text = (ROOT / "CONTRIBUTING.md").read_text()
    assert "[skip release]" in text and "AUTO_RELEASE" in text and "scripts/next_release.py" in text
