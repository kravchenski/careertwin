//! A voice without the network: Kokoro, in pure Rust on the candle library, inside this program.
//! It speaks English (American and British); Russian needs a voice server or the cloud.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use any_tts::{ModelType, SynthesisRequest, TtsConfig, TtsModel, load_model};
use futures_util::FutureExt;
use futures_util::future::BoxFuture;

use super::{Speech, SpeechError, Tts, Voice};
use crate::models::kokoro_voice_known;
use crate::prompts::Lang;

pub struct LocalTts {
    folder: PathBuf,
    model: Arc<Mutex<Option<Arc<dyn TtsModel>>>>,
    /// One piece is made at a time, in the order asked for. Several at once share the same cores and
    /// all finish late, and the piece that is wanted first is the one the person waits for.
    queue: Arc<tokio::sync::Semaphore>,
}

impl LocalTts {
    /// `folder` holds the Kokoro files (see [`crate::models::kokoro_assets`]).
    pub fn new(folder: PathBuf) -> Self {
        Self { folder, model: Arc::new(Mutex::new(None)), queue: Arc::new(tokio::sync::Semaphore::new(1)) }
    }

    fn model(
        slot: &Mutex<Option<Arc<dyn TtsModel>>>,
        folder: &std::path::Path,
    ) -> Result<Arc<dyn TtsModel>, SpeechError> {
        let mut guard = slot.lock().map_err(|_| SpeechError::Status("the offline voice stopped working".into()))?;
        if let Some(model) = guard.as_ref() {
            return Ok(Arc::clone(model));
        }
        if !folder.join("kokoro-v1_0.pth").is_file() {
            return Err(SpeechError::Status(format!(
                "The offline voice is not downloaded yet ({}).",
                folder.display()
            )));
        }
        let config = TtsConfig::new(ModelType::Kokoro).with_model_path(folder.to_string_lossy().into_owned());
        let model: Arc<dyn TtsModel> = Arc::from(
            load_model(config)
                .map_err(|e| SpeechError::Status(format!("The offline voice could not be loaded: {e}")))?,
        );
        *guard = Some(Arc::clone(&model));
        Ok(model)
    }
}

/// Kokoro tags: `en` for American voices (`a…`), `en-gb` for British ones (`b…`).
pub fn language_tag(voice: &str) -> &'static str {
    if voice.starts_with('b') { "en-gb" } else { "en" }
}

impl Tts for LocalTts {
    fn id(&self) -> String {
        "kokoro-local|v1_0".into()
    }

    fn warm_up(&self) -> BoxFuture<'static, ()> {
        let slot = Arc::clone(&self.model);
        let folder = self.folder.clone();
        async move {
            let _ = tokio::task::spawn_blocking(move || {
                crate::polite::lower_this_thread();
                let _ = LocalTts::model(&slot, &folder);
            })
            .await;
        }
        .boxed()
    }

    fn synthesize(&self, text: String, voice: Voice) -> BoxFuture<'static, Result<Speech, SpeechError>> {
        let slot = Arc::clone(&self.model);
        let folder = self.folder.clone();
        let queue = Arc::clone(&self.queue);
        async move {
            if voice.language != Lang::En {
                return Err(SpeechError::Status(
                    "The offline voice speaks English only. For Russian, use a voice server or the cloud.".into(),
                ));
            }
            if !kokoro_voice_known(&voice.name) {
                return Err(SpeechError::Status(format!(
                    "{:?} is not one of the voices that come with the program.",
                    voice.name
                )));
            }
            // asking for the turn is where the order is kept: the queue lets in whoever asked first
            let _turn =
                queue.acquire_owned().await.map_err(|_| SpeechError::Status("the offline voice stopped".into()))?;
            tokio::task::spawn_blocking(move || {
                crate::polite::lower_this_thread();
                let model = LocalTts::model(&slot, &folder)?;
                let request = SynthesisRequest::new(&text)
                    .with_language(language_tag(&voice.name))
                    .with_voice(&voice.name)
                    .with_speed(f64::from(voice.speed.clamp(0.5, 2.0)));
                let audio = model
                    .synthesize(&request)
                    .map_err(|e| SpeechError::Status(format!("The offline voice failed: {e}")))?;
                if audio.samples.is_empty() {
                    return Err(SpeechError::BadAudio("The offline voice made no sound.".into()));
                }
                Ok(Speech { pcm: audio.samples, sample_rate: audio.sample_rate })
            })
            .await
            .map_err(|e| SpeechError::Status(format!("the offline voice stopped: {e}")))?
        }
        .boxed()
    }
}

/// The offline voices together: Kokoro for English and Piper for Russian.
pub struct LocalVoices {
    english: LocalTts,
    russian: super::tts_piper::PiperTts,
}

impl LocalVoices {
    pub fn new(english_folder: PathBuf, russian_folder: PathBuf) -> Self {
        Self { english: LocalTts::new(english_folder), russian: super::tts_piper::PiperTts::new(russian_folder) }
    }
}

impl Tts for LocalVoices {
    fn id(&self) -> String {
        format!("{}+{}", self.english.id(), self.russian.id())
    }

    fn warm_up(&self) -> BoxFuture<'static, ()> {
        let (english, russian) = (self.english.warm_up(), self.russian.warm_up());
        async move {
            futures_util::future::join(english, russian).await;
        }
        .boxed()
    }

    fn synthesize(&self, text: String, voice: Voice) -> BoxFuture<'static, Result<Speech, SpeechError>> {
        match voice.language {
            Lang::En => self.english.synthesize(text, voice),
            Lang::Ru if crate::models::PIPER_HERE => self.russian.synthesize(text, voice),
            Lang::Ru => async {
                Err(SpeechError::Status(
                    "The offline voice speaks English only here. For Russian, use a voice server or the cloud.".into(),
                ))
            }
            .boxed(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn voice(name: &str, language: Lang) -> Voice {
        Voice { name: name.into(), speed: 1.0, instructions: String::new(), language }
    }

    #[test]
    fn british_voices_get_the_british_tag() {
        assert_eq!(language_tag("bf_emma"), "en-gb");
        assert_eq!(language_tag("af_heart"), "en");
    }

    #[tokio::test]
    async fn russian_is_refused_with_a_way_out() {
        let tts = LocalTts::new(PathBuf::from("/nonexistent"));
        let error = tts.synthesize("Привет".into(), voice("af_heart", Lang::Ru)).await.unwrap_err().to_string();
        assert!(error.contains("English only") && error.contains("voice server"), "{error}");
    }

    #[tokio::test]
    async fn an_unknown_voice_cannot_name_a_path() {
        let tts = LocalTts::new(PathBuf::from("/nonexistent"));
        let error = tts.synthesize("Hi".into(), voice("../../x", Lang::En)).await.unwrap_err().to_string();
        assert!(error.contains("not one of the voices"), "{error}");
    }

    #[tokio::test]
    async fn a_missing_model_says_so_instead_of_crashing() {
        let tts = LocalTts::new(PathBuf::from("/nonexistent"));
        let error = tts.synthesize("Hi".into(), voice("af_heart", Lang::En)).await.unwrap_err().to_string();
        assert!(error.contains("not downloaded yet"), "{error}");
    }
}
