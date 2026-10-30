//! What the coach knows: your ABOUT.md, the CV the employer received, and the vacancy.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use fancy_regex::Regex;

use crate::prompt_text::TOPIC_TERMS;
use crate::questions::Kind;

pub const CONTEXT_CHARS: usize = 26_000;
pub const CV_CHARS: usize = 9_000;
pub const JOB_CHARS: usize = 6_000;
pub const DOSSIER_IN_PROMPT: usize = 12_000;
pub const MAX_VOCABULARY: usize = 50;
/// How much of the vacancy goes with a question about the project: the project is the subject, not the job ad.
pub const VACANCY_FOR_PROJECT: usize = 600;
/// The sections of ABOUT.md that help with a question about a project.
const PROJECT_SECTIONS: [&str; 4] = ["education", "experience", "projects", "skills"];
const LOW_PRIORITY_SECTIONS: [&str; 3] = ["applications log", "cv files", "contact"];
/// Parts of ABOUT.md written for CV writing and bookkeeping, not for answering a question: the log of
/// applications, CV file names, the job-ad matching rules and the project catalogue. Dropping them makes
/// every request about 40% smaller, so the first words come sooner.
const SKIP_FOR_ANSWERS: [&str; 3] = ["applications log", "cv files", "matching a job ad"];
/// A question about a technology is answered from the skills and projects: contact details, form answers and
/// interview stories stay out of its request, which is then a third smaller and rate limited later.
const SKIP_FOR_KNOWLEDGE: [&str; 3] = ["contact", "standard form answers", "interview stories"];

/// A file the coach reads. `kind` is `cv` for what the employer received, `about` for extra facts.
#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    pub name: String,
    pub text: String,
    pub path: String,
    pub kind: String,
    pub enabled: bool,
}

impl Document {
    pub fn new(name: &str, text: &str, kind: &str) -> Self {
        Self { name: name.to_owned(), text: text.to_owned(), path: String::new(), kind: kind.to_owned(), enabled: true }
    }
}

/// CVs are recognised by name; anything else is treated as extra facts about you.
pub fn guess_kind(name: &str) -> &'static str {
    let lower = name.to_lowercase();
    if ["cv", "resume", "резюме", "curriculum", "životopis"].iter().any(|word| lower.contains(word)) {
        "cv"
    } else {
        "about"
    }
}

/// The project the candidate defends: its dossier goes with every question.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProjectDossier {
    pub name: String,
    pub dossier: String,
}

impl ProjectDossier {
    pub fn for_prompt(&self) -> String {
        if self.dossier.trim().is_empty() {
            return String::new();
        }
        let dossier: String = self.dossier.trim().chars().take(DOSSIER_IN_PROMPT).collect();
        format!("PROJECT THE CANDIDATE WILL DEFEND: {}\n{dossier}", self.name)
    }
}

#[derive(Clone, Debug, Default)]
pub struct InterviewBrief {
    pub candidate: String,
    pub job_title: String,
    pub company: String,
    pub job_description: String,
    pub salary: String,
    pub notes: String,
    pub documents: Vec<Document>,
    pub project: Option<ProjectDossier>,
}

fn chars(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
}

impl InterviewBrief {
    pub fn new(candidate: &str) -> Self {
        Self { candidate: candidate.to_owned(), salary: describe_salary(&salary_policy(candidate)), ..Self::default() }
    }

    pub fn set_candidate(&mut self, text: &str) {
        self.candidate = text.to_owned();
        self.salary = describe_salary(&salary_policy(text));
    }

    /// True when there is nothing to ground an answer in.
    pub fn is_empty(&self) -> bool {
        self.candidate.trim().is_empty() && !self.documents.iter().any(|doc| doc.enabled)
    }

    /// The request's background. `kind` is the kind of question: a knowledge question gets less.
    pub fn render(&self, kind: Option<Kind>) -> String {
        let knowledge = kind.is_some_and(Kind::is_knowledge);
        let mut parts: Vec<String> = Vec::new();
        if !self.candidate.trim().is_empty() {
            let about = fit_context(&slim_for_answers(&self.candidate, knowledge), CONTEXT_CHARS);
            parts.push(format!("CANDIDATE CONTEXT (ABOUT.md, verified facts)\n{about}"));
        }
        for doc in &self.documents {
            if !doc.enabled || (knowledge && doc.kind == "cv" && !self.candidate.trim().is_empty()) {
                continue;
            }
            let heading = if doc.kind == "cv" {
                "CV THE EMPLOYER HAS READ"
            } else {
                "MORE FACTS ABOUT THE CANDIDATE (the employer has not seen this)"
            };
            parts.push(format!("{heading}: {}\n{}", doc.name, chars(&doc.text, CV_CHARS)));
        }
        if parts.is_empty() {
            parts.push("CANDIDATE CONTEXT: (none provided)".to_owned());
        }
        if !self.job_description.is_empty() || !self.job_title.is_empty() {
            let head = [self.job_title.as_str(), self.company.as_str()]
                .iter()
                .filter(|part| !part.is_empty())
                .copied()
                .collect::<Vec<_>>()
                .join(" at ");
            parts.push(format!("VACANCY: {head}\n{}", chars(&self.job_description, JOB_CHARS)).trim().to_owned());
        }
        parts.push(format!("SALARY POLICY: {}", self.salary));
        if !self.notes.is_empty() {
            parts.push(format!("CANDIDATE NOTES FOR THIS INTERVIEW\n{}", self.notes));
        }
        if let Some(project) = &self.project {
            let dossier = project.for_prompt();
            if !dossier.is_empty() {
                parts.push(dossier);
            }
        }
        parts.join("\n\n")
    }

    /// The background for a question about the project being defended: who the candidate is (education, experience,
    /// projects, skills), a short look at the vacancy and the part of the dossier that bears on the question. The
    /// rest of ABOUT.md and the CV are left out, so the request stays under a small provider's limit.
    pub fn render_for_project(&self, dossier: &str) -> String {
        let mut parts: Vec<String> = Vec::new();
        let profile = profile_for_project(&self.candidate);
        if !profile.trim().is_empty() {
            parts.push(format!("CANDIDATE CONTEXT (ABOUT.md, verified facts)\n{profile}"));
        }
        if !self.job_description.is_empty() || !self.job_title.is_empty() {
            let head = [self.job_title.as_str(), self.company.as_str()]
                .iter()
                .filter(|part| !part.is_empty())
                .copied()
                .collect::<Vec<_>>()
                .join(" at ");
            parts.push(
                format!("VACANCY: {head}\n{}", chars(&self.job_description, VACANCY_FOR_PROJECT)).trim().to_owned(),
            );
        }
        if !self.notes.is_empty() {
            parts.push(format!("CANDIDATE NOTES FOR THIS INTERVIEW\n{}", chars(&self.notes, 600)));
        }
        if let Some(project) = &self.project
            && !dossier.trim().is_empty()
        {
            parts.push(format!("PROJECT THE CANDIDATE WILL DEFEND: {}\n{dossier}", project.name));
        }
        parts.join("\n\n")
    }

    /// The candidate's main technologies, so a coding task is solved in a language they know.
    pub fn stack_hint(&self, limit: usize) -> String {
        let mut terms = skill_terms(&self.candidate);
        for doc in self.documents.iter().filter(|doc| doc.enabled) {
            terms.extend(skill_terms(&doc.text));
        }
        unique(terms).into_iter().take(limit).collect::<Vec<_>>().join(", ")
    }

    /// Names worth biasing speech recognition toward: the vacancy and your skills.
    pub fn vocabulary(&self) -> String {
        let mut words = vec![self.company.clone(), self.job_title.clone()];
        words.extend(vacancy_terms(&self.job_description).into_iter().take(25));
        words.extend(skill_terms(&self.candidate));
        for doc in self.documents.iter().filter(|doc| doc.enabled) {
            words.extend(skill_terms(&doc.text));
        }
        let mut names: Vec<String> =
            unique(words.into_iter().filter(|word| !word.is_empty()).collect()).into_iter().take(24).collect();
        let topics = format!("{} {}", self.job_title, self.job_description).to_lowercase();
        for (topic, terms) in TOPIC_TERMS {
            if topics.contains(topic) {
                for term in terms.split(", ") {
                    if !names.iter().any(|name| name == term) {
                        names.push(term.to_owned());
                    }
                }
            }
        }
        names.into_iter().take(MAX_VOCABULARY).collect::<Vec<_>>().join(", ")
    }

    /// What the coach is working from, as (kind, label) pairs for the window.
    pub fn sources(&self) -> Vec<(String, String)> {
        let mut items = Vec::new();
        if !self.candidate.trim().is_empty() {
            items.push((
                "about".to_owned(),
                format!("ABOUT.md · {:.1}k", self.candidate.chars().count() as f64 / 1000.0),
            ));
        }
        for doc in self.documents.iter().filter(|doc| doc.enabled) {
            items.push((doc.kind.clone(), format!("{} · {:.1}k", doc.name, doc.text.chars().count() as f64 / 1000.0)));
        }
        if !self.job_title.is_empty() || !self.job_description.is_empty() {
            let head = [self.job_title.as_str(), self.company.as_str()]
                .iter()
                .filter(|part| !part.is_empty())
                .copied()
                .collect::<Vec<_>>()
                .join(" at ");
            items.push(("job".to_owned(), if head.is_empty() { "vacancy".to_owned() } else { head }));
        }
        if let Some(project) = self.project.as_ref().filter(|project| !project.dossier.trim().is_empty()) {
            items.push(("project".to_owned(), format!("project {}", project.name)));
        }
        items
    }
}

fn unique(words: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    words.into_iter().filter(|word| seen.insert(word.clone())).collect()
}

/// The text cut before every line that starts a `## ` section; the first piece is whatever comes before.
fn split_sections(text: &str) -> Vec<&str> {
    let mut starts = vec![0usize];
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        if offset > 0 && line.starts_with("## ") {
            starts.push(offset);
        }
        offset += line.len();
    }
    starts.push(text.len());
    starts.windows(2).map(|pair| &text[pair[0]..pair[1]]).collect()
}

/// ABOUT.md without the sections that never help answer a question (and, for a question about a technology,
/// the ones that only help with a question about the candidate).
pub fn slim_for_answers(text: &str, knowledge: bool) -> String {
    let skip: Vec<String> = SKIP_FOR_ANSWERS
        .iter()
        .chain(if knowledge { SKIP_FOR_KNOWLEDGE.iter() } else { [].iter() })
        .map(|name| format!("## {name}"))
        .collect();
    let kept: String = split_sections(text)
        .into_iter()
        .filter(|part| !skip.iter().any(|name| part.to_lowercase().starts_with(name)))
        .collect();
    drop_project_catalog(&kept)
}

/// Only the sections of ABOUT.md that help defend a project, without the catalogue of every project.
pub fn profile_for_project(text: &str) -> String {
    let kept: String = split_sections(text)
        .into_iter()
        .filter(|part| {
            let lower = part.to_lowercase();
            PROJECT_SECTIONS.iter().any(|name| lower.starts_with(&format!("## {name}")))
        })
        .collect();
    drop_project_catalog(&kept)
}

fn drop_project_catalog(text: &str) -> String {
    let mut out = String::new();
    let mut dropping = false;
    for line in text.split_inclusive('\n') {
        if line.starts_with("### Project catalog") {
            dropping = true;
        } else if dropping && line.starts_with("## ") {
            dropping = false;
        }
        if !dropping {
            out.push_str(line);
        }
    }
    out
}

/// Keep ABOUT.md whole when it fits; otherwise drop its least useful sections first. The tail of a long
/// ABOUT.md holds the interview stories and honest limits, so cutting by length would remove exactly what the
/// coach needs.
pub fn fit_context(text: &str, budget: usize) -> String {
    if text.chars().count() <= budget {
        return text.to_owned();
    }
    let mut kept: Vec<&str> = split_sections(text);
    for marker in LOW_PRIORITY_SECTIONS {
        if kept.iter().map(|part| part.chars().count()).sum::<usize>() <= budget {
            break;
        }
        let heading = format!("## {marker}");
        kept.retain(|part| !part.to_lowercase().starts_with(&heading));
    }
    let joined: String = kept.concat();
    if joined.chars().count() <= budget { joined } else { chars(&joined, budget) }
}

static SKILLS_SECTION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?mi)^##[ \t]+Skills[ \t]*$([\s\S]*?)(?=^##\s|\z)").expect("a fixed pattern"));
static TABLE_RULE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^:?-{3,}:?$").expect("a fixed pattern"));
static TABLE_RULE_ROW: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\|[\s:|-]+\|?$").expect("a fixed pattern"));
static UPPER_OR_DIGIT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[A-Z0-9]").expect("a fixed pattern"));

/// Technology names from the `## Skills` section of the candidate context.
pub fn skill_terms(candidate: &str) -> Vec<String> {
    let Some(section) = SKILLS_SECTION.captures(candidate).ok().flatten().and_then(|found| found.get(1)) else {
        return Vec::new();
    };
    let lines: Vec<&str> = section.as_str().lines().collect();
    let mut terms = Vec::new();
    for (number, raw) in lines.iter().enumerate() {
        let stripped = raw.trim();
        let mut line = stripped.to_owned();
        if stripped.starts_with('|') {
            let cells: Vec<String> = stripped.trim_matches('|').split('|').map(|cell| cell.trim().to_owned()).collect();
            let separator = cells.iter().all(|cell| TABLE_RULE.is_match(cell).unwrap_or(false));
            let before_separator =
                lines.get(number + 1).is_some_and(|next| TABLE_RULE_ROW.is_match(next.trim()).unwrap_or(false));
            if separator || before_separator {
                continue; // the rule and the header row of a Markdown table
            }
            if cells.len() > 1 {
                line = cells[1..].join(", "); // the first cell names a level or a group
            }
        }
        let line = line.rsplit_once(':').map_or(line.as_str(), |(_, after)| after).to_owned();
        for part in line.split([',', ';', '(', ')', '|']) {
            let term = part.trim_matches(|c: char| c.is_whitespace() || "-*.".contains(c));
            let words = term.split_whitespace().count();
            if words > 0 && words <= 3 && term.chars().count() <= 30 && UPPER_OR_DIGIT.is_match(term).unwrap_or(false) {
                terms.push(term.to_owned());
            }
        }
    }
    terms
}

static VACANCY_WORD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?<![\w])[A-Z][A-Za-z0-9+#.]+(?![\w])").expect("a fixed pattern"));

/// Names of technologies in a vacancy: words with a capital that do not simply begin a sentence or a line
/// ("What", "Our", "Build"), unless they have a capital inside, a digit or a symbol (PySpark, C#). Every word
/// given to speech recognition is one it may write back on music, so the list must be short.
pub fn vacancy_terms(text: &str) -> Vec<String> {
    let mut found: BTreeMap<usize, String> = BTreeMap::new();
    let mut seen = std::collections::HashSet::new();
    for (order, hit) in VACANCY_WORD.find_iter(text).flatten().enumerate() {
        let word = hit.as_str().trim_end_matches('.');
        let previous = text[..hit.start()].trim_end_matches([' ', '\t']).chars().last();
        let begins = previous.is_none_or(|c| ".!?\n\u{2022}*-".contains(c));
        let technical = word.chars().skip(1).any(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || "+#".contains(c));
        if word.chars().count() > 1 && (technical || !begins) && seen.insert(word.to_owned()) {
            found.insert(order, word.to_owned());
        }
    }
    found.into_values().collect()
}

const MARKER: &str = "<!-- CAREERTWIN_STUDIO_STATE -->";

/// `fixed` figures from the studio snapshot in ABOUT.md, or the automatic policy.
pub fn salary_policy(context: &str) -> serde_json::Map<String, serde_json::Value> {
    let auto = || serde_json::Map::from_iter([("mode".to_owned(), serde_json::Value::String("auto".to_owned()))]);
    let Some(start) = context.find(MARKER) else { return auto() };
    let rest = &context[start + MARKER.len()..];
    let Some(open) = rest.find("```json") else { return auto() };
    let body = &rest[open + "```json".len()..];
    let Some(close) = body.find("\n```") else { return auto() };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body[..close].trim()) else { return auto() };
    match value.pointer("/preferences/salary").and_then(|salary| salary.as_object()) {
        Some(salary) if salary.get("mode").and_then(|mode| mode.as_str()) == Some("fixed") => salary.clone(),
        _ => auto(),
    }
}

pub fn describe_salary(policy: &serde_json::Map<String, serde_json::Value>) -> String {
    if policy.get("mode").and_then(|mode| mode.as_str()) != Some("fixed") {
        return "auto: do not invent a figure; give a researched market range or ask the range".to_owned();
    }
    let figures: Vec<String> = policy
        .iter()
        .filter(|(key, value)| *key != "mode" && !value.is_null() && value.as_str() != Some(""))
        .map(|(key, value)| format!("{key}={}", value.as_str().map_or_else(|| value.to_string(), str::to_owned)))
        .collect();
    format!("fixed: {}", figures.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ABOUT: &str = "# Maksim: candidate profile\n\n## Contact\nphone 123\n\n## Experience\nBuilt async pipelines.\n\n\
## Skills\n| Level | Skills |\n| --- | --- |\n| Strong | Python, SQL / PostgreSQL, Docker |\n| Working | FastAPI (async), Redis |\n\n\
## Standard form answers\nSalary: negotiable.\n\n## Interview stories (STAR)\nThe night the queue stalled.\n\n\
## Applications log\n| Intel | sent |\n\n## Honest limits\nNo Spark in production.\n";

    #[test]
    fn answers_leave_out_the_bookkeeping_sections() {
        let slim = slim_for_answers(ABOUT, false);
        assert!(slim.contains("## Experience") && slim.contains("## Honest limits") && slim.contains("## Contact"));
        assert!(!slim.contains("Applications log"));
    }

    #[test]
    fn a_question_about_a_technology_gets_a_smaller_background() {
        let slim = slim_for_answers(ABOUT, true);
        assert!(slim.contains("## Skills") && slim.contains("## Experience"));
        assert!(!slim.contains("## Contact") && !slim.contains("Standard form") && !slim.contains("Interview stories"));
    }

    #[test]
    fn the_project_catalogue_is_dropped_but_not_the_next_section() {
        let text = "## Projects\n- a\n### Project catalog (public)\n- x\n- y\n## Skills\nPython\n";
        let slim = slim_for_answers(text, false);
        assert!(!slim.contains("- x") && slim.contains("## Skills") && slim.contains("- a"));
    }

    #[test]
    fn a_long_about_loses_its_least_useful_sections_first() {
        let long = format!("{ABOUT}{}", "filler ".repeat(100));
        let fitted = fit_context(&long, ABOUT.len());
        assert!(!fitted.contains("Applications log") && fitted.contains("## Honest limits"));
        assert_eq!(fit_context("short", 100), "short");
        assert_eq!(fit_context(&"x".repeat(50), 10).len(), 10);
    }

    #[test]
    fn skills_are_read_from_the_table_without_its_header() {
        assert_eq!(skill_terms(ABOUT), ["Python", "SQL / PostgreSQL", "Docker", "FastAPI", "Redis"]);
        assert_eq!(InterviewBrief::new(ABOUT).stack_hint(3), "Python, SQL / PostgreSQL, Docker");
    }

    #[test]
    fn vacancy_terms_are_technologies_not_sentence_starts() {
        let terms =
            vacancy_terms("What you do: build pipelines in PySpark and Python. Our stack: C#, Kafka and Airflow.");
        assert!(
            terms.contains(&"PySpark".to_owned())
                && terms.contains(&"C#".to_owned())
                && terms.contains(&"Kafka".to_owned())
        );
        assert!(!terms.contains(&"What".to_owned()) && !terms.contains(&"Our".to_owned()));
    }

    #[test]
    fn a_knowledge_question_skips_the_cv_when_about_is_there() {
        let mut brief = InterviewBrief::new(ABOUT);
        brief.documents.push(Document::new("cv.pdf", "CV: pipelines", "cv"));
        assert!(brief.render(Some(Kind::Intro)).contains("CV THE EMPLOYER HAS READ: cv.pdf"));
        assert!(!brief.render(Some(Kind::Technical)).contains("CV THE EMPLOYER HAS READ"));
        assert!(brief.render(None).contains("SALARY POLICY: auto"));
    }

    #[test]
    fn the_vacancy_notes_and_project_go_into_the_background() {
        let mut brief = InterviewBrief::new("");
        brief.job_title = "Data Engineer".into();
        brief.company = "Vention".into();
        brief.job_description = "Python and SQL".into();
        brief.notes = "I study in the evenings".into();
        brief.project = Some(ProjectDossier { name: "bot".into(), dossier: "A Telegram bot.".into() });
        let text = brief.render(Some(Kind::Intro));
        assert!(
            text.contains("CANDIDATE CONTEXT: (none provided)")
                && text.contains("VACANCY: Data Engineer at Vention\nPython and SQL")
        );
        assert!(text.contains("CANDIDATE NOTES FOR THIS INTERVIEW\nI study in the evenings"));
        assert!(text.contains("PROJECT THE CANDIDATE WILL DEFEND: bot\nA Telegram bot."));
        assert_eq!(brief.sources().last().unwrap().1, "project bot");
    }

    #[test]
    fn a_fixed_salary_comes_from_the_studio_snapshot() {
        let about = format!(
            "{MARKER}\n```json\n{{\"preferences\": {{\"salary\": {{\"mode\": \"fixed\", \"min\": 5000, \"currency\": \"PLN\"}}}}}}\n```\n"
        );
        assert_eq!(describe_salary(&salary_policy(&about)), "fixed: min=5000, currency=PLN");
        assert!(describe_salary(&salary_policy("nothing")).starts_with("auto"));
    }

    #[test]
    fn a_name_tells_a_cv_from_other_facts() {
        assert_eq!(guess_kind("Maksim_CV_EN.pdf"), "cv");
        assert_eq!(guess_kind("notes.md"), "about");
    }

    #[test]
    fn the_vocabulary_names_the_vacancy_the_skills_and_the_topics_it_mentions() {
        let mut brief = InterviewBrief::new(ABOUT);
        brief.company = "Vention".into();
        brief.job_title = "Python Data Engineer".into();
        let vocabulary = brief.vocabulary();
        assert!(vocabulary.starts_with("Vention") && vocabulary.contains("Docker") && vocabulary.contains("GIL"));
    }

    #[test]
    fn a_project_question_gets_the_profile_a_short_vacancy_and_the_given_part_of_the_dossier_only() {
        let mut brief = InterviewBrief::new(
            "## Contact\nmax@example.com\n\n## Education\nBSc.\n\n## Experience\nIntern.\n\n## Projects\n### Project catalog\nbig list\n\n## Skills\nPython\n\n## Interview stories\nlong story\n",
        );
        brief.job_title = "Backend".into();
        brief.company = "Acme".into();
        brief.job_description = "x".repeat(5000);
        brief.documents.push(Document::new("cv.pdf", "CV TEXT", "cv"));
        brief.project = Some(ProjectDossier { name: "bot".into(), dossier: "whole dossier".into() });
        let text = brief.render_for_project("## Overview\nA bot.");
        for kept in [
            "## Education",
            "## Experience",
            "## Skills",
            "VACANCY: Backend at Acme",
            "PROJECT THE CANDIDATE WILL DEFEND: bot\n## Overview\nA bot.",
        ] {
            assert!(text.contains(kept), "{kept} missing: {text}");
        }
        for dropped in ["max@example.com", "big list", "long story", "CV TEXT", "whole dossier", "SALARY POLICY"] {
            assert!(!text.contains(dropped), "{dropped} should be out: {text}");
        }
        assert!(text.chars().count() < brief.render(None).chars().count() / 2);
        brief.project = None;
        assert!(!brief.render_for_project("").contains("PROJECT THE CANDIDATE"));
    }
}
