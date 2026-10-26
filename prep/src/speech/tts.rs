//! Giving the interviewer a voice.
//!
//! Two kinds of engine speak through the same interface. An OpenAI-style `/v1/audio/speech`
//! address covers OpenAI and any local server that wraps an open model (Kokoro, Chatterbox,
//! Qwen3-TTS and so on). ElevenLabs has its own.

use std::time::Duration;

use anyhow::{Context, Result};
use futures_util::FutureExt;
use futures_util::future::BoxFuture;
use serde_json::json;

use super::{Speech, SpeechError, Tts, Voice, wav};
use crate::config::{TtsSettings, validate_service_url};
use crate::llm::base_url;
use crate::secrets::{Secret, redact};

const TIMEOUT: Duration = Duration::from_secs(40);
/// OpenAI refuses more than 4096 characters; a single sentence is far below that.
const MAX_TEXT: usize = 4000;
pub const ELEVEN_URL: &str = "https://api.elevenlabs.io";
pub const ELEVEN_PCM_RATE: u32 = 24_000;

fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(concat!("careertwin-prep/", env!("CARGO_PKG_VERSION")))
        .build()
        .context("could not set up the network client")
}

fn cap(text: &str) -> String {
    text.chars().take(MAX_TEXT).collect()
}

fn failure(base: &str, error: &reqwest::Error) -> SpeechError {
    let why = if error.is_timeout() { "timed out" } else { "connection failed" };
    SpeechError::Unreachable(format!("No voice engine answers at {base} ({why})."))
}

#[derive(Clone)]
pub struct OpenAiTts {
    client: reqwest::Client,
    base: String,
    key: Secret,
    model: String,
    secrets: Vec<String>,
}

impl OpenAiTts {
    pub fn new(settings: &TtsSettings, secrets: Vec<String>) -> Result<Self> {
        validate_service_url(&settings.url, "the voice address")?;
        Ok(Self {
            client: client()?,
            base: base_url(&settings.url),
            key: settings.key.clone(),
            model: settings.model.clone(),
            secrets,
        })
    }

    /// Only the gpt-4o voices take instructions; older models reject the field.
    fn takes_instructions(&self) -> bool {
        self.model.starts_with("gpt-4o")
    }
}

impl Tts for OpenAiTts {
    fn id(&self) -> String {
        format!("openai|{}|{}", self.base, self.model)
    }

    fn synthesize(&self, text: String, voice: Voice) -> BoxFuture<'static, Result<Speech, SpeechError>> {
        let me = self.clone();
        async move {
            let mut body = json!({
                "model": me.model,
                "input": cap(&text),
                "voice": voice.name,
                "response_format": "wav",
                "speed": voice.speed.clamp(0.25, 4.0),
            });
            if me.takes_instructions() && !voice.instructions.trim().is_empty() {
                body["instructions"] = json!(voice.instructions.trim());
            }
            let mut request = me.client.post(format!("{}/v1/audio/speech", me.base)).json(&body);
            if !me.key.is_empty() {
                request = request.bearer_auth(me.key.expose());
            }
            let response = request.send().await.map_err(|e| failure(&me.base, &e))?;
            let status = response.status();
            if !status.is_success() {
                let text = response.text().await.unwrap_or_default();
                let known: Vec<&str> = me.secrets.iter().map(String::as_str).collect();
                let mut message = format!(
                    "The voice engine returned {}: {}",
                    status.as_u16(),
                    redact(&text.chars().take(200).collect::<String>(), &known)
                );
                if matches!(status.as_u16(), 401 | 403) {
                    message.push_str(" (check the voice key in Settings)");
                }
                return Err(SpeechError::Status(message));
            }
            let bytes = response.bytes().await.map_err(|e| failure(&me.base, &e))?;
            let (pcm, sample_rate) = wav::decode(&bytes).map_err(|e| {
                SpeechError::BadAudio(format!("The voice engine sent audio that is not a WAV file ({e})."))
            })?;
            if pcm.is_empty() {
                return Err(SpeechError::BadAudio("The voice engine sent no audio.".into()));
            }
            Ok(Speech { pcm, sample_rate })
        }
        .boxed()
    }
}

#[derive(Clone)]
pub struct ElevenTts {
    client: reqwest::Client,
    base: String,
    key: Secret,
    model: String,
    secrets: Vec<String>,
}

impl ElevenTts {
    pub fn new(settings: &TtsSettings, secrets: Vec<String>) -> Result<Self> {
        // the shared address field still holds the OpenAI default until the user changes it
        let url = if settings.url.trim().is_empty() || settings.url == TtsSettings::default().url {
            ELEVEN_URL
        } else {
            &settings.url
        };
        validate_service_url(url, "the voice address")?;
        let model = if settings.model.starts_with("eleven") {
            settings.model.clone()
        } else {
            "eleven_multilingual_v2".to_owned()
        };
        Ok(Self { client: client()?, base: base_url(url), key: settings.key.clone(), model, secrets })
    }
}

/// An ElevenLabs voice id goes into the address, so it must be only what an id can be.
pub fn valid_voice_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

impl Tts for ElevenTts {
    fn id(&self) -> String {
        format!("elevenlabs|{}|{}", self.base, self.model)
    }

    fn synthesize(&self, text: String, voice: Voice) -> BoxFuture<'static, Result<Speech, SpeechError>> {
        let me = self.clone();
        async move {
            if !valid_voice_id(&voice.name) {
                return Err(SpeechError::Status(
                    "The ElevenLabs voice must be a voice id (letters and digits). Set it in Settings.".into(),
                ));
            }
            let body = json!({
                "text": cap(&text),
                "model_id": me.model,
                "voice_settings": {"speed": voice.speed.clamp(0.7, 1.2)},
            });
            let response = me
                .client
                .post(format!("{}/v1/text-to-speech/{}?output_format=pcm_{ELEVEN_PCM_RATE}", me.base, voice.name))
                .header("xi-api-key", me.key.expose())
                .json(&body)
                .send()
                .await
                .map_err(|e| failure(&me.base, &e))?;
            let status = response.status();
            if !status.is_success() {
                let text = response.text().await.unwrap_or_default();
                let known: Vec<&str> = me.secrets.iter().map(String::as_str).collect();
                let mut message = format!(
                    "ElevenLabs returned {}: {}",
                    status.as_u16(),
                    redact(&text.chars().take(200).collect::<String>(), &known)
                );
                if matches!(status.as_u16(), 401 | 403) {
                    message.push_str(" (check the voice key in Settings)");
                }
                return Err(SpeechError::Status(message));
            }
            let bytes = response.bytes().await.map_err(|e| failure(&me.base, &e))?;
            let pcm = wav::from_pcm16le(&bytes);
            if pcm.is_empty() {
                return Err(SpeechError::BadAudio("ElevenLabs sent no audio.".into()));
            }
            Ok(Speech { pcm, sample_rate: ELEVEN_PCM_RATE })
        }
        .boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompts::Lang;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn voice() -> Voice {
        Voice { name: "alloy".into(), speed: 1.0, instructions: "calm and friendly".into(), language: Lang::En }
    }

    fn settings(url: &str, model: &str) -> TtsSettings {
        TtsSettings {
            url: url.into(),
            key: Secret::new("sk-voice-1234567890"),
            model: model.into(),
            ..TtsSettings::default()
        }
    }

    fn wav_bytes() -> Vec<u8> {
        wav::encode_i16(&vec![1000i16; 2400], 24_000).unwrap()
    }

    #[tokio::test]
    async fn openai_style_speech_returns_decoded_audio() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/audio/speech"))
            .and(header("authorization", "Bearer sk-voice-1234567890"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(wav_bytes(), "audio/wav"))
            .expect(1)
            .mount(&server)
            .await;
        let tts = OpenAiTts::new(&settings(&server.uri(), "gpt-4o-mini-tts"), vec![]).unwrap();
        let speech = tts.synthesize("Nice to meet you.".into(), voice()).await.unwrap();
        assert_eq!(speech.sample_rate, 24_000);
        assert_eq!(speech.pcm.len(), 2400);
        let sent: serde_json::Value =
            serde_json::from_slice(&server.received_requests().await.unwrap()[0].body).unwrap();
        assert_eq!(sent["model"], "gpt-4o-mini-tts");
        assert_eq!(sent["voice"], "alloy");
        assert_eq!(sent["response_format"], "wav");
        assert_eq!(sent["input"], "Nice to meet you.");
        assert_eq!(sent["instructions"], "calm and friendly");
    }

    #[tokio::test]
    async fn older_models_do_not_get_the_instructions_field() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(wav_bytes(), "audio/wav"))
            .mount(&server)
            .await;
        let tts = OpenAiTts::new(&settings(&server.uri(), "tts-1"), vec![]).unwrap();
        tts.synthesize("Hi.".into(), voice()).await.unwrap();
        let sent: serde_json::Value =
            serde_json::from_slice(&server.received_requests().await.unwrap()[0].body).unwrap();
        assert!(sent.get("instructions").is_none());
    }

    #[tokio::test]
    async fn long_text_is_capped() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(wav_bytes(), "audio/wav"))
            .mount(&server)
            .await;
        let tts = OpenAiTts::new(&settings(&server.uri(), "tts-1"), vec![]).unwrap();
        tts.synthesize("a".repeat(10_000), voice()).await.unwrap();
        let sent: serde_json::Value =
            serde_json::from_slice(&server.received_requests().await.unwrap()[0].body).unwrap();
        assert_eq!(sent["input"].as_str().unwrap().len(), MAX_TEXT);
    }

    #[tokio::test]
    async fn errors_explain_themselves_without_the_key() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(401).set_body_string("bad key sk-voice-1234567890"))
            .mount(&server)
            .await;
        let tts = OpenAiTts::new(&settings(&server.uri(), "tts-1"), vec!["sk-voice-1234567890".into()]).unwrap();
        let error = tts.synthesize("Hi.".into(), voice()).await.unwrap_err().to_string();
        assert!(error.contains("401") && error.contains("check the voice key"), "{error}");
        assert!(!error.contains("sk-voice-1234567890"), "{error}");
    }

    #[tokio::test]
    async fn audio_that_is_not_a_wav_is_a_clear_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(b"ID3 mp3 data".to_vec(), "audio/mpeg"))
            .mount(&server)
            .await;
        let tts = OpenAiTts::new(&settings(&server.uri(), "tts-1"), vec![]).unwrap();
        let error = tts.synthesize("Hi.".into(), voice()).await.unwrap_err().to_string();
        assert!(error.contains("not a WAV file"), "{error}");
    }

    #[tokio::test]
    async fn a_dead_engine_says_where_it_looked() {
        let tts = OpenAiTts::new(&settings("http://127.0.0.1:9", "tts-1"), vec![]).unwrap();
        let error = tts.synthesize("Hi.".into(), voice()).await.unwrap_err();
        assert!(matches!(error, SpeechError::Unreachable(_)) && error.to_string().contains("127.0.0.1:9"));
    }

    #[tokio::test]
    async fn elevenlabs_sends_the_key_in_its_header_and_reads_raw_pcm() {
        let server = MockServer::start().await;
        let pcm: Vec<u8> = (0..2400i16).flat_map(|i| (i * 10).to_le_bytes()).collect();
        Mock::given(method("POST"))
            .and(path("/v1/text-to-speech/21m00Tcm4TlvDq8ikWAM"))
            .and(query_param("output_format", "pcm_24000"))
            .and(header("xi-api-key", "sk-voice-1234567890"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(pcm, "application/octet-stream"))
            .expect(1)
            .mount(&server)
            .await;
        let mut s = settings(&server.uri(), "eleven_flash_v2_5");
        s.engine = crate::config::TtsEngine::Elevenlabs;
        let tts = ElevenTts::new(&s, vec![]).unwrap();
        let mut v = voice();
        v.name = "21m00Tcm4TlvDq8ikWAM".into();
        v.speed = 5.0;
        let speech = tts.synthesize("Hello.".into(), v).await.unwrap();
        assert_eq!((speech.sample_rate, speech.pcm.len()), (24_000, 2400));
        let sent: serde_json::Value =
            serde_json::from_slice(&server.received_requests().await.unwrap()[0].body).unwrap();
        assert_eq!(sent["model_id"], "eleven_flash_v2_5");
        assert!(
            (sent["voice_settings"]["speed"].as_f64().unwrap() - 1.2).abs() < 0.001,
            "the speed is clamped to what the API allows"
        );
    }

    #[tokio::test]
    async fn an_elevenlabs_voice_id_cannot_smuggle_a_path() {
        let server = MockServer::start().await;
        let tts = ElevenTts::new(&settings(&server.uri(), "eleven_flash_v2_5"), vec![]).unwrap();
        for bad in ["../admin", "a/b", "a?b=c", "", "voice id", &"x".repeat(80)] {
            let mut v = voice();
            v.name = bad.into();
            assert!(tts.synthesize("Hi.".into(), v).await.is_err(), "{bad}");
        }
        assert!(server.received_requests().await.unwrap().is_empty(), "nothing was sent");
    }

    #[test]
    fn elevenlabs_defaults_its_address_and_model() {
        let s = TtsSettings { model: "gpt-4o-mini-tts".into(), key: Secret::new("k"), ..TtsSettings::default() };
        let tts = ElevenTts::new(&s, vec![]).unwrap();
        assert_eq!(tts.base, ELEVEN_URL);
        assert_eq!(tts.model, "eleven_multilingual_v2");
    }

    #[test]
    fn ids_differ_by_engine_and_model_so_a_cache_cannot_mix_them() {
        let a = OpenAiTts::new(&settings("https://api.openai.com", "tts-1"), vec![]).unwrap();
        let b = OpenAiTts::new(&settings("https://api.openai.com", "gpt-4o-mini-tts"), vec![]).unwrap();
        assert_ne!(a.id(), b.id());
    }

    #[test]
    fn plain_http_to_another_machine_is_refused() {
        assert!(OpenAiTts::new(&settings("http://example.com", "tts-1"), vec![]).is_err());
    }
}
