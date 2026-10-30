//! Decide the next release from the commits since the last tag; optionally write the version into the project.
//!
//! ```text
//! next-release            say what would happen, change nothing
//! next-release --apply    also write the version into pyproject.toml, uv.lock and overlay/
//! ```
//!
//! It prints `key=value` lines (`version=`, `reason=`), so the output can go straight to `$GITHUB_OUTPUT`. An empty
//! `version` means: no release.
//!
//! The rules (conventional commits, as in CONTRIBUTING.md), counted over the commits since the last `vX.Y.Z` tag:
//!
//! * `feat:` makes a minor release, `fix:` and `perf:` a patch release, and a breaking change (`feat!:` or
//!   `BREAKING CHANGE:` in the body) a minor release while the major version is 0, a major one after that;
//! * `docs:`, `chore:`, `test:`, `ci:`, `style:`, `refactor:` and `build:` alone make no release;
//! * a version raised by hand in pyproject.toml (above the last tag) is released as it is;
//! * a commit that says `[skip release]`, and the commit that records a release, make none.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

pub type Version = (u32, u32, u32);

const USAGE: &str = "usage: next-release [--apply] [--root DIR]\n\n  --apply     write the version into pyproject.toml, uv.lock and overlay/\n  --root DIR  the repository (default: the current directory)";

pub fn text(version: Version) -> String {
    format!("{}.{}.{}", version.0, version.1, version.2)
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|error| format!("could not run git: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// `1.2.3` as numbers; anything else (a fourth part, a sign, a suffix such as `-rc1`) is not a version.
fn parse_version(text: &str) -> Option<Version> {
    let mut parts = text.split('.');
    let mut next = || {
        let part = parts.next()?;
        if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        part.parse::<u32>().ok()
    };
    let version = (next()?, next()?, next()?);
    parts.next().is_none().then_some(version)
}

/// The version on a line that says exactly `version = "X.Y.Z"`.
fn version_line(line: &str) -> Option<Version> {
    parse_version(line.strip_prefix("version = \"")?.strip_suffix('"')?)
}

/// The highest plain `vX.Y.Z` tag (a pre-release such as v0.2.0-rc1 is not a base for the next version).
fn last_tag(root: &Path) -> Result<Option<(String, Version)>, String> {
    let tags = git(root, &["tag", "--list", "v[0-9]*"])?;
    Ok(tags
        .split_whitespace()
        .filter_map(|name| Some((parse_version(name.strip_prefix('v')?)?, name)))
        .max()
        .map(|(version, name)| (name.to_owned(), version)))
}

fn declared(root: &Path) -> Result<Version, String> {
    let path = root.join("pyproject.toml");
    let content = fs::read_to_string(&path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    content
        .lines()
        .find_map(version_line)
        .ok_or_else(|| "pyproject.toml has no `version = \"X.Y.Z\"` line".to_owned())
}

/// The type of a conventional commit title (`feat(scope)!: …`) and whether it is marked as breaking.
fn conventional(subject: &str) -> Option<(&str, bool)> {
    let kind_end = subject
        .find(|c: char| !c.is_ascii_lowercase())
        .unwrap_or(subject.len());
    if kind_end == 0 {
        return None;
    }
    let (kind, mut rest) = subject.split_at(kind_end);
    if let Some(scoped) = rest.strip_prefix('(') {
        rest = &scoped[scoped.find(')')? + 1..];
    }
    let (breaking, rest) = match rest.strip_prefix('!') {
        Some(after) => (true, after),
        None => (false, rest),
    };
    rest.starts_with(':').then_some((kind, breaking))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Level {
    Patch,
    Minor,
    Major,
}

/// The biggest change in the commits after `since`, merge commits left out.
fn level(root: &Path, since: &str, major: u32) -> Result<Option<Level>, String> {
    let range = format!("{since}..HEAD");
    let raw = git(
        root,
        &["log", "--no-merges", "--format=%s%x1f%b%x1e", &range],
    )?;
    let mut found = None;
    for entry in raw.split('\x1e') {
        let (subject, body) = entry
            .trim_matches('\n')
            .split_once('\x1f')
            .unwrap_or((entry.trim_matches('\n'), ""));
        let parsed = conventional(subject.trim());
        if parsed.is_some_and(|(_, breaking)| breaking) || body.contains("BREAKING CHANGE:") {
            return Ok(Some(if major >= 1 {
                Level::Major
            } else {
                Level::Minor
            }));
        }
        match parsed {
            Some(("feat", _)) => found = Some(Level::Minor),
            Some(("fix" | "perf", _)) if found.is_none() => found = Some(Level::Patch),
            _ => {}
        }
    }
    Ok(found)
}

fn bump((major, minor, patch): Version, how: Level) -> Version {
    match how {
        Level::Major => (major + 1, 0, 0),
        Level::Minor => (major, minor + 1, 0),
        Level::Patch => (major, minor, patch + 1),
    }
}

/// The version to release (if any) and why.
pub fn decide(root: &Path) -> Result<(Option<Version>, String), String> {
    let head = git(root, &["log", "-1", "--format=%B"])?;
    let head = head.trim();
    if head.contains("[skip release]") {
        return Ok((None, "the last commit says [skip release]".to_owned()));
    }
    if head.starts_with("chore: release v") {
        return Ok((None, "the last commit is a release commit".to_owned()));
    }
    let tag = last_tag(root)?;
    let current = declared(root)?;
    let Some((name, base)) = tag else {
        return Ok((
            Some(current),
            format!(
                "no release yet: releasing the declared version {}",
                text(current)
            ),
        ));
    };
    if current > base {
        return Ok((
            Some(current),
            format!(
                "the version was raised by hand to {} (last release {name})",
                text(current)
            ),
        ));
    }
    let Some(how) = level(root, &name, base.0)? else {
        return Ok((
            None,
            format!("nothing to release since {name}: only docs, chores, tests or refactors"),
        ));
    };
    let (word, why) = match how {
        Level::Major => ("major", "breaking changes"),
        Level::Minor => ("minor", "new features"),
        Level::Patch => ("patch", "fixes"),
    };
    Ok((
        Some(bump(base, how)),
        format!("{word} release: {why} since {name}"),
    ))
}

/// `content` with the first line that says exactly `version = "X.Y.Z"` given the new version.
fn with_version(content: &str, new: &str) -> String {
    let mut done = false;
    let mut out = String::with_capacity(content.len());
    for line in content.split_inclusive('\n') {
        let bare = line.strip_suffix('\n').unwrap_or(line);
        if !done && version_line(bare).is_some() {
            done = true;
            out.push_str(&format!("version = \"{new}\"{}", &line[bare.len()..]));
        } else {
            out.push_str(line);
        }
    }
    out
}

/// A lock file with the version of the package `name` (the first one by that name) replaced.
fn with_package_version(content: &str, name: &str, new: &str) -> String {
    let head = format!("name = \"{name}\"\nversion = \"");
    for (start, _) in content.match_indices(&head) {
        if start != 0 && !content[..start].ends_with('\n') {
            continue;
        }
        let from = start + head.len();
        if let Some(length) = content[from..].find('"').filter(|length| *length > 0) {
            return format!("{}{}{}", &content[..from], new, &content[from + length..]);
        }
    }
    content.to_owned()
}

fn rewrite(path: &Path, change: impl Fn(&str) -> String) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let content = fs::read_to_string(path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    fs::write(path, change(&content))
        .map_err(|error| format!("could not write {}: {error}", path.display()))
}

/// Put the version into pyproject.toml and uv.lock (the line of the project's own package), and into the Rust
/// window: overlay/Cargo.toml and overlay/Cargo.lock. The window reports this version and compares it with the
/// latest release to tell when an update is there.
pub fn write(root: &Path, version: Version) -> Result<(), String> {
    let new = text(version);
    rewrite(&root.join("pyproject.toml"), |content| {
        with_version(content, &new)
    })?;
    rewrite(&root.join("uv.lock"), |content| {
        with_package_version(content, "careertwin", &new)
    })?;
    let overlay = root.join("overlay");
    rewrite(&overlay.join("Cargo.toml"), |content| {
        with_version(content, &new)
    })?;
    rewrite(&overlay.join("Cargo.lock"), |content| {
        with_package_version(content, "careertwin-overlay", &new)
    })
}

/// What the command does with its arguments; the two lines go to `out`.
pub fn run(args: &[String], out: &mut dyn Write) -> Result<(), String> {
    let mut apply = false;
    let mut root = PathBuf::from(".");
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--apply" => apply = true,
            "--root" => root = args.next().ok_or("--root needs a directory")?.into(),
            "-h" | "--help" => {
                writeln!(out, "{USAGE}").map_err(|error| error.to_string())?;
                return Ok(());
            }
            other => match other.strip_prefix("--root=") {
                Some(dir) => root = dir.into(),
                None => return Err(format!("unknown argument {other}\n{USAGE}")),
            },
        }
    }
    let (version, reason) = decide(&root)?;
    if let Some(version) = version
        && apply
        && version != declared(&root)?
    {
        write(&root, version)?;
    }
    let shown = version.map(text).unwrap_or_default();
    writeln!(out, "version={shown}\nreason={reason}").map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_title_is_conventional_with_a_type_an_optional_scope_and_a_colon() {
        assert_eq!(conventional("feat: a thing"), Some(("feat", false)));
        assert_eq!(conventional("feat(api): scoped"), Some(("feat", false)));
        assert_eq!(conventional("feat!: drops a route"), Some(("feat", true)));
        assert_eq!(
            conventional("fix(api)!: changes a reply"),
            Some(("fix", true))
        );
        for not in [
            "Update README",
            "wip",
            "Feat: capital",
            "feat(oops: x",
            "feat - x",
            "feat",
            ": x",
            "feat()x: y",
        ] {
            assert_eq!(conventional(not), None, "{not}");
        }
        assert_eq!(conventional("feat(): empty scope"), Some(("feat", false)));
    }

    #[test]
    fn a_version_is_three_plain_numbers() {
        assert_eq!(parse_version("0.10.2"), Some((0, 10, 2)));
        for not in [
            "0.1",
            "0.1.2.3",
            "0.1.x",
            "0.1.2-rc1",
            "+1.2.3",
            "",
            "..",
            "1..2",
        ] {
            assert_eq!(parse_version(not), None, "{not}");
        }
        assert_eq!(version_line("version = \"0.6.0\""), Some((0, 6, 0)));
        assert_eq!(version_line("version = \"0.6\""), None);
        assert_eq!(version_line("  version = \"0.6.0\""), None);
        assert_eq!(version_line("rust-version = \"1.88.0\""), None);
    }

    #[test]
    fn only_the_first_version_line_of_a_manifest_changes() {
        let manifest = "[package]\nname = \"x\"\nversion = \"0.1.2\"\n\n[dependencies]\na = { version = \"1.0.0\" }\nversion = \"9.9.9\"\n";
        assert_eq!(
            with_version(manifest, "0.2.0"),
            "[package]\nname = \"x\"\nversion = \"0.2.0\"\n\n[dependencies]\na = { version = \"1.0.0\" }\nversion = \"9.9.9\"\n"
        );
        assert_eq!(
            with_version("version = \"0.1.2\"", "0.2.0"),
            "version = \"0.2.0\"",
            "no newline at the end"
        );
        assert_eq!(
            with_version("[package]\r\nversion = \"0.1.2\"\r\n", "0.2.0"),
            "[package]\r\nversion = \"0.1.2\"\r\n",
            "a CR is not a version line"
        );
    }

    #[test]
    fn a_lock_file_changes_the_one_package_and_no_other_with_a_similar_name() {
        let lock = "[[package]]\nname = \"careertwin-extra\"\nversion = \"1.0.0\"\n\n[[package]]\nname = \"careertwin\"\nversion = \"0.1.2\"\n\n[[package]]\nname = \"careertwin\"\nversion = \"7.7.7\"\n";
        assert_eq!(
            with_package_version(lock, "careertwin", "0.2.0"),
            "[[package]]\nname = \"careertwin-extra\"\nversion = \"1.0.0\"\n\n[[package]]\nname = \"careertwin\"\nversion = \"0.2.0\"\n\n[[package]]\nname = \"careertwin\"\nversion = \"7.7.7\"\n"
        );
        assert_eq!(
            with_package_version(
                "# name = \"careertwin\"\nversion = \"1.0.0\"\n",
                "careertwin",
                "9.9.9"
            ),
            "# name = \"careertwin\"\nversion = \"1.0.0\"\n"
        );
        assert_eq!(
            with_package_version("nothing here", "careertwin", "9.9.9"),
            "nothing here"
        );
    }

    #[test]
    fn arguments_are_checked() {
        let mut out = Vec::new();
        assert!(
            run(&["--nope".to_owned()], &mut out)
                .unwrap_err()
                .contains("unknown argument --nope")
        );
        assert!(
            run(&["--root".to_owned()], &mut out)
                .unwrap_err()
                .contains("needs a directory")
        );
        run(&["--help".to_owned()], &mut out).unwrap();
        assert!(
            String::from_utf8(out)
                .unwrap()
                .starts_with("usage: next-release")
        );
    }
}
