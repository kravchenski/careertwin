//! Recognition through any OpenAI-style `/v1/audio/transcriptions` address: OpenAI, Groq (a
//! very fast Whisper), or a Whisper server of your own.

use std::time::Duration;

use anyhow::{Context, Result};
use futures_util::FutureExt;
use futures_util::future::BoxFuture;
use serde_json::Value;

use super::{SpeechError, Stt, SttHint, Transcript, language_code, wav};
use crate::config::{SttSettings, validate_service_url};
use crate::llm::base_url;
use crate::secrets::{Secret, redact};

const TIMEOUT: Duration = Duration::from_secs(60);
/// Whisper takes at most this much of a vocabulary hint into account.
const PROMPT_CHARS: usize = 400;

#[derive(Clone)]
pub struct HttpStt {
    client: reqwest::Client,
    base: String,
    key: Secret,
    model: String,
    language: Option<String>,
    secrets: Vec<String>,
}

impl HttpStt {
    pub fn new(settings: &SttSettings, secrets: Vec<String>) -> Result<Self> {
        validate_service_url(&settings.url, "the speech recognition address")?;
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("careertwin-prep/", env!("CARGO_PKG_VERSION")))
            .build()
            .context("could not set up the network client")?;
        let language = match settings.language.trim().to_lowercase().as_str() {
            "" | "auto" => None,
            other => Some(other.to_owned()),
        };
        Ok(Self {
            client,
            base: base_url(&settings.url),
            key: settings.key.clone(),
            model: settings.model.clone(),
            language,
            secrets,
        })
    }

    fn scrub(&self, text: &str) -> String {
        let known: Vec<&str> = self.secrets.iter().map(String::as_str).collect();
        redact(text, &known)
    }
}

impl Stt for HttpStt {
    fn transcribe(&self, pcm: Vec<i16>, hint: SttHint) -> BoxFuture<'static, Result<Transcript, SpeechError>> {
        let me = self.clone();
        async move {
            let bytes = wav::encode_i16(&pcm, 16_000).map_err(|e| SpeechError::BadAudio(e.to_string()))?;
            let file = reqwest::multipart::Part::bytes(bytes)
                .file_name("speech.wav")
                .mime_str("audio/wav")
                .map_err(|e| SpeechError::BadAudio(e.to_string()))?;
            let mut form = reqwest::multipart::Form::new()
                .part("file", file)
                .text("model", me.model.clone())
                .text("response_format", "verbose_json")
                .text("temperature", "0");
            if let Some(language) = hint.language.as_deref().or(me.language.as_deref()) {
                form = form.text("language", language.to_owned());
            }
            let vocabulary: String = hint.vocabulary.chars().take(PROMPT_CHARS).collect();
            if !vocabulary.trim().is_empty() {
                form = form.text("prompt", vocabulary);
            }
            let mut request = me.client.post(format!("{}/v1/audio/transcriptions", me.base)).multipart(form);
            if !me.key.is_empty() {
                request = request.bearer_auth(me.key.expose());
            }
            let response = request.send().await.map_err(|error| {
                let why = if error.is_timeout() { "timed out" } else { "connection failed" };
                SpeechError::Unreachable(format!("No speech recognition answers at {} ({why}).", me.base))
            })?;
            let status = response.status();
            if !status.is_success() {
                let body = response.text().await.unwrap_or_default();
                let mut message = format!(
                    "Speech recognition returned {}: {}",
                    status.as_u16(),
                    me.scrub(&body.chars().take(200).collect::<String>())
                );
                if matches!(status.as_u16(), 401 | 403) {
                    message.push_str(" (check the recognition key in Settings)");
                }
                return Err(SpeechError::Status(message));
            }
            let value: Value = response.json().await.map_err(|e| SpeechError::BadAudio(me.scrub(&e.to_string())))?;
            Ok(Transcript {
                text: value["text"].as_str().unwrap_or_default().trim().to_owned(),
                language: value["language"].as_str().and_then(language_code),
            })
        }
        .boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn settings(url: &str) -> SttSettings {
        SttSettings { url: url.into(), key: Secret::new("gsk_test_1234567890"), ..SttSettings::default() }
    }

    fn pcm() -> Vec<i16> {
        (0..16_000).map(|i| ((i as f32 / 10.0).sin() * 6000.0) as i16).collect()
    }

    #[tokio::test]
    async fn the_audio_goes_up_as_a_wav_with_the_model_and_the_key() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/audio/transcriptions"))
            .and(header("authorization", "Bearer gsk_test_1234567890"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"text": " I added an index. ", "language": "english"})),
            )
            .expect(1)
            .mount(&server)
            .await;
        let stt = HttpStt::new(&settings(&server.uri()), vec![]).unwrap();
        let transcript =
            stt.transcribe(pcm(), SttHint { language: None, vocabulary: "FastAPI, Northwind".into() }).await.unwrap();
        assert_eq!(transcript.text, "I added an index.");
        assert_eq!(transcript.language.as_deref(), Some("en"));
        let request = &server.received_requests().await.unwrap()[0];
        let body = String::from_utf8_lossy(&request.body).into_owned();
        assert!(request.headers.get("content-type").unwrap().to_str().unwrap().starts_with("multipart/form-data"));
        assert!(body.contains("whisper-large-v3-turbo") && body.contains("verbose_json"));
        assert!(body.contains("FastAPI, Northwind"), "the vocabulary hint is sent as the prompt");
        assert!(body.contains("RIFF"), "the file is a WAV");
        assert!(!body.contains("name=\"language\""), "auto language sends none");
    }

    #[tokio::test]
    async fn a_fixed_language_is_sent_and_a_hint_overrides_it() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"text": "ok"})))
            .mount(&server)
            .await;
        let mut s = settings(&server.uri());
        s.language = "en".into();
        let stt = HttpStt::new(&s, vec![]).unwrap();
        stt.transcribe(pcm(), SttHint::default()).await.unwrap();
        stt.transcribe(pcm(), SttHint { language: Some("ru".into()), vocabulary: String::new() }).await.unwrap();
        let requests = server.received_requests().await.unwrap();
        let first = String::from_utf8_lossy(&requests[0].body).into_owned();
        let second = String::from_utf8_lossy(&requests[1].body).into_owned();
        assert!(first.contains("name=\"language\"") && first.contains("\r\n\r\nen\r\n"));
        assert!(second.contains("\r\n\r\nru\r\n"));
    }

    #[tokio::test]
    async fn an_error_never_echoes_the_key() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(401).set_body_string("invalid key gsk_test_1234567890"))
            .mount(&server)
            .await;
        let stt = HttpStt::new(&settings(&server.uri()), vec!["gsk_test_1234567890".into()]).unwrap();
        let error = stt.transcribe(pcm(), SttHint::default()).await.unwrap_err().to_string();
        assert!(error.contains("401") && error.contains("check the recognition key"), "{error}");
        assert!(!error.contains("gsk_test_1234567890"), "{error}");
    }

    #[tokio::test]
    async fn a_dead_server_says_where_it_looked() {
        let stt = HttpStt::new(&settings("http://127.0.0.1:9"), vec![]).unwrap();
        let error = stt.transcribe(pcm(), SttHint::default()).await.unwrap_err();
        assert!(matches!(error, SpeechError::Unreachable(_)));
        assert!(error.to_string().contains("http://127.0.0.1:9"));
    }

    #[tokio::test]
    async fn an_answer_without_text_is_an_empty_transcript() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .mount(&server)
            .await;
        let stt = HttpStt::new(&settings(&server.uri()), vec![]).unwrap();
        assert_eq!(stt.transcribe(pcm(), SttHint::default()).await.unwrap().text, "");
    }

    #[test]
    fn plain_http_to_another_machine_is_refused() {
        assert!(HttpStt::new(&settings("http://example.com"), vec![]).is_err());
    }
}
