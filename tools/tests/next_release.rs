//! Releases are automatic: the version comes from the commit titles. Each test builds a real git repository.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use careertwin_tools::release::{Version, decide, run};
use tempfile::TempDir;

const LOCK: &str = "version = 1\n\n[[package]]\nname = \"anyio\"\nversion = \"4.15.1\"\n\n[[package]]\nname = \"careertwin\"\nversion = \"0.1.2\"\nsource = { editable = \".\" }\n\n[[package]]\nname = \"httpx\"\nversion = \"0.28.1\"\n";

const CARGO_LOCK: &str = "version = 4\n\n[[package]]\nname = \"anyhow\"\nversion = \"1.0.104\"\n\n[[package]]\nname = \"careertwin-overlay\"\nversion = \"0.1.2\"\ndependencies = [\n \"anyhow\",\n]\n";

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .expect("git runs");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn init(dir: &Path) {
    git(dir, &["init", "-q", "-b", "master"]);
    git(dir, &["config", "user.name", "t"]);
    git(dir, &["config", "user.email", "t@example.com"]);
    git(dir, &["config", "commit.gpgsign", "false"]);
    git(dir, &["config", "tag.gpgsign", "false"]);
}

fn commit(root: &Path, subject: &str, body: &str) {
    let message = if body.is_empty() {
        subject.to_owned()
    } else {
        format!("{subject}\n\n{body}")
    };
    git(root, &["commit", "-q", "--allow-empty", "-m", &message]);
}

/// A repository with the project file, the lock and one release, v0.1.2.
fn repo() -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("repo");
    fs::create_dir(&root).unwrap();
    init(&root);
    fs::write(
        root.join("pyproject.toml"),
        "[project]\nname = \"careertwin\"\nversion = \"0.1.2\"\ndependencies = [\"httpx>=0.27\"]\n",
    )
    .unwrap();
    fs::write(root.join("uv.lock"), LOCK).unwrap();
    git(&root, &["add", "."]);
    commit(&root, "chore: start", "");
    git(&root, &["tag", "v0.1.2"]);
    (dir, root)
}

fn version_of(root: &Path) -> Option<Version> {
    decide(root).unwrap().0
}

fn command(root: &Path, extra: &[&str]) -> Vec<String> {
    let mut args = vec!["--root".to_owned(), root.to_string_lossy().into_owned()];
    args.extend(extra.iter().map(|arg| (*arg).to_owned()));
    args
}

fn output_of(root: &Path, extra: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    run(&command(root, extra), &mut out).unwrap();
    String::from_utf8(out)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

#[test]
fn the_version_follows_the_commit_titles() {
    type Case<'a> = (&'a [&'a str], Option<Version>);
    let cases: [Case; 13] = [
        (&["feat: a new thing"], Some((0, 2, 0))),
        (&["fix: a bug"], Some((0, 1, 3))),
        (&["perf: faster"], Some((0, 1, 3))),
        (
            &["fix: a bug", "feat: a thing", "fix: another"],
            Some((0, 2, 0)),
        ), // the largest change wins, whatever the order
        (&["feat(api): scoped"], Some((0, 2, 0))),
        (&["feat!: drops a route"], Some((0, 2, 0))), // breaking is a minor release while the major version is 0
        (&["fix(api)!: changes a reply"], Some((0, 2, 0))),
        (&["docs: words"], None),
        (
            &[
                "chore: tidy",
                "test: more",
                "ci: pin",
                "style: format",
                "refactor: move",
                "build: pack",
            ],
            None,
        ),
        (&["docs: words", "fix: a bug"], Some((0, 1, 3))),
        (&["Update README", "wip"], None), // titles that are not conventional release nothing
        (&["feature: not a type", "fixes: not a type"], None),
        (&[], None),
    ];
    for (subjects, expected) in cases {
        let (_dir, root) = repo();
        for subject in subjects {
            commit(&root, subject, "");
        }
        assert_eq!(version_of(&root), expected, "{subjects:?}");
    }
}

#[test]
fn a_breaking_change_in_the_body_counts() {
    let (_dir, root) = repo();
    commit(
        &root,
        "fix: tighten",
        "BREAKING CHANGE: the token is required",
    );
    assert_eq!(version_of(&root), Some((0, 2, 0)));
}

#[test]
fn after_version_one_a_breaking_change_is_a_major_release() {
    let (_dir, root) = repo();
    git(&root, &["tag", "v1.4.2"]);
    commit(&root, "feat: small", "");
    assert_eq!(version_of(&root), Some((1, 5, 0)));
    commit(&root, "feat!: big", "");
    assert_eq!(version_of(&root), Some((2, 0, 0)));
}

#[test]
fn merge_commits_are_not_read_only_the_commits_inside_them() {
    let (_dir, root) = repo();
    git(&root, &["switch", "-q", "-c", "topic"]);
    commit(&root, "feat: on the branch", "");
    git(&root, &["switch", "-q", "master"]);
    git(
        &root,
        &[
            "merge",
            "-q",
            "--no-ff",
            "topic",
            "-m",
            "Merge pull request #9 from x/topic\n\nfix: the title of the pull request",
        ],
    );
    assert_eq!(version_of(&root), Some((0, 2, 0))); // the feat inside; the merge's own title is not a feat either way
}

#[test]
fn a_prerelease_tag_is_not_a_base() {
    let (_dir, root) = repo();
    commit(&root, "feat: a thing", "");
    git(&root, &["tag", "v0.9.0-rc1"]);
    git(&root, &["tag", "not-a-version"]);
    assert_eq!(version_of(&root), Some((0, 2, 0)));
}

#[test]
fn the_highest_tag_is_the_base_not_the_newest() {
    let (_dir, root) = repo();
    commit(&root, "chore: x", "");
    git(&root, &["tag", "v0.10.0"]);
    commit(&root, "fix: y", "");
    git(&root, &["tag", "v0.9.5"]);
    commit(&root, "fix: z", "");
    assert_eq!(version_of(&root), Some((0, 10, 1)));
}

#[test]
fn a_commit_can_say_skip_and_a_release_commit_never_releases() {
    let (_dir, root) = repo();
    commit(&root, "feat: a thing [skip release]", "");
    let (version, reason) = decide(&root).unwrap();
    assert!(
        version.is_none() && reason.contains("skip release"),
        "{reason}"
    );
    commit(&root, "feat: another", "");
    assert_eq!(version_of(&root), Some((0, 2, 0)));
    commit(&root, "chore: release v0.2.0", "");
    let (version, reason) = decide(&root).unwrap();
    assert!(
        version.is_none() && reason.contains("release commit"),
        "{reason}"
    );
}

#[test]
fn a_version_raised_by_hand_is_released_as_it_is() {
    let (_dir, root) = repo();
    let project = fs::read_to_string(root.join("pyproject.toml")).unwrap();
    fs::write(
        root.join("pyproject.toml"),
        project.replace("0.1.2", "0.3.0"),
    )
    .unwrap();
    commit(&root, "chore: raise the version", "");
    let (version, reason) = decide(&root).unwrap();
    assert_eq!(version, Some((0, 3, 0)));
    assert!(reason.contains("by hand"), "{reason}");
    output_of(&root, &["--apply"]);
    assert!(
        fs::read_to_string(root.join("pyproject.toml"))
            .unwrap()
            .contains("version = \"0.3.0\"")
    ); // nothing to rewrite
    assert!(
        fs::read_to_string(root.join("uv.lock"))
            .unwrap()
            .contains("name = \"careertwin\"\nversion = \"0.1.2\"")
    ); // the lock follows only a bump
}

#[test]
fn without_any_tag_the_declared_version_is_the_first_release() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("fresh");
    fs::create_dir(&root).unwrap();
    init(&root);
    fs::write(
        root.join("pyproject.toml"),
        "[project]\nversion = \"0.4.1\"\n",
    )
    .unwrap();
    git(&root, &["add", "."]);
    commit(&root, "docs: start", "");
    assert_eq!(
        decide(&root).unwrap(),
        (
            Some((0, 4, 1)),
            "no release yet: releasing the declared version 0.4.1".to_owned()
        )
    );
}

#[test]
fn applying_writes_the_version_into_the_project_and_the_lock_and_nothing_else() {
    let (_dir, root) = repo();
    commit(&root, "feat: a thing", "");
    assert_eq!(output_of(&root, &["--apply"])[0], "version=0.2.0");
    assert_eq!(
        fs::read_to_string(root.join("pyproject.toml")).unwrap(),
        "[project]\nname = \"careertwin\"\nversion = \"0.2.0\"\ndependencies = [\"httpx>=0.27\"]\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("uv.lock")).unwrap(),
        LOCK.replace(
            "name = \"careertwin\"\nversion = \"0.1.2\"",
            "name = \"careertwin\"\nversion = \"0.2.0\""
        )
    );
}

#[test]
fn the_rust_window_gets_the_same_version() {
    let (_dir, root) = repo();
    let overlay = root.join("overlay");
    fs::create_dir(&overlay).unwrap();
    fs::write(overlay.join("Cargo.toml"), "[package]\nname = \"careertwin-overlay\"\nversion = \"0.1.2\"\n\n[dependencies]\nanyhow = { version = \"1.0.104\" }\n").unwrap();
    fs::write(overlay.join("Cargo.lock"), CARGO_LOCK).unwrap();
    commit(&root, "feat: a thing", "");
    assert_eq!(output_of(&root, &["--apply"])[0], "version=0.2.0");
    assert_eq!(
        fs::read_to_string(overlay.join("Cargo.toml")).unwrap(),
        "[package]\nname = \"careertwin-overlay\"\nversion = \"0.2.0\"\n\n[dependencies]\nanyhow = { version = \"1.0.104\" }\n"
    );
    assert_eq!(
        fs::read_to_string(overlay.join("Cargo.lock")).unwrap(),
        CARGO_LOCK.replace(
            "name = \"careertwin-overlay\"\nversion = \"0.1.2\"",
            "name = \"careertwin-overlay\"\nversion = \"0.2.0\""
        )
    );
}

#[test]
fn a_repository_without_the_rust_window_still_releases() {
    let (_dir, root) = repo();
    commit(&root, "fix: a thing", "");
    assert_eq!(output_of(&root, &["--apply"])[0], "version=0.1.3");
    assert!(!root.join("overlay").exists());
}

#[test]
fn without_apply_nothing_changes() {
    let (_dir, root) = repo();
    commit(&root, "feat: a thing", "");
    let before = (
        fs::read_to_string(root.join("pyproject.toml")).unwrap(),
        fs::read_to_string(root.join("uv.lock")).unwrap(),
    );
    assert_eq!(output_of(&root, &[])[0], "version=0.2.0");
    assert_eq!(
        before,
        (
            fs::read_to_string(root.join("pyproject.toml")).unwrap(),
            fs::read_to_string(root.join("uv.lock")).unwrap()
        )
    );
}

#[test]
fn the_whole_sequence_the_workflow_runs_releases_once() {
    // decide -> commit the bump -> tag, then the next push finds nothing left to release
    let (_dir, root) = repo();
    commit(&root, "feat: a thing", "");
    commit(&root, "fix: a bug", "");
    let version = output_of(&root, &["--apply"])[0]
        .split('=')
        .nth(1)
        .unwrap()
        .to_owned();
    assert_eq!(version, "0.2.0");
    git(
        &root,
        &["commit", "-q", "-am", &format!("chore: release v{version}")],
    );
    git(&root, &["tag", &format!("v{version}")]);
    assert!(version_of(&root).is_none()); // the release commit itself
    commit(&root, "docs: and a note", "");
    assert!(version_of(&root).is_none()); // only a doc since the tag
    commit(&root, "fix: one more", "");
    assert_eq!(version_of(&root), Some((0, 2, 1)));
}

#[test]
fn the_output_is_one_line_per_key_so_it_can_go_to_github_output() {
    let (_dir, root) = repo();
    commit(&root, "feat: a thing", "with a body\nover lines");
    let lines = output_of(&root, &[]);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0], "version=0.2.0");
    assert!(lines[1].starts_with("reason="), "{lines:?}");
    commit(&root, "docs: only docs", "");
    git(&root, &["tag", "v0.2.0"]);
    assert_eq!(output_of(&root, &[])[0], "version=");
}

#[test]
fn a_directory_that_is_not_a_repository_is_an_error_not_a_release() {
    let dir = tempfile::tempdir().unwrap();
    let mut out = Vec::new();
    let error = run(&command(dir.path(), &[]), &mut out).unwrap_err();
    assert!(error.contains("git"), "{error}");
    assert!(out.is_empty());
}
