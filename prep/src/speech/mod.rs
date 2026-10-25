//! Speech in and out: recognising what you say and giving the interviewer a voice.

pub mod cache;
pub mod download;
pub mod stt;
#[cfg(feature = "local-stt")]
pub mod stt_local;
pub mod tts;
#[cfg(feature = "local-tts")]
pub mod tts_local;
pub mod wav;

use std::sync::Arc;

use anyhow::Result;
#[cfg(not(all(feature = "local-stt", feature = "local-tts")))]
use anyhow::bail;
use futures_util::future::BoxFuture;
use thiserror::Error;

use crate::config::{Paths, Settings, SttEngine, TtsEngine};
use crate::prompts::Lang;

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum SpeechError {
    #[error("{0}")]
    Unreachable(String),
    #[error("{0}")]
    Status(String),
    #[error("{0}")]
    BadAudio(String),
}

/// What the recogniser needs to know to do better.
#[derive(Debug, Clone, Default)]
pub struct SttHint {
    /// A language code, or `None` to let the recogniser decide.
    pub language: Option<String>,
    /// Names it should expect: the company, the job title, your technologies.
    pub vocabulary: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Transcript {
    pub text: String,
    /// The language it heard, as a code like `en`.
    pub language: Option<String>,
}

pub trait Stt: Send + Sync {
    /// Recognises 16 kHz mono speech.
    fn transcribe(&self, pcm: Vec<i16>, hint: SttHint) -> BoxFuture<'static, Result<Transcript, SpeechError>>;
}

#[derive(Debug, Clone)]
pub struct Voice {
    pub name: String,
    pub speed: f32,
    pub instructions: String,
    pub language: Lang,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Speech {
    pub pcm: Vec<f32>,
    pub sample_rate: u32,
}

impl Speech {
    pub fn seconds(&self) -> f32 {
        if self.sample_rate == 0 { 0.0 } else { self.pcm.len() as f32 / self.sample_rate as f32 }
    }
}

pub trait Tts: Send + Sync {
    fn synthesize(&self, text: String, voice: Voice) -> BoxFuture<'static, Result<Speech, SpeechError>>;
    /// Names the engine and model, so a cache never mixes two voices up.
    fn id(&self) -> String;
    /// Loads what the first sentence would otherwise wait for, while the model is still thinking.
    fn warm_up(&self) -> BoxFuture<'static, ()> {
        futures_util::FutureExt::boxed(async {})
    }
}

/// The Whisper model the offline engine uses: the one chosen, or the one that suits this computer.
pub fn local_stt_model(settings: &Settings) -> &'static crate::models::SttModel {
    crate::models::stt_model(&settings.stt.local_model)
        .unwrap_or_else(|| crate::models::recommend(&crate::models::Hardware::detect()).stt)
}

/// Where the offline Whisper model lives.
pub fn local_stt_path(settings: &Settings, paths: &Paths) -> std::path::PathBuf {
    local_stt_model(settings).path_in(&paths.models_dir())
}

/// Where the offline voice lives.
pub fn local_tts_folder(paths: &Paths) -> std::path::PathBuf {
    paths.models_dir().join("kokoro")
}

/// The recogniser the settings ask for.
pub fn build_stt(settings: &Settings, paths: &Paths) -> Result<Arc<dyn Stt>> {
    match settings.stt.engine {
        SttEngine::Http => Ok(Arc::new(stt::HttpStt::new(&settings.stt, settings.secrets())?)),
        #[cfg(feature = "local-stt")]
        SttEngine::Local => {
            let language = match settings.stt.language.as_str() {
                "" | "auto" => None,
                other => Some(other.to_owned()),
            };
            Ok(Arc::new(stt_local::LocalStt::new(local_stt_path(settings, paths), language)))
        }
        #[cfg(not(feature = "local-stt"))]
        SttEngine::Local => {
            let _ = paths;
            bail!("offline recognition needs a build with the local-stt feature")
        }
    }
}

/// The voice the settings ask for, with a disk cache in front. `None` when the interviewer is
/// set to write only.
pub fn build_tts(settings: &Settings, paths: &Paths) -> Result<Option<Arc<dyn Tts>>> {
    let engine: Arc<dyn Tts> = match settings.tts.engine {
        TtsEngine::Off => return Ok(None),
        TtsEngine::Openai => Arc::new(tts::OpenAiTts::new(&settings.tts, settings.secrets())?),
        TtsEngine::Elevenlabs => Arc::new(tts::ElevenTts::new(&settings.tts, settings.secrets())?),
        #[cfg(feature = "local-tts")]
        TtsEngine::Local => Arc::new(tts_local::LocalTts::new(local_tts_folder(paths))),
        #[cfg(not(feature = "local-tts"))]
        TtsEngine::Local => bail!("the offline voice needs a build with the local-tts feature"),
    };
    Ok(Some(Arc::new(cache::CachedTts::new(engine, paths.speech_cache()))))
}

/// The voice for a language, from the settings.
pub fn voice_for(settings: &Settings, language: Lang) -> Voice {
    let tts = &settings.tts;
    let name = match language {
        Lang::En => &tts.voice_en,
        Lang::Ru => &tts.voice_ru,
    };
    Voice { name: name.clone(), speed: tts.speed, instructions: tts.instructions.clone(), language }
}

/// `english`, `English` and `en` all mean `en`.
pub fn language_code(name: &str) -> Option<String> {
    let lower = name.trim().to_lowercase();
    if lower.is_empty() {
        return None;
    }
    let by_name = [
        ("english", "en"),
        ("russian", "ru"),
        ("ukrainian", "uk"),
        ("polish", "pl"),
        ("german", "de"),
        ("french", "fr"),
        ("spanish", "es"),
        ("italian", "it"),
        ("portuguese", "pt"),
        ("belarusian", "be"),
        ("czech", "cs"),
    ];
    if let Some((_, code)) = by_name.iter().find(|(full, _)| *full == lower) {
        return Some((*code).to_owned());
    }
    (lower.len() == 2 && lower.chars().all(|c| c.is_ascii_alphabetic())).then_some(lower)
}

const NOISE_PHRASES: [&str; 14] = [
    "you",
    "thank you",
    "thanks",
    "thank you for watching",
    "thanks for watching",
    "bye",
    "bye bye",
    "subtitles by the amaraorg community",
    "продолжение следует",
    "спасибо за просмотр",
    "субтитры сделал dimatorzok",
    "uh",
    "um",
    "hmm",
];

/// Recognisers invent words from silence and breath ("Thank you.", "you"). A short sound that
/// comes back as one of those is not a turn.
pub fn is_noise(text: &str, speech_ms: u32) -> bool {
    let normalized: String = text
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if normalized.is_empty() {
        return true;
    }
    speech_ms < 2500 && NOISE_PHRASES.contains(&normalized.as_str())
}

const CONTINUERS: &[&str] = &[
    "and", "but", "so", "because", "or", "then", "which", "that", "with", "to", "of", "the", "a", "an", "uh", "um",
    "like", "well", "also", "if", "when", "while", "as", "actually", "i", "we", "they", "he", "she", "is", "are",
    "was", "were", "have", "has", "had", "for", "in", "on", "at", "by", "from", "about", "into", "would", "will",
    "can", "could", "should", "my", "our", "their",
];

/// Whether what was said sounds like the middle of a thought, so it is worth waiting a moment
/// before the interviewer answers.
pub fn looks_unfinished(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.ends_with([',', '-', '–', '—', ':', ';']) || trimmed.ends_with("...") || trimmed.ends_with('…') {
        return true;
    }
    if trimmed.ends_with(['.', '?', '!']) {
        return false;
    }
    let last = trimmed
        .rsplit(|c: char| c.is_whitespace())
        .next()
        .unwrap_or_default()
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase();
    CONTINUERS.contains(&last.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_names_and_codes() {
        assert_eq!(language_code("english").as_deref(), Some("en"));
        assert_eq!(language_code("Russian").as_deref(), Some("ru"));
        assert_eq!(language_code("EN").as_deref(), Some("en"));
        assert_eq!(language_code(""), None);
        assert_eq!(language_code("klingon"), None);
    }

    #[test]
    fn silence_hallucinations_are_noise_only_when_short() {
        assert!(is_noise("Thank you.", 800));
        assert!(is_noise("you", 1000));
        assert!(is_noise("  ... ", 5000));
        assert!(is_noise("Hmm.", 1200));
        assert!(!is_noise("Thank you.", 6000), "a real thank-you after a long answer");
        assert!(!is_noise("I added an index", 900));
        assert!(!is_noise("Thank you for the question, I would start with the query plan.", 1500));
    }

    #[test]
    fn unfinished_thoughts_are_recognised() {
        for text in [
            "I added an index and,",
            "so we decided to",
            "it was about the",
            "well...",
            "I think —",
            "um",
            "and then we",
        ] {
            assert!(looks_unfinished(text), "{text}");
        }
        for text in ["I added an index.", "Why?", "That is right!", "Yes", "I worked at Northwind for two years", ""] {
            assert!(!looks_unfinished(text), "{text}");
        }
    }

    #[test]
    fn a_voice_is_chosen_by_language() {
        let mut settings = Settings::default();
        settings.tts.voice_en = "nova".into();
        settings.tts.voice_ru = "onyx".into();
        assert_eq!(voice_for(&settings, Lang::En).name, "nova");
        assert_eq!(voice_for(&settings, Lang::Ru).name, "onyx");
    }

    #[test]
    fn the_engines_follow_the_settings() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::under(dir.path().to_path_buf());
        let mut settings = Settings::default();
        settings.tts.engine = TtsEngine::Off;
        assert!(build_tts(&settings, &paths).unwrap().is_none());
        settings.tts.engine = TtsEngine::Openai;
        assert!(build_tts(&settings, &paths).unwrap().is_some());
        settings.stt.engine = SttEngine::Local;
        // without the local-stt feature the offline engine is refused; with it, it is built lazily
        assert_eq!(build_stt(&settings, &paths).is_err(), !cfg!(feature = "local-stt"));
    }

    #[test]
    fn speech_knows_its_length() {
        assert_eq!(Speech { pcm: vec![0.0; 12_000], sample_rate: 24_000 }.seconds(), 0.5);
        assert_eq!(Speech { pcm: vec![], sample_rate: 0 }.seconds(), 0.0);
    }
}
