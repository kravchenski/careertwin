//! Hearing without the network: Whisper, through whisper.cpp, inside this program.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use futures_util::FutureExt;
use futures_util::future::BoxFuture;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use super::{SpeechError, Stt, SttHint, Transcript, language_code};
use crate::audio::resample::i16_to_f32;

/// Whisper takes at most this much of a vocabulary hint into account.
const PROMPT_CHARS: usize = 400;

pub struct LocalStt {
    model: PathBuf,
    language: Option<String>,
    threads: i32,
    /// Loaded on first use: it takes a second or two, and the window should open without it.
    context: Arc<Mutex<Option<Arc<WhisperContext>>>>,
}

/// The encoder context for a clip of `seconds`: Whisper always reads 30 s of audio unless told
/// otherwise, so a four-second answer costs as much as thirty. A shorter context makes short
/// clips several times faster; long ones keep the full window.
pub fn audio_context(seconds: f32) -> i32 {
    const FRAMES_PER_SECOND: f32 = 50.0;
    const FULL: i32 = 1500;
    if seconds >= 20.0 {
        return 0; // 0 means the model's own full window
    }
    (((seconds * FRAMES_PER_SECOND) as i32) + 150).clamp(500, FULL)
}

impl LocalStt {
    pub fn new(model: PathBuf, language: Option<String>) -> Self {
        // whisper.cpp prints a line per token; nobody is listening, so nothing is printed
        whisper_rs::install_logging_hooks();
        let threads = std::thread::available_parallelism().map_or(4, std::num::NonZero::get).clamp(1, 8) as i32;
        Self { model, language, threads, context: Arc::new(Mutex::new(None)) }
    }

    fn load(slot: &Mutex<Option<Arc<WhisperContext>>>, model: &Path) -> Result<Arc<WhisperContext>, SpeechError> {
        let mut guard = slot.lock().map_err(|_| SpeechError::Status("the recogniser stopped working".into()))?;
        if let Some(context) = guard.as_ref() {
            return Ok(Arc::clone(context));
        }
        if !model.is_file() {
            return Err(SpeechError::Status(format!(
                "The offline recognition model is not downloaded yet ({}).",
                model.display()
            )));
        }
        let context = WhisperContext::new_with_params(model, WhisperContextParameters::default())
            .map_err(|e| SpeechError::Status(format!("The offline recognition model could not be loaded: {e}")))?;
        let context = Arc::new(context);
        *guard = Some(Arc::clone(&context));
        Ok(context)
    }
}

impl Stt for LocalStt {
    fn transcribe(&self, pcm: Vec<i16>, hint: SttHint) -> BoxFuture<'static, Result<Transcript, SpeechError>> {
        let slot = Arc::clone(&self.context);
        let model = self.model.clone();
        let language = hint.language.or_else(|| self.language.clone());
        let threads = self.threads;
        async move {
            tokio::task::spawn_blocking(move || {
                let context = Self::load(&slot, &model)?;
                let audio = i16_to_f32(&pcm);
                let mut state = context
                    .create_state()
                    .map_err(|e| SpeechError::Status(format!("the recogniser could not start: {e}")))?;
                let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
                params.set_n_threads(threads);
                params.set_audio_ctx(audio_context(pcm.len() as f32 / 16_000.0));
                params.set_language(language.as_deref().or(Some("auto")));
                params.set_translate(false);
                params.set_no_context(true);
                params.set_print_progress(false);
                params.set_print_realtime(false);
                params.set_print_special(false);
                params.set_print_timestamps(false);
                params.set_suppress_blank(true);
                let vocabulary: String = hint.vocabulary.chars().take(PROMPT_CHARS).collect();
                if !vocabulary.trim().is_empty() {
                    params.set_initial_prompt(&vocabulary);
                }
                state.full(params, &audio).map_err(|e| SpeechError::Status(format!("the recogniser failed: {e}")))?;
                let text = state.as_iter().map(|segment| segment.to_string()).collect::<Vec<_>>().join(" ");
                let language = whisper_rs::get_lang_str(state.full_lang_id_from_state()).and_then(language_code);
                Ok(Transcript { text: text.trim().to_owned(), language })
            })
            .await
            .map_err(|e| SpeechError::Status(format!("the recogniser stopped: {e}")))?
        }
        .boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_clips_get_a_short_window_and_long_ones_the_full_one() {
        assert_eq!(audio_context(1.0), 500);
        assert_eq!(audio_context(4.0), 500);
        assert_eq!(audio_context(10.0), 650);
        assert_eq!(audio_context(19.9), 1145);
        assert_eq!(audio_context(20.0), 0);
        assert_eq!(audio_context(60.0), 0);
        for seconds in [0.0, 0.5, 3.0, 12.0, 19.0] {
            let context = audio_context(seconds);
            assert!((500..=1500).contains(&context), "{seconds}: {context}");
            assert!(context as f32 >= seconds * 50.0, "the window must cover the whole clip");
        }
    }

    #[tokio::test]
    async fn a_missing_model_says_so_instead_of_crashing() {
        let stt = LocalStt::new(PathBuf::from("/nonexistent/ggml-small-q5_1.bin"), None);
        let error = stt.transcribe(vec![0i16; 16_000], SttHint::default()).await.unwrap_err().to_string();
        assert!(error.contains("not downloaded yet"), "{error}");
    }

    #[tokio::test]
    async fn a_file_that_is_not_a_model_is_refused_cleanly() {
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("ggml-small-q5_1.bin");
        std::fs::write(&fake, b"this is not a whisper model").unwrap();
        let stt = LocalStt::new(fake, Some("en".into()));
        let error = stt.transcribe(vec![0i16; 16_000], SttHint::default()).await.unwrap_err().to_string();
        assert!(error.contains("could not be loaded"), "{error}");
    }
}
