//! A project you will defend: read once into a dossier, searched for every question.
//!
//! The folder is read locally (secrets and binaries skipped, tokens blanked), the AI writes a dossier of it once, and
//! from then on every answer sees the dossier plus the few code excerpts that best match the question. Finding the
//! excerpts is a keyword search over the project's own text, so it costs milliseconds and never waits for the network.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{LazyLock, OnceLock};

use fancy_regex::Regex;
use serde_json::{Value, json};
use sha1::{Digest, Sha1};
use thiserror::Error;

use crate::project_text::{
    DOSSIER_SYSTEM, ENTRY_NAMES, ENV_EXAMPLES, KEY_FILE, ROLE_WORDS, SECRET_NAMES, SECRET_SUFFIXES, SKIP_DIRS,
    SKIP_NAMES, SKIP_SUFFIXES, STOP_WORDS,
};

pub const MAX_FILES: usize = 4000;
pub const MAX_FILE_BYTES: u64 = 200_000;
pub const MAX_TOTAL_BYTES: usize = 6_000_000;
pub const CHUNK_CHARS: usize = 2_400;
/// A project question carries three excerpts of at most this many characters in all, and this much dossier.
pub const LEAN_EXCERPTS: usize = 3;
pub const LEAN_CODE_CHARS: usize = 3 * CHUNK_CHARS;
pub const LEAN_DOSSIER_CHARS: usize = 4_500;
pub const DIGEST_CHARS: usize = 36_000;
/// What the dossier request may be on a provider with a small limit per request (Groq's free tier: 8000 tokens).
pub const SMALL_LIMIT_DIGEST_CHARS: usize = 16_000;

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("{0}")]
    Message(String),
}

fn fail<T>(text: impl Into<String>) -> Result<T, ProjectError> {
    Err(ProjectError::Message(text.into()))
}

/// How big the dossier request is on this provider: a provider with a small limit per request gets a smaller digest,
/// because a request over the limit is refused whole, with no dossier at all.
pub fn digest_budget(gateway: &str) -> usize {
    if crate::providers::provider_for(gateway).is_some_and(|provider| provider.id == "groq") {
        SMALL_LIMIT_DIGEST_CHARS
    } else {
        DIGEST_CHARS
    }
}

/// The room for the dossier itself: smaller where the request is, so request and reply fit the limit together.
pub fn dossier_tokens(gateway: &str) -> u32 {
    if digest_budget(gateway) < DIGEST_CHARS { 3000 } else { 3500 }
}

fn compile(pattern: &str) -> Regex {
    Regex::new(pattern).unwrap_or_else(|error| panic!("a fixed pattern does not compile: {error}"))
}

static TOKENS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----",
        r"\b(?:sk|gsk|pk|rk)[-_][A-Za-z0-9_-]{20,}",
        r"\bgh[pousr]_[A-Za-z0-9]{30,}",
        r"\bgithub_pat_[A-Za-z0-9_]{40,}",
        r"\bAKIA[0-9A-Z]{16}\b",
        r"\bxox[abprs]-[A-Za-z0-9-]{10,}",
        r"\bAIza[0-9A-Za-z_-]{35}\b",
    ]
    .iter()
    .map(|pattern| compile(pattern))
    .collect()
});
// No run in front of the keyword and a bounded one after it: an unbounded form is quadratic on a long line of dotted
// names (minified code, a lock file) and could hold the analysis for minutes.
static ASSIGNED: LazyLock<Regex> = LazyLock::new(|| {
    compile(
        r#"(?i)((?:api[_-]?key|secret|token|passw(?:or)?d|authorization|credential)[A-Za-z0-9_.-]{0,40}\s{0,5}[:=]\s{0,5}(?:(?:bearer|basic|token)\s+)?)(["']?)[^\s"',;)]{8,}\2"#,
    )
});

/// Blank anything that looks like a key, token or password before it can leave the machine.
pub fn redact(text: &str) -> String {
    let mut text = text.to_owned();
    for pattern in TOKENS.iter() {
        text = pattern.replace_all(&text, "[redacted]").into_owned();
    }
    ASSIGNED.replace_all(&text, "${1}${2}[redacted]${2}").into_owned()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceFile {
    pub path: String,
    pub text: String,
}

impl SourceFile {
    pub fn lines(&self) -> usize {
        self.text.matches('\n').count() + 1
    }
}

#[derive(Clone, Debug, Default)]
pub struct Scan {
    pub files: Vec<SourceFile>,
    pub skipped: usize,
    pub truncated: bool,
}

impl Scan {
    pub fn size(&self) -> usize {
        self.files.iter().map(|file| file.text.chars().count()).sum()
    }
}

fn wanted(relative: &str) -> bool {
    let path = Path::new(relative);
    let name = path.file_name().map(|name| name.to_string_lossy().to_lowercase()).unwrap_or_default();
    let folders: Vec<String> = path
        .parent()
        .map(|parent| parent.components().map(|part| part.as_os_str().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    if folders.iter().any(|part| SKIP_DIRS.contains(&part.as_str())) {
        return false;
    }
    if SKIP_NAMES.contains(&name.as_str())
        || SECRET_NAMES.contains(&name.as_str())
        || name.ends_with(".min.js")
        || name.ends_with(".min.css")
    {
        return false;
    }
    if name.starts_with(".env") && !ENV_EXAMPLES.contains(&name.as_str()) {
        return false;
    }
    let suffix = path.extension().map(|ext| format!(".{}", ext.to_string_lossy().to_lowercase())).unwrap_or_default();
    if SKIP_SUFFIXES.contains(&suffix.as_str()) || SECRET_SUFFIXES.contains(&suffix.as_str()) {
        return false;
    }
    !name.starts_with("service-account")
}

/// What git would show (tracked and untracked, minus ignored), or `None` outside a repository.
fn git_files(root: &Path) -> Option<Vec<String>> {
    let output = Command::new("git")
        .args([
            "-c",
            "core.fsmonitor=false",
            "-c",
            &format!("core.hooksPath={}", if cfg!(windows) { "NUL" } else { "/dev/null" }),
            "-c",
            "core.pager=cat",
            "-C",
        ])
        .arg(root)
        .args(["ls-files", "-z", "--cached", "--others", "--exclude-standard"])
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() || output.stdout.is_empty() {
        return None;
    }
    let mut files: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .split('\0')
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect();
    files.sort();
    files.dedup();
    Some(files)
}

fn walk(root: &Path) -> Vec<String> {
    fn visit(root: &Path, folder: &Path, found: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(folder) else { return };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_dir() {
                if !SKIP_DIRS.contains(&name.as_str()) {
                    visit(root, &path, found);
                }
            } else if let Ok(relative) = path.strip_prefix(root) {
                found.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    let mut found = Vec::new();
    visit(root, root, &mut found);
    found
}

/// Read the text files of a project folder; the result is redacted and size-limited.
pub fn scan(folder: &Path) -> Result<Scan, ProjectError> {
    let root =
        folder.canonicalize().map_err(|_| ProjectError::Message(format!("{} is not a folder", folder.display())))?;
    if !root.is_dir() {
        return fail(format!("{} is not a folder", root.display()));
    }
    let mut result = Scan::default();
    let mut total = 0usize;
    for relative in git_files(&root).unwrap_or_else(|| walk(&root)) {
        if !wanted(&relative) {
            result.skipped += 1;
            continue;
        }
        let path = root.join(&relative);
        let Ok(meta) = std::fs::metadata(&path) else {
            result.skipped += 1;
            continue;
        };
        if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
            result.skipped += 1;
            continue;
        }
        let Ok(data) = std::fs::read(&path) else {
            result.skipped += 1;
            continue;
        };
        if data.iter().take(4096).any(|byte| *byte == 0) {
            result.skipped += 1;
            continue;
        }
        let text = redact(&String::from_utf8_lossy(&data));
        if result.files.len() >= MAX_FILES || total + text.chars().count() > MAX_TOTAL_BYTES {
            result.truncated = true;
            break;
        }
        total += text.chars().count();
        result.files.push(SourceFile { path: relative.replace('\\', "/"), text });
    }
    Ok(result)
}

// ---- the search -----------------------------------------------------------------------------------------------------

static WORD: LazyLock<Regex> = LazyLock::new(|| compile(r"[A-Za-z][A-Za-z0-9]*"));

/// Lower-case words; an identifier like `OrderService` or `order_service` also gives `order`, `service`.
pub fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    for found in WORD.find_iter(text).flatten() {
        let word = found.as_str();
        let mut parts: Vec<String> = Vec::new();
        let mut current = String::new();
        let mut previous: Option<char> = None;
        for c in word.chars() {
            // a capital after a small letter or a digit starts a new part
            if c.is_ascii_uppercase()
                && previous.is_some_and(|p| p.is_ascii_lowercase() || p.is_ascii_digit())
                && !current.is_empty()
            {
                parts.push(std::mem::take(&mut current).to_lowercase());
            }
            current.push(c);
            previous = Some(c);
        }
        if !current.is_empty() {
            parts.push(current.to_lowercase());
        }
        if parts.len() > 1 {
            tokens.push(word.to_lowercase());
        }
        tokens.extend(parts);
    }
    tokens.into_iter().filter(|token| !STOP_WORDS.contains(&token.as_str()) && token.chars().count() > 1).collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chunk {
    pub path: String,
    pub start: usize,
    pub text: String,
}

/// A file in pieces of about `size` characters, cut at line ends, each knowing where it starts.
pub fn chunks_of(file: &SourceFile, size: usize) -> Vec<Chunk> {
    let mut pieces = Vec::new();
    let (mut current, mut length, mut start): (Vec<&str>, usize, usize) = (Vec::new(), 0, 1);
    for (index, line) in file.text.lines().enumerate() {
        let number = index + 1;
        if !current.is_empty() && length + line.chars().count() > size {
            pieces.push(Chunk { path: file.path.clone(), start, text: current.join("\n") });
            current.clear();
            length = 0;
            start = number;
        }
        current.push(line);
        length += line.chars().count() + 1;
    }
    if !current.is_empty() {
        pieces.push(Chunk { path: file.path.clone(), start, text: current.join("\n") });
    }
    pieces
}

/// BM25 over the chunks; the file path counts as text too, so a question naming a module finds it.
#[derive(Debug)]
struct Index {
    counts: Vec<HashMap<String, usize>>,
    lengths: Vec<usize>,
    average: f32,
    idf: HashMap<String, f32>,
}

const K1: f32 = 1.5;
const B: f32 = 0.75;

impl Index {
    fn new(chunks: &[Chunk]) -> Self {
        let counts: Vec<HashMap<String, usize>> = chunks
            .iter()
            .map(|chunk| {
                let mut count = HashMap::new();
                for token in tokenize(&format!("{} {} {}", chunk.path, chunk.path, chunk.text)) {
                    *count.entry(token).or_insert(0) += 1;
                }
                count
            })
            .collect();
        let lengths: Vec<usize> = counts.iter().map(|count| count.values().sum()).collect();
        let average = if chunks.is_empty() { 1.0 } else { lengths.iter().sum::<usize>() as f32 / chunks.len() as f32 };
        let mut frequency: HashMap<&str, usize> = HashMap::new();
        for count in &counts {
            for term in count.keys() {
                *frequency.entry(term.as_str()).or_insert(0) += 1;
            }
        }
        let total = chunks.len() as f32;
        let idf = frequency
            .into_iter()
            .map(|(term, seen)| (term.to_owned(), (1.0 + (total - seen as f32 + 0.5) / (seen as f32 + 0.5)).ln()))
            .collect();
        Self { counts, lengths, average, idf }
    }

    fn search(&self, question: &str, limit: usize) -> Vec<(f32, usize)> {
        let mut terms = tokenize(question);
        terms.sort();
        terms.dedup();
        terms.retain(|term| self.idf.contains_key(term));
        let mut scored: Vec<(f32, usize)> = self
            .counts
            .iter()
            .enumerate()
            .filter_map(|(index, count)| {
                let norm = K1 * (1.0 - B + B * self.lengths[index] as f32 / self.average);
                let score: f32 = terms
                    .iter()
                    .filter_map(|term| {
                        count.get(term).map(|seen| self.idf[term] * *seen as f32 * (K1 + 1.0) / (*seen as f32 + norm))
                    })
                    .sum();
                (score > 0.0).then_some((score, index))
            })
            .collect();
        scored.sort_by(|a, b| b.0.total_cmp(&a.0));
        scored.truncate(limit);
        scored
    }
}

/// A project the candidate defends: where it is, its dossier and the searchable code.
#[derive(Debug, Default)]
pub struct ProjectContext {
    pub path: String,
    pub name: String,
    pub dossier: String,
    pub files: usize,
    pub characters: usize,
    pub analyzed_at: String,
    pub chunks: Vec<Chunk>,
    index: OnceLock<Index>,
}

impl ProjectContext {
    pub fn new(
        path: &str,
        name: &str,
        dossier: &str,
        files: usize,
        characters: usize,
        analyzed_at: &str,
        chunks: Vec<Chunk>,
    ) -> Self {
        Self {
            path: path.to_owned(),
            name: name.to_owned(),
            dossier: dossier.to_owned(),
            files,
            characters,
            analyzed_at: analyzed_at.to_owned(),
            chunks,
            index: OnceLock::new(),
        }
    }

    fn index(&self) -> &Index {
        self.index.get_or_init(|| Index::new(&self.chunks))
    }

    /// Build the search index now, so the first question does not wait for it.
    pub fn prepare(&self) {
        let _ = self.index();
    }

    /// The excerpts of the project that best match the question, ready for a prompt.
    pub fn retrieve(&self, question: &str, limit: usize, budget: usize) -> String {
        if self.chunks.is_empty() {
            return String::new();
        }
        let hits = self.index().search(question, limit);
        let Some(best) = hits.first().map(|hit| hit.0) else { return String::new() };
        let (mut parts, mut used) = (Vec::new(), 0usize);
        for (score, index) in hits {
            let chunk = &self.chunks[index];
            let length = chunk.text.chars().count();
            if score < best * 0.35 || (!parts.is_empty() && used + length > budget) {
                continue;
            }
            let body: String = chunk.text.chars().take(budget.saturating_sub(used).max(400)).collect();
            used += body.chars().count();
            parts.push(format!("--- {} (from line {}) ---\n{body}", chunk.path, chunk.start));
        }
        parts.join("\n\n")
    }

    /// What the project says about a question, sized for a request that fits a small provider's limit: three code
    /// excerpts and about 4500 characters of the dossier. Empty when no code matches.
    pub fn find(&self, question: &str) -> crate::session::Found {
        let code = self.retrieve(question, LEAN_EXCERPTS, LEAN_CODE_CHARS);
        if code.is_empty() {
            return crate::session::Found::default();
        }
        crate::session::Found { dossier: self.dossier_for(question, LEAN_DOSSIER_CHARS), code }
    }

    /// The part of the dossier that bears on the question: the overview and the key modules always, then the other
    /// sections that share the most words with the question, up to `limit` characters.
    pub fn dossier_for(&self, question: &str, limit: usize) -> String {
        let sections = dossier_sections(&self.dossier);
        let asked: std::collections::HashSet<String> = tokenize(question).into_iter().collect();
        let overlap = |text: &str| tokenize(text).into_iter().filter(|word| asked.contains(word)).count();
        let always = |heading: &str| {
            let heading = heading.to_lowercase();
            heading.contains("overview") || heading.contains("key modules")
        };
        let mut order: Vec<usize> = (0..sections.len()).collect();
        let rank = |heading: &str| match heading.to_lowercase() {
            heading if heading.contains("overview") => 0,
            heading if heading.contains("key modules") => 1,
            _ => 2,
        };
        order.sort_by_key(|index| (rank(sections[*index].0), std::cmp::Reverse(overlap(sections[*index].1))));
        let (mut chosen, mut used) = (Vec::new(), 0usize);
        for index in order {
            let (heading, body) = sections[index];
            let length = body.chars().count();
            if !chosen.is_empty() && (used + length > limit || (!always(heading) && overlap(body) == 0)) {
                continue;
            }
            chosen.push(index);
            used += length;
        }
        chosen.sort_unstable();
        let text: String =
            chosen.into_iter().map(|index| sections[index].1).collect::<Vec<_>>().join("\n").trim().to_owned();
        if text.chars().count() > limit { text.chars().take(limit).collect() } else { text }
    }

    /// The dossier and the brief that goes with the project in a prompt.
    pub fn as_dossier(&self) -> crate::brief::ProjectDossier {
        crate::brief::ProjectDossier { name: self.name.clone(), dossier: self.dossier.clone() }
    }
}

// ---- the dossier --------------------------------------------------------------------------------------------------------

/// A dossier cut at its `## ` headings: (heading line, the whole section including the heading).
fn dossier_sections(dossier: &str) -> Vec<(&str, &str)> {
    let mut starts: Vec<usize> = Vec::new();
    let mut offset = 0;
    for line in dossier.split_inclusive('\n') {
        if line.starts_with("## ") {
            starts.push(offset);
        }
        offset += line.len();
    }
    if starts.first() != Some(&0) {
        starts.insert(0, 0);
    }
    starts.push(dossier.len());
    starts
        .windows(2)
        .map(|pair| (dossier[pair[0]..pair[1]].lines().next().unwrap_or(""), &dossier[pair[0]..pair[1]]))
        .filter(|(_, body)| !body.trim().is_empty())
        .collect()
}

fn importance(file: &SourceFile) -> f32 {
    static TESTS: LazyLock<Regex> =
        LazyLock::new(|| compile(r"(^|/)(tests?|__tests__|spec)(/|$)|test_|_test\.|\.test\.|\.spec\."));
    let lowered = file.path.to_lowercase();
    let stem = Path::new(&lowered).file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
    let mut score = -2.0 * lowered.matches('/').count() as f32;
    if ENTRY_NAMES.contains(&stem.as_str()) {
        score += 6.0;
    }
    if ROLE_WORDS.iter().any(|word| lowered.contains(word)) {
        score += 3.0;
    }
    score += 2.0 * file.lines().min(200) as f32 / 100.0;
    if TESTS.is_match(&lowered).unwrap_or(false) {
        score -= 5.0;
    }
    score
}

fn tree(scan: &Scan, limit: usize) -> String {
    let rows: Vec<String> = scan.files.iter().map(|item| format!("{} ({} lines)", item.path, item.lines())).collect();
    if rows.len() <= limit {
        return rows.join("\n");
    }
    let mut folders: Vec<(String, usize)> = Vec::new();
    for item in &scan.files {
        let top = item.path.split_once('/').map_or(".".to_owned(), |(first, _)| first.to_owned());
        match folders.iter_mut().find(|(name, _)| *name == top) {
            Some((_, count)) => *count += 1,
            None => folders.push((top, 1)),
        }
    }
    folders.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    let top: Vec<String> = folders
        .iter()
        .map(
            |(name, count)| {
                if name == "." { format!("(top level: {count} files)") } else { format!("{name}/ ({count} files)") }
            },
        )
        .collect();
    format!(
        "{}\n... {} more files; by folder:\n{}",
        rows[..limit / 2].join("\n"),
        rows.len() - limit / 2,
        top.join("\n")
    )
}

/// The prompt for the dossier: the tree, the manifests and READMEs, then the most important code.
pub fn digest_prompt(name: &str, scan: &Scan, budget: usize) -> String {
    static KEY: LazyLock<Regex> = LazyLock::new(|| compile(&format!("(?i){KEY_FILE}")));
    let first = format!("PROJECT: {name}\nFILES: {}\n\nFILE TREE\n{}", scan.files.len(), tree(scan, 320));
    let mut used = first.chars().count();
    let mut parts = vec![first];
    let mut key: Vec<&SourceFile> =
        scan.files.iter().filter(|item| KEY.is_match(&item.path).unwrap_or(false)).collect();
    key.sort_by_key(|item| (item.path.matches('/').count(), item.path.clone()));
    for item in &key {
        let body: String = item.text.chars().take(4000).collect();
        if (used + body.chars().count()) as f32 > budget as f32 * 0.45 {
            break;
        }
        used += body.chars().count();
        parts.push(format!("--- {} ---\n{body}", item.path));
    }
    let mut others: Vec<&SourceFile> =
        scan.files.iter().filter(|item| !key.iter().any(|known| known.path == item.path)).collect();
    others.sort_by(|a, b| importance(b).total_cmp(&importance(a)));
    for item in others {
        let body: String = item.text.chars().take(3500).collect();
        if used + body.chars().count() > budget {
            continue;
        }
        used += body.chars().count();
        parts.push(format!("--- {} ---\n{body}", item.path));
    }
    parts.join("\n\n")
}

// ---- storing -------------------------------------------------------------------------------------------------------------

/// The same twelve characters the Python overlay names a project's folder by, so one can open what the other stored.
pub fn project_id(path: &Path) -> String {
    let resolved = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    hex::encode(Sha1::digest(resolved.to_string_lossy().as_bytes()))[..12].to_owned()
}

pub fn store_dir(interview_dir: &Path, path: &Path) -> PathBuf {
    interview_dir.join("projects").join(project_id(path))
}

pub fn save(interview_dir: &Path, project: &ProjectContext) -> std::io::Result<()> {
    let directory = store_dir(interview_dir, Path::new(&project.path));
    std::fs::create_dir_all(&directory)?;
    std::fs::write(directory.join("dossier.md"), &project.dossier)?;
    let rows: Vec<Value> = project.chunks.iter().map(|chunk| json!([chunk.path, chunk.start, chunk.text])).collect();
    std::fs::write(directory.join("index.json"), serde_json::to_string(&rows).unwrap_or_default())?;
    let meta = json!({"path": project.path, "name": project.name, "files": project.files, "characters": project.characters, "analyzed_at": project.analyzed_at});
    std::fs::write(directory.join("meta.json"), serde_json::to_string(&meta).unwrap_or_default())
}

/// Keep the candidate's corrections to the dossier.
pub fn save_dossier(interview_dir: &Path, project: &mut ProjectContext, text: &str) -> std::io::Result<()> {
    project.dossier = text.trim().to_owned();
    let directory = store_dir(interview_dir, Path::new(&project.path));
    std::fs::create_dir_all(&directory)?;
    std::fs::write(directory.join("dossier.md"), &project.dossier)
}

/// A project that was analysed before, or `None`.
pub fn load(interview_dir: &Path, path: &Path) -> Option<ProjectContext> {
    let directory = store_dir(interview_dir, path);
    let meta: Value = serde_json::from_str(&std::fs::read_to_string(directory.join("meta.json")).ok()?).ok()?;
    let dossier = std::fs::read_to_string(directory.join("dossier.md")).ok()?;
    let rows: Value = serde_json::from_str(&std::fs::read_to_string(directory.join("index.json")).ok()?).ok()?;
    let chunks = rows
        .as_array()?
        .iter()
        .filter_map(|row| {
            let row = row.as_array()?;
            Some(Chunk {
                path: row.first()?.as_str()?.to_owned(),
                start: usize::try_from(row.get(1)?.as_u64()?).ok()?,
                text: row.get(2)?.as_str()?.to_owned(),
            })
        })
        .collect();
    let text = |key: &str| meta.get(key).and_then(Value::as_str).unwrap_or("").to_owned();
    let number = |key: &str| meta.get(key).and_then(Value::as_u64).unwrap_or(0) as usize;
    let project = ProjectContext::new(
        &if text("path").is_empty() { path.to_string_lossy().into_owned() } else { text("path") },
        &if text("name").is_empty() {
            path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default()
        } else {
            text("name")
        },
        &dossier,
        number("files"),
        number("characters"),
        &text("analyzed_at"),
        chunks,
    );
    project.prepare();
    Some(project)
}

/// Delete the stored analysis of a project.
pub fn forget(interview_dir: &Path, path: &Path) {
    let directory = store_dir(interview_dir, path);
    for name in ["dossier.md", "index.json", "meta.json"] {
        let _ = std::fs::remove_file(directory.join(name));
    }
    let _ = std::fs::remove_dir(&directory);
}

/// Read the folder, have the AI write the dossier, store both. `complete(system, prompt, max_tokens)` is the AI call;
/// its errors are left to the caller. `progress(stage, files)` is told `read` and then `ask` (with the number of files).
pub fn analyze(
    interview_dir: &Path,
    folder: &Path,
    gateway: &str,
    complete: &dyn Fn(&str, &str, u32) -> Result<String, String>,
    mut progress: impl FnMut(&str, usize),
) -> Result<ProjectContext, ProjectError> {
    progress("read", 0);
    let result = scan(folder)?;
    if result.files.is_empty() {
        return fail("No readable text files in that folder");
    }
    let root = folder.canonicalize().map_err(|error| ProjectError::Message(error.to_string()))?;
    let name = root.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    progress("ask", result.files.len());
    let dossier =
        complete(DOSSIER_SYSTEM, &digest_prompt(&name, &result, digest_budget(gateway)), dossier_tokens(gateway))
            .map_err(ProjectError::Message)?;
    let dossier = dossier.trim().to_owned();
    if dossier.is_empty() {
        return fail("The AI returned no dossier. Try again or pick another model.");
    }
    let chunks = result.files.iter().flat_map(|file| chunks_of(file, CHUNK_CHARS)).collect();
    let project = ProjectContext::new(
        &root.to_string_lossy(),
        &name,
        &dossier,
        result.files.len(),
        result.size(),
        &chrono::Local::now().format("%Y-%m-%d %H:%M").to_string(),
        chunks,
    );
    save(interview_dir, &project)
        .map_err(|error| ProjectError::Message(format!("Could not save the project: {error}")))?;
    project.prepare();
    Ok(project)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, relative: &str, text: &str) {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn sample() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(root, "README.md", "# Shop\nAn online shop API.\n");
        write(root, "pyproject.toml", "[project]\nname = \"shop\"\n");
        write(
            root,
            "app/main.py",
            "from app.service import OrderService\n\ndef main():\n    return OrderService().total([1, 2])\n",
        );
        write(
            root,
            "app/service.py",
            "class OrderService:\n    def total(self, items):\n        \"\"\"The order total with the loyalty discount of 10 percent.\"\"\"\n        return sum(items) * 0.9\n",
        );
        write(root, "tests/test_service.py", "def test_total():\n    assert True\n");
        write(root, ".env", "SECRET_KEY=hunter2hunter2\n");
        write(root, ".env.example", "SECRET_KEY=changeme\n");
        write(root, "id_rsa", "-----BEGIN PRIVATE KEY-----\nabc\n-----END PRIVATE KEY-----\n");
        write(root, "poetry.lock", "lock\n");
        write(root, "logo.png", "png");
        write(root, "node_modules/left-pad/index.js", "module.exports = 1;\n");
        write(root, "app/blob.bin", "text\0binary");
        dir
    }

    #[test]
    fn a_scan_keeps_the_text_of_the_project_and_leaves_secrets_binaries_and_noise_out() {
        let dir = sample();
        let read = scan(dir.path()).unwrap();
        let mut paths: Vec<&str> = read.files.iter().map(|file| file.path.as_str()).collect();
        paths.sort_unstable();
        assert_eq!(
            paths,
            [".env.example", "README.md", "app/main.py", "app/service.py", "pyproject.toml", "tests/test_service.py"]
        );
        assert!(read.skipped >= 3 && !read.truncated);
        assert!(scan(Path::new("/no/such/folder")).is_err());
    }

    #[test]
    fn anything_that_looks_like_a_key_is_blanked() {
        let text = "token = \"ghp_abcdefghijklmnopqrstuvwxyz0123456789\"\nkey: gsk_abcdefghijklmnopqrstuv\nAWS AKIAABCDEFGHIJKLMNOP\nclient.password = hunter2hunter2\nlet name = \"plain\"\n-----BEGIN RSA PRIVATE KEY-----\nxx\n-----END RSA PRIVATE KEY-----";
        let redacted = redact(text);
        for secret in ["ghp_abc", "gsk_abc", "AKIAABC", "hunter2hunter2", "xx\n-----END"] {
            assert!(!redacted.contains(secret), "{secret} survived: {redacted}");
        }
        assert!(redacted.contains("token = \"[redacted]\"") && redacted.contains("let name = \"plain\""));
        assert!(redact(&"a.".repeat(50_000)).len() > 90_000); // a long line of dotted names is not slow or lost
    }

    #[test]
    fn an_identifier_gives_its_parts_and_common_words_are_dropped() {
        assert_eq!(tokenize("OrderService"), ["orderservice", "order", "service"]);
        assert_eq!(tokenize("order_total"), ["order", "total"]);
        assert_eq!(tokenize("How is the order total calculated?"), ["order", "total", "calculated"]);
        assert!(tokenize("a b the").is_empty());
    }

    #[test]
    fn files_are_cut_at_line_ends_and_each_piece_knows_its_line() {
        let file = SourceFile {
            path: "a.py".into(),
            text: (1..=10).map(|n| format!("line number {n}")).collect::<Vec<_>>().join("\n"),
        };
        let pieces = chunks_of(&file, 50);
        assert!(pieces.len() > 1 && pieces[0].start == 1 && pieces[1].start > 1);
        assert_eq!(pieces.iter().map(|piece| piece.text.lines().count()).sum::<usize>(), 10);
        assert!(chunks_of(&SourceFile { path: "e".into(), text: String::new() }, 50).is_empty());
    }

    fn project(dir: &Path) -> ProjectContext {
        let scan = scan(dir).unwrap();
        let chunks = scan.files.iter().flat_map(|file| chunks_of(file, CHUNK_CHARS)).collect();
        ProjectContext::new(
            &dir.to_string_lossy(),
            "shop",
            "## Overview\nAn online shop.\n",
            scan.files.len(),
            scan.size(),
            "2026-10-30 12:00",
            chunks,
        )
    }

    #[test]
    fn a_question_finds_the_code_that_answers_it() {
        let dir = sample();
        let project = project(dir.path());
        let found = project.retrieve("How do you calculate the order total with the discount?", 4, 3600);
        assert!(
            found.starts_with("--- app/service.py (from line 1) ---") && found.contains("loyalty discount"),
            "{found}"
        );
        assert!(
            project.retrieve("zzz qqq", 4, 3600).is_empty()
                && ProjectContext::default().retrieve("order", 4, 3600).is_empty()
        );
        let budgeted = project.retrieve("order total service main", 4, 100);
        assert!(budgeted.chars().count() < 3600);
    }

    #[test]
    fn the_digest_has_the_tree_the_manifests_and_the_important_code_within_the_budget() {
        let dir = sample();
        let scan = scan(dir.path()).unwrap();
        let digest = digest_prompt("shop", &scan, DIGEST_CHARS);
        assert!(digest.starts_with("PROJECT: shop\nFILES: 6") && digest.contains("app/service.py (5 lines)"));
        assert!(
            digest.contains("--- README.md ---")
                && digest.contains("--- pyproject.toml ---")
                && digest.contains("--- app/main.py ---")
        );
        assert!(
            digest.find("--- app/main.py ---").unwrap() < digest.find("--- tests/test_service.py ---").unwrap(),
            "tests come last"
        );
        let small = digest_prompt("shop", &scan, 200);
        assert!(small.len() < digest.len());
    }

    #[test]
    fn a_provider_with_a_small_limit_gets_a_smaller_dossier_request() {
        assert_eq!(digest_budget("https://api.groq.com/openai"), SMALL_LIMIT_DIGEST_CHARS);
        assert_eq!(digest_budget("http://127.0.0.1:3260"), DIGEST_CHARS);
        assert_eq!(digest_budget("https://api.openai.com"), DIGEST_CHARS);
        assert_eq!(
            (dossier_tokens("https://api.groq.com/openai"), dossier_tokens("http://127.0.0.1:3260")),
            (3000, 3500)
        );
    }

    #[test]
    fn a_project_is_stored_in_the_python_layout_and_read_back() {
        let dir = sample();
        let store = tempfile::tempdir().unwrap();
        let original = project(dir.path());
        save(store.path(), &original).unwrap();
        let folder = store_dir(store.path(), dir.path());
        assert!(
            folder.join("dossier.md").is_file()
                && folder.join("index.json").is_file()
                && folder.join("meta.json").is_file()
        );
        assert_eq!(folder.file_name().unwrap().to_string_lossy().len(), 12);
        let loaded = load(store.path(), dir.path()).unwrap();
        assert_eq!(
            (loaded.name.as_str(), loaded.files, loaded.chunks.len()),
            ("shop", original.files, original.chunks.len())
        );
        assert!(loaded.retrieve("order total", 4, 3600).contains("OrderService"));
        let mut loaded = loaded;
        save_dossier(store.path(), &mut loaded, "  ## Overview\nMine.  ").unwrap();
        assert_eq!(load(store.path(), dir.path()).unwrap().dossier, "## Overview\nMine.");
        forget(store.path(), dir.path());
        assert!(load(store.path(), dir.path()).is_none());
    }

    #[test]
    fn analyzing_asks_the_model_once_with_the_digest_and_keeps_the_result() {
        let dir = sample();
        let store = tempfile::tempdir().unwrap();
        let asked = std::cell::RefCell::new(Vec::new());
        let stages = std::cell::RefCell::new(Vec::new());
        let complete = |system: &str, prompt: &str, tokens: u32| {
            asked.borrow_mut().push((system.to_owned(), prompt.to_owned(), tokens));
            Ok("  ## Overview\nA shop.  ".to_owned())
        };
        let project = analyze(store.path(), dir.path(), "https://api.groq.com/openai", &complete, |stage, files| {
            stages.borrow_mut().push((stage.to_owned(), files))
        })
        .unwrap();
        assert_eq!(project.dossier, "## Overview\nA shop.");
        let asked = asked.borrow();
        assert_eq!((asked.len(), asked[0].0.as_str(), asked[0].2), (1, DOSSIER_SYSTEM, 3000));
        assert!(asked[0].1.contains("PROJECT:") && asked[0].1.len() <= SMALL_LIMIT_DIGEST_CHARS + 2_000);
        assert_eq!(*stages.borrow(), [("read".to_owned(), 0), ("ask".to_owned(), 6)]);
        assert!(load(store.path(), dir.path()).is_some());
        assert!(
            analyze(store.path(), dir.path(), "", &|_, _, _| Ok("   ".into()), |_, _| {})
                .unwrap_err()
                .to_string()
                .contains("no dossier")
        );
        let empty = tempfile::tempdir().unwrap();
        assert!(
            analyze(store.path(), empty.path(), "", &|_, _, _| Ok("x".into()), |_, _| {})
                .unwrap_err()
                .to_string()
                .contains("No readable text files")
        );
        assert!(
            analyze(store.path(), dir.path(), "", &|_, _, _| Err("refused".into()), |_, _| {})
                .unwrap_err()
                .to_string()
                .contains("refused")
        );
    }

    #[test]
    fn the_dossier_for_a_question_keeps_the_overview_the_key_modules_and_the_sections_that_match() {
        let project = ProjectContext { dossier: "## Overview\nA shop bot.\n\n## Tech stack\nPython, aiogram.\n\n## Key modules\n- throttling.py limits flooding.\n\n## Decisions and trade-offs\nWe use Redis for the rate limiter.\n\n## How to run and test\nmake run\n".into(), ..ProjectContext::default() };
        let text = project.dossier_for("How do you stop flooding with the rate limiter?", 4500);
        assert!(
            text.starts_with("## Overview")
                && text.contains("## Key modules")
                && text.contains("## Decisions and trade-offs"),
            "{text}"
        );
        assert!(!text.contains("make run") && !text.contains("aiogram"), "unrelated sections are left out: {text}");
        let small = project.dossier_for("How do you stop flooding with the rate limiter?", 60);
        assert!(small.chars().count() <= 60 && small.starts_with("## Overview"), "{small}");
        assert_eq!(ProjectContext::default().dossier_for("anything", 100), "");
    }
}
