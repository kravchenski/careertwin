//! `careertwin-prep --doctor`: checks the setup one piece at a time and says what to fix.

use std::time::Duration;

use crate::audio::vad;
use crate::audio::{Mic, Player};
use crate::brief::Brief;
use crate::config::{Paths, Settings, SttEngine, TtsEngine};
use crate::llm::HttpLlm;
use crate::speech::{self, SttHint};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub ok: bool,
    pub name: &'static str,
    pub detail: String,
}

/// One line of at most 220 characters: error bodies from services are often several lines long.
fn one_line(text: impl Into<String>) -> String {
    let joined = text.into().split_whitespace().collect::<Vec<_>>().join(" ");
    if joined.chars().count() > 220 { format!("{}…", joined.chars().take(219).collect::<String>()) } else { joined }
}

impl Check {
    fn pass(name: &'static str, detail: impl Into<String>) -> Self {
        Self { ok: true, name, detail: one_line(detail) }
    }

    fn fail(name: &'static str, detail: impl Into<String>) -> Self {
        Self { ok: false, name, detail: one_line(detail) }
    }

    pub fn line(&self) -> String {
        format!("{} {:<22} {}", if self.ok { "ok  " } else { "FAIL" }, self.name, self.detail)
    }
}

pub struct Options {
    /// Try the microphone and the speakers.
    pub devices: bool,
    /// Ask the recogniser and the voice for one tiny sample each.
    pub probes: bool,
}

pub async fn run(settings: &Settings, paths: &Paths, options: &Options) -> Vec<Check> {
    let effective = settings.effective();
    let mut checks = vec![settings_check(paths)];
    checks.push(profile_check(&effective));
    checks.push(model_check(&effective).await);
    checks.push(recognition_check(&effective, paths, options.probes).await);
    checks.push(voice_check(&effective, paths, options.probes).await);
    if options.devices {
        checks.push(microphone_check(&effective.audio.input_device).await);
        checks.push(speakers_check(&effective.audio.output_device));
    }
    checks.push(privacy_check(paths));
    checks
}

fn settings_check(paths: &Paths) -> Check {
    let file = paths.settings_file();
    if file.is_file() {
        Check::pass("settings", file.display().to_string())
    } else {
        Check::pass("settings", format!("no file yet, using defaults ({} will be made when you save)", file.display()))
    }
}

fn profile_check(settings: &Settings) -> Check {
    match Brief::load(settings) {
        Ok((brief, warnings)) => {
            let sources = brief.sources().iter().map(|(_, label)| label.clone()).collect::<Vec<_>>().join(", ");
            if warnings.is_empty() {
                Check::pass("profile", sources)
            } else {
                Check::fail("profile", format!("{sources}; {}", warnings.join("; ")))
            }
        }
        Err(error) => Check::fail("profile", error.to_string()),
    }
}

async fn model_check(settings: &Settings) -> Check {
    let llm = match HttpLlm::new(&settings.llm, settings.secrets()) {
        Ok(llm) => llm,
        Err(error) => return Check::fail("language model", error.to_string()),
    };
    match llm.models().await {
        Ok(models) => {
            let wanted = &settings.llm.model;
            if wanted == "auto" || models.iter().any(|m| m == wanted) {
                Check::pass(
                    "language model",
                    format!("{} answers, {} models, using {wanted}", settings.llm.url, models.len()),
                )
            } else {
                Check::fail("language model", format!("{} answers but does not list {wanted}", settings.llm.url))
            }
        }
        Err(error) => Check::fail("language model", error.to_string()),
    }
}

/// What the recogniser is, in words: an address for a server, a model name for the offline one.
fn recognition_name(settings: &Settings) -> String {
    match settings.stt.engine {
        SttEngine::Http => format!("{} ({})", settings.stt.url, settings.stt.model),
        SttEngine::Local => format!("offline {}", crate::speech::local_stt_model(settings).label),
    }
}

/// What the voice is, in words.
fn voice_name(settings: &Settings) -> String {
    match settings.tts.engine {
        TtsEngine::Openai | TtsEngine::Elevenlabs => format!("{} ({})", settings.tts.url, settings.tts.model),
        TtsEngine::Local => format!("offline Kokoro, voice {}", settings.tts.voice_en),
        TtsEngine::Off => "off".to_owned(),
    }
}

async fn recognition_check(settings: &Settings, paths: &Paths, probe: bool) -> Check {
    let stt = match speech::build_stt(settings, paths) {
        Ok(stt) => stt,
        Err(error) => return Check::fail("recognition", error.to_string()),
    };
    if !probe {
        return Check::pass("recognition", format!("{} (not tried)", recognition_name(settings)));
    }
    // a second of silence: nothing is understood, but the address, the key and the model are all used
    match stt.transcribe(vec![0i16; 16_000], SttHint::default()).await {
        Ok(_) => Check::pass("recognition", format!("{} answers", recognition_name(settings))),
        Err(error) => Check::fail("recognition", error.to_string()),
    }
}

async fn voice_check(settings: &Settings, paths: &Paths, probe: bool) -> Check {
    if settings.tts.engine == TtsEngine::Off {
        return Check::pass("voice", "off: the interviewer only writes");
    }
    let tts = match speech::build_tts(settings, paths) {
        Ok(Some(tts)) => tts,
        Ok(None) => return Check::pass("voice", "off"),
        Err(error) => return Check::fail("voice", error.to_string()),
    };
    if !probe {
        return Check::pass("voice", format!("{} (not tried)", voice_name(settings)));
    }
    match tts.synthesize("Ready.".into(), speech::voice_for(settings, crate::prompts::Lang::En)).await {
        Ok(speech) => Check::pass("voice", format!("{} spoke {:.1} s", voice_name(settings), speech.seconds())),
        Err(error) => Check::fail("voice", error.to_string()),
    }
}

async fn microphone_check(device: &str) -> Check {
    let mut mic = match Mic::open(device) {
        Ok(mic) => mic,
        Err(error) => return Check::fail("microphone", error.to_string()),
    };
    let name = mic.device.clone();
    let end = tokio::time::Instant::now() + Duration::from_millis(1200);
    let (mut frames, mut loudest) = (0usize, 0.0f32);
    while let Ok(Some(frame)) = tokio::time::timeout_at(end, mic.frames.recv()).await {
        frames += 1;
        loudest = loudest.max(vad::rms(&frame));
    }
    if frames < 20 {
        Check::fail("microphone", format!("{name} gave only {frames} frames in a second"))
    } else if loudest < 1.0 {
        Check::fail("microphone", format!("{name} is silent: is it muted?"))
    } else {
        Check::pass("microphone", format!("{name}, level {loudest:.0}"))
    }
}

fn speakers_check(device: &str) -> Check {
    match Player::open(device) {
        Ok(_) => {
            Check::pass("speakers", if device.is_empty() { "system default".to_owned() } else { device.to_owned() })
        }
        Err(error) => Check::fail("speakers", error.to_string()),
    }
}

#[cfg(unix)]
fn privacy_check(paths: &Paths) -> Check {
    use std::os::unix::fs::PermissionsExt;
    let mut loose = Vec::new();
    for file in [paths.settings_file(), paths.deck_file()] {
        if let Ok(meta) = std::fs::metadata(&file)
            && meta.permissions().mode() & 0o077 != 0
        {
            loose.push(file.display().to_string());
        }
    }
    if loose.is_empty() {
        Check::pass("privacy", "your files are readable only by you")
    } else {
        Check::fail("privacy", format!("readable by others: {}; run chmod 600 on them", loose.join(", ")))
    }
}

#[cfg(not(unix))]
fn privacy_check(_paths: &Paths) -> Check {
    Check::pass("privacy", "not checked on this system")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::Secret;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn settings(about: &std::path::Path, server: &str) -> Settings {
        let mut settings = Settings::default();
        settings.about_path = about.to_string_lossy().into_owned();
        settings.llm.url = server.to_owned();
        settings.llm.model = "auto".into();
        settings.stt.url = server.to_owned();
        settings.stt.key = Secret::new("gsk_test_1234567890");
        settings.tts.url = server.to_owned();
        settings.tts.key = Secret::new("sk-test-1234567890");
        settings
    }

    async fn healthy_server() -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": [{"id": "m1"}, {"id": "m2"}]})))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/audio/transcriptions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"text": ""})))
            .mount(&server)
            .await;
        let wav = crate::speech::wav::encode_i16(&vec![500i16; 12_000], 24_000).unwrap();
        Mock::given(method("POST"))
            .and(path("/v1/audio/speech"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(wav, "audio/wav"))
            .mount(&server)
            .await;
        server
    }

    fn about() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("ABOUT.md");
        std::fs::write(&file, include_str!("../tests/fixtures/ABOUT.demo.md")).unwrap();
        (dir, file)
    }

    #[tokio::test]
    async fn a_healthy_setup_passes_every_check() {
        let server = healthy_server().await;
        let (dir, file) = about();
        let paths = Paths::under(dir.path().join("home"));
        let checks = run(&settings(&file, &server.uri()), &paths, &Options { devices: false, probes: true }).await;
        let failed: Vec<String> = checks.iter().filter(|c| !c.ok).map(Check::line).collect();
        assert!(failed.is_empty(), "{failed:?}");
        let names: Vec<&str> = checks.iter().map(|c| c.name).collect();
        assert_eq!(names, ["settings", "profile", "language model", "recognition", "voice", "privacy"]);
    }

    #[tokio::test]
    async fn each_broken_piece_is_named_and_the_rest_still_run() {
        let (dir, _) = about();
        let paths = Paths::under(dir.path().join("home"));
        let mut broken = Settings::default();
        broken.about_path = dir.path().join("nope.md").to_string_lossy().into_owned();
        broken.llm.url = "http://127.0.0.1:9".into();
        broken.stt.url = "http://127.0.0.1:9".into();
        broken.tts.url = "http://127.0.0.1:9".into();
        let checks = run(&broken, &paths, &Options { devices: false, probes: true }).await;
        let failed: Vec<&str> = checks.iter().filter(|c| !c.ok).map(|c| c.name).collect();
        assert_eq!(failed, ["profile", "language model", "recognition", "voice"]);
    }

    #[tokio::test]
    async fn an_unlisted_model_is_reported() {
        let server = healthy_server().await;
        let (dir, file) = about();
        let mut s = settings(&file, &server.uri());
        s.llm.model = "not-there".into();
        let checks = run(&s, &Paths::under(dir.path().join("home")), &Options { devices: false, probes: false }).await;
        let model = checks.iter().find(|c| c.name == "language model").unwrap();
        assert!(!model.ok && model.detail.contains("does not list not-there"), "{model:?}");
    }

    #[tokio::test]
    async fn without_probes_nothing_is_sent_to_the_speech_services() {
        let server = healthy_server().await;
        let (dir, file) = about();
        run(
            &settings(&file, &server.uri()),
            &Paths::under(dir.path().join("home")),
            &Options { devices: false, probes: false },
        )
        .await;
        let paths: Vec<String> =
            server.received_requests().await.unwrap().iter().map(|r| r.url.path().to_owned()).collect();
        assert_eq!(paths, vec!["/v1/models"]);
    }

    #[tokio::test]
    async fn a_voice_that_is_off_is_fine_and_no_key_ever_appears_in_the_report() {
        let (dir, file) = about();
        let mut s = settings(&file, "http://127.0.0.1:9");
        s.tts.engine = TtsEngine::Off;
        let checks = run(&s, &Paths::under(dir.path().join("home")), &Options { devices: false, probes: true }).await;
        assert!(checks.iter().find(|c| c.name == "voice").unwrap().ok);
        let report = checks.iter().map(Check::line).collect::<Vec<_>>().join("\n");
        assert!(!report.contains("gsk_test_1234567890") && !report.contains("sk-test-1234567890"), "{report}");
    }

    #[test]
    fn the_report_names_the_engine_that_was_really_used() {
        let mut settings = Settings::default();
        settings.stt.url = "https://api.groq.com/openai".into();
        assert!(
            recognition_name(&settings).contains("api.groq.com")
                && recognition_name(&settings).contains("whisper-large-v3-turbo")
        );
        assert!(voice_name(&settings).contains("api.openai.com"));
        settings.stt.engine = SttEngine::Local;
        settings.stt.local_model = "small".into();
        settings.tts.engine = TtsEngine::Local;
        settings.tts.voice_en = "af_heart".into();
        assert_eq!(recognition_name(&settings), "offline Whisper small");
        assert_eq!(voice_name(&settings), "offline Kokoro, voice af_heart");
        assert!(!recognition_name(&settings).contains("groq") && !voice_name(&settings).contains("openai"));
        settings.tts.engine = TtsEngine::Off;
        assert_eq!(voice_name(&settings), "off");
    }

    #[test]
    fn a_report_line_is_one_line_however_long_the_error() {
        let check = Check::fail(
            "voice",
            format!(
                "The voice engine returned 401: {{\n  \"error\": {{\n   \"message\": \"{}\"\n }}\n}}",
                "x".repeat(400)
            ),
        );
        assert!(!check.line().contains('\n'));
        assert!(check.detail.chars().count() <= 220);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn files_others_can_read_are_called_out() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, file) = about();
        let paths = Paths::under(dir.path().join("home"));
        let s = settings(&file, "http://127.0.0.1:9");
        s.save(&paths).unwrap();
        std::fs::set_permissions(paths.settings_file(), std::fs::Permissions::from_mode(0o644)).unwrap();
        let check = privacy_check(&paths);
        assert!(!check.ok && check.detail.contains("chmod 600"), "{check:?}");
        std::fs::set_permissions(paths.settings_file(), std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(privacy_check(&paths).ok);
    }
}
