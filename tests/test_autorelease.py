"""The workflow that makes releases automatic. The rules that choose the version are tested where the tool lives, in
tools/tests/next_release.rs (it builds real git repositories); this file checks that the workflow wires it up safely."""

from __future__ import annotations

from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]


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


def test_the_tool_is_built_from_its_lock_and_a_failure_does_not_read_as_nothing_to_release(workflow):
    _, data = workflow
    step = next(s for s in data["jobs"]["release"]["steps"] if s.get("id") == "next")
    assert "--manifest-path tools/Cargo.toml --bin next-release" in step["run"] and "--locked" in step["run"]
    assert step["shell"] == "bash", "bash runs with pipefail: a failed build must fail the step, not leave the version empty"
    assert "GITHUB_OUTPUT" in step["run"]


def test_the_tool_has_no_dependencies_so_a_bare_runner_builds_it_at_once():
    manifest = (ROOT / "tools" / "Cargo.toml").read_text()
    default = manifest.split("[dependencies]")[0]
    assert 'default = []' in default, "the link checker's dependencies are behind a feature the release does not use"
    assert (ROOT / "tools" / "Cargo.lock").exists()


def test_the_documentation_describes_the_automatic_release():
    text = (ROOT / "CONTRIBUTING.md").read_text()
    assert "[skip release]" in text and "AUTO_RELEASE" in text and "tools/Cargo.toml" in text and "next-release" in text
