//! The things the program talks to, built from the settings: the model, the recogniser, the voice,
//! the speakers and the microphone. Tests build the same struct from fakes.

use std::any::Any;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use tokio::sync::mpsc::Receiver;

use crate::audio::{Mic, Player, Speaker};
use crate::config::{Paths, Settings};
use crate::llm::{HttpLlm, Llm};
use crate::speech::{self, Stt, Tts};

/// Opens a microphone; the second part keeps it open for as long as it is held.
pub type MicFactory = Arc<dyn Fn(&str) -> Result<(Receiver<Vec<i16>>, Box<dyn Any + Send>)> + Send + Sync>;

#[derive(Clone)]
pub struct Services {
    pub llm: Arc<dyn Llm>,
    /// `None` when recognition is not set up.
    pub stt: Option<Arc<dyn Stt>>,
    /// `None` when the interviewer is set to write only.
    pub tts: Option<Arc<dyn Tts>>,
    pub speaker: Arc<dyn Speaker>,
    pub mic: MicFactory,
}

/// Speakers that are not there: everything is accepted and nothing is heard.
pub struct NoSpeaker;

impl Speaker for NoSpeaker {
    fn enqueue(&self, _: Vec<f32>, _: u32) {}
    fn stop(&self) {}
    fn queued(&self) -> Duration {
        Duration::ZERO
    }
    fn level(&self) -> f32 {
        0.0
    }
}

/// What could not be set up, for the screen; the rest still works.
#[derive(Debug, Default, Clone)]
pub struct Problems {
    pub model: Option<String>,
    pub recognition: Option<String>,
    pub voice: Option<String>,
    pub speakers: Option<String>,
}

pub fn real_mic_factory() -> MicFactory {
    Arc::new(|device: &str| Ok(Mic::open(device)?.split()))
}

/// Builds everything the settings describe. Parts that fail are left out and reported.
pub fn build(settings: &Settings, paths: &Paths) -> (Services, Problems) {
    let effective = settings.effective();
    let mut problems = Problems::default();
    let llm: Arc<dyn Llm> = match HttpLlm::new(&effective.llm, effective.secrets()) {
        Ok(llm) => Arc::new(llm),
        Err(error) => {
            problems.model = Some(error.to_string());
            Arc::new(Unavailable(error.to_string()))
        }
    };
    let stt = match speech::build_stt(&effective) {
        Ok(stt) => Some(stt),
        Err(error) => {
            problems.recognition = Some(error.to_string());
            None
        }
    };
    let tts = match speech::build_tts(&effective, paths) {
        Ok(tts) => tts,
        Err(error) => {
            problems.voice = Some(error.to_string());
            None
        }
    };
    let speaker: Arc<dyn Speaker> = match Player::open(&effective.audio.output_device) {
        Ok(player) => Arc::new(player),
        Err(error) => {
            problems.speakers = Some(error.to_string());
            Arc::new(NoSpeaker)
        }
    };
    (Services { llm, stt, tts, speaker, mic: real_mic_factory() }, problems)
}

/// A model that cannot be reached because its settings are wrong; every question fails with the reason.
struct Unavailable(String);

impl Llm for Unavailable {
    fn complete(
        &self,
        _: Vec<crate::llm::Message>,
        _: crate::llm::ChatOptions,
    ) -> futures_util::future::BoxFuture<'static, Result<String, crate::llm::LlmError>> {
        let reason = self.0.clone();
        Box::pin(async move { Err(crate::llm::LlmError::Other(reason)) })
    }

    fn stream(
        &self,
        _: Vec<crate::llm::Message>,
        _: crate::llm::ChatOptions,
    ) -> futures_util::stream::BoxStream<'static, Result<String, crate::llm::LlmError>> {
        let reason = self.0.clone();
        Box::pin(futures_util::stream::once(async move { Err(crate::llm::LlmError::Other(reason)) }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;

    #[tokio::test]
    async fn bad_settings_are_reported_and_the_model_says_why_when_asked() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::under(dir.path().to_path_buf());
        let mut settings = Settings::default();
        settings.llm.url = "http://example.com".into();
        settings.stt.url = "not a url".into();
        let (services, problems) = build(&settings, &paths);
        assert!(problems.model.as_deref().unwrap().contains("plain http"));
        assert!(problems.recognition.is_some());
        let reply = services.llm.complete(vec![], crate::llm::ChatOptions::new("m")).await;
        assert!(reply.unwrap_err().to_string().contains("plain http"));
        let mut stream = services.llm.stream(vec![], crate::llm::ChatOptions::new("m"));
        assert!(stream.next().await.unwrap().is_err());
    }

    #[test]
    fn the_no_speaker_swallows_everything() {
        let speaker = NoSpeaker;
        speaker.enqueue(vec![0.0; 10], 24_000);
        assert!(speaker.queued().is_zero());
        assert_eq!(speaker.level(), 0.0);
        speaker.stop();
    }
}
