//! Talking to a language model through any OpenAI-style `/v1/chat/completions` address:
//! Switchyard, Groq, OpenAI, OpenRouter, Ollama, LM Studio or your own server.

use std::time::Duration;

use anyhow::{Context, Result};
use futures_util::future::BoxFuture;
use futures_util::stream::BoxStream;
use futures_util::{FutureExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;

use crate::config::{LlmSettings, validate_service_url};
use crate::secrets::{Secret, redact};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: Role::System, content: content.into() }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: Role::User, content: content.into() }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: Role::Assistant, content: content.into() }
    }
}

#[derive(Debug, Clone)]
pub struct ChatOptions {
    pub model: String,
    pub temperature: f32,
    pub max_tokens: u32,
}

impl ChatOptions {
    pub fn new(model: impl Into<String>) -> Self {
        Self { model: model.into(), temperature: 0.4, max_tokens: 1200 }
    }
    pub fn temperature(mut self, value: f32) -> Self {
        self.temperature = value;
        self
    }
    pub fn max_tokens(mut self, value: u32) -> Self {
        self.max_tokens = value;
        self
    }
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum LlmError {
    #[error("{0}")]
    Unreachable(String),
    #[error("{0}")]
    Status(String),
    #[error("The model returned no text. Pick another model in Settings, or check the gateway has a provider.")]
    Empty,
    #[error("{0}")]
    Other(String),
}

/// What the rest of the program needs from a model; tests stand in a scripted one.
pub trait Llm: Send + Sync {
    fn complete(&self, messages: Vec<Message>, options: ChatOptions) -> BoxFuture<'static, Result<String, LlmError>>;
    /// The reply as it is written, in pieces of any size.
    fn stream(&self, messages: Vec<Message>, options: ChatOptions) -> BoxStream<'static, Result<String, LlmError>>;
}

/// The address without a trailing slash, `/v1` or `/chat/completions`: the form people paste.
pub fn base_url(url: &str) -> String {
    let mut url = url.trim().trim_end_matches('/');
    for suffix in ["/v1/chat/completions", "/chat/completions", "/v1"] {
        if let Some(stripped) = url.strip_suffix(suffix) {
            url = stripped.trim_end_matches('/');
            break;
        }
    }
    url.to_owned()
}

#[derive(Clone)]
pub struct HttpLlm {
    client: reqwest::Client,
    base: String,
    key: Secret,
    secrets: Vec<String>,
}

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const COMPLETE_TIMEOUT: Duration = Duration::from_secs(120);
/// How long a stream may stay silent before it counts as stuck.
const IDLE_TIMEOUT: Duration = Duration::from_secs(45);

impl HttpLlm {
    pub fn new(settings: &LlmSettings, secrets: Vec<String>) -> Result<Self> {
        validate_service_url(&settings.url, "the model address")?;
        let client = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("careertwin-prep/", env!("CARGO_PKG_VERSION")))
            .build()
            .context("could not set up the network client")?;
        Ok(Self { client, base: base_url(&settings.url), key: settings.key.clone(), secrets })
    }

    fn scrub(&self, text: &str) -> String {
        let known: Vec<&str> = self.secrets.iter().map(String::as_str).collect();
        redact(text, &known)
    }

    fn request(&self, messages: &[Message], options: &ChatOptions, stream: bool) -> reqwest::RequestBuilder {
        let mut request = self.client.post(format!("{}/v1/chat/completions", self.base)).json(&json!({
            "model": options.model,
            "messages": messages,
            "temperature": options.temperature,
            "max_tokens": options.max_tokens,
            "stream": stream,
        }));
        if !self.key.is_empty() {
            request = request.bearer_auth(self.key.expose());
        }
        request
    }

    fn unreachable(&self, error: &reqwest::Error) -> LlmError {
        let why = if error.is_timeout() {
            "timed out"
        } else if error.is_connect() {
            "connection refused"
        } else {
            "request failed"
        };
        LlmError::Unreachable(format!(
            "No AI gateway answers at {} ({why}). Pick a provider in Settings, or start Switchyard.",
            self.base
        ))
    }

    async fn failed(&self, response: reqwest::Response, model: &str) -> LlmError {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        let body = self.scrub(&body.chars().take(200).collect::<String>());
        let mut message = format!("The AI gateway returned {status}: {body}");
        if matches!(status, 400 | 404) && model == "auto" {
            message.push_str(" (choose a model in Settings)");
        }
        if matches!(status, 401 | 403) {
            message.push_str(" (check the API key in Settings)");
        }
        LlmError::Status(message)
    }

    /// Model ids the gateway offers, for the picker in Settings.
    pub async fn models(&self) -> Result<Vec<String>, LlmError> {
        let mut request = self.client.get(format!("{}/v1/models", self.base)).timeout(Duration::from_secs(15));
        if !self.key.is_empty() {
            request = request.bearer_auth(self.key.expose());
        }
        let response = request.send().await.map_err(|e| self.unreachable(&e))?;
        if !response.status().is_success() {
            return Err(self.failed(response, "").await);
        }
        let value: Value = response.json().await.map_err(|e| LlmError::Other(self.scrub(&e.to_string())))?;
        let mut ids: Vec<String> = value["data"]
            .as_array()
            .map(|items| items.iter().filter_map(|item| item["id"].as_str().map(str::to_owned)).collect())
            .unwrap_or_default();
        ids.sort();
        ids.dedup();
        Ok(ids)
    }
}

impl Llm for HttpLlm {
    fn complete(&self, messages: Vec<Message>, options: ChatOptions) -> BoxFuture<'static, Result<String, LlmError>> {
        let request = self.request(&messages, &options, false).timeout(COMPLETE_TIMEOUT);
        let me = self.clone();
        async move {
            let response = request.send().await.map_err(|e| me.unreachable(&e))?;
            if !response.status().is_success() {
                return Err(me.failed(response, &options.model).await);
            }
            let value: Value = response.json().await.map_err(|e| LlmError::Other(me.scrub(&e.to_string())))?;
            let text = value["choices"][0]["message"]["content"].as_str().unwrap_or_default().trim().to_owned();
            if text.is_empty() { Err(LlmError::Empty) } else { Ok(text) }
        }
        .boxed()
    }

    fn stream(&self, messages: Vec<Message>, options: ChatOptions) -> BoxStream<'static, Result<String, LlmError>> {
        let request = self.request(&messages, &options, true);
        let me = self.clone();
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<String, LlmError>>(64);
        tokio::spawn(async move {
            let response = match request.send().await {
                Ok(response) => response,
                Err(error) => {
                    let _ = tx.send(Err(me.unreachable(&error))).await;
                    return;
                }
            };
            if !response.status().is_success() {
                let _ = tx.send(Err(me.failed(response, &options.model).await)).await;
                return;
            }
            let json_body = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .is_some_and(|v| v.contains("application/json"));
            if json_body {
                // A gateway that ignored `stream: true` and sent the whole reply at once.
                let text = match response.json::<Value>().await {
                    Ok(value) => {
                        value["choices"][0]["message"]["content"].as_str().unwrap_or_default().trim().to_owned()
                    }
                    Err(error) => {
                        let _ = tx.send(Err(LlmError::Other(me.scrub(&error.to_string())))).await;
                        return;
                    }
                };
                let _ = tx.send(if text.is_empty() { Err(LlmError::Empty) } else { Ok(text) }).await;
                return;
            }
            let mut decoder = SseDecoder::default();
            let mut body = response.bytes_stream();
            let mut produced = false;
            loop {
                match tokio::time::timeout(IDLE_TIMEOUT, body.next()).await {
                    Err(_) => {
                        let _ = tx
                            .send(Err(LlmError::Unreachable("The AI gateway stopped answering mid-reply.".into())))
                            .await;
                        return;
                    }
                    Ok(None) => break,
                    Ok(Some(Err(error))) => {
                        let _ = tx.send(Err(LlmError::Unreachable(me.scrub(&error.to_string())))).await;
                        return;
                    }
                    Ok(Some(Ok(bytes))) => {
                        for event in decoder.push(&bytes) {
                            match event {
                                SseEvent::Done => {
                                    if !produced {
                                        let _ = tx.send(Err(LlmError::Empty)).await;
                                    }
                                    return;
                                }
                                SseEvent::Text(piece) => {
                                    produced = true;
                                    if tx.send(Ok(piece)).await.is_err() {
                                        return; // the listener went away: stop reading
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if !produced {
                let _ = tx.send(Err(LlmError::Empty)).await;
            }
        });
        tokio_stream_from(rx).boxed()
    }
}

fn tokio_stream_from<T: Send + 'static>(rx: tokio::sync::mpsc::Receiver<T>) -> impl futures_util::Stream<Item = T> {
    futures_util::stream::unfold(rx, |mut rx| async move { rx.recv().await.map(|item| (item, rx)) })
}

#[derive(Debug, PartialEq, Eq)]
pub enum SseEvent {
    Text(String),
    Done,
}

/// Turns the bytes of a server-sent-events body into pieces of the reply, whatever way the
/// network cut them.
#[derive(Default)]
pub struct SseDecoder {
    pending: Vec<u8>,
}

impl SseDecoder {
    pub fn push(&mut self, bytes: &[u8]) -> Vec<SseEvent> {
        self.pending.extend_from_slice(bytes);
        let mut events = Vec::new();
        while let Some((end, skip)) = find_event_end(&self.pending) {
            let block: Vec<u8> = self.pending.drain(..end + skip).take(end).collect();
            let block = String::from_utf8_lossy(&block);
            let data: Vec<&str> = block
                .lines()
                .filter_map(|line| line.strip_prefix("data:"))
                .map(|rest| rest.strip_prefix(' ').unwrap_or(rest))
                .collect();
            if data.is_empty() {
                continue;
            }
            let data = data.join("\n");
            if data.trim() == "[DONE]" {
                events.push(SseEvent::Done);
                continue;
            }
            if let Ok(value) = serde_json::from_str::<Value>(&data)
                && let Some(piece) = value["choices"][0]["delta"]["content"].as_str()
                && !piece.is_empty()
            {
                events.push(SseEvent::Text(piece.to_owned()));
            }
        }
        events
    }
}

/// The end of the first complete event: a blank line, in `\n\n` or `\r\n\r\n` form.
fn find_event_end(buffer: &[u8]) -> Option<(usize, usize)> {
    let mut index = 0;
    while index < buffer.len() {
        if buffer[index..].starts_with(b"\n\n") {
            return Some((index, 2));
        }
        if buffer[index..].starts_with(b"\r\n\r\n") {
            return Some((index, 4));
        }
        index += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn settings(url: &str, key: &str) -> LlmSettings {
        LlmSettings { url: url.into(), key: Secret::new(key), model: "m".into(), fast_model: String::new() }
    }

    fn sse(pieces: &[&str]) -> String {
        let mut body = String::new();
        for piece in pieces {
            body.push_str(&format!("data: {}\n\n", json!({"choices": [{"delta": {"content": piece}}]})));
        }
        body.push_str("data: [DONE]\n\n");
        body
    }

    async fn collect(llm: &HttpLlm) -> Result<String, LlmError> {
        let mut stream = llm.stream(vec![Message::user("hi")], ChatOptions::new("m"));
        let mut text = String::new();
        while let Some(piece) = stream.next().await {
            text.push_str(&piece?);
        }
        Ok(text)
    }

    #[test]
    fn base_urls_are_normalised() {
        assert_eq!(base_url("http://x:3260/"), "http://x:3260");
        assert_eq!(base_url("http://x:3260/v1"), "http://x:3260");
        assert_eq!(base_url("https://a.b/openai/v1/chat/completions"), "https://a.b/openai");
        assert_eq!(base_url("  https://api.groq.com/openai  "), "https://api.groq.com/openai");
    }

    #[test]
    fn the_decoder_handles_any_cut_of_the_bytes() {
        let body = sse(&["Hel", "lo ", "wor", "ld"]);
        for step in [1, 2, 3, 7, 50, 1000] {
            let mut decoder = SseDecoder::default();
            let mut text = String::new();
            let mut done = false;
            for chunk in body.as_bytes().chunks(step) {
                for event in decoder.push(chunk) {
                    match event {
                        SseEvent::Text(piece) => text.push_str(&piece),
                        SseEvent::Done => done = true,
                    }
                }
            }
            assert_eq!(text, "Hello world", "step {step}");
            assert!(done, "step {step}");
        }
    }

    #[test]
    fn the_decoder_accepts_crlf_comments_and_unspaced_data() {
        let mut decoder = SseDecoder::default();
        let body =
            ": keep-alive\r\n\r\ndata:{\"choices\":[{\"delta\":{\"content\":\"A\"}}]}\r\n\r\ndata: [DONE]\r\n\r\n";
        let events = decoder.push(body.as_bytes());
        assert_eq!(events, vec![SseEvent::Text("A".into()), SseEvent::Done]);
    }

    #[test]
    fn the_decoder_ignores_garbage_and_empty_deltas() {
        let mut decoder = SseDecoder::default();
        let body = "data: not json\n\ndata: {\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"\"}}]}\n\n";
        assert!(decoder.push(body.as_bytes()).is_empty());
    }

    #[test]
    fn multibyte_text_survives_a_cut_inside_a_character() {
        let body = sse(&["привет"]);
        let bytes = body.as_bytes();
        let mut decoder = SseDecoder::default();
        let mut text = String::new();
        for chunk in bytes.chunks(1) {
            for event in decoder.push(chunk) {
                if let SseEvent::Text(piece) = event {
                    text.push_str(&piece);
                }
            }
        }
        assert_eq!(text, "привет");
    }

    #[tokio::test]
    async fn complete_sends_the_key_and_the_model_and_returns_the_text() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("authorization", "Bearer sk-test-1234567890"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"choices": [{"message": {"content": " Hello "}}]})),
            )
            .expect(1)
            .mount(&server)
            .await;
        let llm = HttpLlm::new(&settings(&server.uri(), "sk-test-1234567890"), vec![]).unwrap();
        let text = llm.complete(vec![Message::system("s"), Message::user("u")], ChatOptions::new("m")).await.unwrap();
        assert_eq!(text, "Hello");
        let request = &server.received_requests().await.unwrap()[0];
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(body["model"], "m");
        assert_eq!(body["stream"], false);
        assert_eq!(body["messages"][0]["role"], "system");
    }

    #[tokio::test]
    async fn streaming_yields_the_pieces_in_order() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(sse(&["Nice ", "to meet ", "you."]), "text/event-stream"),
            )
            .mount(&server)
            .await;
        let llm = HttpLlm::new(&settings(&server.uri(), ""), vec![]).unwrap();
        assert_eq!(collect(&llm).await.unwrap(), "Nice to meet you.");
        let request = &server.received_requests().await.unwrap()[0];
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(body["stream"], true);
        assert!(request.headers.get("authorization").is_none(), "no key, no header");
    }

    #[tokio::test]
    async fn a_gateway_that_ignores_stream_still_works() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"choices": [{"message": {"content": "all at once"}}]})),
            )
            .mount(&server)
            .await;
        let llm = HttpLlm::new(&settings(&server.uri(), ""), vec![]).unwrap();
        assert_eq!(collect(&llm).await.unwrap(), "all at once");
    }

    #[tokio::test]
    async fn an_empty_reply_is_an_error_not_silence() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_raw("data: [DONE]\n\n", "text/event-stream"))
            .mount(&server)
            .await;
        let llm = HttpLlm::new(&settings(&server.uri(), ""), vec![]).unwrap();
        assert_eq!(collect(&llm).await.unwrap_err(), LlmError::Empty);
        let empty = llm.complete(vec![Message::user("x")], ChatOptions::new("m")).await;
        assert!(empty.is_err());
    }

    #[tokio::test]
    async fn an_error_status_explains_itself_and_never_echoes_the_key() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(401).set_body_string("bad key sk-test-1234567890 rejected"))
            .mount(&server)
            .await;
        let secret = "sk-test-1234567890";
        let llm = HttpLlm::new(&settings(&server.uri(), secret), vec![secret.to_owned()]).unwrap();
        let error = llm.complete(vec![Message::user("x")], ChatOptions::new("m")).await.unwrap_err().to_string();
        assert!(error.contains("401") && error.contains("check the API key"), "{error}");
        assert!(!error.contains(secret), "{error}");
        let streamed = collect(&llm).await.unwrap_err().to_string();
        assert!(!streamed.contains(secret), "{streamed}");
    }

    #[tokio::test]
    async fn auto_model_errors_suggest_choosing_a_model() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(404).set_body_string("no such model"))
            .mount(&server)
            .await;
        let llm = HttpLlm::new(&settings(&server.uri(), ""), vec![]).unwrap();
        let error = llm.complete(vec![Message::user("x")], ChatOptions::new("auto")).await.unwrap_err().to_string();
        assert!(error.contains("choose a model"), "{error}");
    }

    #[tokio::test]
    async fn a_closed_port_says_who_to_start() {
        let llm = HttpLlm::new(&settings("http://127.0.0.1:9", ""), vec![]).unwrap();
        let error = llm.complete(vec![Message::user("x")], ChatOptions::new("m")).await.unwrap_err().to_string();
        assert!(error.contains("No AI gateway answers at http://127.0.0.1:9"), "{error}");
        let streamed = collect(&llm).await.unwrap_err().to_string();
        assert!(streamed.contains("No AI gateway"), "{streamed}");
    }

    #[tokio::test]
    async fn models_are_listed_sorted_and_unique() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"data": [{"id": "b"}, {"id": "a"}, {"id": "b"}]})),
            )
            .mount(&server)
            .await;
        let llm = HttpLlm::new(&settings(&server.uri(), ""), vec![]).unwrap();
        assert_eq!(llm.models().await.unwrap(), vec!["a", "b"]);
    }

    #[tokio::test]
    async fn a_redirect_is_not_followed_so_a_key_cannot_be_forwarded() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(302).insert_header("location", "http://127.0.0.1:9/steal"))
            .mount(&server)
            .await;
        let llm = HttpLlm::new(&settings(&server.uri(), "sk-test-1234567890"), vec![]).unwrap();
        let error = llm.complete(vec![Message::user("x")], ChatOptions::new("m")).await.unwrap_err().to_string();
        assert!(error.contains("302"), "{error}");
    }

    #[test]
    fn plain_http_to_another_machine_is_refused() {
        assert!(HttpLlm::new(&settings("http://example.com", "sk-abcdefgh"), vec![]).is_err());
    }
}
