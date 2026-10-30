//! Speech recognition: Whisper on this computer, or Deepgram in the cloud, behind one interface so the session does
//! not know the difference.

use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use flate2::Compression;
use flate2::write::GzEncoder;
use regex::Regex;
use serde_json::Value;
use tokio::runtime::Handle;

use crate::vad::SAMPLE_RATE;

/// What was heard, and in which language when that is known.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Transcript {
    pub text: String,
    pub language: Option<String>,
    pub probability: f32,
}

pub trait Transcriber: Send + Sync {
    /// Get ready: load the model (slow the first time). Errors say what to change.
    fn load(&self) -> Result<(), String>;
    /// `prompt` is a list of names worth recognising (the company, the skills).
    fn transcribe(&self, pcm: &[i16], prompt: &str) -> Result<Transcript, String>;
    /// Fix the language of the speech, or `None` to detect it.
    fn set_language(&self, language: Option<&str>);
    /// Where it runs, for the status line: `cpu`, `cuda`, `deepgram`.
    fn device(&self) -> String;
}

// ---- the figures Whisper gives about its own text ---------------------------------------------------------------

/// Below this Whisper is guessing.
pub const MIN_LOGPROB: f32 = -1.5;
/// It thinks nobody speaks and is not sure of what it wrote.
pub const SILENCE_PROB: f32 = 0.6;
pub const SILENCE_LOGPROB: f32 = -1.0;
/// Text that repeats itself ("REST, REST, REST") is a loop, not speech.
pub const MAX_COMPRESSION: f32 = 2.4;
/// A fixed language that reads this badly is looked at again in the language Whisper hears.
pub const RETRY_LOGPROB: f32 = -0.5;
pub const SWITCH_PROBABILITY: f32 = 0.8;
/// A reply this short made only of the words of the prompt is the prompt repeated.
pub const ECHO_WORDS: usize = 6;

/// Whether Whisper believes its own segment. On music and noise it writes words anyway, and with a prompt of terms
/// it writes the terms; its own confidence figures tell those from speech.
pub fn is_speech(avg_logprob: f32, no_speech_prob: f32, compression_ratio: f32) -> bool {
    if avg_logprob < MIN_LOGPROB {
        return false;
    }
    if no_speech_prob > SILENCE_PROB && avg_logprob < SILENCE_LOGPROB {
        return false;
    }
    compression_ratio <= MAX_COMPRESSION
}

/// How much a text compresses: speech does not repeat itself, a loop does.
pub fn compression_ratio(text: &str) -> f32 {
    let bytes = text.as_bytes();
    if bytes.is_empty() {
        return 1.0;
    }
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    let _ = encoder.write_all(bytes);
    let compressed = encoder.finish().map_or(bytes.len(), |out| out.len());
    bytes.len() as f32 / compressed.max(1) as f32
}

fn words(text: &str) -> Vec<String> {
    static WORD: std::sync::LazyLock<Regex> =
        std::sync::LazyLock::new(|| Regex::new(r"[\w+#]+(?:\.[\w+#]+)*").expect("a fixed pattern"));
    WORD.find_iter(&text.to_lowercase()).map(|found| found.as_str().to_owned()).collect()
}

/// A few words that are all in the prompt and no question: Whisper read its prompt back.
pub fn echoes_prompt(text: &str, prompt: &str) -> bool {
    let heard = words(text);
    let known = words(prompt);
    !heard.is_empty()
        && heard.len() <= ECHO_WORDS
        && !text.contains('?')
        && heard.iter().all(|word| known.contains(word))
}

// ---- Deepgram -----------------------------------------------------------------------------------------------------------

pub const LISTEN_URL: &str = "https://api.deepgram.com/v1/listen";
pub const PROJECTS_URL: &str = "https://api.deepgram.com/v1/projects";
pub const DEEPGRAM_MODELS: [&str; 2] = ["nova-3", "nova-2"];
const MAX_KEY_TERMS: usize = 20;
/// A tenth of a second is not worth a request.
const MIN_SAMPLES: usize = SAMPLE_RATE as usize / 10;

/// 16 kHz mono PCM16 samples in a WAV container, which Deepgram recognises by itself.
pub fn wav_bytes(pcm: &[i16]) -> Vec<u8> {
    let data_len = (pcm.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + pcm.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for sample in pcm {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}

/// The comma separated names worth recognising (company, skills) as a short list.
pub fn key_terms(vocabulary: &str) -> Vec<String> {
    vocabulary
        .split(',')
        .map(str::trim)
        .filter(|term| !term.is_empty() && term.chars().count() <= 40)
        .take(MAX_KEY_TERMS)
        .map(str::to_owned)
        .collect()
}

/// A failed response as one line; Deepgram answers with `err_msg`, `message` or plain text.
pub fn explain(status: u16, body: &str) -> String {
    let detail = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|data| data.get("err_msg").or_else(|| data.get("message")).and_then(Value::as_str).map(str::to_owned))
        .filter(|detail| !detail.is_empty())
        .unwrap_or_else(|| body.trim().chars().take(200).collect());
    match status {
        401 | 403 => "Deepgram rejected the API key: check it in Settings → AI".to_owned(),
        402 => "Deepgram says the account has no credit left".to_owned(),
        429 => "Deepgram says there are too many requests; try again in a moment".to_owned(),
        _ => format!("Deepgram returned {status}: {detail}"),
    }
}

/// Ask Deepgram for the account's projects: succeeds when the key works. `url` is [`PROJECTS_URL`] outside tests.
pub async fn check_deepgram_key(api_key: &str, url: &str) -> Result<(), String> {
    if api_key.trim().is_empty() {
        return Err("Enter the Deepgram API key first".to_owned());
    }
    let client =
        reqwest::Client::builder().timeout(Duration::from_secs(10)).build().map_err(|error| error.to_string())?;
    let response = client
        .get(url)
        .header("authorization", format!("Token {api_key}"))
        .send()
        .await
        .map_err(|_| "Deepgram is not reachable".to_owned())?;
    if response.status().as_u16() >= 400 {
        let status = response.status().as_u16();
        return Err(explain(status, &response.text().await.unwrap_or_default()));
    }
    Ok(())
}

pub struct DeepgramTranscriber {
    key: String,
    model: String,
    url: String,
    language: Mutex<Option<String>>,
    client: reqwest::Client,
    handle: Handle,
}

impl DeepgramTranscriber {
    pub fn new(key: &str, model: &str, language: Option<&str>, handle: Handle) -> Self {
        Self::with_url(key, model, language, handle, LISTEN_URL)
    }

    pub fn with_url(key: &str, model: &str, language: Option<&str>, handle: Handle, url: &str) -> Self {
        let key = if key.is_empty() { std::env::var("DEEPGRAM_API_KEY").unwrap_or_default() } else { key.to_owned() };
        Self {
            key,
            model: if model.is_empty() { DEEPGRAM_MODELS[0].to_owned() } else { model.to_owned() },
            url: url.to_owned(),
            language: Mutex::new(language.filter(|code| !code.is_empty() && *code != "auto").map(str::to_owned)),
            // one connection for the whole call: no new handshake per sentence
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(20))
                .build()
                .expect("the network client has fixed settings"),
            handle,
        }
    }

    fn post(&self, pcm: &[i16], terms: &[String], language: Option<&str>) -> Result<(u16, String), String> {
        let mut query: Vec<(&str, String)> = vec![("model", self.model.clone()), ("smart_format", "true".to_owned())];
        match language {
            Some(code) => query.push(("language", code.to_owned())),
            None => query.push(("detect_language", "true".to_owned())),
        }
        query.extend(terms.iter().map(|term| ("keyterm", term.clone())));
        let url = reqwest::Url::parse_with_params(&self.url, &query).map_err(|error| error.to_string())?;
        let request = self
            .client
            .post(url)
            .header("authorization", format!("Token {}", self.key))
            .header("content-type", "audio/wav")
            .body(wav_bytes(pcm));
        self.handle.block_on(async {
            let response = request.send().await.map_err(|_| "Deepgram is not reachable".to_owned())?;
            let status = response.status().as_u16();
            Ok((status, response.text().await.unwrap_or_default()))
        })
    }
}

impl Transcriber for DeepgramTranscriber {
    fn load(&self) -> Result<(), String> {
        if self.key.is_empty() {
            Err("Deepgram needs an API key: Settings → AI (or DEEPGRAM_API_KEY)".to_owned())
        } else {
            Ok(())
        }
    }

    fn transcribe(&self, pcm: &[i16], prompt: &str) -> Result<Transcript, String> {
        self.load()?;
        if pcm.len() < MIN_SAMPLES {
            return Ok(Transcript::default());
        }
        let language = self.language.lock().map_err(|_| "speech settings are poisoned".to_owned())?.clone();
        let terms = if self.model.starts_with("nova-3") { key_terms(prompt) } else { Vec::new() };
        let mut answer = self.post(pcm, &terms, language.as_deref())?;
        if answer.0 == 400 && !terms.is_empty() {
            answer = self.post(pcm, &[], language.as_deref())?; // a key term the API dislikes must not cost a sentence
        }
        if answer.0 >= 400 {
            return Err(explain(answer.0, &answer.1));
        }
        let data: Value =
            serde_json::from_str(&answer.1).map_err(|_| "Deepgram returned an unreadable answer".to_owned())?;
        let channel = &data["results"]["channels"][0];
        let text = channel["alternatives"][0]["transcript"]
            .as_str()
            .ok_or_else(|| "Deepgram returned an unreadable answer".to_owned())?;
        let detected = channel["detected_language"].as_str().map(str::to_owned);
        let probability =
            if language.is_some() { 1.0 } else { channel["language_confidence"].as_f64().unwrap_or(0.0) as f32 };
        Ok(Transcript { text: text.trim().to_owned(), language: language.or(detected), probability })
    }

    fn set_language(&self, language: Option<&str>) {
        if let Ok(mut slot) = self.language.lock() {
            *slot = language.filter(|code| !code.is_empty() && *code != "auto").map(str::to_owned);
        }
    }

    fn device(&self) -> String {
        "deepgram".to_owned()
    }
}

// ---- Whisper on this computer ------------------------------------------------------------------------------------------

#[cfg(feature = "local-stt")]
pub use whisper::WhisperTranscriber;

#[cfg(feature = "local-stt")]
mod whisper {
    use std::path::PathBuf;

    use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState};

    use super::*;
    use crate::vad::SAMPLE_RATE;

    /// Whisper takes at most this much of a vocabulary hint into account.
    const PROMPT_CHARS: usize = 400;

    pub struct WhisperTranscriber {
        model: PathBuf,
        language: Mutex<Option<String>>,
        threads: i32,
        /// Loaded on first use: it takes a second or two, and the window should open without it.
        context: Mutex<Option<Arc<WhisperContext>>>,
    }

    /// The encoder context for a clip of `seconds`: Whisper always reads 30 s of audio unless told otherwise, so a
    /// four-second answer costs as much as thirty. A shorter context makes short clips several times faster.
    pub fn audio_context(seconds: f32) -> i32 {
        if seconds >= 20.0 {
            return 0; // 0 means the model's own full window
        }
        (((seconds * 50.0) as i32) + 150).clamp(500, 1500)
    }

    /// What one pass of the model gave.
    struct Decoded {
        text: String,
        avg_logprob: f32,
        language: Option<String>,
        probability: f32,
    }

    impl WhisperTranscriber {
        pub fn new(model: PathBuf, language: Option<&str>) -> Self {
            // whisper.cpp prints a line per token; nobody is listening, so nothing is printed
            whisper_rs::install_logging_hooks();
            let threads = std::thread::available_parallelism().map_or(4, std::num::NonZero::get).clamp(1, 8) as i32;
            Self {
                model,
                language: Mutex::new(language.filter(|code| !code.is_empty() && *code != "auto").map(str::to_owned)),
                threads,
                context: Mutex::new(None),
            }
        }

        fn context(&self) -> Result<Arc<WhisperContext>, String> {
            let mut slot = self.context.lock().map_err(|_| "the recogniser stopped working".to_owned())?;
            if let Some(context) = slot.as_ref() {
                return Ok(context.clone());
            }
            if !self.model.is_file() {
                return Err(format!("The speech model is not downloaded yet ({}).", self.model.display()));
            }
            let context = WhisperContext::new_with_params(&self.model, WhisperContextParameters::default())
                .map_err(|error| format!("The speech model could not be loaded: {error}"))?;
            let context = Arc::new(context);
            *slot = Some(context.clone());
            Ok(context)
        }

        fn decode(
            &self,
            context: &WhisperContext,
            audio: &[f32],
            prompt: &str,
            language: Option<&str>,
        ) -> Result<Decoded, String> {
            let mut state: WhisperState =
                context.create_state().map_err(|error| format!("the recogniser could not start: {error}"))?;
            let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
            params.set_n_threads(self.threads);
            params.set_audio_ctx(audio_context(audio.len() as f32 / SAMPLE_RATE as f32));
            params.set_language(language.or(Some("auto")));
            params.set_translate(false);
            params.set_no_context(true);
            params.set_print_progress(false);
            params.set_print_realtime(false);
            params.set_print_special(false);
            params.set_print_timestamps(false);
            params.set_suppress_blank(true);
            let vocabulary: String = prompt.chars().take(PROMPT_CHARS).collect();
            if !vocabulary.trim().is_empty() {
                params.set_initial_prompt(&vocabulary);
            }
            state.full(params, audio).map_err(|error| format!("the recogniser failed: {error}"))?;
            let (mut kept, mut lowest) = (Vec::<String>::new(), 0.0f32);
            for segment in state.as_iter() {
                let text = segment.to_str_lossy().map(|text| text.trim().to_owned()).unwrap_or_default();
                let logs: Vec<f32> = (0..segment.n_tokens())
                    .filter_map(|index| segment.get_token(index))
                    .filter(|token| {
                        token.to_str_lossy().is_ok_and(|text| !text.starts_with("[_") && !text.starts_with("<|"))
                    })
                    .map(|token| token.token_probability().max(1e-6).ln())
                    .collect();
                let avg = if logs.is_empty() { 0.0 } else { logs.iter().sum::<f32>() / logs.len() as f32 };
                lowest = lowest.min(avg);
                if is_speech(avg, segment.no_speech_probability(), compression_ratio(&text)) && !text.is_empty() {
                    kept.push(text);
                }
            }
            let detected = whisper_rs::get_lang_str(state.full_lang_id_from_state()).map(str::to_owned);
            let probability = match language {
                Some(_) => 1.0,
                None => state
                    .lang_detect(0, self.threads as usize)
                    .ok()
                    .and_then(|(id, probs)| probs.get(id as usize).copied())
                    .unwrap_or(0.0),
            };
            Ok(Decoded {
                text: kept.join(" ").trim().to_owned(),
                avg_logprob: lowest,
                language: language.map(str::to_owned).or(detected),
                probability,
            })
        }
    }

    impl Transcriber for WhisperTranscriber {
        fn load(&self) -> Result<(), String> {
            self.context().map(|_| ())
        }

        fn transcribe(&self, pcm: &[i16], prompt: &str) -> Result<Transcript, String> {
            let context = self.context()?;
            let audio: Vec<f32> = pcm.iter().map(|sample| f32::from(*sample) / 32768.0).collect();
            let fixed = self.language.lock().map_err(|_| "speech settings are poisoned".to_owned())?.clone();
            let mut best = self.decode(&context, &audio, prompt, fixed.as_deref())?;
            if fixed.is_some() && (best.text.is_empty() || best.avg_logprob < RETRY_LOGPROB) {
                // a fixed language is wrong for a speaker of another one (a Russian question read as English gives
                // nonsense): look again without it and keep that reading if it is surely another language and better
                let other = self.decode(&context, &audio, prompt, None)?;
                let switches =
                    other.language.is_some() && other.language != fixed && other.probability >= SWITCH_PROBABILITY;
                if !other.text.is_empty() && switches && (best.text.is_empty() || other.avg_logprob > best.avg_logprob)
                {
                    best = other;
                }
            }
            let text = if echoes_prompt(&best.text, prompt) { String::new() } else { best.text };
            Ok(Transcript { text, language: best.language, probability: best.probability })
        }

        fn set_language(&self, language: Option<&str>) {
            if let Ok(mut slot) = self.language.lock() {
                *slot = language.filter(|code| !code.is_empty() && *code != "auto").map(str::to_owned);
            }
        }

        fn device(&self) -> String {
            "cpu".to_owned()
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn short_clips_get_a_short_window_and_long_ones_the_full_one() {
            assert_eq!(audio_context(1.0), 500);
            assert_eq!(audio_context(10.0), 650);
            assert_eq!(audio_context(20.0), 0);
            for seconds in [0.0, 0.5, 3.0, 12.0, 19.0] {
                let context = audio_context(seconds);
                assert!((500..=1500).contains(&context) && context as f32 >= seconds * 50.0, "{seconds}: {context}");
            }
        }

        #[test]
        fn a_missing_model_says_what_to_do() {
            let transcriber = WhisperTranscriber::new(PathBuf::from("/no/such/model.bin"), Some("en"));
            assert!(transcriber.load().unwrap_err().contains("not downloaded yet"));
            assert_eq!(transcriber.device(), "cpu");
        }
    }
}

/// A transcriber that answers from a list: for tests of whatever uses one.
pub struct Scripted {
    replies: Mutex<std::collections::VecDeque<Result<Transcript, String>>>,
    pub language: Mutex<Option<String>>,
    pub seen: Mutex<Vec<(usize, String)>>,
}

impl Scripted {
    pub fn new(replies: Vec<Result<Transcript, String>>) -> Arc<Self> {
        Arc::new(Self { replies: Mutex::new(replies.into()), language: Mutex::new(None), seen: Mutex::new(Vec::new()) })
    }

    pub fn saying(texts: &[&str]) -> Arc<Self> {
        Self::new(
            texts
                .iter()
                .map(|text| {
                    Ok(Transcript { text: (*text).to_owned(), language: Some("en".to_owned()), probability: 1.0 })
                })
                .collect(),
        )
    }
}

impl Transcriber for Scripted {
    fn load(&self) -> Result<(), String> {
        Ok(())
    }

    fn transcribe(&self, pcm: &[i16], prompt: &str) -> Result<Transcript, String> {
        self.seen.lock().unwrap().push((pcm.len(), prompt.to_owned()));
        self.replies.lock().unwrap().pop_front().unwrap_or_else(|| Ok(Transcript::default()))
    }

    fn set_language(&self, language: Option<&str>) {
        *self.language.lock().unwrap() = language.map(str::to_owned);
    }

    fn device(&self) -> String {
        "scripted".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn whisper_is_believed_only_when_it_is_sure_of_speech() {
        assert!(is_speech(-0.3, 0.1, 1.2));
        assert!(!is_speech(-2.0, 0.0, 1.2)); // guessing
        assert!(!is_speech(-1.2, 0.8, 1.2)); // thinks it is silence and is not sure of the text
        assert!(is_speech(-0.4, 0.8, 1.2)); // sure of the text: silence doubt does not count
        assert!(!is_speech(-0.3, 0.1, 3.5)); // a loop
    }

    #[test]
    fn a_loop_compresses_far_better_than_speech() {
        assert!(compression_ratio(&"REST, REST, ".repeat(20)) > MAX_COMPRESSION);
        assert!(compression_ratio("Tell me about the last time a pipeline of yours broke at night") < MAX_COMPRESSION);
        assert_eq!(compression_ratio(""), 1.0);
    }

    #[test]
    fn a_reply_made_of_the_prompt_is_the_prompt_read_back() {
        let prompt = "Vention, Python, SQL, Airflow, PySpark";
        assert!(echoes_prompt("Python, SQL", prompt) && echoes_prompt("Vention Airflow", prompt));
        assert!(!echoes_prompt("What is Python?", prompt)); // a question
        assert!(!echoes_prompt("I use Python and Kafka", prompt) && !echoes_prompt("", prompt));
        assert!(!echoes_prompt("Python SQL Airflow PySpark Vention Python SQL", prompt)); // too long to be an echo
    }

    #[test]
    fn a_wav_has_a_header_the_right_size_and_the_samples() {
        let wav = wav_bytes(&[1, -1, 256]);
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(&wav[8..16], b"WAVEfmt ");
        assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 16_000);
        assert_eq!(wav.len(), 44 + 6);
        assert_eq!(u32::from_le_bytes(wav[40..44].try_into().unwrap()), 6);
        assert_eq!(&wav[44..], [1, 0, 255, 255, 0, 1]);
    }

    #[test]
    fn key_terms_and_errors_are_short_and_say_what_to_change() {
        assert_eq!(key_terms("Vention, , Python,  SQL"), ["Vention", "Python", "SQL"]);
        assert_eq!(key_terms(&"term, ".repeat(50)).len(), MAX_KEY_TERMS);
        assert!(key_terms(&"x".repeat(41)).is_empty());
        assert!(
            explain(401, "").contains("rejected the API key")
                && explain(402, "").contains("no credit")
                && explain(429, "").contains("too many")
        );
        assert_eq!(explain(500, r#"{"err_msg": "boom"}"#), "Deepgram returned 500: boom");
        assert_eq!(explain(500, "plain text"), "Deepgram returned 500: plain text");
    }

    fn deepgram_reply(text: &str) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(serde_json::json!({"results": {"channels": [{"detected_language": "en", "language_confidence": 0.93, "alternatives": [{"transcript": text}]}]}}))
    }

    /// Run `work` on a plain thread, as the session's recogniser thread does: it blocks on the runtime.
    async fn off_runtime<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
        tokio::task::spawn_blocking(work).await.unwrap()
    }

    #[tokio::test]
    async fn deepgram_is_asked_with_the_model_the_terms_and_the_key() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(header("authorization", "Token secret"))
            .and(header("content-type", "audio/wav"))
            .and(query_param("model", "nova-3"))
            .and(query_param("detect_language", "true"))
            .and(query_param("keyterm", "Airflow"))
            .respond_with(deepgram_reply(" What is a left join? "))
            .mount(&server)
            .await;
        let transcriber = DeepgramTranscriber::with_url("secret", "nova-3", None, Handle::current(), &server.uri());
        let pcm = vec![100i16; 16_000];
        let heard = off_runtime(move || transcriber.transcribe(&pcm, "Vention, Airflow")).await.unwrap();
        assert_eq!(heard.text, "What is a left join?");
        assert_eq!((heard.language.as_deref(), (heard.probability * 100.0).round() as i32), (Some("en"), 93));
    }

    #[tokio::test]
    async fn a_fixed_language_is_sent_and_trusted() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(query_param("language", "ru"))
            .respond_with(deepgram_reply("Расскажите о себе"))
            .mount(&server)
            .await;
        let transcriber =
            Arc::new(DeepgramTranscriber::with_url("k", "nova-2", Some("ru"), Handle::current(), &server.uri()));
        let again = transcriber.clone();
        let heard = off_runtime(move || again.transcribe(&vec![5i16; 16_000], "")).await.unwrap();
        assert_eq!(
            (heard.text.as_str(), heard.language.as_deref(), heard.probability),
            ("Расскажите о себе", Some("ru"), 1.0)
        );
        transcriber.set_language(None);
        assert!(transcriber.language.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn a_key_term_the_api_dislikes_does_not_cost_the_sentence_and_errors_are_explained() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(query_param("keyterm", "Bad Term"))
            .respond_with(ResponseTemplate::new(400))
            .mount(&server)
            .await;
        Mock::given(method("POST")).respond_with(deepgram_reply("plain text")).mount(&server).await;
        let transcriber = DeepgramTranscriber::with_url("k", "nova-3", None, Handle::current(), &server.uri());
        let heard = off_runtime(move || transcriber.transcribe(&vec![5i16; 16_000], "Bad Term")).await.unwrap();
        assert_eq!(heard.text, "plain text");
        let refusing = MockServer::start().await;
        Mock::given(method("POST")).respond_with(ResponseTemplate::new(401)).mount(&refusing).await;
        let transcriber = DeepgramTranscriber::with_url("k", "nova-3", None, Handle::current(), &refusing.uri());
        let error = off_runtime(move || transcriber.transcribe(&vec![5i16; 16_000], "")).await.unwrap_err();
        assert!(error.contains("rejected the API key"));
    }

    #[tokio::test]
    async fn too_little_audio_and_a_missing_key_are_not_sent() {
        let transcriber = DeepgramTranscriber::with_url("", "nova-3", None, Handle::current(), "http://127.0.0.1:1");
        if std::env::var("DEEPGRAM_API_KEY").unwrap_or_default().is_empty() {
            assert!(transcriber.load().unwrap_err().contains("needs an API key"));
        }
        let keyed = DeepgramTranscriber::with_url("k", "nova-3", None, Handle::current(), "http://127.0.0.1:1");
        let heard = off_runtime(move || keyed.transcribe(&vec![5i16; 100], "")).await.unwrap(); // under a tenth of a second
        assert_eq!(heard, Transcript::default());
    }

    #[tokio::test]
    async fn the_key_check_tells_a_good_key_from_a_bad_one() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(header("authorization", "Token good"))
            .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
            .mount(&server)
            .await;
        Mock::given(method("GET")).respond_with(ResponseTemplate::new(401)).mount(&server).await;
        assert!(check_deepgram_key("good", &server.uri()).await.is_ok());
        assert!(check_deepgram_key("bad", &server.uri()).await.unwrap_err().contains("rejected"));
        assert!(check_deepgram_key("", &server.uri()).await.unwrap_err().contains("Enter the Deepgram"));
        assert!(check_deepgram_key("x", "http://127.0.0.1:1").await.unwrap_err().contains("not reachable"));
    }
}
