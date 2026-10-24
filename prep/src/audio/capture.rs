//! Listening through the microphone: 20 ms frames of 16 kHz mono audio.

use std::sync::mpsc;

use anyhow::{Context, Result, anyhow};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use tokio::sync::mpsc::{Receiver, Sender, channel};

use super::resample::{StreamResampler, f32_to_i16, to_mono};
use super::vad::{FRAME_SAMPLES, SAMPLE_RATE};

/// The microphone. Frames arrive on `frames`; dropping the `Mic` stops listening.
pub struct Mic {
    pub frames: Receiver<Vec<i16>>,
    pub device: String,
    _stop: mpsc::Sender<()>,
}

pub fn input_devices() -> Vec<String> {
    cpal::default_host().input_devices().map(|devices| devices.map(|d| d.to_string()).collect()).unwrap_or_default()
}

fn find_input(name: &str) -> Result<cpal::Device> {
    let host = cpal::default_host();
    if name.trim().is_empty() {
        return host.default_input_device().context("no microphone found");
    }
    host.input_devices()
        .map_err(|e| anyhow!("could not list the microphones: {e}"))?
        .find(|device| device.to_string() == name)
        .with_context(|| format!("no microphone named {name}"))
}

impl Mic {
    /// The frames, and a guard that keeps the microphone open for as long as it is held.
    pub fn split(self) -> (Receiver<Vec<i16>>, Box<dyn std::any::Any + Send>) {
        (self.frames, Box::new(self._stop))
    }

    /// `device` is a name from [`input_devices`]; empty means the system default.
    pub fn open(device: &str) -> Result<Self> {
        let (frames_tx, frames) = channel::<Vec<i16>>(512);
        let (ready_tx, ready_rx) = mpsc::channel::<Result<String>>();
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let wanted = device.to_owned();
        std::thread::Builder::new()
            .name("prep-microphone".into())
            .spawn(move || match build(&wanted, frames_tx) {
                Ok((stream, name)) => {
                    let _ = ready_tx.send(Ok(name));
                    let _ = stop_rx.recv();
                    drop(stream);
                }
                Err(error) => {
                    let _ = ready_tx.send(Err(error));
                }
            })
            .context("could not start the audio thread")?;
        let device = ready_rx.recv().map_err(|_| anyhow!("the audio thread stopped"))??;
        Ok(Self { frames, device, _stop: stop_tx })
    }
}

/// Collects converted samples into whole frames and sends them on.
struct Framer {
    resampler: StreamResampler,
    channels: usize,
    pending: Vec<f32>,
    out: Sender<Vec<i16>>,
    /// Frames still to throw away: a sound card opens with a loud, fading click.
    skip: usize,
}

/// How much of the start-up click is thrown away.
const WARM_UP_FRAMES: usize = 15;

impl Framer {
    fn feed(&mut self, interleaved: &[f32]) {
        let mono = to_mono(interleaved, self.channels);
        self.pending.extend(self.resampler.process(&mono));
        while self.pending.len() >= FRAME_SAMPLES {
            let frame: Vec<f32> = self.pending.drain(..FRAME_SAMPLES).collect();
            if self.skip > 0 {
                self.skip -= 1;
                continue;
            }
            // a listener that cannot keep up loses audio rather than stalling the sound card
            let _ = self.out.try_send(f32_to_i16(&frame));
        }
    }
}

fn build(name: &str, out: Sender<Vec<i16>>) -> Result<(cpal::Stream, String)> {
    let device = find_input(name)?;
    let label = device.to_string();
    let supported = device.default_input_config().map_err(|e| anyhow!("the microphone cannot be used: {e}"))?;
    let config = supported.config();
    let rate: u32 = config.sample_rate;
    let mut framer = Framer {
        resampler: StreamResampler::new(rate, SAMPLE_RATE),
        channels: usize::from(config.channels),
        pending: Vec::new(),
        out,
        skip: WARM_UP_FRAMES,
    };
    let on_error = |error| eprintln!("microphone error: {error}");
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(
            config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| framer.feed(data),
            on_error,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_input_stream(
            config,
            move |data: &[i16], _: &cpal::InputCallbackInfo| {
                let converted: Vec<f32> = data.iter().map(|&s| f32::from(s) / 32768.0).collect();
                framer.feed(&converted);
            },
            on_error,
            None,
        ),
        cpal::SampleFormat::U16 => device.build_input_stream(
            config,
            move |data: &[u16], _: &cpal::InputCallbackInfo| {
                let converted: Vec<f32> = data.iter().map(|&s| (f32::from(s) - 32768.0) / 32768.0).collect();
                framer.feed(&converted);
            },
            on_error,
            None,
        ),
        other => return Err(anyhow!("the microphone uses the sample format {other:?}, which is not supported")),
    }
    .map_err(|e| anyhow!("could not open the microphone: {e}"))?;
    stream.play().map_err(|e| anyhow!("could not start the microphone: {e}"))?;
    Ok((stream, label))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_framer_cuts_whole_frames_and_keeps_the_remainder() {
        let (tx, mut rx) = channel(16);
        let mut framer = Framer {
            resampler: StreamResampler::new(16_000, 16_000),
            channels: 1,
            pending: Vec::new(),
            out: tx,
            skip: 0,
        };
        framer.feed(&vec![0.1; FRAME_SAMPLES + 10]);
        let first = rx.try_recv().unwrap();
        assert_eq!(first.len(), FRAME_SAMPLES);
        assert!(rx.try_recv().is_err());
        framer.feed(&vec![0.1; FRAME_SAMPLES - 10]);
        assert_eq!(rx.try_recv().unwrap().len(), FRAME_SAMPLES);
    }

    #[test]
    fn stereo_48k_becomes_mono_16k_frames() {
        let (tx, mut rx) = channel(64);
        let mut framer = Framer {
            resampler: StreamResampler::new(48_000, 16_000),
            channels: 2,
            pending: Vec::new(),
            out: tx,
            skip: 0,
        };
        // 100 ms of stereo audio at 48 kHz
        framer.feed(&vec![0.2; 4800 * 2]);
        let mut frames = 0;
        while rx.try_recv().is_ok() {
            frames += 1;
        }
        assert_eq!(frames, 5);
    }

    #[test]
    fn the_start_up_click_is_thrown_away() {
        let (tx, mut rx) = channel(64);
        let mut framer = Framer {
            resampler: StreamResampler::new(16_000, 16_000),
            channels: 1,
            pending: Vec::new(),
            out: tx,
            skip: 3,
        };
        for loudness in [0.9, 0.8, 0.7, 0.1, 0.1] {
            framer.feed(&vec![loudness; FRAME_SAMPLES]);
        }
        let first = rx.try_recv().unwrap();
        assert!(first.iter().all(|&s| s < 4000), "the first frame kept is the quiet one");
        assert!(rx.try_recv().is_ok() && rx.try_recv().is_err());
    }

    #[test]
    fn a_full_channel_drops_frames_instead_of_blocking() {
        let (tx, _rx) = channel(1);
        let mut framer = Framer {
            resampler: StreamResampler::new(16_000, 16_000),
            channels: 1,
            pending: Vec::new(),
            out: tx,
            skip: 0,
        };
        framer.feed(&vec![0.0; FRAME_SAMPLES * 10]);
    }
}
