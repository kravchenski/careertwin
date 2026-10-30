//! Audio capture: PulseAudio or PipeWire (`parec`) on Linux, the output in loopback mode and the microphone
//! through WASAPI on Windows. Frames are 20 ms of mono 16 kHz PCM16.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use thiserror::Error;

use crate::vad::{FRAME_MS, FRAME_SAMPLES};

#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("{0}")]
pub struct AudioError(pub String);

impl AudioError {
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }
}

/// The two things the window can hear: the call and, if asked, the person at the computer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sources {
    pub interviewer: String,
    pub microphone: String,
}

/// Something that hands over frames until it is closed. `run` returns when `on_frame` says stop, when the source
/// ends (a device went away) or when `close` is called from another thread.
pub trait FrameSource: Send + Sync {
    fn run(&self, on_frame: &mut dyn FnMut(&[i16]) -> bool) -> Result<(), AudioError>;
    fn close(&self);
}

/// Which monitor to listen to: stay on one that is playing, else follow what plays, else the default.
pub fn next_monitor(current: Option<&str>, default: &str, playing: &[String], last_default: Option<&str>) -> String {
    if let Some(current) = current.filter(|current| playing.iter().any(|name| name == current)) {
        return current.to_owned();
    }
    if !playing.is_empty() {
        return if playing.iter().any(|name| name == default) { default.to_owned() } else { playing[0].clone() };
    }
    match current {
        Some(current) if last_default == Some(default) => current.to_owned(),
        _ => default.to_owned(),
    }
}

/// What the follower asks about the audio of the computer: (the monitor of the default output, the monitors of the
/// outputs that are playing something now).
pub type StateFn = dyn Fn() -> Result<(String, Vec<String>), AudioError> + Send + Sync;
pub type SpawnFn = dyn Fn(&str) -> Box<dyn FrameSource> + Send + Sync;

/// Call audio that follows the output actually playing the call.
///
/// A laptop with a headset has two outputs. The call plays on whichever Zoom or Meet picked, and the system default
/// can change when a headset is plugged in, so a monitor fixed at start can be silent. This source re-checks every
/// `poll` and moves to the right monitor; it moves only after two checks agree, so a notification sound on another
/// output does not make it jump.
pub struct FollowingSource {
    poll: Duration,
    on_change: Box<dyn Fn(&str) + Send + Sync>,
    state: Box<StateFn>,
    spawn: Box<SpawnFn>,
    shared: Arc<Mutex<Follow>>,
    stop: Arc<AtomicBool>,
}

#[derive(Default)]
struct Follow {
    current: Option<String>,
    target: Option<String>,
    pending: Option<String>,
    last_default: Option<String>,
    switching: bool,
    source: Option<Arc<Box<dyn FrameSource>>>,
}

impl FollowingSource {
    pub fn new(
        poll: Duration,
        on_change: impl Fn(&str) + Send + Sync + 'static,
        state: Box<StateFn>,
        spawn: Box<SpawnFn>,
    ) -> Self {
        Self { poll, on_change: Box::new(on_change), state, spawn, shared: Arc::default(), stop: Arc::default() }
    }

    /// The monitor being listened to, once it has started.
    pub fn current(&self) -> Option<String> {
        self.shared.lock().ok().and_then(|shared| shared.current.clone())
    }

    /// One look at the audio: whether to move, after two looks that agree.
    fn watch_once(&self) {
        let Ok((default, playing)) = (self.state)() else { return };
        let Ok(mut shared) = self.shared.lock() else { return };
        let wanted = next_monitor(shared.current.as_deref(), &default, &playing, shared.last_default.as_deref());
        if Some(&wanted) == shared.current.as_ref() {
            shared.pending = None;
            shared.last_default = Some(default);
        } else if shared.pending.as_ref() == Some(&wanted) {
            shared.pending = None;
            shared.target = Some(wanted);
            shared.last_default = Some(default);
            shared.switching = true;
            if let Some(source) = shared.source.clone() {
                source.close();
            }
        } else {
            shared.pending = Some(wanted);
        }
    }
}

impl FrameSource for FollowingSource {
    fn run(&self, on_frame: &mut dyn FnMut(&[i16]) -> bool) -> Result<(), AudioError> {
        self.stop.store(false, Ordering::SeqCst);
        let (default, playing) = (self.state)()?;
        let start = next_monitor(None, &default, &playing, None);
        {
            let mut shared = self.shared.lock().map_err(|_| AudioError::new("audio state is poisoned"))?;
            shared.last_default = Some(default);
            shared.current = Some(start.clone());
        }
        (self.on_change)(&start);
        let watcher_stop = self.stop.clone();
        let poll = self.poll;
        std::thread::scope(|scope| {
            scope.spawn(|| {
                loop {
                    // sleep in small steps so that a close is noticed at once
                    let mut waited = Duration::ZERO;
                    while waited < poll {
                        if watcher_stop.load(Ordering::SeqCst) {
                            return;
                        }
                        std::thread::sleep(Duration::from_millis(20).min(poll));
                        waited += Duration::from_millis(20).max(Duration::from_millis(1));
                    }
                    self.watch_once();
                }
            });
            let mut result = Ok(());
            let mut keep_going = true;
            while keep_going && !self.stop.load(Ordering::SeqCst) {
                let current = self.shared.lock().ok().and_then(|shared| shared.current.clone()).unwrap_or_default();
                let source: Arc<Box<dyn FrameSource>> = Arc::new((self.spawn)(&current));
                if let Ok(mut shared) = self.shared.lock() {
                    shared.source = Some(source.clone());
                }
                let outcome = source.run(&mut |frame| {
                    let wanted = !self.stop.load(Ordering::SeqCst)
                        && !self.shared.lock().map(|shared| shared.switching).unwrap_or(false);
                    wanted && {
                        keep_going = on_frame(frame);
                        keep_going
                    }
                });
                source.close();
                if self.stop.load(Ordering::SeqCst) || !keep_going {
                    result = outcome;
                    break;
                }
                let moved = self.shared.lock().ok().and_then(|mut shared| {
                    if shared.switching {
                        shared.switching = false;
                        shared.current = shared.target.clone();
                        shared.current.clone()
                    } else {
                        None
                    }
                });
                match moved {
                    Some(name) => (self.on_change)(&name),
                    None => std::thread::sleep(Duration::from_millis(500)), // the source ended by itself: try again
                }
            }
            self.stop.store(true, Ordering::SeqCst); // lets the watcher end
            result
        })
    }

    fn close(&self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Ok(shared) = self.shared.lock()
            && let Some(source) = shared.source.clone()
        {
            source.close();
        }
    }
}

/// A source fed by a thread of its own (a sound card callback): frames come through a channel, and when the device
/// sends nothing (a loopback stream is silent while nothing plays) the source makes silent frames itself, because the
/// speech detector has to see silence to know a sentence is over.
pub struct ChannelSource {
    frames: Mutex<Option<Receiver<Vec<i16>>>>,
    closed: Arc<AtomicBool>,
    fill_silence: bool,
}

impl ChannelSource {
    pub fn new(frames: Receiver<Vec<i16>>, fill_silence: bool) -> Self {
        Self { frames: Mutex::new(Some(frames)), closed: Arc::default(), fill_silence }
    }
}

impl FrameSource for ChannelSource {
    fn run(&self, on_frame: &mut dyn FnMut(&[i16]) -> bool) -> Result<(), AudioError> {
        let Some(frames) = self.frames.lock().ok().and_then(|mut slot| slot.take()) else { return Ok(()) };
        let silence = vec![0i16; FRAME_SAMPLES];
        while !self.closed.load(Ordering::SeqCst) {
            match frames.recv_timeout(Duration::from_millis(FRAME_MS as u64 * 2)) {
                Ok(frame) => {
                    if !on_frame(&frame) {
                        return Ok(());
                    }
                }
                Err(RecvTimeoutError::Timeout) if self.fill_silence => {
                    if !on_frame(&silence) {
                        return Ok(());
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return Ok(()), // the device went away
            }
        }
        Ok(())
    }

    fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }
}

/// Floats in [-1, 1] to 16-bit samples.
pub fn to_pcm16(samples: &[f32]) -> Vec<i16> {
    samples.iter().map(|sample| (sample.clamp(-1.0, 1.0) * 32767.0) as i16).collect()
}

/// Cuts a stream of samples into whole frames, keeping the rest for the next call.
#[derive(Default)]
pub struct Framer {
    pending: VecDeque<i16>,
}

impl Framer {
    pub fn push(&mut self, samples: &[i16], out: &Sender<Vec<i16>>) {
        self.pending.extend(samples);
        while self.pending.len() >= FRAME_SAMPLES {
            let frame: Vec<i16> = self.pending.drain(..FRAME_SAMPLES).collect();
            // a listener that cannot keep up loses audio rather than stalling the sound card
            let _ = out.send(frame);
        }
    }
}

/// Mono samples at `from` Hz to `to` Hz; averaging when going down, interpolating when going up.
pub fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if input.is_empty() || from == 0 || to == 0 {
        return Vec::new();
    }
    if from == to {
        return input.to_vec();
    }
    let ratio = f64::from(from) / f64::from(to);
    let out_len = ((input.len() as f64) / ratio).floor() as usize;
    (0..out_len)
        .map(|n| {
            let position = n as f64 * ratio;
            if ratio > 1.0 {
                let start = position.floor() as usize;
                let end = ((position + ratio).ceil() as usize).min(input.len()).max(start + 1);
                input[start..end].iter().sum::<f32>() / (end - start) as f32
            } else {
                let index = position.floor() as usize;
                let fraction = (position - index as f64) as f32;
                let a = input[index];
                let b = input.get(index + 1).copied().unwrap_or(a);
                a + (b - a) * fraction
            }
        })
        .collect()
}

/// Interleaved channels to one by averaging.
pub fn to_mono(samples: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return samples.to_vec();
    }
    samples.chunks(channels).map(|frame| frame.iter().sum::<f32>() / frame.len() as f32).collect()
}

// ---- Linux: PulseAudio and PipeWire ---------------------------------------------------------------------------------

#[cfg(all(unix, not(target_os = "macos")))]
pub use pulse::{
    ParecSource, audio_state, default_sources, list_sources, open_source, parse_audio_state, parse_short_sources,
};

#[cfg(all(unix, not(target_os = "macos")))]
mod pulse {
    use std::io::Read;
    use std::process::{Child, Command, Stdio};

    use serde_json::Value;

    use super::{AtomicBool, AudioError, Duration, FrameSource, Mutex, Ordering, Sources};
    use crate::screenshot::on_path;
    use crate::vad::{FRAME_BYTES, SAMPLE_RATE};

    fn pactl(args: &[&str]) -> Result<String, AudioError> {
        if cfg!(target_os = "macos") {
            return Err(AudioError::new(
                "Call audio is not available on macOS yet. Type a question, or use the Screenshot button.",
            ));
        }
        if !on_path("pactl") {
            return Err(AudioError::new("`pactl` not found. Install pipewire-pulse (or pulseaudio-utils)."));
        }
        let output = Command::new("pactl")
            .args(args)
            .stdin(Stdio::null())
            .output()
            .map_err(|error| AudioError::new(format!("pactl: {error}")))?;
        if !output.status.success() {
            return Err(AudioError::new(format!(
                "pactl {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }

    /// The names of `pactl list short sources`: tab separated, the name is the second field.
    pub fn parse_short_sources(text: &str) -> Vec<String> {
        text.lines().filter_map(|line| line.split('\t').nth(1)).map(str::to_owned).collect()
    }

    pub fn list_sources() -> Result<Vec<String>, AudioError> {
        Ok(parse_short_sources(&pactl(&["list", "short", "sources"])?))
    }

    /// The sink monitor carries the call audio; the default source is the microphone.
    pub fn default_sources() -> Result<Sources, AudioError> {
        Ok(Sources {
            interviewer: format!("{}.monitor", pactl(&["get-default-sink"])?),
            microphone: pactl(&["get-default-source"])?,
        })
    }

    /// The monitors of the outputs that have a playing (not corked) stream, from `pactl --format=json` lists.
    pub fn parse_audio_state(default: &str, sinks: &str, inputs: &str) -> (String, Vec<String>) {
        let default = format!("{default}.monitor");
        let (Ok(Value::Array(sinks)), Ok(Value::Array(inputs))) =
            (serde_json::from_str::<Value>(sinks), serde_json::from_str::<Value>(inputs))
        else {
            return (default, Vec::new());
        };
        let mut playing: Vec<String> = Vec::new();
        for stream in &inputs {
            if stream.get("corked").and_then(Value::as_bool).unwrap_or(false) {
                continue;
            }
            let Some(sink) = sinks.iter().find(|sink| sink.get("index") == stream.get("sink")) else { continue };
            let monitor = sink
                .get("monitor_source")
                .and_then(Value::as_str)
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
                .or_else(|| sink.get("name").and_then(Value::as_str).map(|name| format!("{name}.monitor")));
            if let Some(monitor) = monitor.filter(|monitor| !playing.contains(monitor)) {
                playing.push(monitor);
            }
        }
        (default, playing)
    }

    pub fn audio_state() -> Result<(String, Vec<String>), AudioError> {
        let default = pactl(&["get-default-sink"])?;
        let lists = pactl(&["--format=json", "list", "sinks"])
            .and_then(|sinks| Ok((sinks, pactl(&["--format=json", "list", "sink-inputs"])?)));
        Ok(match lists {
            Ok((sinks, inputs)) => parse_audio_state(&default, &sinks, &inputs),
            Err(_) => (format!("{default}.monitor"), Vec::new()),
        })
    }

    /// Reads 20 ms frames of mono 16 kHz PCM16 from a Pulse source.
    pub struct ParecSource {
        source: String,
        child: Mutex<Option<Child>>,
        closed: AtomicBool,
    }

    impl ParecSource {
        pub fn new(source: &str) -> Self {
            Self { source: source.to_owned(), child: Mutex::new(None), closed: AtomicBool::new(false) }
        }
    }

    impl FrameSource for ParecSource {
        fn run(&self, on_frame: &mut dyn FnMut(&[i16]) -> bool) -> Result<(), AudioError> {
            if !on_path("parec") {
                return Err(AudioError::new("`parec` not found. Install pipewire-pulse (or pulseaudio-utils)."));
            }
            let mut child = Command::new("parec")
                .args([
                    &format!("--device={}", self.source),
                    &format!("--rate={SAMPLE_RATE}"),
                    "--channels=1",
                    "--format=s16le",
                    "--latency-msec=20",
                ])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|error| AudioError::new(format!("could not start parec: {error}")))?;
            let mut stdout = child.stdout.take().ok_or_else(|| AudioError::new("parec has no output"))?;
            if let Ok(mut slot) = self.child.lock() {
                *slot = Some(child);
            }
            let mut bytes = [0u8; FRAME_BYTES];
            let mut samples = [0i16; FRAME_BYTES / 2];
            while !self.closed.load(Ordering::SeqCst) {
                if stdout.read_exact(&mut bytes).is_err() {
                    break;
                }
                for (slot, pair) in samples.iter_mut().zip(bytes.as_chunks::<2>().0) {
                    *slot = i16::from_le_bytes([pair[0], pair[1]]);
                }
                if !on_frame(&samples) {
                    break;
                }
            }
            self.close();
            Ok(())
        }

        fn close(&self) {
            self.closed.store(true, Ordering::SeqCst);
            if let Ok(mut slot) = self.child.lock()
                && let Some(mut child) = slot.take()
            {
                let _ = child.kill();
                let _ = child.wait();
            }
            let _ = Duration::ZERO;
        }
    }

    /// A source for a device name: a Pulse source. (`microphone` makes no difference here: a microphone is a source
    /// too.)
    pub fn open_source(name: &str, _microphone: bool) -> Result<Box<dyn FrameSource>, AudioError> {
        Ok(Box::new(ParecSource::new(name)))
    }
}

// ---- Windows: WASAPI through cpal ------------------------------------------------------------------------------------

#[cfg(any(windows, target_os = "macos"))]
pub use sound_card::{audio_state, default_sources, list_sources, open_source};

#[cfg(any(windows, target_os = "macos"))]
mod sound_card {
    use anyhow::{Context, anyhow};
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    use std::sync::mpsc::channel;

    use super::*;
    use crate::vad::SAMPLE_RATE;

    fn host() -> cpal::Host {
        cpal::default_host()
    }

    /// The outputs (each can be heard in loopback on Windows), then the microphones not already listed.
    pub fn list_sources() -> Result<Vec<String>, AudioError> {
        let host = host();
        let mut names: Vec<String> = if cfg!(windows) {
            host.output_devices().map(|devices| devices.map(|d| d.to_string()).collect()).unwrap_or_default()
        } else {
            Vec::new()
        };
        for name in
            host.input_devices().map(|devices| devices.map(|d| d.to_string()).collect::<Vec<_>>()).unwrap_or_default()
        {
            if !names.contains(&name) {
                names.push(name);
            }
        }
        Ok(names)
    }

    pub fn default_sources() -> Result<Sources, AudioError> {
        let host = host();
        let interviewer = if cfg!(windows) {
            host.default_output_device()
                .map(|device| device.to_string())
                .ok_or_else(|| AudioError::new("Windows audio devices are not available: no output"))?
        } else {
            String::new()
        };
        Ok(Sources {
            interviewer,
            microphone: host.default_input_device().map(|device| device.to_string()).unwrap_or_default(),
        })
    }

    /// Windows does not say which output plays: only the default one is known.
    pub fn audio_state() -> Result<(String, Vec<String>), AudioError> {
        let name = host()
            .default_output_device()
            .map(|device| device.to_string())
            .ok_or_else(|| AudioError::new("No audio output"))?;
        Ok((name, Vec::new()))
    }

    fn find(name: &str, loopback: bool) -> Result<cpal::Device, AudioError> {
        let host = host();
        if name.trim().is_empty() {
            let device = if loopback { host.default_output_device() } else { host.default_input_device() };
            return device.ok_or_else(|| {
                AudioError::new(if loopback { "No audio output found" } else { "No microphone found" })
            });
        }
        let devices: Vec<cpal::Device> = if loopback {
            host.output_devices().map(Iterator::collect).unwrap_or_default()
        } else {
            host.input_devices().map(Iterator::collect).unwrap_or_default()
        };
        let wanted = name.to_lowercase();
        devices
            .iter()
            .find(|device| device.to_string() == name)
            .or_else(|| devices.iter().find(|device| device.to_string().to_lowercase().contains(&wanted)))
            .cloned()
            .ok_or_else(|| {
                AudioError::new(format!(
                    "No {} called {name:?}. Pick one in Settings → General.",
                    if loopback { "output" } else { "microphone" }
                ))
            })
    }

    fn build(device: cpal::Device, out: Sender<Vec<i16>>) -> anyhow::Result<cpal::Stream> {
        let supported = device.default_input_config().map_err(|error| anyhow!("the device cannot be used: {error}"))?;
        let config = supported.config();
        let (rate, channels) = (config.sample_rate, usize::from(config.channels));
        let mut framer = Framer::default();
        let mut carry: Vec<f32> = Vec::new();
        let mut feed = move |mono: Vec<f32>| {
            carry.extend(mono);
            let converted = resample(&carry, rate, SAMPLE_RATE);
            // the resampler needs a little look-ahead: what it could not use yet stays for the next callback
            let used = ((converted.len() as f64) * f64::from(rate) / f64::from(SAMPLE_RATE)).floor() as usize;
            carry.drain(..used.min(carry.len()));
            framer.push(&to_pcm16(&converted), &out);
        };
        let on_error = |error| eprintln!("audio error: {error}");
        let stream = match supported.sample_format() {
            cpal::SampleFormat::F32 => device.build_input_stream(
                config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| feed(to_mono(data, channels)),
                on_error,
                None,
            ),
            cpal::SampleFormat::I16 => device.build_input_stream(
                config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    let floats: Vec<f32> = data.iter().map(|&s| f32::from(s) / 32768.0).collect();
                    feed(to_mono(&floats, channels));
                },
                on_error,
                None,
            ),
            cpal::SampleFormat::U16 => device.build_input_stream(
                config,
                move |data: &[u16], _: &cpal::InputCallbackInfo| {
                    let floats: Vec<f32> = data.iter().map(|&s| (f32::from(s) - 32768.0) / 32768.0).collect();
                    feed(to_mono(&floats, channels));
                },
                on_error,
                None,
            ),
            other => return Err(anyhow!("the device uses the sample format {other:?}, which is not supported")),
        }
        .map_err(|error| anyhow!("could not open the device: {error}"))?;
        stream.play().map_err(|error| anyhow!("could not start the device: {error}"))?;
        Ok(stream)
    }

    /// An output in loopback mode (the call) or a microphone, opened when the source starts running.
    struct CardSource {
        name: String,
        loopback: bool,
        closed: Arc<AtomicBool>,
    }

    impl FrameSource for CardSource {
        fn run(&self, on_frame: &mut dyn FnMut(&[i16]) -> bool) -> Result<(), AudioError> {
            let (frames_tx, frames_rx) = channel::<Vec<i16>>();
            let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
            let (name, loopback, closed) = (self.name.clone(), self.loopback, self.closed.clone());
            std::thread::Builder::new()
                .name("careertwin-audio".into())
                .spawn(move || {
                    // the stream lives on this thread: a cpal stream is not Send everywhere
                    let result = find(&name, loopback)
                        .map_err(|error| error.0)
                        .and_then(|device| build(device, frames_tx).map_err(|error| error.to_string()));
                    match result {
                        Ok(stream) => {
                            let _ = ready_tx.send(Ok(()));
                            while !closed.load(Ordering::SeqCst) {
                                std::thread::sleep(Duration::from_millis(50));
                            }
                            drop(stream);
                        }
                        Err(error) => {
                            let _ = ready_tx.send(Err(error));
                        }
                    }
                })
                .context("could not start the audio thread")
                .map_err(|error| AudioError::new(error.to_string()))?;
            ready_rx.recv().map_err(|_| AudioError::new("the audio thread stopped"))?.map_err(AudioError::new)?;
            ChannelSource {
                frames: Mutex::new(Some(frames_rx)),
                closed: self.closed.clone(),
                fill_silence: self.loopback,
            }
            .run(on_frame)
        }

        fn close(&self) {
            self.closed.store(true, Ordering::SeqCst);
        }
    }

    pub fn open_source(name: &str, microphone: bool) -> Result<Box<dyn FrameSource>, AudioError> {
        if !microphone && !cfg!(windows) {
            return Err(AudioError::new(
                "Call audio is not available on macOS yet. Type a question, or use the Screenshot button.",
            ));
        }
        Ok(Box::new(CardSource { name: name.to_owned(), loopback: !microphone, closed: Arc::default() }))
    }
}

/// Open the call audio for a moment and return the loudest sample heard: proof that capture works.
pub fn listen_briefly(source: &dyn FrameSource, seconds: f32) -> Result<i16, AudioError> {
    let deadline = std::time::Instant::now() + Duration::from_secs_f32(seconds);
    let mut peak = 0i16;
    source.run(&mut |frame| {
        peak = peak.max(frame.iter().map(|sample| sample.saturating_abs()).max().unwrap_or(0));
        std::time::Instant::now() < deadline
    })?;
    Ok(peak)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::sync::mpsc::channel;

    fn names(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_owned()).collect()
    }

    #[test]
    fn the_follower_stays_on_a_monitor_that_plays_and_follows_what_plays() {
        let playing = names(&["usb.monitor"]);
        assert_eq!(
            next_monitor(Some("usb.monitor"), "speaker.monitor", &playing, Some("speaker.monitor")),
            "usb.monitor"
        ); // stays
        assert_eq!(
            next_monitor(Some("speaker.monitor"), "speaker.monitor", &playing, Some("speaker.monitor")),
            "usb.monitor"
        ); // follows
        let both = names(&["usb.monitor", "speaker.monitor"]);
        assert_eq!(next_monitor(None, "speaker.monitor", &both, None), "speaker.monitor"); // the default when it is among them
        assert_eq!(next_monitor(None, "speaker.monitor", &[], None), "speaker.monitor");
        // nothing plays: stay where it is, unless the default changed (a headset was plugged in)
        assert_eq!(next_monitor(Some("old.monitor"), "speaker.monitor", &[], Some("speaker.monitor")), "old.monitor");
        assert_eq!(
            next_monitor(Some("old.monitor"), "headset.monitor", &[], Some("speaker.monitor")),
            "headset.monitor"
        );
    }

    #[test]
    fn frames_are_cut_whole_and_the_rest_waits() {
        let (tx, rx) = channel();
        let mut framer = Framer::default();
        framer.push(&vec![1; FRAME_SAMPLES + 10], &tx);
        assert_eq!(rx.try_recv().unwrap().len(), FRAME_SAMPLES);
        assert!(rx.try_recv().is_err());
        framer.push(&vec![1; FRAME_SAMPLES - 10], &tx);
        assert_eq!(rx.try_recv().unwrap().len(), FRAME_SAMPLES);
    }

    #[test]
    fn samples_are_converted_mixed_and_resampled() {
        assert_eq!(to_pcm16(&[0.0, 1.0, -1.0, 2.0]), [0, 32767, -32767, 32767]);
        assert_eq!(to_mono(&[0.2, 0.4, 1.0, 0.0], 2), [0.3, 0.5]);
        let down = resample(&vec![0.5; 4800], 48_000, 16_000);
        assert_eq!(down.len(), 1600);
        assert!(down.iter().all(|sample| (sample - 0.5).abs() < 1e-6));
        assert_eq!(resample(&[0.0, 1.0], 8_000, 16_000).len(), 4);
        assert!(resample(&[], 48_000, 16_000).is_empty() && resample(&[1.0], 0, 16_000).is_empty());
    }

    #[test]
    fn a_silent_device_still_gives_the_detector_silence() {
        let (tx, rx) = channel::<Vec<i16>>();
        let source = ChannelSource::new(rx, true);
        tx.send(vec![5; FRAME_SAMPLES]).unwrap();
        let (mut loud, mut silent) = (0, 0);
        source
            .run(&mut |frame| {
                if frame.iter().any(|sample| *sample != 0) {
                    loud += 1;
                } else {
                    silent += 1;
                }
                silent < 3
            })
            .unwrap();
        assert_eq!((loud, silent), (1, 3));
        drop(tx);
        let (tx2, rx2) = channel::<Vec<i16>>();
        drop(tx2);
        assert!(ChannelSource::new(rx2, true).run(&mut |_| true).is_ok()); // a device that went away ends the source
    }

    #[test]
    fn a_closed_source_stops() {
        let (_tx, rx) = channel::<Vec<i16>>();
        let source = Arc::new(ChannelSource::new(rx, false));
        let runner = source.clone();
        let handle = std::thread::spawn(move || runner.run(&mut |_| true));
        std::thread::sleep(Duration::from_millis(60));
        source.close();
        assert!(handle.join().unwrap().is_ok());
    }

    /// A source of a fixed list of frames; it ends when they are used.
    struct Fixed {
        label: String,
        frames: usize,
        started: Arc<AtomicUsize>,
        closed: AtomicBool,
    }

    impl FrameSource for Fixed {
        fn run(&self, on_frame: &mut dyn FnMut(&[i16]) -> bool) -> Result<(), AudioError> {
            self.started.fetch_add(1, Ordering::SeqCst);
            let marker = if self.label.starts_with("usb") { 9 } else { 1 };
            for _ in 0..self.frames {
                if self.closed.load(Ordering::SeqCst) || !on_frame(&vec![marker; FRAME_SAMPLES]) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            // a real source would go on; this one waits to be closed
            while !self.closed.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(5));
            }
            Ok(())
        }

        fn close(&self) {
            self.closed.store(true, Ordering::SeqCst);
        }
    }

    #[test]
    fn the_follower_moves_to_the_output_that_plays_after_two_checks_that_agree() {
        let state = Arc::new(Mutex::new(("speaker.monitor".to_owned(), Vec::<String>::new())));
        let announced: Arc<Mutex<Vec<String>>> = Arc::default();
        let started = Arc::new(AtomicUsize::new(0));
        let (state_read, announce, count) = (state.clone(), announced.clone(), started.clone());
        let follower = Arc::new(FollowingSource::new(
            Duration::from_millis(30),
            move |name| announce.lock().unwrap().push(name.to_owned()),
            Box::new(move || Ok(state_read.lock().unwrap().clone())),
            Box::new(move |name| {
                Box::new(Fixed {
                    label: name.to_owned(),
                    frames: 1000,
                    started: count.clone(),
                    closed: AtomicBool::new(false),
                })
            }),
        ));
        let heard: Arc<Mutex<Vec<i16>>> = Arc::default();
        let (runner, record) = (follower.clone(), heard.clone());
        let handle = std::thread::spawn(move || {
            runner.run(&mut |frame| {
                record.lock().unwrap().push(frame[0]);
                true
            })
        });
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(follower.current().as_deref(), Some("speaker.monitor"));
        *state.lock().unwrap() = ("speaker.monitor".into(), vec!["usb.monitor".into()]); // the call plays on the headset
        for _ in 0..100 {
            if follower.current().as_deref() == Some("usb.monitor") {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(follower.current().as_deref(), Some("usb.monitor"));
        std::thread::sleep(Duration::from_millis(100));
        follower.close();
        handle.join().unwrap().unwrap();
        assert_eq!(*announced.lock().unwrap(), ["speaker.monitor", "usb.monitor"]);
        let heard = heard.lock().unwrap();
        assert!(heard.contains(&1) && heard.contains(&9) && started.load(Ordering::SeqCst) == 2);
    }

    #[test]
    fn a_short_notification_on_another_output_does_not_move_the_follower() {
        let state = Arc::new(Mutex::new(("speaker.monitor".to_owned(), Vec::<String>::new())));
        let (state_read, started) = (state.clone(), Arc::new(AtomicUsize::new(0)));
        let count = started.clone();
        let follower = Arc::new(FollowingSource::new(
            Duration::from_millis(40),
            |_| {},
            Box::new(move || Ok(state_read.lock().unwrap().clone())),
            Box::new(move |name| {
                Box::new(Fixed {
                    label: name.to_owned(),
                    frames: 1000,
                    started: count.clone(),
                    closed: AtomicBool::new(false),
                })
            }),
        ));
        let runner = follower.clone();
        let handle = std::thread::spawn(move || runner.run(&mut |_| true));
        std::thread::sleep(Duration::from_millis(60));
        *state.lock().unwrap() = ("speaker.monitor".into(), vec!["usb.monitor".into()]); // one check sees it...
        std::thread::sleep(Duration::from_millis(50));
        *state.lock().unwrap() = ("speaker.monitor".into(), Vec::new()); // ...the next does not
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(follower.current().as_deref(), Some("speaker.monitor"));
        follower.close();
        handle.join().unwrap().unwrap();
        assert_eq!(started.load(Ordering::SeqCst), 1);
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn pulse_lists_are_read() {
        assert_eq!(
            parse_short_sources(
                "1\talsa_output.speaker.monitor\tmodule\ts16le 2ch\tIDLE\n2\talsa_input.mic\tmodule\ts16le\tRUNNING\nbroken"
            ),
            ["alsa_output.speaker.monitor", "alsa_input.mic"]
        );
        let sinks =
            r#"[{"index": 1, "name": "speaker", "monitor_source": "speaker.monitor"}, {"index": 2, "name": "usb"}]"#;
        let inputs = r#"[{"sink": 2, "corked": false}, {"sink": 1, "corked": true}, {"sink": 2}, {"sink": 99}]"#;
        let (default, playing) = parse_audio_state("speaker", sinks, inputs);
        assert_eq!((default.as_str(), playing), ("speaker.monitor", vec!["usb.monitor".to_owned()])); // a missing monitor name is made from the sink's
        assert_eq!(parse_audio_state("speaker", "not json", "[]"), ("speaker.monitor".to_owned(), vec![]));
    }

    #[test]
    fn listening_briefly_returns_the_loudest_sample() {
        let (tx, rx) = channel::<Vec<i16>>();
        for level in [10, 900, 40] {
            tx.send(vec![level; FRAME_SAMPLES]).unwrap();
        }
        assert_eq!(listen_briefly(&ChannelSource::new(rx, true), 0.15).unwrap(), 900);
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn a_source_that_runs_a_missing_program_says_so() {
        let source = ParecSource::new("nothing");
        if !crate::screenshot::on_path("parec") {
            assert!(source.run(&mut |_| true).unwrap_err().0.contains("parec"));
        }
    }
}
