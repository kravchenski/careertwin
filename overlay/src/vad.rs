//! Energy-based utterance segmentation for 16 kHz mono PCM16 audio.

use std::collections::VecDeque;

pub const SAMPLE_RATE: u32 = 16_000;
pub const FRAME_MS: usize = 20;
pub const FRAME_SAMPLES: usize = SAMPLE_RATE as usize * FRAME_MS / 1000;
pub const FRAME_BYTES: usize = FRAME_SAMPLES * 2;

/// How loud a frame is: the root of the mean of the squares.
pub fn rms(frame: &[i16]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    let sum: f64 = frame.iter().map(|sample| f64::from(*sample) * f64::from(*sample)).sum();
    (sum / frame.len() as f64).sqrt() as f32
}

/// The level the window shows for a frame, 0 to 1: quiet speech already moves the dots.
pub fn display_level(frame: &[i16]) -> f32 {
    (rms(frame) / 6000.0).sqrt().min(1.0)
}

/// Groups frames into utterances: speech, then enough silence to call it finished.
pub struct UtteranceSegmenter {
    end_frames: usize,
    min_frames: usize,
    max_frames: usize,
    tail_frames: usize,
    preroll: VecDeque<Vec<i16>>,
    preroll_capacity: usize,
    min_threshold: f32,
    noise_multiplier: f32,
    noise: f32,
    samples: Vec<i16>,
    speech_frames: usize,
    silent_frames: usize,
    active: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub end_silence_ms: usize,
    pub min_speech_ms: usize,
    pub max_utterance_ms: usize,
    pub preroll_ms: usize,
    pub tail_ms: usize,
    pub min_threshold: f32,
    pub noise_multiplier: f32,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            end_silence_ms: 500,
            min_speech_ms: 300,
            max_utterance_ms: 12_000,
            preroll_ms: 200,
            tail_ms: 200,
            min_threshold: 250.0,
            noise_multiplier: 3.0,
        }
    }
}

impl UtteranceSegmenter {
    pub fn new(options: Options) -> Self {
        Self {
            end_frames: options.end_silence_ms / FRAME_MS,
            min_frames: options.min_speech_ms / FRAME_MS,
            max_frames: options.max_utterance_ms / FRAME_MS,
            tail_frames: options.tail_ms / FRAME_MS,
            preroll: VecDeque::new(),
            preroll_capacity: options.preroll_ms / FRAME_MS,
            min_threshold: options.min_threshold,
            noise_multiplier: options.noise_multiplier,
            noise: options.min_threshold / options.noise_multiplier,
            samples: Vec::new(),
            speech_frames: 0,
            silent_frames: 0,
            active: false,
        }
    }

    pub fn threshold(&self) -> f32 {
        self.min_threshold.max(self.noise * self.noise_multiplier)
    }

    /// Whether an utterance is being heard now.
    pub fn active(&self) -> bool {
        self.active
    }

    /// How many frames the utterance so far has, the pre-roll included.
    pub fn frame_count(&self) -> usize {
        self.samples.len() / FRAME_SAMPLES
    }

    /// Feed one frame; a finished utterance comes back as samples.
    pub fn push(&mut self, frame: &[i16]) -> Option<Vec<i16>> {
        let energy = rms(frame);
        let voiced = energy >= self.threshold();
        if !self.active {
            if !voiced {
                self.noise = 0.95 * self.noise + 0.05 * energy;
                if self.preroll_capacity > 0 {
                    if self.preroll.len() == self.preroll_capacity {
                        self.preroll.pop_front();
                    }
                    self.preroll.push_back(frame.to_vec());
                }
                return None;
            }
            self.active = true;
            self.samples = self.preroll.drain(..).flatten().collect();
            self.speech_frames = 0;
            self.silent_frames = 0;
        }
        self.samples.extend_from_slice(frame);
        if voiced {
            self.speech_frames += 1;
            self.silent_frames = 0;
        } else {
            self.silent_frames += 1;
        }
        if self.silent_frames >= self.end_frames || self.frame_count() >= self.max_frames {
            self.finish()
        } else {
            None
        }
    }

    /// The speech heard so far in the current utterance; nothing while idle or too short.
    pub fn snapshot(&self) -> Option<Vec<i16>> {
        (self.active && self.speech_frames >= self.min_frames).then(|| self.samples.clone())
    }

    pub fn flush(&mut self) -> Option<Vec<i16>> {
        if self.active { self.finish() } else { None }
    }

    fn finish(&mut self) -> Option<Vec<i16>> {
        let enough = self.speech_frames >= self.min_frames;
        let surplus = self.silent_frames.saturating_sub(self.tail_frames);
        let mut samples = std::mem::take(&mut self.samples);
        samples.truncate(samples.len().saturating_sub(surplus * FRAME_SAMPLES));
        self.active = false;
        self.speech_frames = 0;
        self.silent_frames = 0;
        (enough && !samples.is_empty()).then_some(samples)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(level: i16) -> Vec<i16> {
        // a square wave of the given amplitude has that rms
        (0..FRAME_SAMPLES).map(|n| if n % 2 == 0 { level } else { -level }).collect()
    }

    fn feed(segmenter: &mut UtteranceSegmenter, level: i16, frames: usize) -> Vec<Vec<i16>> {
        (0..frames).filter_map(|_| segmenter.push(&frame(level))).collect()
    }

    #[test]
    fn rms_and_the_display_level_follow_loudness() {
        assert_eq!(rms(&[]), 0.0);
        assert!((rms(&frame(1000)) - 1000.0).abs() < 1.0);
        assert!(
            display_level(&frame(0)) < 0.01
                && display_level(&frame(1000)) > 0.3
                && display_level(&frame(20_000)) == 1.0
        );
    }

    #[test]
    fn speech_then_silence_is_one_utterance_without_the_long_tail() {
        let mut segmenter = UtteranceSegmenter::new(Options::default());
        assert!(feed(&mut segmenter, 20, 10).is_empty()); // silence builds the pre-roll
        assert!(feed(&mut segmenter, 3000, 25).is_empty()); // 0.5 s of speech
        assert!(segmenter.active() && segmenter.frame_count() >= 25);
        let done = feed(&mut segmenter, 20, 25); // silence ends it after 0.5 s
        assert_eq!(done.len(), 1);
        // 0.2 s of pre-roll, the 0.5 s of speech, and only 0.2 s of the 0.5 s of silence that ended it
        assert_eq!(done[0].len() / FRAME_SAMPLES, 10 + 25 + 10);
        assert!(!segmenter.active() && segmenter.flush().is_none());
    }

    #[test]
    fn a_click_is_not_speech() {
        let mut segmenter = UtteranceSegmenter::new(Options::default());
        assert!(feed(&mut segmenter, 3000, 5).is_empty()); // 0.1 s: under the minimum
        assert!(feed(&mut segmenter, 20, 30).is_empty());
        assert!(!segmenter.active());
    }

    #[test]
    fn a_long_monologue_is_cut_at_the_limit() {
        let mut segmenter = UtteranceSegmenter::new(Options::default());
        let done = feed(&mut segmenter, 3000, 700); // 14 s
        assert_eq!(done.len(), 1);
        assert_eq!(done[0].len() / FRAME_SAMPLES, 600);
    }

    #[test]
    fn a_snapshot_waits_for_enough_speech_and_flush_ends_the_utterance() {
        let mut segmenter = UtteranceSegmenter::new(Options::default());
        feed(&mut segmenter, 3000, 5);
        assert!(segmenter.snapshot().is_none()); // 0.1 s is not yet something to caption
        feed(&mut segmenter, 3000, 20);
        assert_eq!(segmenter.snapshot().unwrap().len() / FRAME_SAMPLES, 25);
        assert_eq!(segmenter.flush().unwrap().len() / FRAME_SAMPLES, 25);
        assert!(segmenter.snapshot().is_none());
    }

    #[test]
    fn the_threshold_follows_the_room_noise_but_never_goes_under_the_minimum() {
        let mut segmenter = UtteranceSegmenter::new(Options { min_threshold: 120.0, ..Options::default() });
        assert!((segmenter.threshold() - 120.0).abs() < 1e-3);
        feed(&mut segmenter, 400, 200); // loud but constant: the noise estimate rises... only while not voiced
        assert!(segmenter.threshold() >= 120.0);
        let quiet = UtteranceSegmenter::new(Options { min_threshold: 120.0, ..Options::default() });
        assert!(quiet.threshold() >= 120.0);
    }
}
