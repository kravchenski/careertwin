//! Remembering what was already said, so a phrase heard twice costs nothing the second time
//! and the short acknowledgements ("Mm-hm.") come out the instant they are needed.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use futures_util::FutureExt;
use futures_util::future::BoxFuture;
use sha2::{Digest, Sha256};

use super::{Speech, SpeechError, Tts, Voice, wav};
use crate::audio::resample::f32_to_i16;
use crate::config::write_private;

const MAX_BYTES: u64 = 200 * 1024 * 1024;
/// The folder is tidied once in this many saves.
const PRUNE_EVERY: usize = 25;

pub struct CachedTts {
    inner: Arc<dyn Tts>,
    dir: PathBuf,
    saves: Arc<AtomicUsize>,
    max_bytes: u64,
}

impl CachedTts {
    pub fn new(inner: Arc<dyn Tts>, dir: PathBuf) -> Self {
        Self { inner, dir, saves: Arc::new(AtomicUsize::new(0)), max_bytes: MAX_BYTES }
    }

    pub fn with_limit(mut self, max_bytes: u64) -> Self {
        self.max_bytes = max_bytes;
        self
    }
}

/// The file name for one phrase in one voice of one engine.
pub fn key(engine: &str, voice: &Voice, text: &str) -> String {
    let mut hasher = Sha256::new();
    for part in [engine, &voice.name, &format!("{:.2}", voice.speed), &voice.instructions, text] {
        hasher.update(part.as_bytes());
        hasher.update([0u8]); // keeps "ab"+"c" apart from "a"+"bc"
    }
    hex::encode(hasher.finalize())
}

impl Tts for CachedTts {
    fn id(&self) -> String {
        self.inner.id()
    }

    fn synthesize(&self, text: String, voice: Voice) -> BoxFuture<'static, Result<Speech, SpeechError>> {
        let inner = Arc::clone(&self.inner);
        let path = self.dir.join(format!("{}.wav", key(&inner.id(), &voice, &text)));
        let dir = self.dir.clone();
        let saves = Arc::clone(&self.saves);
        let max_bytes = self.max_bytes;
        async move {
            if let Some(speech) = read(&path) {
                return Ok(speech);
            }
            let speech = inner.synthesize(text, voice).await?;
            let saved = wav::encode_i16(&f32_to_i16(&speech.pcm), speech.sample_rate)
                .ok()
                .is_some_and(|bytes| write_private(&path, &bytes).is_ok());
            if saved && saves.fetch_add(1, Ordering::Relaxed) % PRUNE_EVERY == PRUNE_EVERY - 1 {
                prune(&dir, max_bytes);
            }
            Ok(speech)
        }
        .boxed()
    }
}

fn read(path: &Path) -> Option<Speech> {
    let bytes = fs::read(path).ok()?;
    match wav::decode(&bytes) {
        Ok((pcm, sample_rate)) if !pcm.is_empty() => Some(Speech { pcm, sample_rate }),
        _ => {
            let _ = fs::remove_file(path); // a damaged entry is dropped and made again
            None
        }
    }
}

/// Deletes the oldest files until the folder is under `max_bytes`.
pub fn prune(dir: &Path, max_bytes: u64) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|e| e == "wav"))
        .filter_map(|entry| {
            let meta = entry.metadata().ok()?;
            Some((meta.modified().ok()?, meta.len(), entry.path()))
        })
        .collect();
    let mut total: u64 = files.iter().map(|(_, size, _)| size).sum();
    files.sort_by_key(|(modified, _, _)| *modified);
    for (_, size, path) in files {
        if total <= max_bytes {
            break;
        }
        if fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(size);
        }
    }
}

/// Removes everything the cache holds.
pub fn clear(dir: &Path) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.filter_map(Result::ok) {
            if entry.path().extension().is_some_and(|e| e == "wav") {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompts::Lang;
    use std::sync::atomic::AtomicUsize;

    struct Counting {
        calls: Arc<AtomicUsize>,
        fail: bool,
    }

    impl Tts for Counting {
        fn id(&self) -> String {
            "counting|v1".into()
        }
        fn synthesize(&self, text: String, _: Voice) -> BoxFuture<'static, Result<Speech, SpeechError>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let fail = self.fail;
            async move {
                if fail {
                    return Err(SpeechError::Status("down".into()));
                }
                Ok(Speech { pcm: vec![0.25; 240 + text.len()], sample_rate: 24_000 })
            }
            .boxed()
        }
    }

    fn voice(name: &str) -> Voice {
        Voice { name: name.into(), speed: 1.0, instructions: String::new(), language: Lang::En }
    }

    fn cached(dir: &Path, fail: bool) -> (CachedTts, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        (CachedTts::new(Arc::new(Counting { calls: Arc::clone(&calls), fail }), dir.to_path_buf()), calls)
    }

    #[tokio::test]
    async fn the_second_time_costs_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let (tts, calls) = cached(dir.path(), false);
        let first = tts.synthesize("Mm-hm.".into(), voice("alloy")).await.unwrap();
        let second = tts.synthesize("Mm-hm.".into(), voice("alloy")).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(first.pcm.len(), second.pcm.len());
        assert_eq!(second.sample_rate, 24_000);
    }

    #[tokio::test]
    async fn another_voice_or_text_is_another_entry() {
        let dir = tempfile::tempdir().unwrap();
        let (tts, calls) = cached(dir.path(), false);
        tts.synthesize("Okay.".into(), voice("alloy")).await.unwrap();
        tts.synthesize("Okay.".into(), voice("nova")).await.unwrap();
        tts.synthesize("Right.".into(), voice("alloy")).await.unwrap();
        let mut slow = voice("alloy");
        slow.speed = 0.8;
        tts.synthesize("Okay.".into(), slow).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 4);
    }

    #[tokio::test]
    async fn a_failure_is_not_cached() {
        let dir = tempfile::tempdir().unwrap();
        let (tts, calls) = cached(dir.path(), true);
        assert!(tts.synthesize("Hi.".into(), voice("alloy")).await.is_err());
        assert!(tts.synthesize("Hi.".into(), voice("alloy")).await.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(fs::read_dir(dir.path()).map(|d| d.count()).unwrap_or(0), 0);
    }

    #[tokio::test]
    async fn a_damaged_entry_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let (tts, calls) = cached(dir.path(), false);
        tts.synthesize("Hi.".into(), voice("alloy")).await.unwrap();
        let file = fs::read_dir(dir.path()).unwrap().next().unwrap().unwrap().path();
        fs::write(&file, b"garbage").unwrap();
        tts.synthesize("Hi.".into(), voice("alloy")).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        tts.synthesize("Hi.".into(), voice("alloy")).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cached_audio_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("speech");
        let (tts, _) = cached(&sub, false);
        tts.synthesize("Hi.".into(), voice("alloy")).await.unwrap();
        let file = fs::read_dir(&sub).unwrap().next().unwrap().unwrap().path();
        assert_eq!(fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(fs::metadata(&sub).unwrap().permissions().mode() & 0o777, 0o700);
    }

    #[test]
    fn the_key_separates_fields() {
        let a = key("e", &voice("ab"), "c");
        let b = key("e", &voice("a"), "bc");
        assert_ne!(a, b);
        assert_eq!(a.len(), 64);
        assert_eq!(a, key("e", &voice("ab"), "c"));
    }

    #[test]
    fn the_oldest_files_go_first_when_the_folder_is_too_big() {
        let dir = tempfile::tempdir().unwrap();
        for (i, name) in ["old", "mid", "new"].iter().enumerate() {
            let path = dir.path().join(format!("{name}.wav"));
            fs::write(&path, vec![0u8; 1000]).unwrap();
            let time = std::time::SystemTime::now() - std::time::Duration::from_secs(1000 - i as u64 * 100);
            fs::File::options().write(true).open(&path).unwrap().set_modified(time).unwrap();
        }
        fs::write(dir.path().join("keep.txt"), "x").unwrap();
        prune(dir.path(), 1500);
        assert!(!dir.path().join("old.wav").exists());
        assert!(!dir.path().join("mid.wav").exists());
        assert!(dir.path().join("new.wav").exists());
        assert!(dir.path().join("keep.txt").exists());
    }

    #[test]
    fn clear_removes_only_audio() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.wav"), "x").unwrap();
        fs::write(dir.path().join("note.txt"), "x").unwrap();
        clear(dir.path());
        assert!(!dir.path().join("a.wav").exists() && dir.path().join("note.txt").exists());
    }
}
