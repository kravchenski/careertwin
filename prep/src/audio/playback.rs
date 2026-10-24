//! Playing the interviewer's voice, and stopping it the instant you speak over it.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use super::resample::{resample, to_mono};

/// What the conversation needs from a speaker. Tests stand in a fake one.
pub trait Speaker: Send + Sync {
    /// Queues mono audio to play after what is already queued.
    fn enqueue(&self, pcm: Vec<f32>, sample_rate: u32);
    /// Drops everything queued and goes quiet at once.
    fn stop(&self);
    /// How much audio is still waiting to be played.
    fn queued(&self) -> Duration;
    /// How loud the output is right now, 0 to 1.
    fn level(&self) -> f32;
}

struct Shared {
    samples: Mutex<VecDeque<f32>>,
    device_rate: u32,
    level: AtomicU32,
}

/// The real speakers. The stream lives on its own thread because some platforms do not let it
/// move between threads.
pub struct Player {
    shared: Arc<Shared>,
    _stop: mpsc::Sender<()>,
}

impl Player {
    /// `device` is a name from [`output_devices`]; empty means the system default.
    pub fn open(device: &str) -> Result<Self> {
        let (ready_tx, ready_rx) = mpsc::channel::<Result<Arc<Shared>>>();
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let wanted = device.to_owned();
        std::thread::Builder::new()
            .name("prep-speaker".into())
            .spawn(move || match build(&wanted) {
                Ok((stream, shared)) => {
                    let _ = ready_tx.send(Ok(shared));
                    let _ = stop_rx.recv(); // keeps the stream alive until the player is dropped
                    drop(stream);
                }
                Err(error) => {
                    let _ = ready_tx.send(Err(error));
                }
            })
            .context("could not start the audio thread")?;
        let shared = ready_rx.recv().map_err(|_| anyhow!("the audio thread stopped"))??;
        Ok(Self { shared, _stop: stop_tx })
    }
}

fn find_output(name: &str) -> Result<cpal::Device> {
    let host = cpal::default_host();
    if name.trim().is_empty() {
        return host.default_output_device().context("no speakers found");
    }
    host.output_devices()
        .map_err(|e| anyhow!("could not list the speakers: {e}"))?
        .find(|device| device.to_string() == name)
        .with_context(|| format!("no speakers named {name}"))
}

pub fn output_devices() -> Vec<String> {
    let host = cpal::default_host();
    host.output_devices().map(|devices| devices.map(|d| d.to_string()).collect()).unwrap_or_default()
}

fn build(name: &str) -> Result<(cpal::Stream, Arc<Shared>)> {
    let device = find_output(name)?;
    let supported = device.default_output_config().map_err(|e| anyhow!("the speakers cannot be used: {e}"))?;
    let config = supported.config();
    let channels = usize::from(config.channels);
    let device_rate: u32 = config.sample_rate;
    let shared = Arc::new(Shared { samples: Mutex::new(VecDeque::new()), device_rate, level: AtomicU32::new(0) });
    let sink = Arc::clone(&shared);
    let fill = move |out: &mut [f32]| {
        let mut sum = 0.0f32;
        let mut count = 0usize;
        match sink.samples.try_lock() {
            Ok(mut queue) => {
                for frame in out.chunks_mut(channels) {
                    let value = queue.pop_front().unwrap_or(0.0);
                    sum += value * value;
                    count += 1;
                    frame.fill(value);
                }
            }
            Err(_) => out.fill(0.0),
        }
        let rms = if count == 0 { 0.0 } else { (sum / count as f32).sqrt() };
        sink.level.store(rms.to_bits(), Ordering::Relaxed);
    };
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => device.build_output_stream(
            config,
            move |out: &mut [f32], _: &cpal::OutputCallbackInfo| fill(out),
            |error| eprintln!("speaker error: {error}"),
            None,
        ),
        cpal::SampleFormat::I16 => {
            let mut scratch: Vec<f32> = Vec::new();
            device.build_output_stream(
                config,
                move |out: &mut [i16], _: &cpal::OutputCallbackInfo| {
                    scratch.resize(out.len(), 0.0);
                    fill(&mut scratch);
                    for (target, value) in out.iter_mut().zip(&scratch) {
                        *target = (value.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16;
                    }
                },
                |error| eprintln!("speaker error: {error}"),
                None,
            )
        }
        other => return Err(anyhow!("the speakers use the sample format {other:?}, which is not supported")),
    }
    .map_err(|e| anyhow!("could not open the speakers: {e}"))?;
    stream.play().map_err(|e| anyhow!("could not start the speakers: {e}"))?;
    Ok((stream, shared))
}

impl Speaker for Player {
    fn enqueue(&self, pcm: Vec<f32>, sample_rate: u32) {
        let converted = resample(&pcm, sample_rate, self.shared.device_rate);
        if let Ok(mut queue) = self.shared.samples.lock() {
            queue.extend(converted);
        }
    }

    fn stop(&self) {
        if let Ok(mut queue) = self.shared.samples.lock() {
            queue.clear();
        }
    }

    fn queued(&self) -> Duration {
        let samples = self.shared.samples.lock().map_or(0, |queue| queue.len());
        Duration::from_secs_f64(samples as f64 / f64::from(self.shared.device_rate.max(1)))
    }

    fn level(&self) -> f32 {
        let rms = f32::from_bits(self.shared.level.load(Ordering::Relaxed));
        (rms * 4.0).clamp(0.0, 1.0).sqrt()
    }
}

/// Mono samples from interleaved ones; kept here so decoders can share it.
pub fn mono(samples: &[f32], channels: usize) -> Vec<f32> {
    to_mono(samples, channels)
}
