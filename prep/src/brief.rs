//! What the interviewer knows about you: your ABOUT.md, the CV the employer received and the
//! vacancy. Everything said about *your* experience must come from here and nowhere else.

use std::fs;
use std::path::Path;

use thiserror::Error;

use crate::config::{Settings, expand_home};

pub const CONTEXT_CHARS: usize = 26_000;
pub const CV_CHARS: usize = 9_000;
pub const JOB_CHARS: usize = 6_000;
const MIN_TEXT: usize = 120;
const MAX_TEXT_FILE: u64 = 2 * 1024 * 1024;
const MAX_PDF_FILE: u64 = 25 * 1024 * 1024;
/// Parts of ABOUT.md written for CV writing and bookkeeping, not for answering a question.
const SKIP_FOR_ANSWERS: [&str; 3] = ["## applications log", "## cv files", "## matching a job ad"];
const LOW_PRIORITY: [&str; 3] = ["## applications log", "## cv files", "## contact"];

#[derive(Debug, Error)]
pub enum BriefError {
    #[error("{0}")]
    Document(String),
    #[error("No candidate context. Fill ABOUT.md (the file {0} was not found or is empty).")]
    NoContext(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocKind {
    /// What the employer has read.
    Cv,
    /// Extra facts the employer has not seen.
    About,
}

#[derive(Debug, Clone)]
pub struct Document {
    pub name: String,
    pub text: String,
    pub path: String,
    pub kind: DocKind,
}

#[derive(Debug, Clone, Default)]
pub struct Brief {
    pub candidate: String,
    pub documents: Vec<Document>,
    pub job_title: String,
    pub company: String,
    pub job_description: String,
    pub notes: String,
}

impl Brief {
    /// ABOUT.md and the CVs named in the settings. A missing ABOUT.md is an error unless the CVs
    /// can stand in for it; an unreadable CV is reported but does not stop the rest.
    pub fn load(settings: &Settings) -> Result<(Self, Vec<String>), BriefError> {
        let about = settings.about_file();
        let candidate = fs::read_to_string(&about).unwrap_or_default();
        let mut brief = Self { candidate, ..Self::default() };
        let mut warnings = Vec::new();
        for path in &settings.cv_paths {
            if path.trim().is_empty() {
                continue;
            }
            if let Err(error) = brief.add_document(path) {
                warnings.push(error.to_string());
            }
        }
        if brief.is_empty() {
            return Err(BriefError::NoContext(about.display().to_string()));
        }
        Ok((brief, warnings))
    }

    pub fn add_document(&mut self, path: &str) -> Result<(), BriefError> {
        let resolved = expand_home(path);
        let resolved = resolved.canonicalize().unwrap_or(resolved);
        let key = resolved.to_string_lossy().into_owned();
        if self.documents.iter().any(|doc| doc.path == key) {
            return Ok(());
        }
        let text = read_document(&resolved)?;
        let name = resolved.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let kind = guess_kind(&name);
        self.documents.push(Document { name, text, path: key, kind });
        Ok(())
    }

    pub fn set_vacancy(&mut self, title: &str, company: &str, description: &str) {
        self.job_title = title.trim().to_owned();
        self.company = company.trim().to_owned();
        self.job_description = description.trim().to_owned();
    }

    pub fn has_vacancy(&self) -> bool {
        !self.job_title.is_empty() || !self.job_description.is_empty()
    }

    pub fn is_empty(&self) -> bool {
        self.candidate.trim().is_empty() && self.documents.is_empty()
    }

    /// The text every prompt starts with.
    pub fn render(&self) -> String {
        let mut parts = Vec::new();
        if !self.candidate.trim().is_empty() {
            parts.push(format!(
                "CANDIDATE CONTEXT (ABOUT.md, verified facts)\n{}",
                fit_context(&slim_for_answers(&self.candidate), CONTEXT_CHARS)
            ));
        }
        for doc in &self.documents {
            let heading = match doc.kind {
                DocKind::Cv => "CV THE EMPLOYER HAS READ",
                DocKind::About => "MORE FACTS ABOUT THE CANDIDATE (the employer has not seen this)",
            };
            parts.push(format!("{heading}: {}\n{}", doc.name, truncate_chars(&doc.text, CV_CHARS)));
        }
        if parts.is_empty() {
            parts.push("CANDIDATE CONTEXT: (none provided)".to_owned());
        }
        if self.has_vacancy() {
            let head = [self.job_title.as_str(), self.company.as_str()]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join(" at ");
            parts.push(
                format!("VACANCY: {head}\n{}", truncate_chars(&self.job_description, JOB_CHARS)).trim().to_owned(),
            );
        }
        parts.push(
            "SALARY POLICY: state only a figure that is written in the candidate context; otherwise say it can be discussed."
                .to_owned(),
        );
        if !self.notes.trim().is_empty() {
            parts.push(format!("CANDIDATE NOTES FOR THIS SESSION\n{}", self.notes.trim()));
        }
        parts.join("\n\n")
    }

    /// The candidate's name from the contact table, if ABOUT.md has one.
    pub fn candidate_name(&self) -> Option<String> {
        for line in self.candidate.lines() {
            let cells: Vec<&str> = line.trim().trim_matches('|').split('|').map(str::trim).collect();
            if cells.len() >= 2 && cells[0].eq_ignore_ascii_case("full name") && !cells[1].is_empty() {
                return Some(cells[1].to_owned());
            }
        }
        None
    }

    pub fn first_name(&self) -> Option<String> {
        self.candidate_name().and_then(|name| name.split_whitespace().next().map(str::to_owned))
    }

    /// The first e-mail address written in the candidate's own notes, for the panel at the bottom.
    pub fn email(&self) -> Option<String> {
        static PATTERN: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
        let pattern = PATTERN.get_or_init(|| {
            regex::Regex::new(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)+").expect("a fixed pattern")
        });
        pattern.find(&self.candidate).map(|found| found.as_str().to_owned())
    }

    /// Technologies from the `## Skills` section, for choosing topics and biasing recognition.
    pub fn skill_terms(&self) -> Vec<String> {
        let mut terms = skill_terms(&self.candidate);
        for doc in &self.documents {
            terms.extend(skill_terms(&doc.text));
        }
        let mut seen = std::collections::HashSet::new();
        terms.retain(|term| seen.insert(term.to_lowercase()));
        terms
    }

    /// Names worth telling the speech recogniser about: the vacancy and your skills.
    pub fn vocabulary(&self) -> String {
        let mut words: Vec<String> = vec![self.company.clone(), self.job_title.clone()];
        words.extend(self.skill_terms());
        let mut seen = std::collections::HashSet::new();
        words.retain(|word| !word.is_empty() && seen.insert(word.to_lowercase()));
        words.truncate(45);
        words.join(", ")
    }

    /// `(kind, label)` for what the interviewer is working from, for the screen.
    pub fn sources(&self) -> Vec<(&'static str, String)> {
        let mut items = Vec::new();
        if !self.candidate.trim().is_empty() {
            items.push(("about", format!("ABOUT.md · {:.1}k", self.candidate.chars().count() as f32 / 1000.0)));
        }
        for doc in &self.documents {
            let kind = if doc.kind == DocKind::Cv { "cv" } else { "about" };
            items.push((kind, format!("{} · {:.1}k", doc.name, doc.text.chars().count() as f32 / 1000.0)));
        }
        if self.has_vacancy() {
            let head = [self.job_title.as_str(), self.company.as_str()]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join(" at ");
            items.push(("job", if head.is_empty() { "vacancy".into() } else { head }));
        }
        items
    }
}

fn guess_kind(name: &str) -> DocKind {
    let lower = name.to_lowercase();
    if ["cv", "resume", "резюме", "curriculum", "životopis"].iter().any(|word| lower.contains(word)) {
        DocKind::Cv
    } else {
        DocKind::About
    }
}

pub fn read_document(path: &Path) -> Result<String, BriefError> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string());
    let meta = fs::metadata(path).map_err(|_| BriefError::Document(format!("{} does not exist", path.display())))?;
    if !meta.is_file() {
        return Err(BriefError::Document(format!("{name} is not a file")));
    }
    let extension = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let text = match extension.as_str() {
        "pdf" => {
            if meta.len() > MAX_PDF_FILE {
                return Err(BriefError::Document(format!("{name} is too large to read")));
            }
            pdf_text(path).map_err(|why| BriefError::Document(format!("Could not read {name}: {why}")))?
        }
        "txt" | "md" | "" => {
            if meta.len() > MAX_TEXT_FILE {
                return Err(BriefError::Document(format!("{name} is too large to read")));
            }
            fs::read(path)
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                .map_err(|e| BriefError::Document(format!("Could not read {name}: {e}")))?
        }
        other => {
            return Err(BriefError::Document(format!(
                "{name}: unsupported format .{other}; use PDF, Markdown or text"
            )));
        }
    };
    let text = tidy(&text);
    if text.chars().count() < MIN_TEXT {
        return Err(BriefError::Document(format!(
            "{name} has almost no readable text ({} characters). A scanned or blank PDF cannot be used; attach the real CV.",
            text.chars().count()
        )));
    }
    Ok(text)
}

/// PDF parsers can panic on a damaged file; that must not take the program down.
fn pdf_text(path: &Path) -> Result<String, String> {
    let path = path.to_path_buf();
    std::panic::catch_unwind(move || pdf_extract::extract_text(&path))
        .map_err(|_| "the file is damaged".to_owned())?
        .map_err(|e| e.to_string())
}

fn tidy(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut blank = 0;
    for line in text.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        out.push_str(line);
        out.push('\n');
    }
    out.trim().to_owned()
}

pub fn truncate_chars(text: &str, limit: usize) -> &str {
    match text.char_indices().nth(limit) {
        Some((index, _)) => &text[..index],
        None => text,
    }
}

/// The `## ` sections of a Markdown text, the text before the first one included.
fn sections(text: &str) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    for line in text.split_inclusive('\n') {
        if line.starts_with("## ") || parts.is_empty() {
            parts.push(String::new());
        }
        if let Some(last) = parts.last_mut() {
            last.push_str(line);
        }
    }
    parts
}

/// ABOUT.md without the sections that never help answer a question.
pub fn slim_for_answers(text: &str) -> String {
    let mut kept = String::new();
    for part in sections(text) {
        let lower = part.to_lowercase();
        if SKIP_FOR_ANSWERS.iter().any(|skip| lower.starts_with(skip)) {
            continue;
        }
        kept.push_str(&drop_subsection(&part, "### project catalog"));
    }
    kept
}

fn drop_subsection(section: &str, heading: &str) -> String {
    let mut out = String::new();
    let mut skipping = false;
    for line in section.split_inclusive('\n') {
        if line.to_lowercase().starts_with(heading) {
            skipping = true;
            continue;
        }
        if skipping && line.starts_with("## ") {
            skipping = false;
        }
        if !skipping {
            out.push_str(line);
        }
    }
    out
}

/// Keep ABOUT.md whole when it fits; otherwise drop its least useful sections first. The tail of
/// a long ABOUT.md holds the stories and honest limits, so cutting by length would lose them.
pub fn fit_context(text: &str, budget: usize) -> String {
    if text.chars().count() <= budget {
        return text.to_owned();
    }
    let mut kept = sections(text);
    for marker in LOW_PRIORITY {
        if kept.iter().map(|p| p.chars().count()).sum::<usize>() <= budget {
            break;
        }
        kept.retain(|part| !part.to_lowercase().starts_with(marker));
    }
    truncate_chars(&kept.concat(), budget).to_owned()
}

fn skill_terms(text: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut in_skills = false;
    for line in text.lines() {
        if line.starts_with("## ") {
            in_skills = line.trim_start_matches("## ").trim().eq_ignore_ascii_case("skills");
            continue;
        }
        if !in_skills {
            continue;
        }
        let stripped = line.trim();
        let mut body = stripped.to_owned();
        if stripped.starts_with('|') {
            let cells: Vec<&str> = stripped.trim_matches('|').split('|').map(str::trim).collect();
            let is_rule = cells.iter().all(|c| !c.is_empty() && c.chars().all(|ch| matches!(ch, '-' | ':')));
            if is_rule
                || cells.len() < 2
                || cells[0].eq_ignore_ascii_case("level")
                || cells[0].eq_ignore_ascii_case("group")
            {
                continue;
            }
            body = cells[1..].join(", ");
        }
        let body = body.rsplit_once(':').map_or(body.as_str(), |(_, rest)| rest);
        for part in body.split([',', ';', '(', ')', '|']) {
            let term = part.trim_matches(|c: char| c.is_whitespace() || matches!(c, '-' | '*' | '.'));
            if (2..=40).contains(&term.chars().count()) && term.chars().any(|c| c.is_alphabetic()) {
                terms.push(term.to_owned());
            }
        }
    }
    terms
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEMO: &str = include_str!("../tests/fixtures/ABOUT.demo.md");

    fn demo_brief() -> Brief {
        Brief { candidate: DEMO.to_owned(), ..Brief::default() }
    }

    #[test]
    fn finds_the_name_and_first_name() {
        let brief = demo_brief();
        assert_eq!(brief.candidate_name().as_deref(), Some("Alex Demo"));
        assert_eq!(brief.first_name().as_deref(), Some("Alex"));
        assert_eq!(Brief::default().candidate_name(), None);
    }

    #[test]
    fn the_first_email_in_the_notes_is_found() {
        let brief = Brief {
            candidate: "| Contact | Write to <alex.demo+jobs@example.com>, or a@b.org |".into(),
            ..Brief::default()
        };
        assert_eq!(brief.email().as_deref(), Some("alex.demo+jobs@example.com"));
        assert_eq!(Brief::default().email(), None);
        let none = Brief { candidate: "no address here, only at-signs @ and words".into(), ..Brief::default() };
        assert_eq!(none.email(), None);
    }

    #[test]
    fn skill_terms_come_from_the_skills_section_only() {
        let terms = demo_brief().skill_terms();
        for expected in ["Python", "FastAPI", "PostgreSQL", "Docker", "Kubernetes"] {
            assert!(terms.iter().any(|t| t == expected), "{expected} in {terms:?}");
        }
        assert!(!terms.iter().any(|t| t.contains("Gdansk")), "{terms:?}");
    }

    #[test]
    fn vocabulary_names_the_vacancy_and_skills() {
        let mut brief = demo_brief();
        brief.set_vacancy("Backend Engineer", "Acme", "We use Rust.");
        let vocabulary = brief.vocabulary();
        assert!(
            vocabulary.contains("Acme") && vocabulary.contains("Backend Engineer") && vocabulary.contains("FastAPI")
        );
    }

    #[test]
    fn render_holds_the_facts_the_vacancy_and_the_honesty_rule() {
        let mut brief = demo_brief();
        brief.set_vacancy("Backend Engineer", "Acme", "Python and PostgreSQL.");
        brief.notes = "Interview on Friday".into();
        let text = brief.render();
        assert!(text.contains("CANDIDATE CONTEXT"));
        assert!(text.contains("Honest limits") && text.contains("No production Kubernetes"));
        assert!(text.contains("VACANCY: Backend Engineer at Acme"));
        assert!(text.contains("SALARY POLICY"));
        assert!(text.contains("Interview on Friday"));
    }

    #[test]
    fn an_empty_brief_says_there_is_nothing_to_go_on() {
        assert!(Brief::default().is_empty());
        assert!(Brief::default().render().contains("none provided"));
    }

    #[test]
    fn bookkeeping_sections_are_left_out_of_answers() {
        let text = "## Experience\nreal\n## Applications log\nspam\n## CV files\nfiles\n## Honest limits\nkeep";
        let slim = slim_for_answers(text);
        assert!(slim.contains("real") && slim.contains("keep"));
        assert!(!slim.contains("spam") && !slim.contains("files"));
    }

    #[test]
    fn the_project_catalog_subsection_is_dropped() {
        let text = "## Projects\n### Project catalog\nbig list\n### Other\nstays\n## Next\nhere";
        let slim = slim_for_answers(text);
        assert!(!slim.contains("big list"), "{slim}");
        assert!(slim.contains("here"));
    }

    #[test]
    fn a_long_context_drops_low_priority_sections_before_the_honest_limits() {
        let filler = "x".repeat(300);
        let text = format!("## Experience\nwork\n## Applications log\n{filler}\n## Honest limits\nno kubernetes");
        let fitted = fit_context(&text, 120);
        assert!(fitted.contains("no kubernetes"), "{fitted}");
        assert!(!fitted.contains("xxx"));
    }

    #[test]
    fn truncation_respects_character_boundaries() {
        let text = "ёжик".repeat(10);
        assert_eq!(truncate_chars(&text, 3), "ёжи");
        assert_eq!(truncate_chars("ab", 10), "ab");
    }

    #[test]
    fn loads_about_and_a_text_cv_from_disk() {
        let dir = tempfile::tempdir().unwrap();
        let about = dir.path().join("ABOUT.md");
        fs::write(&about, DEMO).unwrap();
        let cv = dir.path().join("alex-cv.md");
        fs::write(&cv, format!("# Alex Demo CV\n\n{}", "Backend developer with Python and FastAPI. ".repeat(6)))
            .unwrap();
        let mut settings = Settings::default();
        settings.about_path = about.to_string_lossy().into_owned();
        settings.cv_paths = vec![cv.to_string_lossy().into_owned()];
        let (brief, warnings) = Brief::load(&settings).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(brief.documents.len(), 1);
        assert_eq!(brief.documents[0].kind, DocKind::Cv);
        assert!(brief.render().contains("CV THE EMPLOYER HAS READ: alex-cv.md"));
        assert_eq!(brief.sources().len(), 2);
    }

    #[test]
    fn a_missing_about_with_no_cv_is_an_error_naming_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut settings = Settings::default();
        settings.about_path = dir.path().join("nope.md").to_string_lossy().into_owned();
        let error = Brief::load(&settings).unwrap_err().to_string();
        assert!(error.contains("nope.md"), "{error}");
    }

    #[test]
    fn a_bad_cv_is_a_warning_and_the_rest_still_loads() {
        let dir = tempfile::tempdir().unwrap();
        let about = dir.path().join("ABOUT.md");
        fs::write(&about, DEMO).unwrap();
        let mut settings = Settings::default();
        settings.about_path = about.to_string_lossy().into_owned();
        settings.cv_paths = vec![dir.path().join("missing.pdf").to_string_lossy().into_owned()];
        let (brief, warnings) = Brief::load(&settings).unwrap();
        assert_eq!(warnings.len(), 1);
        assert!(!brief.is_empty());
    }

    #[test]
    fn the_same_file_is_attached_once() {
        let dir = tempfile::tempdir().unwrap();
        let cv = dir.path().join("resume.txt");
        fs::write(&cv, "Experienced engineer. ".repeat(20)).unwrap();
        let mut brief = demo_brief();
        brief.add_document(&cv.to_string_lossy()).unwrap();
        brief.add_document(&cv.to_string_lossy()).unwrap();
        assert_eq!(brief.documents.len(), 1);
    }

    #[test]
    fn unsupported_tiny_and_oversized_files_are_refused_with_a_reason() {
        let dir = tempfile::tempdir().unwrap();
        let tiny = dir.path().join("tiny.txt");
        fs::write(&tiny, "short").unwrap();
        assert!(read_document(&tiny).unwrap_err().to_string().contains("almost no readable text"));
        let odd = dir.path().join("cv.exe");
        fs::write(&odd, "x").unwrap();
        assert!(read_document(&odd).unwrap_err().to_string().contains("unsupported format"));
        let big = dir.path().join("big.txt");
        fs::write(&big, vec![b'a'; (MAX_TEXT_FILE + 1) as usize]).unwrap();
        assert!(read_document(&big).unwrap_err().to_string().contains("too large"));
    }

    #[test]
    fn a_damaged_pdf_is_an_error_not_a_crash() {
        let dir = tempfile::tempdir().unwrap();
        let pdf = dir.path().join("cv.pdf");
        fs::write(&pdf, b"%PDF-1.4 this is not really a pdf \x00\x01\x02").unwrap();
        assert!(read_document(&pdf).is_err());
    }

    #[test]
    fn cv_kinds_are_guessed_from_the_name() {
        assert_eq!(guess_kind("Alex_CV_2026.pdf"), DocKind::Cv);
        assert_eq!(guess_kind("резюме.pdf"), DocKind::Cv);
        assert_eq!(guess_kind("stories.md"), DocKind::About);
    }
}
