//! What the window listens to and with what: the call and the microphone as sources, Whisper or Deepgram as the
//! recogniser. The session does the listening; this decides what it is given.

use std::sync::Arc;
#[cfg(feature = "local-stt")]
use std::sync::Mutex;
use std::sync::mpsc::Sender;
use std::time::Duration;

use tokio::runtime::Handle;

use crate::audio::{self, AudioError, FollowingSource, FrameSource};
use crate::paths::Paths;
use crate::session::Event;
use crate::settings::Settings;
#[cfg(feature = "local-stt")]
use crate::speech::Transcript;
use crate::speech::{DeepgramTranscriber, Transcriber};
use crate::transcript::{INTERVIEWER, YOU};

/// How often the follower looks at which output plays the call.
const FOLLOW_EVERY: Duration = Duration::from_secs(2);

/// A source that could not be opened: it says why when it is started, so the window can show it.
struct Unavailable(AudioError);

impl FrameSource for Unavailable {
    fn run(&self, _: &mut dyn FnMut(&[i16]) -> bool) -> Result<(), AudioError> {
        Err(self.0.clone())
    }

    fn close(&self) {}
}

fn open(name: &str, microphone: bool) -> Box<dyn FrameSource> {
    audio::open_source(name, microphone).unwrap_or_else(|error| Box::new(Unavailable(error)))
}

/// The call: the output that plays it, followed when it changes (`auto`), or the one source that was named.
pub fn call_source(setting: &str, notify: impl Fn(&str) + Send + Sync + 'static) -> Arc<dyn FrameSource> {
    let named = setting.trim();
    if named.is_empty() || named == "auto" {
        return Arc::new(FollowingSource::new(
            FOLLOW_EVERY,
            notify,
            Box::new(audio::audio_state),
            Box::new(|name| open(name, false)),
        ));
    }
    notify(named);
    Arc::from(open(named, false))
}

/// The sources the settings ask for: (speaker, source). `notify` is told which output is being listened to.
pub fn sources(
    settings: &Settings,
    notify: impl Fn(&str) + Send + Sync + 'static,
) -> Vec<(String, Arc<dyn FrameSource>)> {
    let mut sources: Vec<(String, Arc<dyn FrameSource>)> = Vec::new();
    if settings.listen {
        sources.push((INTERVIEWER.to_owned(), call_source(&settings.call_source, notify)));
    }
    if settings.use_mic {
        sources.push((YOU.to_owned(), Arc::from(open(&settings.mic_source, true))));
    }
    sources
}

/// The recogniser the settings ask for, or what to change when there is none.
pub fn transcriber(
    settings: &Settings,
    paths: &Paths,
    handle: &Handle,
    events: &Sender<Event>,
) -> Result<Arc<dyn Transcriber>, String> {
    let language = crate::languages::normalize(Some(&settings.language));
    if settings.stt_engine == "deepgram" {
        if settings.deepgram_key.trim().is_empty() {
            return Err("Paste the Deepgram key in Settings > AI, or switch the audio engine to Whisper.".to_owned());
        }
        return Ok(Arc::new(DeepgramTranscriber::new(
            settings.deepgram_key.trim(),
            &settings.deepgram_model,
            language.as_deref(),
            handle.clone(),
        )));
    }
    local(settings, paths, handle, events, language.as_deref())
}

#[cfg(feature = "local-stt")]
fn local(
    settings: &Settings,
    paths: &Paths,
    handle: &Handle,
    events: &Sender<Event>,
    language: Option<&str>,
) -> Result<Arc<dyn Transcriber>, String> {
    let model = crate::models::stt_model(&settings.whisper_model)
        .ok_or_else(|| format!("There is no speech model called {:?}.", settings.whisper_model))?;
    Ok(Arc::new(Downloading::new(model, paths.models_dir(), handle.clone(), events.clone(), language)))
}

#[cfg(not(feature = "local-stt"))]
fn local(
    _: &Settings,
    _: &Paths,
    _: &Handle,
    _: &Sender<Event>,
    _: Option<&str>,
) -> Result<Arc<dyn Transcriber>, String> {
    Err("This build has no speech recognition on the computer. Use Deepgram in Settings > AI.".to_owned())
}

/// Whisper that fetches its model first: the session shows `Loading` and the progress while it is downloaded.
#[cfg(feature = "local-stt")]
pub struct Downloading {
    model: &'static crate::models::SttModel,
    dir: std::path::PathBuf,
    handle: Handle,
    events: Mutex<Sender<Event>>,
    language: Mutex<Option<String>>,
    inner: Mutex<Option<Arc<crate::speech::WhisperTranscriber>>>,
}

#[cfg(feature = "local-stt")]
impl Downloading {
    pub fn new(
        model: &'static crate::models::SttModel,
        dir: std::path::PathBuf,
        handle: Handle,
        events: Sender<Event>,
        language: Option<&str>,
    ) -> Self {
        Self {
            model,
            dir,
            handle,
            events: Mutex::new(events),
            language: Mutex::new(language.map(str::to_owned)),
            inner: Mutex::new(None),
        }
    }

    fn say(&self, text: String) {
        if let Ok(events) = self.events.lock() {
            let _ = events.send(Event::Status(text));
        }
    }

    fn recogniser(&self) -> Result<Arc<crate::speech::WhisperTranscriber>, String> {
        self.inner
            .lock()
            .map_err(|_| "the recogniser stopped working".to_owned())?
            .clone()
            .ok_or_else(|| "The speech model is not loaded yet.".to_owned())
    }
}

#[cfg(feature = "local-stt")]
impl Transcriber for Downloading {
    fn load(&self) -> Result<(), String> {
        let name = self.model.id;
        let mut shown = 101u64;
        let path = self
            .handle
            .block_on(crate::models::ensure(self.model, &self.dir, None, |done, total| {
                let percent = (done * 100).checked_div(total).unwrap_or(0);
                if percent != shown {
                    shown = percent;
                    self.say(format!(
                        "Downloading the speech model ({name}, {} MB): {percent}%",
                        self.model.megabytes()
                    ));
                }
            }))
            .map_err(|error| format!("The speech model could not be downloaded: {error}"))?;
        let language = self.language.lock().map_err(|_| "the recogniser stopped working".to_owned())?.clone();
        let whisper = Arc::new(crate::speech::WhisperTranscriber::new(path, language.as_deref()));
        whisper.load()?;
        *self.inner.lock().map_err(|_| "the recogniser stopped working".to_owned())? = Some(whisper);
        Ok(())
    }

    fn transcribe(&self, pcm: &[i16], prompt: &str) -> Result<Transcript, String> {
        self.recogniser()?.transcribe(pcm, prompt)
    }

    fn set_language(&self, language: Option<&str>) {
        if let Ok(mut slot) = self.language.lock() {
            *slot = language.map(str::to_owned);
        }
        if let Ok(inner) = self.inner.lock()
            && let Some(inner) = inner.as_ref()
        {
            inner.set_language(language);
        }
    }

    fn device(&self) -> String {
        self.recogniser().map_or_else(|_| "cpu".to_owned(), |inner| inner.device())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    fn settings() -> Settings {
        Settings::default()
    }

    #[test]
    fn nothing_is_listened_to_when_both_are_off_and_the_call_is_named_by_its_speaker() {
        let mut settings = settings();
        settings.listen = false;
        settings.use_mic = false;
        assert!(sources(&settings, |_| {}).is_empty());
        settings.listen = true;
        settings.use_mic = true;
        settings.mic_source = "nonexistent-mic".into();
        let found = sources(&settings, |_| {});
        assert_eq!(found.iter().map(|(speaker, _)| speaker.as_str()).collect::<Vec<_>>(), [INTERVIEWER, YOU]);
    }

    #[test]
    fn a_source_that_cannot_be_opened_says_why_when_it_is_started() {
        let source = Unavailable(AudioError::new("no such device"));
        let error = source.run(&mut |_| true).unwrap_err();
        assert!(error.to_string().contains("no such device"));
    }

    #[test]
    fn deepgram_needs_its_key_and_says_where_to_put_it() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let (tx, _rx) = channel();
        let mut settings = settings();
        settings.stt_engine = "deepgram".into();
        settings.deepgram_key = "  ".into();
        let paths = Paths::under("/nowhere/data");
        let error = transcriber(&settings, &paths, runtime.handle(), &tx).err().unwrap();
        assert!(error.contains("Settings > AI"), "{error}");
        settings.deepgram_key = "dg-key".into();
        assert_eq!(transcriber(&settings, &paths, runtime.handle(), &tx).unwrap().device(), "deepgram");
    }

    #[cfg(feature = "local-stt")]
    #[test]
    fn an_unknown_model_is_refused_by_name() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let (tx, _rx) = channel();
        let mut settings = settings();
        settings.stt_engine = "whisper".into();
        settings.whisper_model = "huge".into();
        let error = transcriber(&settings, &Paths::under("/nowhere/data"), runtime.handle(), &tx).err().unwrap();
        assert!(error.contains("huge"));
    }
}
