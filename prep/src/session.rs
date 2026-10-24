//! A practice session and where it is kept: one JSON file per session, private to the user.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::config::write_private;
use crate::prompts::feedback::Feedback;

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// The three tabs of the program.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Track {
    English,
    Technical,
    Interview,
}

impl Track {
    pub const ALL: [Track; 3] = [Track::English, Track::Technical, Track::Interview];
}

/// What a session is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A spoken conversation with the interviewer.
    Live,
    /// Concept checks: a question, your answer and how sure you are.
    Cards,
    /// Say it in Russian, get it in English and say it back.
    Translate,
    /// A vacancy, your CV against it and the questions it invites.
    Prepare,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Speaker {
    Interviewer,
    You,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Turn {
    pub who: Speaker,
    pub text: String,
    /// Seconds since the session began.
    pub at: u32,
    /// The language the recogniser heard, for your turns.
    #[serde(default)]
    pub language: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Active,
    Done,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub track: Track,
    pub kind: Kind,
    pub title: String,
    pub created: u64,
    pub updated: u64,
    pub status: Status,
    /// Which interviewer, level and language the session was set up with.
    #[serde(default)]
    pub setup: serde_json::Value,
    #[serde(default)]
    pub turns: Vec<Turn>,
    #[serde(default)]
    pub feedback: Option<Feedback>,
    /// What belongs to one kind only: the question cards, the translations, the vacancy analysis.
    #[serde(default)]
    pub payload: serde_json::Value,
}

impl Session {
    pub fn new(track: Track, kind: Kind, title: &str) -> Self {
        let now = now();
        Self {
            id: uuid::Uuid::new_v4().simple().to_string(),
            track,
            kind,
            title: title.trim().to_owned(),
            created: now,
            updated: now,
            status: Status::Active,
            setup: serde_json::Value::Null,
            turns: Vec::new(),
            feedback: None,
            payload: serde_json::Value::Null,
        }
    }

    pub fn add_turn(&mut self, who: Speaker, text: &str, language: Option<String>) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        let at = now().saturating_sub(self.created).min(u64::from(u32::MAX)) as u32;
        self.turns.push(Turn { who, text: text.to_owned(), at, language });
        self.updated = now();
    }

    pub fn finish(&mut self) {
        self.status = Status::Done;
        self.updated = now();
    }

    /// The conversation as plain text, for the feedback prompt.
    pub fn transcript(&self) -> String {
        self.turns
            .iter()
            .map(|turn| {
                let who = match turn.who {
                    Speaker::Interviewer => "INTERVIEWER",
                    Speaker::You => "CANDIDATE",
                };
                format!("{who}: {}", turn.text)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn candidate_turns(&self) -> usize {
        self.turns.iter().filter(|t| t.who == Speaker::You).count()
    }
}

/// A session id is 32 hex digits; anything else is not a file this program wrote.
pub fn valid_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit())
}

#[derive(Debug, Clone)]
pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn file(&self, id: &str) -> Result<PathBuf> {
        if !valid_id(id) {
            bail!("not a session id");
        }
        Ok(self.dir.join(format!("{id}.json")))
    }

    pub fn save(&self, session: &Session) -> Result<()> {
        let path = self.file(&session.id)?;
        let bytes = serde_json::to_vec_pretty(session).context("could not write the session")?;
        write_private(&path, &bytes)
    }

    pub fn load(&self, id: &str) -> Result<Session> {
        let path = self.file(id)?;
        let text = fs::read_to_string(&path).with_context(|| format!("could not read {}", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("{} is damaged", path.display()))
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let path = self.file(id)?;
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error).with_context(|| format!("could not delete {}", path.display())),
        }
    }

    /// Every readable session, newest first. A damaged file is skipped, not fatal.
    pub fn list(&self) -> Vec<Session> {
        let Ok(entries) = fs::read_dir(&self.dir) else { return Vec::new() };
        let mut sessions: Vec<Session> = entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let path = entry.path();
                let id = path.file_stem()?.to_str()?.to_owned();
                (path.extension()? == "json" && valid_id(&id)).then(|| read_session(&path))?
            })
            .collect();
        sessions.sort_by(|a, b| b.updated.cmp(&a.updated).then_with(|| a.id.cmp(&b.id)));
        sessions
    }

    pub fn list_track(&self, track: Track) -> Vec<Session> {
        self.list().into_iter().filter(|s| s.track == track).collect()
    }

    /// Sessions whose title or any turn contains `query`, newest first.
    pub fn search(&self, query: &str) -> Vec<Session> {
        let needle = query.trim().to_lowercase();
        if needle.is_empty() {
            return self.list();
        }
        self.list()
            .into_iter()
            .filter(|s| {
                s.title.to_lowercase().contains(&needle)
                    || s.turns.iter().any(|t| t.text.to_lowercase().contains(&needle))
            })
            .collect()
    }
}

fn read_session(path: &Path) -> Option<Session> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

/// "just now", "5m ago", "3h ago", "2d ago", then the date.
pub fn ago(then: u64, now: u64) -> String {
    let seconds = now.saturating_sub(then);
    match seconds {
        0..=59 => "just now".to_owned(),
        60..=3599 => format!("{}m ago", seconds / 60),
        3600..=86_399 => format!("{}h ago", seconds / 3600),
        86_400..=604_799 => format!("{}d ago", seconds / 86_400),
        _ => date(then),
    }
}

/// `YYYY-MM-DD` in UTC.
pub fn date(unix: u64) -> String {
    let days = (unix / 86_400) as i64;
    // days since 1970-01-01 -> civil date (Howard Hinnant's algorithm)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("sessions"));
        (dir, store)
    }

    #[test]
    fn a_session_round_trips() {
        let (_dir, store) = store();
        let mut session = Session::new(Track::Technical, Kind::Live, "  Mock technical interview ");
        session.add_turn(Speaker::Interviewer, "Tell me about indexes.", None);
        session.add_turn(Speaker::You, "An index speeds up reads.", Some("en".into()));
        store.save(&session).unwrap();
        let loaded = store.load(&session.id).unwrap();
        assert_eq!(loaded.title, "Mock technical interview");
        assert_eq!(loaded.turns.len(), 2);
        assert_eq!(loaded.turns[1].language.as_deref(), Some("en"));
        assert_eq!(loaded.track, Track::Technical);
    }

    #[test]
    fn empty_turns_are_ignored() {
        let mut session = Session::new(Track::English, Kind::Live, "x");
        session.add_turn(Speaker::You, "   ", None);
        assert!(session.turns.is_empty());
    }

    #[test]
    fn the_transcript_labels_the_speakers() {
        let mut session = Session::new(Track::Interview, Kind::Live, "x");
        session.add_turn(Speaker::Interviewer, "Hello.", None);
        session.add_turn(Speaker::You, "Hi.", None);
        assert_eq!(session.transcript(), "INTERVIEWER: Hello.\nCANDIDATE: Hi.");
        assert_eq!(session.candidate_turns(), 1);
    }

    #[test]
    fn list_is_newest_first_and_filters_by_track() {
        let (_dir, store) = store();
        let mut old = Session::new(Track::English, Kind::Live, "old");
        old.updated = 100;
        let mut new = Session::new(Track::English, Kind::Live, "new");
        new.updated = 200;
        let mut other = Session::new(Track::Technical, Kind::Cards, "tech");
        other.updated = 300;
        for s in [&old, &new, &other] {
            store.save(s).unwrap();
        }
        let titles: Vec<String> = store.list().into_iter().map(|s| s.title).collect();
        assert_eq!(titles, vec!["tech", "new", "old"]);
        let english: Vec<String> = store.list_track(Track::English).into_iter().map(|s| s.title).collect();
        assert_eq!(english, vec!["new", "old"]);
    }

    #[test]
    fn a_damaged_file_is_skipped() {
        let (_dir, store) = store();
        let good = Session::new(Track::English, Kind::Live, "good");
        store.save(&good).unwrap();
        fs::write(store.dir.join(format!("{}.json", "a".repeat(32))), "{ not json").unwrap();
        fs::write(store.dir.join("notes.txt"), "hello").unwrap();
        assert_eq!(store.list().len(), 1);
    }

    #[test]
    fn ids_that_could_escape_the_folder_are_refused() {
        let (_dir, store) = store();
        for bad in ["../etc/passwd", "..", "", "abc", &"g".repeat(32), "a/../../b"] {
            assert!(store.load(bad).is_err(), "{bad}");
            assert!(store.delete(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn delete_removes_and_is_idempotent() {
        let (_dir, store) = store();
        let session = Session::new(Track::English, Kind::Live, "x");
        store.save(&session).unwrap();
        store.delete(&session.id).unwrap();
        store.delete(&session.id).unwrap();
        assert!(store.list().is_empty());
    }

    #[test]
    fn search_looks_in_titles_and_turns() {
        let (_dir, store) = store();
        let mut a = Session::new(Track::Technical, Kind::Live, "Indexes");
        a.add_turn(Speaker::You, "I added a btree.", None);
        let b = Session::new(Track::English, Kind::Live, "Small talk");
        store.save(&a).unwrap();
        store.save(&b).unwrap();
        assert_eq!(store.search("BTREE").len(), 1);
        assert_eq!(store.search("small").len(), 1);
        assert_eq!(store.search("").len(), 2);
        assert!(store.search("zzz").is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn session_files_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let (_dir, store) = store();
        let session = Session::new(Track::English, Kind::Live, "x");
        store.save(&session).unwrap();
        let mode = fs::metadata(store.dir.join(format!("{}.json", session.id))).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn relative_times_read_naturally() {
        assert_eq!(ago(1000, 1010), "just now");
        assert_eq!(ago(1000, 1000 + 5 * 60), "5m ago");
        assert_eq!(ago(1000, 1000 + 3 * 3600), "3h ago");
        assert_eq!(ago(1000, 1000 + 2 * 86_400), "2d ago");
        assert_eq!(ago(0, 100 * 86_400), "1970-01-01");
    }

    #[test]
    fn dates_are_right() {
        assert_eq!(date(0), "1970-01-01");
        assert_eq!(date(951_782_400), "2000-02-29");
        assert_eq!(date(1_792_800_000), "2026-10-24");
    }
}
