//! What survives between runs: the interviews (each with its vacancy, the CV you sent and your notes) in
//! `data/interview/context.json`, the same file the Python overlay keeps. ABOUT.md is shared by all of them.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};
use uuid::Uuid;

use crate::brief::{Document, InterviewBrief, guess_kind};
use crate::settings::write_private;

pub const DEFAULT_NAME: &str = "Interview";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedDocument {
    pub path: String,
    pub kind: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Vacancy {
    pub title: String,
    pub company: String,
    pub text: String,
    /// `job` when the vacancy is one of the pipeline (an id of the Python agents), which this window cannot open.
    pub job: String,
}

impl Vacancy {
    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty() && self.title.trim().is_empty() && self.job.is_empty()
    }

    /// "Title at Company", or how much text there is.
    pub fn label(&self) -> String {
        let head = [self.title.as_str(), self.company.as_str()].iter().filter(|part| !part.is_empty()).copied().collect::<Vec<_>>().join(" at ");
        if !head.is_empty() {
            head
        } else if !self.text.is_empty() {
            format!("{:.1}k characters", self.text.chars().count() as f64 / 1000.0)
        } else {
            String::new()
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Interview {
    pub id: String,
    pub name: String,
    pub documents: Vec<SavedDocument>,
    pub notes: String,
    pub vacancy: Vacancy,
    /// The folder of the project to defend, if one is chosen.
    pub project: String,
}

impl Interview {
    pub fn new(name: &str) -> Self {
        Self { id: Uuid::new_v4().simple().to_string()[..8].to_owned(), name: name.to_owned(), documents: Vec::new(), notes: String::new(), vacancy: Vacancy::default(), project: String::new() }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Library {
    pub interviews: Vec<Interview>,
    pub active: String,
}

impl Default for Library {
    fn default() -> Self {
        Self::from_parts(Vec::new(), "")
    }
}

fn text(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

impl Library {
    fn from_parts(mut interviews: Vec<Interview>, active: &str) -> Self {
        if interviews.is_empty() {
            interviews.push(Interview::new(&format!("{DEFAULT_NAME} 1")));
        }
        let active = if interviews.iter().any(|item| item.id == active) { active.to_owned() } else { interviews[0].id.clone() };
        Self { interviews, active }
    }

    pub fn current(&self) -> &Interview {
        self.interviews.iter().find(|item| item.id == self.active).unwrap_or(&self.interviews[0])
    }

    pub fn current_mut(&mut self) -> &mut Interview {
        let position = self.interviews.iter().position(|item| item.id == self.active).unwrap_or(0);
        &mut self.interviews[position]
    }

    pub fn load(path: &Path) -> Self {
        let Ok(raw) = std::fs::read_to_string(path) else { return Self::default() };
        let Ok(Value::Object(mut data)) = serde_json::from_str::<Value>(&raw) else { return Self::default() };
        if !data.contains_key("interviews") {
            // the first version of the file held one interview
            let mut only = data.clone();
            only.entry("name").or_insert_with(|| json!(format!("{DEFAULT_NAME} 1")));
            data = Map::from_iter([("interviews".to_owned(), json!([Value::Object(only)]))]);
        }
        let interviews = data["interviews"].as_array().map(|items| items.iter().filter_map(Value::as_object).map(read_interview).collect()).unwrap_or_default();
        Self::from_parts(interviews, &text(data.get("active")))
    }

    pub fn to_json(&self) -> Value {
        json!({
            "active": self.active,
            "interviews": self.interviews.iter().map(|item| {
                let mut vacancy = Map::new();
                if !item.vacancy.job.is_empty() {
                    vacancy.insert("type".into(), json!("job"));
                    vacancy.insert("value".into(), json!(item.vacancy.job));
                } else if !item.vacancy.is_empty() {
                    vacancy.insert("type".into(), json!("text"));
                    vacancy.insert("title".into(), json!(item.vacancy.title));
                    vacancy.insert("company".into(), json!(item.vacancy.company));
                    vacancy.insert("text".into(), json!(item.vacancy.text));
                }
                json!({
                    "id": item.id,
                    "name": item.name,
                    "documents": item.documents.iter().map(|doc| json!({"path": doc.path, "kind": doc.kind, "enabled": doc.enabled})).collect::<Vec<_>>(),
                    "notes": item.notes,
                    "vacancy": Value::Object(vacancy),
                    "project": item.project,
                })
            }).collect::<Vec<_>>(),
        })
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        write_private(path, &serde_json::to_string_pretty(&self.to_json()).unwrap_or_default())
    }

    /// Add an empty interview and make it the active one.
    pub fn new_interview(&mut self, name: &str) -> &Interview {
        let name = if name.trim().is_empty() { format!("{DEFAULT_NAME} {}", self.interviews.len() + 1) } else { name.trim().to_owned() };
        let interview = Interview::new(&name);
        self.active = interview.id.clone();
        self.interviews.push(interview);
        self.current()
    }

    /// Remove an interview (the last one is kept, emptied); the copied files no other interview uses come back.
    pub fn delete(&mut self, id: &str) -> Vec<PathBuf> {
        let Some(gone) = self.interviews.iter().find(|item| item.id == id).cloned() else { return Vec::new() };
        self.interviews.retain(|item| item.id != id);
        *self = Self::from_parts(std::mem::take(&mut self.interviews), &self.active.clone());
        let used: Vec<&String> = self.interviews.iter().flat_map(|item| item.documents.iter().map(|doc| &doc.path)).collect();
        gone.documents.iter().filter(|doc| !used.contains(&&doc.path)).map(|doc| PathBuf::from(&doc.path)).collect()
    }
}

fn read_interview(item: &Map<String, Value>) -> Interview {
    let documents = item.get("documents").and_then(Value::as_array).map(|docs| {
        docs.iter()
            .filter_map(Value::as_object)
            .filter(|doc| doc.get("path").and_then(Value::as_str).is_some_and(|path| !path.is_empty()))
            .map(|doc| SavedDocument {
                path: text(doc.get("path")),
                kind: doc.get("kind").and_then(Value::as_str).unwrap_or("cv").to_owned(),
                enabled: doc.get("enabled").and_then(Value::as_bool).unwrap_or(true),
            })
            .collect()
    });
    let vacancy = item.get("vacancy").and_then(Value::as_object).map(|vacancy| Vacancy {
        title: text(vacancy.get("title")),
        company: text(vacancy.get("company")),
        text: text(vacancy.get("text")),
        job: if vacancy.get("type").and_then(Value::as_str) == Some("job") { text(vacancy.get("value")) } else { String::new() },
    });
    let id = text(item.get("id"));
    let name = text(item.get("name"));
    Interview {
        id: if id.is_empty() { Uuid::new_v4().simple().to_string()[..8].to_owned() } else { id },
        name: if name.is_empty() { DEFAULT_NAME.to_owned() } else { name },
        documents: documents.unwrap_or_default(),
        notes: text(item.get("notes")),
        vacancy: vacancy.unwrap_or_default(),
        project: text(item.get("project")),
    }
}

/// The text of a CV or any file about you: PDF, Markdown or plain text.
pub fn read_document(path: &Path) -> Result<String, String> {
    let extension = path.extension().map(|ext| ext.to_string_lossy().to_lowercase()).unwrap_or_default();
    let text = match extension.as_str() {
        "pdf" => pdf_extract::extract_text(path).map_err(|error| format!("{} could not be read as a PDF: {error}", path.display()))?,
        "txt" | "md" | "markdown" | "" => std::fs::read(path).map(|bytes| String::from_utf8_lossy(&bytes).into_owned()).map_err(|error| format!("{}: {error}", path.display()))?,
        other => return Err(format!("{}: .{other} files are not read yet (use PDF, Markdown or text)", path.display())),
    };
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ").trim().to_owned();
    if text.is_empty() { Err(format!("{} has no text in it", path.display())) } else { Ok(text) }
}

/// The brief of an interview: ABOUT.md, the documents that are switched on, the vacancy and the notes.
/// A file that cannot be read is left out and named in the second value.
pub fn build_brief(about: &Path, interview: &Interview) -> (InterviewBrief, Vec<String>) {
    let candidate = std::fs::read_to_string(about).unwrap_or_default();
    let mut brief = InterviewBrief::new(&candidate);
    let mut problems = Vec::new();
    for saved in &interview.documents {
        match read_document(Path::new(&saved.path)) {
            Ok(body) => {
                let name = Path::new(&saved.path).file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
                let kind = if saved.kind.is_empty() { guess_kind(&name).to_owned() } else { saved.kind.clone() };
                brief.documents.push(Document { name, text: body, path: saved.path.clone(), kind, enabled: saved.enabled });
            }
            Err(problem) => problems.push(problem),
        }
    }
    brief.job_title = interview.vacancy.title.clone();
    brief.company = interview.vacancy.company.clone();
    brief.job_description = interview.vacancy.text.clone();
    brief.notes = interview.notes.trim().to_owned();
    (brief, problems)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_library_is_written_and_read_back_in_the_python_shape() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("interview").join("context.json");
        let mut library = Library::default();
        {
            let current = library.current_mut();
            current.name = "Vention: Data Engineering".into();
            current.notes = "I study in the evenings".into();
            current.vacancy = Vacancy { title: "Data Engineering Intern".into(), company: "Vention".into(), text: "Python and SQL".into(), job: String::new() };
            current.documents.push(SavedDocument { path: "/x/cv.pdf".into(), kind: "cv".into(), enabled: true });
        }
        library.new_interview("");
        library.save(&path).unwrap();
        let json: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(json["interviews"][0]["vacancy"], json!({"type": "text", "title": "Data Engineering Intern", "company": "Vention", "text": "Python and SQL"}));
        assert_eq!(json["interviews"][1]["name"], "Interview 2");
        assert_eq!(Library::load(&path), library);
    }

    #[test]
    fn the_context_file_of_the_python_overlay_is_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("context.json");
        std::fs::write(&path, r#"{"active": "bbb", "interviews": [
            {"id": "aaa", "name": "Grid", "documents": [], "notes": "", "vacancy": {}, "project": ""},
            {"id": "bbb", "name": "Vention", "documents": [{"path": "/d/cv.pdf", "kind": "cv", "enabled": false}, {"nope": 1}],
             "notes": "n", "vacancy": {"type": "text", "title": "DE", "company": "V", "text": "t"}, "project": "/p"}]}"#).unwrap();
        let library = Library::load(&path);
        assert_eq!((library.current().name.as_str(), library.current().project.as_str()), ("Vention", "/p"));
        assert_eq!(library.current().documents, [SavedDocument { path: "/d/cv.pdf".into(), kind: "cv".into(), enabled: false }]);
        assert_eq!(library.current().vacancy.label(), "DE at V");
        assert_eq!(library.interviews.len(), 2);
    }

    #[test]
    fn an_old_single_interview_file_and_a_broken_one_both_open() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("context.json");
        std::fs::write(&path, r#"{"notes": "old", "documents": []}"#).unwrap();
        assert_eq!((Library::load(&path).current().name.as_str(), Library::load(&path).current().notes.as_str()), ("Interview 1", "old"));
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(Library::load(&path).interviews.len(), 1);
        assert_eq!(Library::load(&dir.path().join("missing.json")).interviews.len(), 1);
    }

    #[test]
    fn deleting_keeps_one_interview_and_returns_the_files_nobody_uses() {
        let mut library = Library::default();
        library.current_mut().documents.push(SavedDocument { path: "/d/shared.pdf".into(), kind: "cv".into(), enabled: true });
        library.current_mut().documents.push(SavedDocument { path: "/d/mine.pdf".into(), kind: "cv".into(), enabled: true });
        let first = library.current().id.clone();
        library.new_interview("Second");
        library.current_mut().documents.push(SavedDocument { path: "/d/shared.pdf".into(), kind: "cv".into(), enabled: true });
        assert_eq!(library.delete(&first), [PathBuf::from("/d/mine.pdf")]);
        assert_eq!((library.interviews.len(), library.current().name.as_str()), (1, "Second"));
        let last = library.current().id.clone();
        library.delete(&last);
        assert_eq!(library.interviews.len(), 1); // the last one is replaced by an empty one
        assert!(library.delete("nope").is_empty());
    }

    #[test]
    fn a_vacancy_is_labelled_by_its_name_or_its_size() {
        assert_eq!(Vacancy { text: "x".repeat(2500), ..Vacancy::default() }.label(), "2.5k characters");
        assert_eq!(Vacancy::default().label(), "");
        assert!(Vacancy::default().is_empty());
    }

    #[test]
    fn the_brief_is_built_from_about_the_documents_the_vacancy_and_the_notes() {
        let dir = tempfile::tempdir().unwrap();
        let about = dir.path().join("ABOUT.md");
        std::fs::write(&about, "## Skills\n| Strong | Python |\n").unwrap();
        std::fs::write(dir.path().join("Alex_CV.md"), "Built pipelines").unwrap();
        std::fs::write(dir.path().join("empty.txt"), "   ").unwrap();
        let mut interview = Interview::new("Vention");
        interview.documents = vec![
            SavedDocument { path: dir.path().join("Alex_CV.md").to_string_lossy().into_owned(), kind: "cv".into(), enabled: true },
            SavedDocument { path: dir.path().join("empty.txt").to_string_lossy().into_owned(), kind: "cv".into(), enabled: true },
            SavedDocument { path: dir.path().join("nope.docx").to_string_lossy().into_owned(), kind: "about".into(), enabled: true },
        ];
        interview.vacancy = Vacancy { title: "DE".into(), company: "V".into(), text: "SQL".into(), job: String::new() };
        interview.notes = "  evenings  ".into();
        let (brief, problems) = build_brief(&about, &interview);
        assert_eq!((brief.documents.len(), brief.documents[0].name.as_str(), brief.job_title.as_str(), brief.notes.as_str()), (1, "Alex_CV.md", "DE", "evenings"));
        assert!(brief.candidate.contains("Python") && problems.len() == 2 && problems[0].contains("no text") && problems[1].contains("docx"));
    }
}
