//! A few lines about what the window did, for when something does not work: `interview.log` in the data folder.
//!
//! Only events are written (a key registered, a source followed), never the words of an interview or a key.

use std::io::Write;
use std::path::Path;

/// The file is started over when it grows past this.
const LIMIT: u64 = 200_000;

/// Append a timestamped line. A log that cannot be written is not worth an error of its own.
pub fn note(path: &Path, text: &str) {
    if std::fs::metadata(path).is_ok_and(|meta| meta.len() > LIMIT) {
        let _ = std::fs::remove_file(path);
    }
    if let Some(folder) = path.parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{} {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"), text.replace('\n', " "));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_are_appended_with_a_time_and_a_big_log_starts_over() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a").join("interview.log");
        note(&path, "first");
        note(&path, "second\nline");
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().count(), 2);
        assert!(text.lines().next().unwrap().ends_with(" first") && text.contains("second line"));
        std::fs::write(&path, "x".repeat(LIMIT as usize + 1)).unwrap();
        note(&path, "fresh");
        assert_eq!(std::fs::read_to_string(&path).unwrap().lines().count(), 1);
    }
}
