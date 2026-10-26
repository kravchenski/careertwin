//! Where things live and what the user can change.
//!
//! Settings are one TOML file, written private to the user (mode 0600). A key can also come from
//! its usual environment variable, so a key never has to be typed into a file.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::pomodoro::PomodoroSettings;
use crate::secrets::Secret;

pub const APP_DIR: &str = "careertwin-prep";
/// Points every folder below one root; used by the tests and by anyone who wants a portable copy.
pub const HOME_ENV: &str = "CAREERTWIN_PREP_HOME";

#[derive(Debug, Clone)]
pub struct Paths {
    pub config: PathBuf,
    pub data: PathBuf,
    pub cache: PathBuf,
}

impl Paths {
    pub fn discover() -> Result<Self> {
        if let Some(root) = std::env::var_os(HOME_ENV).filter(|v| !v.is_empty()) {
            return Ok(Self::under(PathBuf::from(root)));
        }
        let dirs = ProjectDirs::from("", "", APP_DIR).context("no home directory to keep settings in")?;
        Ok(Self {
            config: dirs.config_dir().to_path_buf(),
            data: dirs.data_dir().to_path_buf(),
            cache: dirs.cache_dir().to_path_buf(),
        })
    }

    pub fn under(root: PathBuf) -> Self {
        Self { config: root.join("config"), data: root.join("data"), cache: root.join("cache") }
    }

    pub fn settings_file(&self) -> PathBuf {
        self.config.join("settings.toml")
    }

    pub fn sessions_dir(&self) -> PathBuf {
        self.data.join("sessions")
    }

    pub fn deck_file(&self) -> PathBuf {
        self.data.join("deck.json")
    }

    pub fn speech_cache(&self) -> PathBuf {
        self.cache.join("speech")
    }

    pub fn models_dir(&self) -> PathBuf {
        self.data.join("models")
    }

    pub fn stats_file(&self) -> PathBuf {
        self.data.join("stats.json")
    }

    pub fn syllabus_file(&self) -> PathBuf {
        self.data.join("syllabus.json")
    }

    pub fn progress_file(&self) -> PathBuf {
        self.data.join("progress.json")
    }

    pub fn notes_file(&self) -> PathBuf {
        self.data.join("notes.json")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum UiLanguage {
    #[default]
    Auto,
    En,
    Ru,
}

impl UiLanguage {
    /// `auto` follows the system locale: Russian for `ru_*`, English otherwise.
    pub fn resolve(self) -> Self {
        match self {
            Self::Auto => {
                let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
                    .iter()
                    .filter_map(|name| std::env::var(name).ok())
                    .find(|value| !value.is_empty())
                    .unwrap_or_default();
                if locale.to_lowercase().starts_with("ru") { Self::Ru } else { Self::En }
            }
            other => other,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LlmSettings {
    pub url: String,
    pub key: Secret,
    /// Answers the feedback, the question cards and the translations.
    pub model: String,
    /// The interviewer's turns. A small fast model keeps the conversation lively.
    pub fast_model: String,
}

impl Default for LlmSettings {
    fn default() -> Self {
        Self {
            url: "http://127.0.0.1:3260".into(),
            key: Secret::default(),
            model: "auto".into(),
            fast_model: String::new(),
        }
    }
}

impl LlmSettings {
    pub fn interviewer_model(&self) -> &str {
        if self.fast_model.trim().is_empty() { &self.model } else { &self.fast_model }
    }

    /// The fast model when there is one, for quick looks; `main` otherwise.
    pub fn fast_model_or(&self, main: &str) -> String {
        if self.fast_model.trim().is_empty() { main.to_owned() } else { self.fast_model.clone() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SttEngine {
    /// An OpenAI-style `/v1/audio/transcriptions` endpoint (OpenAI, Groq, a local server).
    #[default]
    Http,
    /// whisper.cpp inside the program; needs the `local-stt` build.
    Local,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SttSettings {
    pub engine: SttEngine,
    pub url: String,
    pub key: Secret,
    pub model: String,
    /// `auto` lets the recogniser pick, which is what a learner who slips into Russian needs.
    pub language: String,
    /// Which Whisper model the offline engine uses (`tiny`, `base`, `small`, `turbo`); empty means
    /// the one that suits this computer.
    pub local_model: String,
}

impl Default for SttSettings {
    fn default() -> Self {
        Self {
            engine: SttEngine::Http,
            url: "https://api.groq.com/openai".into(),
            key: Secret::default(),
            model: "whisper-large-v3-turbo".into(),
            language: "auto".into(),
            local_model: String::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TtsEngine {
    /// Nothing is spoken; the interviewer writes only.
    Off,
    /// An OpenAI-style `/v1/audio/speech` endpoint: OpenAI, or a local server for an open model.
    #[default]
    Openai,
    Elevenlabs,
    /// Kokoro inside this program, offline; needs the `local-tts` build. English only.
    Local,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TtsSettings {
    pub engine: TtsEngine,
    pub url: String,
    pub key: Secret,
    pub model: String,
    /// The interviewer's voice when they speak English.
    pub voice_en: String,
    /// The voice for Russian prompts and explanations.
    pub voice_ru: String,
    /// Tone for models that take instructions (gpt-4o-mini-tts).
    pub instructions: String,
    pub speed: f32,
}

impl Default for TtsSettings {
    fn default() -> Self {
        Self {
            engine: TtsEngine::Openai,
            url: "https://api.openai.com".into(),
            key: Secret::default(),
            model: "gpt-4o-mini-tts".into(),
            voice_en: "alloy".into(),
            voice_ru: "alloy".into(),
            instructions: String::new(),
            speed: 1.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct AudioSettings {
    /// Empty means the system default.
    pub input_device: String,
    pub output_device: String,
    /// With headphones the interviewer can be interrupted; on speakers the microphone would hear
    /// the interviewer, so it is muted while they talk.
    pub headphones: bool,
}

/// Who you are, for the panel at the bottom of the window. Both may stay empty: the name and the
/// e-mail in your ABOUT.md are used then.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProfileSettings {
    pub name: String,
    pub email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub ui_language: UiLanguage,
    pub profile: ProfileSettings,
    /// The focus timer: block, breaks and how often the long one comes.
    pub pomodoro: PomodoroSettings,
    /// Minutes of practice a day that fill the ring on the home page.
    pub daily_goal_minutes: u32,
    /// The language the lessons, the introduction, live coding and the chat practise; empty means
    /// English. The English tab always practises English.
    pub practice_language: String,
    /// Your ABOUT.md. Empty falls back to `CAREERTWIN_ABOUT_PATH`, then `./ABOUT.md`.
    pub about_path: String,
    pub cv_paths: Vec<String>,
    pub llm: LlmSettings,
    pub stt: SttSettings,
    pub tts: TtsSettings,
    pub audio: AudioSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            ui_language: UiLanguage::Auto,
            profile: ProfileSettings::default(),
            pomodoro: PomodoroSettings::default(),
            daily_goal_minutes: 25,
            practice_language: String::new(),
            about_path: String::new(),
            cv_paths: Vec::new(),
            llm: LlmSettings::default(),
            stt: SttSettings::default(),
            tts: TtsSettings::default(),
            audio: AudioSettings::default(),
        }
    }
}

fn env_secret(name: &str) -> Option<Secret> {
    std::env::var(name).ok().map(Secret::new).filter(|secret| !secret.is_empty())
}

impl Settings {
    /// The saved settings, or the defaults when there is no file or it cannot be read.
    pub fn load(paths: &Paths) -> Self {
        fs::read_to_string(paths.settings_file()).ok().and_then(|text| toml::from_str(&text).ok()).unwrap_or_default()
    }

    pub fn save(&self, paths: &Paths) -> Result<()> {
        let text = toml::to_string_pretty(self).context("could not write the settings")?;
        write_private(&paths.settings_file(), text.as_bytes())
    }

    /// The settings with every empty key filled from its environment variable.
    pub fn effective(&self) -> Self {
        let mut s = self.clone();
        if s.llm.key.is_empty()
            && let Some(key) = env_secret("SWITCHYARD_API_KEY")
        {
            s.llm.key = key;
        }
        if s.stt.key.is_empty()
            && let Some(key) = env_secret("GROQ_API_KEY").or_else(|| env_secret("OPENAI_API_KEY"))
        {
            s.stt.key = key;
        }
        if s.tts.key.is_empty() {
            let name = if s.tts.engine == TtsEngine::Elevenlabs { "ELEVENLABS_API_KEY" } else { "OPENAI_API_KEY" };
            if let Some(key) = env_secret(name) {
                s.tts.key = key;
            }
        }
        if let Ok(url) = std::env::var("SWITCHYARD_URL")
            && !url.trim().is_empty()
            && self.llm.url == LlmSettings::default().url
        {
            s.llm.url = url;
        }
        s
    }

    /// Every secret in use, so error messages can be scrubbed.
    pub fn secrets(&self) -> Vec<String> {
        [&self.llm.key, &self.stt.key, &self.tts.key]
            .into_iter()
            .filter(|key| !key.is_empty())
            .map(|key| key.expose().to_owned())
            .collect()
    }

    /// The language the lessons practise, by name: "English" when none is chosen.
    pub fn practice_name(&self) -> &str {
        if self.practice_language.trim().is_empty() { "English" } else { self.practice_language.trim() }
    }

    /// The path of ABOUT.md that will be read.
    pub fn about_file(&self) -> PathBuf {
        if !self.about_path.trim().is_empty() {
            return expand_home(&self.about_path);
        }
        if let Some(path) = std::env::var_os("CAREERTWIN_ABOUT_PATH").filter(|v| !v.is_empty()) {
            return PathBuf::from(path);
        }
        PathBuf::from("ABOUT.md")
    }

    pub fn validate(&self) -> Result<()> {
        validate_service_url(&self.llm.url, "the model address")?;
        if self.stt.engine == SttEngine::Http {
            validate_service_url(&self.stt.url, "the speech recognition address")?;
        }
        if matches!(self.tts.engine, TtsEngine::Openai | TtsEngine::Elevenlabs) {
            validate_service_url(&self.tts.url, "the voice address")?;
        }
        if self.tts.engine == TtsEngine::Local && !crate::models::kokoro_voice_known(&self.tts.voice_en) {
            bail!("the offline voice {:?} is not one of the voices that come with the program", self.tts.voice_en);
        }
        if self.stt.engine == SttEngine::Local
            && !self.stt.local_model.is_empty()
            && crate::models::stt_model(&self.stt.local_model).is_none()
        {
            bail!("the offline recognition model {:?} is not one of tiny, base, small or turbo", self.stt.local_model);
        }
        if !(0.5..=2.0).contains(&self.tts.speed) {
            bail!("the speaking speed must be between 0.5 and 2.0");
        }
        if !self.profile.email.trim().is_empty() && !looks_like_email(&self.profile.email) {
            bail!("the e-mail address does not look right");
        }
        Ok(())
    }
}

/// Something@something.something, without spaces: enough to catch a typo, not to verify anyone.
pub fn looks_like_email(text: &str) -> bool {
    let text = text.trim();
    let Some((name, host)) = text.split_once('@') else { return false };
    !name.is_empty()
        && !text.contains(char::is_whitespace)
        && !host.contains('@')
        && host.split('.').count() >= 2
        && host.split('.').all(|part| !part.is_empty())
}

pub fn expand_home(path: &str) -> PathBuf {
    let path = path.trim();
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = directories::BaseDirs::new()
    {
        return home.home_dir().join(rest);
    }
    PathBuf::from(path)
}

/// A key must never cross the network in the clear: `https` anywhere, `http` only to this machine.
pub fn validate_service_url(url: &str, what: &str) -> Result<()> {
    let url = url.trim();
    let (scheme, rest) =
        url.split_once("://").with_context(|| format!("{what} must start with http:// or https://"))?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.is_empty() {
        bail!("{what} has no host");
    }
    if authority.contains('@') {
        bail!("{what} must not contain a user name or password; put the key in its own field");
    }
    match scheme {
        "https" => Ok(()),
        "http" if is_loopback(authority) => Ok(()),
        "http" => bail!("{what} uses plain http to another machine, so a key would travel unencrypted; use https"),
        _ => bail!("{what} must start with http:// or https://"),
    }
}

fn is_loopback(authority: &str) -> bool {
    let host = if let Some(stripped) = authority.strip_prefix('[') {
        stripped.split(']').next().unwrap_or_default()
    } else {
        authority
            .rsplit_once(':')
            .map_or(authority, |(host, port)| if port.chars().all(|c| c.is_ascii_digit()) { host } else { authority })
    };
    matches!(host.to_ascii_lowercase().as_str(), "localhost" | "127.0.0.1" | "::1")
}

/// Write a file nobody else can read, replacing the old one in one step.
pub fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path.parent().context("a file needs a folder")?;
    fs::create_dir_all(dir).with_context(|| format!("could not create {}", dir.display()))?;
    restrict_dir(dir);
    let tmp = dir.join(format!(".{}.tmp", uuid::Uuid::new_v4().simple()));
    {
        let mut file = open_private(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path).with_context(|| format!("could not replace {}", path.display()))?;
    Ok(())
}

#[cfg(unix)]
fn open_private(path: &Path) -> Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    Ok(fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(path)?)
}

#[cfg(not(unix))]
fn open_private(path: &Path) -> Result<fs::File> {
    Ok(fs::OpenOptions::new().write(true).create_new(true).open(path)?)
}

#[cfg(unix)]
fn restrict_dir(dir: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
}

#[cfg(not(unix))]
fn restrict_dir(_dir: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_paths() -> (tempfile::TempDir, Paths) {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::under(dir.path().to_path_buf());
        (dir, paths)
    }

    #[test]
    fn defaults_are_valid() {
        Settings::default().validate().unwrap();
    }

    #[test]
    fn settings_round_trip_through_the_file() {
        let (_dir, paths) = temp_paths();
        let mut settings = Settings::default();
        settings.llm.model = "gpt-test".into();
        settings.tts.voice_en = "nova".into();
        settings.cv_paths = vec!["~/cv.pdf".into()];
        settings.save(&paths).unwrap();
        let loaded = Settings::load(&paths);
        assert_eq!(loaded.llm.model, "gpt-test");
        assert_eq!(loaded.tts.voice_en, "nova");
        assert_eq!(loaded.cv_paths, vec!["~/cv.pdf".to_string()]);
    }

    #[cfg(unix)]
    #[test]
    fn the_settings_file_and_its_folder_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let (_dir, paths) = temp_paths();
        let mut settings = Settings::default();
        settings.llm.key = Secret::new("sk-test-123456789");
        settings.save(&paths).unwrap();
        let file_mode = fs::metadata(paths.settings_file()).unwrap().permissions().mode() & 0o777;
        let dir_mode = fs::metadata(&paths.config).unwrap().permissions().mode() & 0o777;
        assert_eq!(file_mode, 0o600);
        assert_eq!(dir_mode, 0o700);
        // saving again keeps it private
        settings.save(&paths).unwrap();
        assert_eq!(fs::metadata(paths.settings_file()).unwrap().permissions().mode() & 0o777, 0o600);
    }

    #[test]
    fn a_missing_or_broken_file_gives_the_defaults() {
        let (_dir, paths) = temp_paths();
        assert_eq!(Settings::load(&paths).llm.model, "auto");
        fs::create_dir_all(&paths.config).unwrap();
        fs::write(paths.settings_file(), "this is = not [valid toml").unwrap();
        assert_eq!(Settings::load(&paths).llm.model, "auto");
    }

    #[test]
    fn unknown_and_missing_fields_do_not_break_loading() {
        let (_dir, paths) = temp_paths();
        fs::create_dir_all(&paths.config).unwrap();
        fs::write(paths.settings_file(), "future_field = 1\n[llm]\nmodel = \"m\"\n").unwrap();
        let loaded = Settings::load(&paths);
        assert_eq!(loaded.llm.model, "m");
        assert_eq!(loaded.llm.url, "http://127.0.0.1:3260");
    }

    #[test]
    fn urls_are_checked_before_a_key_can_travel() {
        assert!(validate_service_url("https://api.openai.com", "x").is_ok());
        assert!(validate_service_url("http://127.0.0.1:3260", "x").is_ok());
        assert!(validate_service_url("http://localhost:8000/v1", "x").is_ok());
        assert!(validate_service_url("http://[::1]:8000", "x").is_ok());
        assert!(validate_service_url("http://example.com", "x").is_err());
        assert!(validate_service_url("http://192.168.1.5:8000", "x").is_err());
        assert!(validate_service_url("http://localhost.evil.com", "x").is_err());
        assert!(validate_service_url("https://user:pass@example.com", "x").is_err());
        assert!(validate_service_url("http://127.0.0.1@evil.com", "x").is_err());
        assert!(validate_service_url("ftp://example.com", "x").is_err());
        assert!(validate_service_url("example.com", "x").is_err());
        assert!(validate_service_url("https://", "x").is_err());
    }

    #[test]
    fn an_email_is_checked_for_its_shape_only() {
        for good in ["a@b.co", " name.surname@mail.example.org "] {
            assert!(looks_like_email(good), "{good}");
        }
        for bad in ["", "a", "a@b", "@b.co", "a@b.", "a b@c.de", "a@@b.co", "a@b..co"] {
            assert!(!looks_like_email(bad), "{bad}");
        }
        let mut settings = Settings::default();
        settings.profile.email = "nonsense".into();
        assert!(settings.validate().is_err());
        settings.profile.email = String::new();
        assert!(settings.validate().is_ok(), "no e-mail is fine");
    }

    #[test]
    fn the_new_settings_have_sensible_defaults_and_old_files_still_load() {
        let (_dir, paths) = temp_paths();
        fs::create_dir_all(&paths.config).unwrap();
        fs::write(paths.settings_file(), "[llm]\nmodel = \"m\"\n").unwrap();
        let loaded = Settings::load(&paths);
        assert_eq!(loaded.pomodoro, PomodoroSettings::default());
        assert_eq!(loaded.daily_goal_minutes, 25);
        assert!(loaded.profile.email.is_empty());
    }

    #[test]
    fn the_practised_language_is_english_unless_another_is_chosen() {
        let mut settings = Settings::default();
        assert_eq!(settings.practice_name(), "English");
        settings.practice_language = "  German ".into();
        assert_eq!(settings.practice_name(), "German");
        settings.practice_language = "   ".into();
        assert_eq!(settings.practice_name(), "English");
    }

    #[test]
    fn speed_must_be_in_range() {
        let mut settings = Settings::default();
        settings.tts.speed = 3.0;
        assert!(settings.validate().is_err());
    }

    #[test]
    fn the_fast_model_falls_back_to_the_main_one() {
        let mut llm = LlmSettings::default();
        llm.model = "big".into();
        assert_eq!(llm.interviewer_model(), "big");
        llm.fast_model = "small".into();
        assert_eq!(llm.interviewer_model(), "small");
    }

    #[test]
    fn about_path_prefers_the_setting() {
        let mut settings = Settings::default();
        settings.about_path = "/tmp/x/ABOUT.md".into();
        assert_eq!(settings.about_file(), PathBuf::from("/tmp/x/ABOUT.md"));
    }

    #[test]
    fn secrets_lists_only_filled_keys() {
        let mut settings = Settings::default();
        assert!(settings.secrets().is_empty());
        settings.tts.key = Secret::new("abcdef123456");
        assert_eq!(settings.secrets(), vec!["abcdef123456".to_string()]);
    }
}
