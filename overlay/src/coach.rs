//! The answer coach: asks the model through the gateway, streams the reply, and does not lose an answer to a
//! limit. A rate limited, rejected or empty-handed key is followed by the next key of the same address on the
//! same model; a model that fails is followed by the next one of the fallback chain.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use regex::Regex;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::providers::{base_url, default_gateway, provider_for};

/// Another key may be accepted where this one is limited, rejected or out of credit.
pub const KEY_PROBLEMS: [u16; 4] = [401, 402, 403, 429];
/// A key the provider rejected is left alone this long.
pub const KEY_REST: Duration = Duration::from_secs(600);
/// A model that said "wait" is left alone this long at most.
pub const MAX_COOLDOWN: Duration = Duration::from_secs(90);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// How long a stream may stay silent before it counts as stuck.
const IDLE_TIMEOUT: Duration = Duration::from_secs(60);
pub const FALLBACK_MODEL: &str = "auto";
pub const MAX_TOKENS: u32 = 1800;
pub const SCREENSHOT_MAX_TOKENS: u32 = 1500;

#[derive(Clone, Debug, thiserror::Error)]
#[error("{message}")]
pub struct CoachError {
    pub message: String,
    /// Seconds until a rate limited model may be asked again.
    pub retry_after: Option<Duration>,
    /// The HTTP status, when the provider answered.
    pub status: Option<u16>,
}

impl CoachError {
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into(), retry_after: None, status: None }
    }
}

pub fn model_name() -> String {
    std::env::var("INTERVIEW_MODEL")
        .or_else(|_| std::env::var("SWITCHYARD_MODEL"))
        .unwrap_or_else(|_| "auto".to_owned())
}

pub fn unreachable_message(base: &str, error: &str) -> String {
    format!("No AI gateway answers at {base} ({error}). Pick a provider in Settings → AI, or start Switchyard.")
}

/// What a provider's error means for the person at the window.
pub fn failed_message(base: &str, model: &str, status: u16, text: &str) -> String {
    let provider = provider_for(base);
    let who = provider.map_or("The AI gateway", |provider| provider.label);
    let mut message = format!("{who} returned {status}: {}", text.chars().take(200).collect::<String>());
    if matches!(status, 400 | 404) && model == "auto" && provider.is_none_or(|provider| provider.id != "switchyard") {
        message.push_str(" (choose a model in Settings → AI: Load models)");
    }
    if matches!(status, 401 | 403) {
        message.push_str(" (check the API key in Settings → AI)");
    }
    message
}

static WAIT_HINT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)next one in (\d+)\s*s").expect("a fixed pattern"));

/// How long a 429 asks to wait: the Retry-After header, or "next one in 32s" in the message.
pub fn retry_after(status: u16, retry_header: Option<&str>, body: &str) -> Option<Duration> {
    if status != 429 {
        return None;
    }
    let seconds = retry_header
        .and_then(|value| value.trim().parse::<f64>().ok())
        .or_else(|| WAIT_HINT.captures(body).and_then(|hint| hint[1].parse::<f64>().ok()))
        .unwrap_or(0.0);
    let seconds = seconds.clamp(0.0, MAX_COOLDOWN.as_secs_f64());
    (seconds > 0.0).then(|| Duration::from_secs_f64(seconds))
}

/// What the model is asked, besides the system text.
#[derive(Clone, Debug)]
pub struct Request {
    pub system: String,
    pub prompt: String,
    /// A screenshot as a data URL: the question is then the picture.
    pub image: Option<String>,
    pub max_tokens: u32,
    pub temperature: f32,
}

impl Request {
    pub fn text(system: &str, prompt: &str) -> Self {
        Self {
            system: system.to_owned(),
            prompt: prompt.to_owned(),
            image: None,
            max_tokens: MAX_TOKENS,
            temperature: 0.3,
        }
    }

    pub fn screenshot(system: &str, prompt: &str, data_url: &str) -> Self {
        Self {
            system: system.to_owned(),
            prompt: prompt.to_owned(),
            image: Some(data_url.to_owned()),
            max_tokens: SCREENSHOT_MAX_TOKENS,
            temperature: 0.2,
        }
    }

    fn body(&self, model: &str, stream: bool) -> Value {
        let content = match &self.image {
            Some(url) => {
                json!([{"type": "text", "text": self.prompt}, {"type": "image_url", "image_url": {"url": url}}])
            }
            None => json!(self.prompt),
        };
        let mut body = json!({
            "model": model,
            "messages": [{"role": "system", "content": self.system}, {"role": "user", "content": content}],
            "temperature": self.temperature,
            "max_tokens": self.max_tokens,
        });
        if stream {
            body["stream"] = json!(true);
        }
        body
    }
}

/// The reply of a finished request.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reply {
    pub text: String,
    /// Why the model stopped: `stop`, or `length` when it was cut off.
    pub finish: Option<String>,
    /// The model that answered, when it was not the chosen one.
    pub fell_back: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct CoachConfig {
    /// The address; empty means where Switchyard listens.
    pub url: String,
    pub key: Option<String>,
    /// Keys of the same address, tried in turn when the one in use is limited or rejected.
    pub spare_keys: Vec<String>,
    pub model: Option<String>,
    pub vision_model: Option<String>,
    pub fallback_models: Vec<String>,
}

#[derive(Clone)]
pub struct Coach {
    config: Arc<Mutex<CoachConfig>>,
    client: reqwest::Client,
    /// (key, model) that was told to wait, and until when; the model `""` is the whole key.
    wait_until: Arc<Mutex<HashMap<(String, String), Instant>>>,
}

impl Coach {
    pub fn new(config: CoachConfig) -> Self {
        let client = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("careertwin-overlay/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("the network client has fixed settings");
        Self { config: Arc::new(Mutex::new(config)), client, wait_until: Arc::default() }
    }

    pub fn configure(&self, config: CoachConfig) {
        *self.config.lock().expect("the config lock is never held across a panic") = config;
    }

    fn config(&self) -> CoachConfig {
        self.config.lock().expect("the config lock is never held across a panic").clone()
    }

    fn base(config: &CoachConfig) -> String {
        if config.url.is_empty() { default_gateway() } else { base_url(&config.url) }
    }

    /// The key in use first, then the spare ones; `[None]` when there is no key at all.
    fn keys(config: &CoachConfig) -> Vec<Option<String>> {
        let own = config
            .key
            .clone()
            .filter(|key| !key.is_empty())
            .or_else(|| std::env::var("SWITCHYARD_API_KEY").ok().filter(|key| !key.is_empty()));
        let mut keys: Vec<String> = Vec::new();
        for key in own.into_iter().chain(config.spare_keys.iter().cloned()) {
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
        if keys.is_empty() { vec![None] } else { keys.into_iter().map(Some).collect() }
    }

    fn fallbacks(config: &CoachConfig, image: bool) -> Vec<String> {
        if !config.fallback_models.is_empty() && !image {
            return config.fallback_models.clone(); // a picture needs a model that can see: leave that to "auto"
        }
        match provider_for(&Self::base(config)) {
            Some(provider) if provider.id != "switchyard" => Vec::new(),
            _ => vec![FALLBACK_MODEL.to_owned()],
        }
    }

    fn waiting(&self, model: &str, key: &Option<String>) -> bool {
        let table = self.wait_until.lock().expect("the table lock is never held across a panic");
        let k = key.clone().unwrap_or_default();
        let now = Instant::now();
        [(k.clone(), model.to_owned()), (k, String::new())]
            .iter()
            .any(|slot| table.get(slot).is_some_and(|until| *until > now))
    }

    /// Leave a limited model alone for as long as it asked; a rejected key (with others to use) for good measure.
    fn rest(&self, model: &str, key: &Option<String>, error: &CoachError, several: bool) {
        let mut table = self.wait_until.lock().expect("the table lock is never held across a panic");
        let k = key.clone().unwrap_or_default();
        if let Some(wait) = error.retry_after {
            table.insert((k, model.to_owned()), Instant::now() + wait);
        } else if several && matches!(error.status, Some(401..=403)) {
            table.insert((k, String::new()), Instant::now() + KEY_REST);
        }
    }

    fn headers(&self, key: &Option<String>) -> Option<String> {
        key.as_ref().map(|key| format!("Bearer {key}"))
    }

    /// One blocking completion through the configured gateway, model and keys.
    pub async fn complete(&self, request: &Request) -> Result<String, CoachError> {
        let config = self.config();
        let base = Self::base(&config);
        let model = config.model.clone().filter(|name| !name.is_empty()).unwrap_or_else(model_name);
        let keys = Self::keys(&config);
        let mut last: Option<(u16, String, Option<String>)> = None;
        for (position, key) in keys.iter().enumerate() {
            let mut builder = self
                .client
                .post(format!("{base}/v1/chat/completions"))
                .timeout(Duration::from_secs(180))
                .json(&request.body(&model, false));
            if let Some(auth) = self.headers(key) {
                builder = builder.header("authorization", auth);
            }
            let response =
                builder.send().await.map_err(|error| CoachError::new(unreachable_message(&base, &describe(&error))))?;
            let status = response.status().as_u16();
            if status < 400 {
                let data: Value = response
                    .json()
                    .await
                    .map_err(|_| CoachError::new("The AI gateway returned an unreadable answer"))?;
                let text = data["choices"][0]["message"]["content"].as_str().unwrap_or("").to_owned();
                if text.trim().is_empty() {
                    return Err(CoachError::new("The model returned no text. Check the model in Settings → AI."));
                }
                return Ok(text);
            }
            let body = response.text().await.unwrap_or_default();
            last = Some((status, body, None));
            if !KEY_PROBLEMS.contains(&status) || position == keys.len() - 1 {
                break; // otherwise the next key of this address may be accepted
            }
        }
        let (status, body, _) = last.unwrap_or((0, String::new(), None));
        Err(CoachError {
            message: failed_message(&base, &model, status, &body),
            retry_after: None,
            status: Some(status),
        })
    }

    /// Ask for an answer and stream it: `on_chunk` gets each piece as it is written.
    ///
    /// If the chosen model fails before saying anything (rate limit, outage), the next key and then the next
    /// model of the fallback chain is asked, so one bad provider does not cost a live answer. A model that said
    /// how long to wait is not asked again until then. By default the chain is one step, "auto", on a Switchyard
    /// gateway; other providers have no "auto", so their error is shown as it is.
    pub async fn ask(
        &self,
        request: &Request,
        cancel: &CancellationToken,
        mut on_chunk: impl FnMut(&str),
    ) -> Result<Reply, CoachError> {
        let config = self.config();
        let image = request.image.is_some();
        let model = if image { config.vision_model.clone() } else { None }
            .filter(|name| !name.is_empty())
            .or_else(|| config.model.clone().filter(|name| !name.is_empty()))
            .unwrap_or_else(model_name);
        let mut chain = vec![model.clone()];
        for name in Self::fallbacks(&config, image) {
            if !chain.contains(&name) {
                chain.push(name);
            }
        }
        let keys = Self::keys(&config);
        let mut first: Option<CoachError> = None;
        for (position, name) in chain.iter().enumerate() {
            'keys: for (turn, key) in keys.iter().enumerate() {
                let last = position == chain.len() - 1 && turn == keys.len() - 1;
                if !last && self.waiting(name, key) {
                    continue;
                }
                let mut started = false;
                let result = self
                    .stream_once(&config, request, name, key, cancel, &mut |piece| {
                        started = true;
                        on_chunk(piece);
                    })
                    .await;
                match result {
                    Ok(mut reply) => {
                        reply.fell_back = (*name != model).then(|| name.clone());
                        return Ok(reply);
                    }
                    Err(error) => {
                        if started || cancel.is_cancelled() {
                            return Err(error);
                        }
                        self.rest(name, key, &error, keys.len() > 1);
                        let key_problem = error.status.is_some_and(|status| KEY_PROBLEMS.contains(&status));
                        first.get_or_insert(error); // the chosen model's error says more than the fallback's
                        if !key_problem {
                            break 'keys; // the same answer would come with another key: on to the next model
                        }
                    }
                }
            }
        }
        Err(first.unwrap_or_else(|| CoachError::new("No model answered")))
    }

    async fn stream_once(
        &self,
        config: &CoachConfig,
        request: &Request,
        model: &str,
        key: &Option<String>,
        cancel: &CancellationToken,
        on_chunk: &mut impl FnMut(&str),
    ) -> Result<Reply, CoachError> {
        let base = Self::base(config);
        let mut builder = self
            .client
            .post(format!("{base}/v1/chat/completions"))
            .header("accept", "text/event-stream")
            .json(&request.body(model, true));
        if let Some(auth) = self.headers(key) {
            builder = builder.header("authorization", auth);
        }
        let sent = tokio::select! {
            () = cancel.cancelled() => return Err(CoachError::new("cancelled")),
            sent = builder.send() => sent,
        };
        let response = sent.map_err(|error| CoachError::new(unreachable_message(&base, &describe(&error))))?;
        let status = response.status().as_u16();
        if status >= 400 {
            let header = response.headers().get("retry-after").and_then(|value| value.to_str().ok()).map(str::to_owned);
            let body = response.text().await.unwrap_or_default();
            return Err(CoachError {
                message: failed_message(&base, model, status, &body),
                retry_after: retry_after(status, header.as_deref(), &body),
                status: Some(status),
            });
        }
        let streaming = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|kind| kind.contains("text/event-stream"));
        let mut reply = Reply::default();
        if !streaming {
            let data: Value =
                response.json().await.map_err(|_| CoachError::new("The AI gateway returned an unreadable answer"))?;
            let choice = &data["choices"][0];
            reply.finish = choice["finish_reason"].as_str().map(str::to_owned);
            if let Some(text) = choice["message"]["content"].as_str().filter(|text| !text.is_empty()) {
                on_chunk(text);
                reply.text.push_str(text);
            }
            return finish(reply);
        }
        let mut decoder = SseDecoder::default();
        let mut bytes = response.bytes_stream();
        loop {
            let next = tokio::select! {
                () = cancel.cancelled() => return Ok(reply),
                next = tokio::time::timeout(IDLE_TIMEOUT, bytes.next()) => next,
            };
            let Ok(next) = next else {
                return Err(CoachError::new("The model stopped answering (no data for 60 seconds)"));
            };
            let Some(piece) = next else { break };
            let piece = piece.map_err(|error| CoachError::new(unreachable_message(&base, &describe(&error))))?;
            for event in decoder.push(&piece) {
                match event {
                    SseEvent::Text(text) => {
                        on_chunk(&text);
                        reply.text.push_str(&text);
                    }
                    SseEvent::Finish(reason) => reply.finish = Some(reason),
                    SseEvent::Done => return finish(reply),
                }
            }
        }
        finish(reply)
    }

    /// Model ids the gateway offers, for the picker in Settings.
    pub async fn models(&self) -> Result<Vec<String>, CoachError> {
        let config = self.config();
        let base = Self::base(&config);
        let key = Self::keys(&config).into_iter().next().flatten();
        let mut builder = self.client.get(format!("{base}/v1/models")).timeout(Duration::from_secs(8));
        if let Some(key) = key {
            builder = builder.bearer_auth(key);
        }
        let response = builder
            .send()
            .await
            .map_err(|error| CoachError::new(format!("Cannot reach {base} ({})", describe(&error))))?;
        if !response.status().is_success() {
            return Err(CoachError::new(failed_message(
                &base,
                "",
                response.status().as_u16(),
                &response.text().await.unwrap_or_default(),
            )));
        }
        let data: Value =
            response.json().await.map_err(|_| CoachError::new(format!("{base} did not return a model list")))?;
        Ok(data["data"]
            .as_array()
            .map(|items| items.iter().filter_map(|item| item["id"].as_str().map(str::to_owned)).collect())
            .unwrap_or_default())
    }

    /// Ask the provider about `key`: `Ok(true)` when it accepts it, `Ok(false)` when it could not say (offline,
    /// no model list), `Err` when it rejects it.
    pub async fn check_key(&self, url: &str, key: &str) -> Result<bool, CoachError> {
        let base = if url.is_empty() { default_gateway() } else { base_url(url) };
        let response = match self
            .client
            .get(format!("{base}/v1/models"))
            .timeout(Duration::from_secs(8))
            .bearer_auth(key)
            .send()
            .await
        {
            Ok(response) => response,
            Err(_) => return Ok(false),
        };
        if matches!(response.status().as_u16(), 401 | 403) {
            let who = provider_for(&base).map_or(base.clone(), |provider| provider.label.to_owned());
            return Err(CoachError {
                message: format!("{who} rejected this key"),
                retry_after: None,
                status: Some(response.status().as_u16()),
            });
        }
        Ok(response.status().is_success())
    }

    /// Open the connection to the gateway now, so the first answer of a call does not pay for it.
    pub async fn warm_up(&self) {
        let config = self.config();
        let base = Self::base(&config);
        let mut builder = self.client.get(format!("{base}/v1/models")).timeout(Duration::from_secs(5));
        if let Some(key) = Self::keys(&config).into_iter().next().flatten() {
            builder = builder.bearer_auth(key);
        }
        let _ = builder.send().await;
    }
}

fn finish(reply: Reply) -> Result<Reply, CoachError> {
    if reply.text.trim().is_empty() {
        return Err(CoachError::new("The model returned no text. Check the model in Settings → AI."));
    }
    Ok(reply)
}

fn describe(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        "timed out".to_owned()
    } else if error.is_connect() {
        "connection refused".to_owned()
    } else {
        "request failed".to_owned()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum SseEvent {
    Text(String),
    Finish(String),
    Done,
}

/// Turns the bytes of a `text/event-stream` into the pieces of a reply.
#[derive(Default)]
pub struct SseDecoder {
    buffer: Vec<u8>,
}

impl SseDecoder {
    pub fn push(&mut self, bytes: &[u8]) -> Vec<SseEvent> {
        self.buffer.extend_from_slice(bytes);
        let mut events = Vec::new();
        while let Some(end) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = self.buffer.drain(..=end).collect();
            let line = String::from_utf8_lossy(&line);
            let line = line.trim_end_matches(['\r', '\n']);
            let Some(payload) = line.strip_prefix("data:") else { continue };
            let payload = payload.trim();
            if payload == "[DONE]" {
                events.push(SseEvent::Done);
                continue;
            }
            let Ok(data) = serde_json::from_str::<Value>(payload) else { continue };
            let choice = &data["choices"][0];
            if let Some(text) = choice["delta"]["content"].as_str().filter(|text| !text.is_empty()) {
                events.push(SseEvent::Text(text.to_owned()));
            }
            if let Some(reason) = choice["finish_reason"].as_str().filter(|reason| !reason.is_empty()) {
                events.push(SseEvent::Finish(reason.to_owned()));
            }
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, Request as MockRequest, Respond, ResponseTemplate};

    const GROQ_LIKE: &str = "https://api.groq.com/openai";

    fn sse(parts: &[&str]) -> ResponseTemplate {
        let body: String = parts
            .iter()
            .map(|part| format!("data: {}\n\n", json!({"choices": [{"delta": {"content": part}}]})))
            .collect::<String>()
            + "data: [DONE]\n\n";
        ResponseTemplate::new(200).set_body_raw(body, "text/event-stream")
    }

    type Decide = Box<dyn Fn(&str, &str) -> ResponseTemplate + Send + Sync>;

    /// Answers by (model, key) and remembers every call.
    struct Script {
        calls: Arc<Mutex<Vec<(String, String)>>>,
        decide: Decide,
    }

    impl Respond for Script {
        fn respond(&self, request: &MockRequest) -> ResponseTemplate {
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            let model = body["model"].as_str().unwrap_or("").to_owned();
            let key = request
                .headers
                .get("authorization")
                .map(|value| value.to_str().unwrap().trim_start_matches("Bearer ").to_owned())
                .unwrap_or_default();
            self.calls.lock().unwrap().push((model.clone(), key.clone()));
            (self.decide)(&model, &key)
        }
    }

    async fn server(
        decide: impl Fn(&str, &str) -> ResponseTemplate + Send + Sync + 'static,
    ) -> (MockServer, Arc<Mutex<Vec<(String, String)>>>) {
        let server = MockServer::start().await;
        let calls = Arc::new(Mutex::new(Vec::new()));
        Mock::given(method("POST"))
            .respond_with(Script { calls: calls.clone(), decide: Box::new(decide) })
            .mount(&server)
            .await;
        (server, calls)
    }

    fn coach(server: &MockServer, edit: impl FnOnce(&mut CoachConfig)) -> Coach {
        // a local test server is no preset, so the chain has no "auto" unless a test asks for it
        let mut config = CoachConfig { url: server.uri(), ..CoachConfig::default() };
        edit(&mut config);
        Coach::new(config)
    }

    async fn ask(coach: &Coach) -> Result<Reply, CoachError> {
        coach.ask(&Request::text("system", "What is a left join?"), &CancellationToken::new(), |_| {}).await
    }

    fn calls(log: &Arc<Mutex<Vec<(String, String)>>>) -> Vec<(String, String)> {
        log.lock().unwrap().clone()
    }

    fn pair(model: &str, key: &str) -> (String, String) {
        (model.to_owned(), key.to_owned())
    }

    #[test]
    fn a_429_says_how_long_to_wait_in_a_header_or_in_the_message() {
        assert_eq!(retry_after(429, Some("7"), "x"), Some(Duration::from_secs(7)));
        assert_eq!(
            retry_after(429, None, "all 4 Groq keys are rate limited or rejected; next one in 32s"),
            Some(Duration::from_secs(32))
        );
        assert_eq!(retry_after(429, Some("9999"), "x"), Some(Duration::from_secs(90))); // never for long
        assert_eq!(retry_after(429, None, "slow down"), None);
        assert_eq!(retry_after(503, Some("7"), "down"), None);
    }

    #[test]
    fn a_provider_error_says_what_to_do() {
        assert!(
            failed_message(GROQ_LIKE, "auto", 404, "no model").contains("Groq returned 404")
                && failed_message(GROQ_LIKE, "auto", 404, "x").contains("Load models")
        );
        assert!(!failed_message("http://127.0.0.1:3260", "auto", 404, "x").contains("Load models"));
        assert!(failed_message(GROQ_LIKE, "m", 401, "x").contains("check the API key"));
        assert_eq!(
            failed_message("http://x", "m", 500, &"y".repeat(500)).len(),
            "The AI gateway returned 500: ".len() + 200
        );
    }

    #[test]
    fn the_sse_decoder_handles_split_lines_and_the_end() {
        let mut decoder = SseDecoder::default();
        let line = format!("data: {}\n", json!({"choices": [{"delta": {"content": "Hel"}}]}));
        let (a, b) = line.as_bytes().split_at(10);
        assert!(decoder.push(a).is_empty());
        assert_eq!(decoder.push(b), [SseEvent::Text("Hel".into())]);
        let finish =
            format!("data: {}\r\n\r\ndata: [DONE]\n", json!({"choices": [{"delta": {}, "finish_reason": "length"}]}));
        assert_eq!(decoder.push(finish.as_bytes()), [SseEvent::Finish("length".into()), SseEvent::Done]);
        assert!(decoder.push(b": comment\ndata: not json\n").is_empty());
    }

    #[tokio::test]
    async fn the_reply_is_streamed_piece_by_piece() {
        let (server, _) = server(|_, _| sse(&["ANSWER:\n", "hello"])).await;
        let coach = coach(&server, |config| config.model = Some("m".into()));
        let mut pieces = Vec::new();
        let reply = coach
            .ask(&Request::text("s", "p"), &CancellationToken::new(), |piece| pieces.push(piece.to_owned()))
            .await
            .unwrap();
        assert_eq!(pieces, ["ANSWER:\n", "hello"]);
        assert_eq!(reply.text, "ANSWER:\nhello");
        assert_eq!(reply.fell_back, None);
    }

    #[tokio::test]
    async fn a_rate_limited_key_is_followed_by_the_next_key_on_the_same_model() {
        let (server, log) = server(|_, key| {
            if key == "k1" { ResponseTemplate::new(429).set_body_string("slow down") } else { sse(&["from ", key]) }
        })
        .await;
        let coach = coach(&server, |c| {
            c.model = Some("groq/big".into());
            c.key = Some("k1".into());
            c.spare_keys = vec!["k2".into()];
            c.fallback_models = vec!["backup".into()];
        });
        let reply = ask(&coach).await.unwrap();
        assert_eq!(reply.text, "from k2");
        assert_eq!(calls(&log), [pair("groq/big", "k1"), pair("groq/big", "k2")]); // the chosen model, not the fallback, answered
        assert_eq!(reply.fell_back, None);
    }

    #[tokio::test]
    async fn a_rejected_key_rests_and_is_not_asked_again_while_another_works() {
        let (server, log) = server(|_, key| {
            if key == "bad" { ResponseTemplate::new(401).set_body_string("invalid api key") } else { sse(&["ok"]) }
        })
        .await;
        let coach = coach(&server, |c| {
            c.model = Some("m".into());
            c.key = Some("bad".into());
            c.spare_keys = vec!["good".into()];
        });
        assert_eq!(ask(&coach).await.unwrap().text, "ok");
        assert_eq!(ask(&coach).await.unwrap().text, "ok");
        let keys: Vec<String> = calls(&log).into_iter().map(|(_, key)| key).collect();
        assert_eq!(keys, ["bad", "good", "good"]); // the second question went straight to the key that works
    }

    #[tokio::test]
    async fn a_model_problem_goes_to_the_next_model_without_trying_the_other_keys() {
        let (server, log) = server(|model, _| {
            if model == "gone" { ResponseTemplate::new(404).set_body_string("no such model") } else { sse(&["ok"]) }
        })
        .await;
        let coach = coach(&server, |c| {
            c.model = Some("gone".into());
            c.key = Some("k1".into());
            c.spare_keys = vec!["k2".into()];
            c.fallback_models = vec!["backup".into()];
        });
        let reply = ask(&coach).await.unwrap();
        assert_eq!(calls(&log), [pair("gone", "k1"), pair("backup", "k1")]);
        assert_eq!(reply.fell_back.as_deref(), Some("backup"));
    }

    #[tokio::test]
    async fn every_key_limited_goes_on_to_the_next_model_and_the_first_error_is_reported_when_all_fail() {
        let (server, log) =
            server(
                |model, _| {
                    if model == "a" { ResponseTemplate::new(429).set_body_string("limited") } else { sse(&["from b"]) }
                },
            )
            .await;
        let coach = coach(&server, |c| {
            c.model = Some("a".into());
            c.key = Some("k1".into());
            c.spare_keys = vec!["k2".into()];
            c.fallback_models = vec!["b".into()];
        });
        assert_eq!(ask(&coach).await.unwrap().text, "from b");
        assert_eq!(calls(&log), [pair("a", "k1"), pair("a", "k2"), pair("b", "k1")]);

        let (server, _) = server_all_429().await;
        let coach = coach_for(&server);
        let error = ask(&coach).await.unwrap_err();
        assert!(error.message.contains("429") && error.status == Some(429));
    }

    async fn server_all_429() -> (MockServer, Arc<Mutex<Vec<(String, String)>>>) {
        server(|_, _| ResponseTemplate::new(429).set_body_string("limited")).await
    }

    fn coach_for(server: &MockServer) -> Coach {
        coach(server, |c| {
            c.model = Some("a".into());
            c.key = Some("k1".into());
            c.spare_keys = vec!["k2".into()];
        })
    }

    #[tokio::test]
    async fn a_cooldown_belongs_to_one_key_so_the_other_key_is_still_asked() {
        let (server, log) = server(|_, key| {
            if key == "k1" { ResponseTemplate::new(429).set_body_string("next one in 30s") } else { sse(&["ok"]) }
        })
        .await;
        let coach = coach(&server, |c| {
            c.model = Some("m".into());
            c.key = Some("k1".into());
            c.spare_keys = vec!["k2".into()];
        });
        ask(&coach).await.unwrap();
        ask(&coach).await.unwrap();
        let keys: Vec<String> = calls(&log).into_iter().map(|(_, key)| key).collect();
        assert_eq!(keys, ["k1", "k2", "k2"]); // k1 was told to wait; k2 was not
    }

    #[tokio::test]
    async fn the_last_choice_is_asked_even_when_it_was_told_to_wait() {
        let (server, log) = server(|_, _| ResponseTemplate::new(429).set_body_string("next one in 30s")).await;
        let coach = coach(&server, |c| {
            c.model = Some("a".into());
            c.fallback_models = vec!["b".into()];
        });
        assert!(ask(&coach).await.is_err());
        assert!(ask(&coach).await.is_err());
        let models: Vec<String> = calls(&log).into_iter().map(|(model, _)| model).collect();
        assert_eq!(models, ["a", "b", "b"]);
    }

    #[tokio::test]
    async fn one_key_behaves_as_before() {
        let (server, log) = server(|_, _| ResponseTemplate::new(401).set_body_string("invalid")).await;
        let coach = coach(&server, |c| {
            c.model = Some("m".into());
            c.key = Some("only".into());
        });
        assert!(ask(&coach).await.unwrap_err().message.contains("401"));
        // an address that is no preset has "auto" after the chosen model; the one key is asked once for each
        assert_eq!(calls(&log), [pair("m", "only"), pair("auto", "only")]);
    }

    #[tokio::test]
    async fn a_blocking_completion_tries_the_next_key_too() {
        let (server, log) = server(|_, key| {
            if key == "k1" {
                ResponseTemplate::new(429).set_body_string("limited")
            } else {
                ResponseTemplate::new(200).set_body_json(json!({"choices": [{"message": {"content": "an answer"}}]}))
            }
        })
        .await;
        let coach = coach(&server, |c| {
            c.model = Some("m".into());
            c.key = Some("k1".into());
            c.spare_keys = vec!["k2".into()];
        });
        assert_eq!(coach.complete(&Request::text("s", "p")).await.unwrap(), "an answer");
        assert_eq!(calls(&log).iter().map(|(_, key)| key.as_str()).collect::<Vec<_>>(), ["k1", "k2"]);
    }

    #[tokio::test]
    async fn a_screenshot_goes_to_the_vision_model_as_an_image_part() {
        let seen = Arc::new(Mutex::new(Value::Null));
        let capture = seen.clone();
        let (server, _) = server(move |model, _| {
            *capture.lock().unwrap() = json!(model);
            sse(&["POINTS:\n- a"])
        })
        .await;
        let coach = coach(&server, |c| {
            c.model = Some("text-model".into());
            c.vision_model = Some("vision-model".into());
        });
        let request = Request::screenshot("s", "solve it", "data:image/jpeg;base64,AAAA");
        coach.ask(&request, &CancellationToken::new(), |_| {}).await.unwrap();
        assert_eq!(*seen.lock().unwrap(), json!("vision-model"));
        let body = request.body("v", true);
        assert_eq!(body["messages"][1]["content"][1]["image_url"]["url"], "data:image/jpeg;base64,AAAA");
        assert_eq!(body["stream"], true);
    }

    #[tokio::test]
    async fn a_cancelled_question_stops_at_once() {
        let (server, _) = server(|_, _| {
            ResponseTemplate::new(200)
                .set_delay(Duration::from_secs(5))
                .set_body_raw("data: [DONE]\n\n", "text/event-stream")
        })
        .await;
        let coach = coach(&server, |c| c.model = Some("m".into()));
        let cancel = CancellationToken::new();
        let trigger = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            trigger.cancel();
        });
        let started = Instant::now();
        assert!(coach.ask(&Request::text("s", "p"), &cancel, |_| {}).await.is_err());
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[tokio::test]
    async fn the_key_check_tells_rejected_from_unknown() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(|request: &MockRequest| {
                let key =
                    request.headers.get("authorization").map(|v| v.to_str().unwrap().to_owned()).unwrap_or_default();
                if key == "Bearer good" {
                    ResponseTemplate::new(200).set_body_json(json!({"data": [{"id": "m1"}]}))
                } else {
                    ResponseTemplate::new(401)
                }
            })
            .mount(&server)
            .await;
        let coach = Coach::new(CoachConfig::default());
        assert!(coach.check_key(&server.uri(), "good").await.unwrap());
        assert!(coach.check_key(&server.uri(), "bad").await.unwrap_err().message.contains("rejected"));
        assert!(!coach.check_key("http://127.0.0.1:1", "x").await.unwrap()); // nothing listens: could not say
        let listed = Coach::new(CoachConfig { url: server.uri(), key: Some("good".into()), ..CoachConfig::default() });
        assert_eq!(listed.models().await.unwrap(), ["m1"]);
    }
}
