//! Settings of the interview window, kept in `data/interview/settings.json` (the Python overlay's file).
//!
//! What only this program knows lives in `app.json` next to it, see [`AppSettings`].

use std::path::Path;

use serde_json::{Map, Value, json};
use thiserror::Error;

use crate::accounts::{self, Account};
use crate::paths::Paths;
use crate::providers::{base_url, default_gateway};

pub const WHISPER_MODELS: [&str; 5] = ["tiny", "base", "small", "medium", "large-v3-turbo"];
pub const ANSWER_STYLES: [&str; 3] = ["concise", "standard", "detailed"];
pub const ANSWER_LAYOUTS: [&str; 2] = ["main", "lines"];
pub const DEVICES: [&str; 3] = ["auto", "cpu", "cuda"];
pub const STT_ENGINES: [&str; 2] = ["whisper", "deepgram"];

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SettingsError {
    #[error("Unknown setting {0:?}")]
    Unknown(String),
    #[error("{0}")]
    Bad(String),
}

fn bad<T>(message: impl Into<String>) -> Result<T, SettingsError> {
    Err(SettingsError::Bad(message.into()))
}

/// Everything adjustable in the window. Empty strings mean "use the default".
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub language: String,
    pub ui_language: String,
    pub listen: bool,
    pub call_source: String,
    pub use_mic: bool,
    pub mic_source: String,
    pub stt_engine: String,
    pub whisper_model: String,
    pub whisper_device: String,
    pub deepgram_key: String,
    pub deepgram_model: String,
    pub gateway_url: String,
    /// The account in use; the others for the same address are in `accounts`.
    pub gateway_key: String,
    pub accounts: Vec<Account>,
    pub model: String,
    pub vision_model: String,
    /// Comma separated; asked in order when the model is rate limited or down.
    pub fallback_models: String,
    pub answer_style: String,
    pub answer_layout: String,
    pub show_full_answer: bool,
    pub screenshot_enabled: bool,
    pub screenshot_tool: String,
    pub screenshot_delay_ms: i64,
    pub pinned: bool,
    pub frameless: bool,
    pub opacity: i64,
    pub font_scale: i64,
    pub save_transcripts: bool,
    /// Keys this program does not know (a newer Python overlay's), written back untouched.
    extra: Map<String, Value>,
}

impl Default for Settings {
    fn default() -> Self {
        let text = |value: &str| value.to_owned();
        Self {
            language: text("auto"),
            ui_language: text("auto"),
            listen: !cfg!(target_os = "macos"), // macOS has no call-audio capture yet
            call_source: String::new(),
            use_mic: false,
            mic_source: String::new(),
            stt_engine: text("whisper"),
            whisper_model: text("small"),
            whisper_device: text("auto"),
            deepgram_key: String::new(),
            deepgram_model: text("nova-3"),
            gateway_url: String::new(),
            gateway_key: String::new(),
            accounts: Vec::new(),
            model: String::new(),
            vision_model: String::new(),
            fallback_models: String::new(),
            answer_style: text("standard"),
            answer_layout: text("lines"),
            show_full_answer: true,
            screenshot_enabled: true,
            screenshot_tool: text("auto"),
            screenshot_delay_ms: 60,
            pinned: true,
            frameless: true,
            opacity: 100,
            font_scale: 100,
            save_transcripts: true,
            extra: Map::new(),
        }
    }
}

impl Settings {
    /// Read the saved settings; unknown keys are kept and bad values fall back to defaults.
    pub fn load(path: &Path) -> Self {
        let mut settings = Self::default();
        let Ok(text) = std::fs::read_to_string(path) else { return settings };
        let Ok(Value::Object(data)) = serde_json::from_str::<Value>(&text) else { return settings };
        for (name, value) in data {
            if name == "screenshot_delay_ms" && value == json!(250) {
                continue; // the old default; the window now needs far less time to hide
            }
            // a bad value keeps the default; a key this program does not know is kept to be written back
            if let Err(SettingsError::Unknown(_)) = settings.set(&name, value.clone()) {
                settings.extra.insert(name, value);
            }
        }
        if !settings.gateway_key.is_empty()
            && !settings.accounts.iter().any(|account| account.key == settings.gateway_key)
        {
            // a key saved before accounts existed becomes the first account of its address
            let url = if settings.gateway_url.is_empty() { default_gateway() } else { settings.gateway_url.clone() };
            if let Ok(next) = accounts::add(&settings.accounts, &url, &settings.gateway_key) {
                settings.accounts = next;
            }
        }
        settings
    }

    /// Validate one value and store it.
    pub fn set(&mut self, name: &str, value: Value) -> Result<(), SettingsError> {
        let text = |value: &Value| match value {
            Value::String(text) => Ok(text.trim().to_owned()),
            _ => bad(format!("{name} must be text")),
        };
        let flag = |value: &Value| match value {
            Value::Bool(flag) => Ok(*flag),
            _ => bad(format!("{name} must be true or false")),
        };
        let choice = |value: &Value, choices: &[&str]| -> Result<String, SettingsError> {
            let value = text(value)?;
            if choices.contains(&value.as_str()) {
                Ok(value)
            } else {
                bad(format!("{name} must be one of {}", choices.join(", ")))
            }
        };
        let number = |value: &Value, low: i64, high: i64| -> Result<i64, SettingsError> {
            let Some(number) = value.as_f64().filter(|_| !value.is_boolean()) else {
                return bad(format!("{name} must be a number"));
            };
            let number = number as i64;
            if (low..=high).contains(&number) {
                Ok(number)
            } else {
                bad(format!("{name} must be between {low} and {high}"))
            }
        };
        match name {
            "language" => self.language = text(&value)?,
            "ui_language" => self.ui_language = text(&value)?,
            "listen" => self.listen = flag(&value)?,
            "call_source" => self.call_source = text(&value)?,
            "use_mic" => self.use_mic = flag(&value)?,
            "mic_source" => self.mic_source = text(&value)?,
            "stt_engine" => self.stt_engine = choice(&value, &STT_ENGINES)?,
            "whisper_model" => self.whisper_model = choice(&value, &WHISPER_MODELS)?,
            "whisper_device" => self.whisper_device = choice(&value, &DEVICES)?,
            "deepgram_key" => self.deepgram_key = text(&value)?,
            "deepgram_model" => self.deepgram_model = text(&value)?,
            "gateway_url" => {
                let url = text(&value)?;
                if !url.is_empty() && !(url.starts_with("http://") || url.starts_with("https://")) {
                    return bad("The gateway URL must start with http:// or https://");
                }
                self.gateway_url = url;
            }
            "gateway_key" => self.gateway_key = text(&value)?,
            "accounts" => {
                let items: Vec<Account> =
                    serde_json::from_value(value).map_err(|_| SettingsError::Bad("accounts must be a list".into()))?;
                self.accounts = accounts::clean(items).map_err(|error| SettingsError::Bad(error.to_string()))?;
            }
            "model" => self.model = text(&value)?,
            "vision_model" => self.vision_model = text(&value)?,
            "fallback_models" => self.fallback_models = text(&value)?,
            "answer_style" => self.answer_style = choice(&value, &ANSWER_STYLES)?,
            "answer_layout" => self.answer_layout = choice(&value, &ANSWER_LAYOUTS)?,
            "show_full_answer" => self.show_full_answer = flag(&value)?,
            "screenshot_enabled" => self.screenshot_enabled = flag(&value)?,
            "screenshot_tool" => self.screenshot_tool = text(&value)?,
            "screenshot_delay_ms" => self.screenshot_delay_ms = number(&value, 0, 5000)?,
            "pinned" => self.pinned = flag(&value)?,
            "frameless" => self.frameless = flag(&value)?,
            "opacity" => self.opacity = number(&value, 35, 100)?,
            "font_scale" => self.font_scale = number(&value, 70, 180)?,
            "save_transcripts" => self.save_transcripts = flag(&value)?,
            other => return Err(SettingsError::Unknown(other.to_owned())),
        }
        Ok(())
    }

    /// The settings as the Python overlay writes them: the same keys in the same order, then unknown ones.
    pub fn to_json(&self) -> Value {
        let mut map = Map::new();
        let mut put = |key: &str, value: Value| {
            map.insert(key.to_owned(), value);
        };
        put("language", json!(self.language));
        put("ui_language", json!(self.ui_language));
        put("listen", json!(self.listen));
        put("call_source", json!(self.call_source));
        put("use_mic", json!(self.use_mic));
        put("mic_source", json!(self.mic_source));
        put("stt_engine", json!(self.stt_engine));
        put("whisper_model", json!(self.whisper_model));
        put("whisper_device", json!(self.whisper_device));
        put("deepgram_key", json!(self.deepgram_key));
        put("deepgram_model", json!(self.deepgram_model));
        put("gateway_url", json!(self.gateway_url));
        put("gateway_key", json!(self.gateway_key));
        put("accounts", json!(self.accounts));
        put("model", json!(self.model));
        put("vision_model", json!(self.vision_model));
        put("fallback_models", json!(self.fallback_models));
        put("answer_style", json!(self.answer_style));
        put("answer_layout", json!(self.answer_layout));
        put("show_full_answer", json!(self.show_full_answer));
        put("screenshot_enabled", json!(self.screenshot_enabled));
        put("screenshot_tool", json!(self.screenshot_tool));
        put("screenshot_delay_ms", json!(self.screenshot_delay_ms));
        put("pinned", json!(self.pinned));
        put("frameless", json!(self.frameless));
        put("opacity", json!(self.opacity));
        put("font_scale", json!(self.font_scale));
        put("save_transcripts", json!(self.save_transcripts));
        for (key, value) in &self.extra {
            map.entry(key.clone()).or_insert_with(|| value.clone());
        }
        Value::Object(map)
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        write_private(path, &serde_json::to_string_pretty(&self.to_json()).unwrap_or_default())
    }

    /// The address in use: what was typed, or where Switchyard listens.
    pub fn gateway(&self) -> String {
        if self.gateway_url.is_empty() { default_gateway() } else { base_url(&self.gateway_url) }
    }

    /// The fallback chain as a list.
    pub fn fallbacks(&self) -> Vec<String> {
        self.fallback_models.split(',').map(str::trim).filter(|name| !name.is_empty()).map(str::to_owned).collect()
    }

    /// The window language to show: the chosen one, or the system's when it is "auto".
    pub fn ui_code(&self) -> String {
        match self.ui_language.as_str() {
            "auto" | "" => std::env::var("LANG")
                .ok()
                .and_then(|lang| lang.split(['_', '.', '-']).next().map(str::to_lowercase))
                .filter(|code| crate::languages::UI_LANGUAGES.iter().any(|(known, _)| known == code))
                .unwrap_or_else(|| "en".to_owned()),
            code => code.to_owned(),
        }
    }
}

/// What a key check or an account change can say.
#[derive(Debug, PartialEq, Eq)]
pub enum AccountChange {
    Added(Account),
    Used(Account),
    Removed(Account),
}

impl Settings {
    /// The accounts saved for the address in use.
    pub fn accounts_here(&self) -> Vec<Account> {
        accounts::for_url(&self.accounts, &self.gateway())
    }

    /// Point the window at another address. A key belongs to its address and never goes to another one: the
    /// first account saved for the new address is used, or none.
    pub fn set_gateway(&mut self, url: &str) -> Result<(), SettingsError> {
        let before = self.gateway();
        self.set("gateway_url", Value::String(url.to_owned()))?;
        if self.gateway() != before {
            let key = self.accounts_here().first().map(|account| account.key.clone()).unwrap_or_default();
            self.gateway_key = key;
        }
        Ok(())
    }

    /// Save `key` for the address in use. The first key becomes the one in use; the others are spare and are
    /// tried in turn when it is limited.
    pub fn add_account(&mut self, key: &str) -> Result<AccountChange, accounts::AccountError> {
        let saved = accounts::add(&self.accounts, &self.gateway(), key)?;
        let account = accounts::for_url(&saved, &self.gateway())
            .into_iter()
            .find(|account| account.key == key.trim())
            .expect("the key was just added");
        self.accounts = saved;
        if self.gateway_key.is_empty() {
            self.gateway_key = account.key.clone();
        }
        Ok(AccountChange::Added(account))
    }

    pub fn use_account(&mut self, name: &str) -> Option<AccountChange> {
        let account = self.accounts_here().into_iter().find(|account| account.name == name)?;
        self.gateway_key = account.key.clone();
        Some(AccountChange::Used(account))
    }

    pub fn remove_account(&mut self, name: &str) -> Option<AccountChange> {
        let gone = self.accounts_here().into_iter().find(|account| account.name == name)?;
        self.accounts = accounts::remove(&self.accounts, &gone.url, name);
        if self.gateway_key == gone.key {
            self.gateway_key = self.accounts_here().first().map(|account| account.key.clone()).unwrap_or_default();
        }
        Some(AccountChange::Removed(gone))
    }
}

/// A file only its owner can read, from the first byte: it may hold API keys.
pub fn write_private(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mut file = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path)?;
        file.write_all(text.as_bytes())?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?; // an older file keeps the mode it had
        Ok(())
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, text)
    }
}

/// What only this program knows: the icon, the update check, the window size.
#[derive(Clone, Debug, PartialEq)]
pub struct AppSettings {
    /// `""` for the default icon, `builtin:<name>`, or the path of an image.
    pub app_icon: String,
    pub check_updates: bool,
    /// A version the user chose to skip, as in `0.5.1`.
    pub skipped_version: String,
    /// Seconds since the epoch of the last look for a new version.
    pub last_update_check: i64,
    /// A theme that comes with the window, or `custom`.
    pub theme: String,
    /// The three colours of the custom theme, as `#rrggbb`.
    pub custom_panel: String,
    pub custom_text: String,
    pub custom_accent: String,
    /// `both`, `dots` (next to the status), `background` or `off`.
    pub equalizer: String,
    /// The equalizer and its background move.
    pub animate: bool,
    /// Windows: leave the window out of screen sharing and recordings (it stays on your own monitor).
    pub hide_from_sharing: bool,
    /// Windows: the key that takes a screenshot from any program, as `Ctrl+Shift+Enter`.
    pub screenshot_key: String,
    /// How the accounts of one address are asked: `off` (one at a time), `hedged` (the next one when the first is
    /// slow) or `all` (together). The first to answer wins.
    pub parallel_accounts: String,
    /// Seconds an account may be silent before the next one is tried; 0 never gives up early.
    pub first_word_seconds: i64,
    /// Windows: an icon in the notification area, like the one for location in use.
    pub tray_icon: bool,
    /// Windows: no button for the window in the taskbar; the tray icon brings it forward and quits (needs the icon).
    pub hide_from_taskbar: bool,
}

pub const PARALLEL_MODES: [&str; 3] = ["off", "hedged", "all"];
pub const FIRST_WORD_RANGE: std::ops::RangeInclusive<i64> = 0..=60;

pub const EQUALIZER_STYLES: [&str; 4] = ["both", "dots", "background", "off"];

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            app_icon: String::new(),
            check_updates: true,
            skipped_version: String::new(),
            last_update_check: 0,
            theme: "midnight".to_owned(),
            custom_panel: "#13141e".to_owned(),
            custom_text: "#d7dae8".to_owned(),
            custom_accent: "#bcb4ff".to_owned(),
            equalizer: "both".to_owned(),
            animate: true,
            hide_from_sharing: true,
            screenshot_key: crate::hotkey::DEFAULT.to_owned(),
            parallel_accounts: "hedged".to_owned(),
            first_word_seconds: 15,
            tray_icon: true,
            hide_from_taskbar: true,
        }
    }
}

impl AppSettings {
    pub fn load(path: &Path) -> Self {
        let mut settings = Self::default();
        let Ok(text) = std::fs::read_to_string(path) else { return settings };
        let Ok(Value::Object(data)) = serde_json::from_str::<Value>(&text) else { return settings };
        if let Some(Value::String(icon)) = data.get("app_icon") {
            settings.app_icon = icon.trim().to_owned();
        }
        if let Some(Value::Bool(flag)) = data.get("check_updates") {
            settings.check_updates = *flag;
        }
        if let Some(Value::String(version)) = data.get("skipped_version") {
            settings.skipped_version = version.trim().to_owned();
        }
        if let Some(seconds) = data.get("last_update_check").and_then(Value::as_i64) {
            settings.last_update_check = seconds;
        }
        let text = |key: &str| data.get(key).and_then(Value::as_str).map(|value| value.trim().to_owned());
        if let Some(theme) = text("theme").filter(|theme| theme == "custom" || crate::look::is_preset(theme)) {
            settings.theme = theme;
        }
        for (key, slot) in [
            ("custom_panel", &mut settings.custom_panel),
            ("custom_text", &mut settings.custom_text),
            ("custom_accent", &mut settings.custom_accent),
        ] {
            if let Some(color) = text(key).filter(|color| crate::look::parse_hex(color).is_some()) {
                *slot = color;
            }
        }
        if let Some(style) = text("equalizer").filter(|style| EQUALIZER_STYLES.contains(&style.as_str())) {
            settings.equalizer = style;
        }
        if let Some(Value::Bool(flag)) = data.get("animate") {
            settings.animate = *flag;
        }
        if let Some(Value::Bool(flag)) = data.get("hide_from_sharing") {
            settings.hide_from_sharing = *flag;
        }
        if let Some(key) = text("screenshot_key").filter(|key| crate::hotkey::parse(key).is_some()) {
            settings.screenshot_key = key;
        }
        if let Some(mode) = text("parallel_accounts").filter(|mode| PARALLEL_MODES.contains(&mode.as_str())) {
            settings.parallel_accounts = mode;
        }
        if let Some(seconds) =
            data.get("first_word_seconds").and_then(Value::as_i64).filter(|n| FIRST_WORD_RANGE.contains(n))
        {
            settings.first_word_seconds = seconds;
        }
        if let Some(Value::Bool(flag)) = data.get("tray_icon") {
            settings.tray_icon = *flag;
        }
        if let Some(Value::Bool(flag)) = data.get("hide_from_taskbar") {
            settings.hide_from_taskbar = *flag;
        }
        settings
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let value = json!({
            "app_icon": self.app_icon,
            "check_updates": self.check_updates,
            "skipped_version": self.skipped_version,
            "last_update_check": self.last_update_check,
            "theme": self.theme,
            "custom_panel": self.custom_panel,
            "custom_text": self.custom_text,
            "custom_accent": self.custom_accent,
            "equalizer": self.equalizer,
            "animate": self.animate,
            "hide_from_sharing": self.hide_from_sharing,
            "screenshot_key": self.screenshot_key,
            "parallel_accounts": self.parallel_accounts,
            "first_word_seconds": self.first_word_seconds,
            "tray_icon": self.tray_icon,
            "hide_from_taskbar": self.hide_from_taskbar,
        });
        write_private(path, &serde_json::to_string_pretty(&value).unwrap_or_default())
    }
}

/// Load both files for these paths.
pub fn load_all(paths: &Paths) -> (Settings, AppSettings) {
    (Settings::load(&paths.settings_file()), AppSettings::load(&paths.app_file()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const GROQ: &str = "https://api.groq.com/openai";

    #[test]
    fn the_defaults_match_the_python_overlay() {
        let settings = Settings::default();
        assert_eq!((settings.answer_style.as_str(), settings.answer_layout.as_str()), ("standard", "lines"));
        assert_eq!(
            (settings.whisper_model.as_str(), settings.deepgram_model.as_str(), settings.screenshot_delay_ms),
            ("small", "nova-3", 60)
        );
        assert!(settings.pinned && settings.frameless && settings.save_transcripts && settings.accounts.is_empty());
    }

    #[test]
    fn a_value_is_checked_before_it_is_stored() {
        let mut settings = Settings::default();
        assert!(settings.set("answer_style", json!("detailed")).is_ok());
        assert_eq!(
            settings.set("answer_style", json!("loud")),
            Err(SettingsError::Bad("answer_style must be one of concise, standard, detailed".into()))
        );
        assert_eq!(
            settings.set("opacity", json!(10)),
            Err(SettingsError::Bad("opacity must be between 35 and 100".into()))
        );
        assert_eq!(
            settings.set("pinned", json!("yes")),
            Err(SettingsError::Bad("pinned must be true or false".into()))
        );
        assert_eq!(settings.set("opacity", json!(true)), Err(SettingsError::Bad("opacity must be a number".into())));
        assert_eq!(
            settings.set("gateway_url", json!("not a url")),
            Err(SettingsError::Bad("The gateway URL must start with http:// or https://".into()))
        );
        assert_eq!(settings.set("nope", json!(1)), Err(SettingsError::Unknown("nope".into())));
        assert_eq!(settings.answer_style, "detailed");
    }

    #[test]
    fn settings_round_trip_and_the_file_is_private() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("interview").join("settings.json");
        let mut settings = Settings::default();
        settings.set("accounts", json!([{"name": "default", "url": GROQ, "key": "gsk_secret_one"}])).unwrap();
        settings.set("model", json!("groq/openai/gpt-oss-120b")).unwrap();
        settings.save(&path).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        let loaded = Settings::load(&path);
        assert_eq!(loaded.model, "groq/openai/gpt-oss-120b");
        assert_eq!(
            loaded.accounts,
            [Account { name: "default".into(), url: GROQ.into(), key: "gsk_secret_one".into() }]
        );
    }

    #[test]
    fn a_bad_file_or_a_bad_value_gives_the_defaults_and_unknown_keys_survive() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
        std::fs::write(&path, r#"{"opacity": 5, "model": "m", "future_flag": [1, 2], "screenshot_delay_ms": 250}"#)
            .unwrap();
        let loaded = Settings::load(&path);
        assert_eq!((loaded.opacity, loaded.model.as_str(), loaded.screenshot_delay_ms), (100, "m", 60));
        loaded.save(&path).unwrap();
        assert!(std::fs::read_to_string(&path).unwrap().contains("future_flag"));
    }

    #[test]
    fn a_key_saved_before_accounts_existed_becomes_the_first_account_of_its_address() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, format!(r#"{{"gateway_url": "{GROQ}/v1", "gateway_key": "old-key"}}"#)).unwrap();
        let loaded = Settings::load(&path);
        assert_eq!(loaded.accounts, [Account { name: "default".into(), url: GROQ.into(), key: "old-key".into() }]);
        assert_eq!(loaded.gateway(), GROQ);
    }

    #[test]
    fn the_fallbacks_are_a_list_and_the_keys_keep_the_python_order() {
        let mut settings = Settings::default();
        settings.set("fallback_models", json!("ollama-cloud/gpt-oss:120b, , auto")).unwrap();
        assert_eq!(settings.fallbacks(), ["ollama-cloud/gpt-oss:120b", "auto"]);
        let keys: Vec<String> = settings.to_json().as_object().unwrap().keys().cloned().collect();
        assert_eq!(&keys[..4], ["language", "ui_language", "listen", "call_source"]);
        assert_eq!(keys.last().map(String::as_str), Some("save_transcripts"));
    }

    #[test]
    fn keys_are_added_used_and_removed_for_the_address_in_use() {
        let mut settings = Settings::default();
        settings.set_gateway(GROQ).unwrap();
        for key in ["gsk_one_0001", "gsk_two_0002"] {
            settings.add_account(key).unwrap();
        }
        assert_eq!((settings.gateway_key.as_str(), settings.accounts.len()), ("gsk_one_0001", 2)); // the first is the one in use
        assert_eq!(settings.add_account("gsk_one_0001"), Err(accounts::AccountError::AlreadySaved));
        assert!(
            matches!(settings.use_account("default-2"), Some(AccountChange::Used(account)) if account.key == "gsk_two_0002")
        );
        assert_eq!(settings.gateway_key, "gsk_two_0002");
        settings.remove_account("default-2"); // the one in use: the other takes over
        assert_eq!(settings.gateway_key, "gsk_one_0001");
        settings.remove_account("default");
        assert!(settings.gateway_key.is_empty() && settings.accounts.is_empty() && settings.use_account("x").is_none());
    }

    #[test]
    fn a_key_never_follows_the_window_to_another_address() {
        let mut settings = Settings::default();
        settings.set_gateway(GROQ).unwrap();
        settings.add_account("groq-key-aaaa").unwrap();
        settings.set_gateway("https://proxy.example.com/groq/v1").unwrap();
        assert!(settings.gateway_key.is_empty() && settings.accounts_here().is_empty());
        settings.add_account("proxy-key-bbbb").unwrap();
        settings.set_gateway(GROQ).unwrap();
        assert_eq!(settings.gateway_key, "groq-key-aaaa");
        assert_eq!(
            settings.set_gateway("not a url"),
            Err(SettingsError::Bad("The gateway URL must start with http:// or https://".into()))
        );
    }

    #[test]
    fn app_only_settings_live_in_their_own_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.json");
        assert_eq!(AppSettings::load(&path), AppSettings::default());
        let settings = AppSettings {
            app_icon: "builtin:mint".into(),
            check_updates: false,
            skipped_version: "0.5.1".into(),
            last_update_check: 1_700_000_000,
            theme: "custom".into(),
            custom_panel: "#101820".into(),
            custom_text: "#e0e8f0".into(),
            custom_accent: "#ff7a59".into(),
            equalizer: "background".into(),
            animate: false,
            hide_from_sharing: false,
            screenshot_key: "ctrl + alt + k".into(),
            parallel_accounts: "all".into(),
            first_word_seconds: 8,
            tray_icon: false,
            hide_from_taskbar: false,
        };
        let settings = AppSettings { screenshot_key: "Ctrl+Alt+K".into(), ..settings };
        settings.save(&path).unwrap();
        assert_eq!(AppSettings::load(&path), settings);
    }

    #[test]
    fn a_look_that_cannot_be_read_is_the_default_one() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.json");
        std::fs::write(&path, r##"{"theme": "neon", "custom_accent": "pink", "equalizer": "loud", "animate": "yes", "hide_from_sharing": "no", "screenshot_key": "Enter", "parallel_accounts": "everyone", "first_word_seconds": 600, "tray_icon": "yes", "hide_from_taskbar": 1}"##)
            .unwrap();
        assert_eq!(AppSettings::load(&path), AppSettings::default());
    }
}
