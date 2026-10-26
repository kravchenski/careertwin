//! Finding where you start and stop talking, from the energy of 20 ms frames of 16 kHz audio.

use std::collections::VecDeque;

pub const SAMPLE_RATE: u32 = 16_000;
pub const FRAME_MS: u32 = 20;
pub const FRAME_SAMPLES: usize = (SAMPLE_RATE * FRAME_MS / 1000) as usize;

/// What the segmenter noticed while consuming a frame.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// Nothing worth reporting.
    None,
    /// Speech just began.
    Started,
    /// An utterance is complete.
    Finished(Vec<i16>),
}

#[derive(Debug, Clone)]
pub struct Options {
    /// Silence that ends an utterance.
    pub end_silence_ms: u32,
    /// Speech shorter than this is a click or a cough, not a turn.
    pub min_speech_ms: u32,
    pub max_utterance_ms: u32,
    pub preroll_ms: u32,
    pub tail_ms: u32,
    /// The quietest RMS (in 16-bit units) that can count as speech.
    pub min_threshold: f32,
    /// Speech must be this many times louder than the noise floor.
    pub noise_multiplier: f32,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            end_silence_ms: 700,
            min_speech_ms: 300,
            max_utterance_ms: 90_000,
            preroll_ms: 200,
            tail_ms: 200,
            min_threshold: 250.0,
            noise_multiplier: 3.0,
        }
    }
}

pub fn rms(frame: &[i16]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    let sum: f64 = frame.iter().map(|&s| f64::from(s) * f64::from(s)).sum();
    (sum / frame.len() as f64).sqrt() as f32
}

/// A level from 0 to 1 for a meter, loud speech being near 1.
pub fn level(frame: &[i16]) -> f32 {
    (rms(frame) / 6000.0).clamp(0.0, 1.0).sqrt()
}

pub struct Segmenter {
    options: Options,
    end_frames: usize,
    min_frames: usize,
    max_frames: usize,
    tail_frames: usize,
    preroll: VecDeque<Vec<i16>>,
    noise: f32,
    frames: Vec<Vec<i16>>,
    speech_frames: usize,
    silent_frames: usize,
    active: bool,
}

impl Segmenter {
    pub fn new(options: Options) -> Self {
        let frames = |ms: u32| (ms / FRAME_MS).max(1) as usize;
        Self {
            end_frames: frames(options.end_silence_ms),
            min_frames: frames(options.min_speech_ms),
            max_frames: frames(options.max_utterance_ms),
            tail_frames: (options.tail_ms / FRAME_MS) as usize,
            preroll: VecDeque::with_capacity(frames(options.preroll_ms)),
            noise: options.min_threshold / options.noise_multiplier,
            frames: Vec::new(),
            speech_frames: 0,
            silent_frames: 0,
            active: false,
            options,
        }
    }

    pub fn threshold(&self) -> f32 {
        self.options.min_threshold.max(self.noise * self.options.noise_multiplier)
    }

    /// Whether an utterance is in progress.
    pub fn active(&self) -> bool {
        self.active
    }

    /// How long the speech in progress has lasted, in milliseconds.
    pub fn speech_ms(&self) -> u32 {
        self.speech_frames as u32 * FRAME_MS
    }

    pub fn push(&mut self, frame: &[i16]) -> Event {
        let energy = rms(frame);
        let voiced = energy >= self.threshold();
        let mut event = Event::None;
        if !self.active {
            if !voiced {
                self.noise = 0.95 * self.noise + 0.05 * energy;
                let capacity = (self.options.preroll_ms / FRAME_MS).max(1) as usize;
                if self.preroll.len() == capacity {
                    self.preroll.pop_front();
                }
                self.preroll.push_back(frame.to_vec());
                return Event::None;
            }
            self.active = true;
            self.frames = self.preroll.drain(..).collect();
            self.speech_frames = 0;
            self.silent_frames = 0;
            event = Event::Started;
        }
        self.frames.push(frame.to_vec());
        if voiced {
            self.speech_frames += 1;
            self.silent_frames = 0;
        } else {
            self.silent_frames += 1;
        }
        if self.silent_frames >= self.end_frames || self.frames.len() >= self.max_frames {
            return match self.finish() {
                Some(pcm) => Event::Finished(pcm),
                None => Event::None,
            };
        }
        event
    }

    /// The utterance so far, if it has enough speech to be one.
    pub fn flush(&mut self) -> Option<Vec<i16>> {
        if self.active { self.finish() } else { None }
    }

    /// Forget an utterance in progress (the microphone was muted).
    pub fn reset(&mut self) {
        self.frames.clear();
        self.preroll.clear();
        self.active = false;
        self.speech_frames = 0;
        self.silent_frames = 0;
    }

    fn finish(&mut self) -> Option<Vec<i16>> {
        let enough = self.speech_frames >= self.min_frames;
        let surplus = self.silent_frames.saturating_sub(self.tail_frames);
        let mut frames = std::mem::take(&mut self.frames);
        let keep = frames.len().saturating_sub(surplus);
        frames.truncate(keep);
        self.active = false;
        self.speech_frames = 0;
        self.silent_frames = 0;
        (enough && !frames.is_empty()).then(|| frames.concat())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(amplitude: i16) -> Vec<i16> {
        // a square-ish wave: its RMS equals the amplitude
        (0..FRAME_SAMPLES).map(|i| if i % 2 == 0 { amplitude } else { -amplitude }).collect()
    }

    fn feed(segmenter: &mut Segmenter, amplitude: i16, ms: u32) -> Vec<Event> {
        (0..ms / FRAME_MS).map(|_| segmenter.push(&frame(amplitude))).collect()
    }

    fn finished(events: &[Event]) -> Vec<&Vec<i16>> {
        events.iter().filter_map(|e| if let Event::Finished(p) = e { Some(p) } else { None }).collect()
    }

    #[test]
    fn silence_alone_produces_nothing() {
        let mut s = Segmenter::new(Options::default());
        assert!(feed(&mut s, 30, 3000).iter().all(|e| *e == Event::None));
        assert!(!s.active());
    }

    #[test]
    fn speech_then_silence_is_one_utterance_with_a_start_event() {
        let mut s = Segmenter::new(Options::default());
        let mut events = feed(&mut s, 30, 400);
        events.extend(feed(&mut s, 3000, 1000));
        events.extend(feed(&mut s, 30, 1000));
        assert_eq!(events.iter().filter(|e| **e == Event::Started).count(), 1);
        let utterances = finished(&events);
        assert_eq!(utterances.len(), 1);
        let ms = utterances[0].len() as u32 * 1000 / SAMPLE_RATE;
        // pre-roll 200 + speech 1000 + tail 200
        assert!((1300..=1500).contains(&ms), "{ms}");
    }

    #[test]
    fn a_short_blip_is_not_a_turn() {
        let mut s = Segmenter::new(Options::default());
        let mut events = feed(&mut s, 3000, 100);
        events.extend(feed(&mut s, 30, 1500));
        assert!(finished(&events).is_empty());
        assert!(!s.active());
    }

    #[test]
    fn a_short_pause_inside_speech_does_not_split_it() {
        let mut s = Segmenter::new(Options::default());
        let mut events = feed(&mut s, 3000, 600);
        events.extend(feed(&mut s, 30, 400)); // a breath
        events.extend(feed(&mut s, 3000, 600));
        events.extend(feed(&mut s, 30, 1000));
        assert_eq!(finished(&events).len(), 1);
    }

    #[test]
    fn a_long_pause_splits_into_two_utterances() {
        let mut s = Segmenter::new(Options::default());
        let mut events = feed(&mut s, 3000, 600);
        events.extend(feed(&mut s, 30, 1000));
        events.extend(feed(&mut s, 3000, 600));
        events.extend(feed(&mut s, 30, 1000));
        assert_eq!(finished(&events).len(), 2);
    }

    #[test]
    fn a_very_long_utterance_is_cut_at_the_maximum() {
        let mut s = Segmenter::new(Options { max_utterance_ms: 2000, ..Options::default() });
        let events = feed(&mut s, 3000, 5000);
        assert!(finished(&events).len() >= 2);
    }

    #[test]
    fn a_noisy_room_raises_the_bar_so_noise_is_not_speech() {
        let mut s = Segmenter::new(Options::default());
        feed(&mut s, 200, 4000); // steady hum, below the minimum threshold
        assert!(s.threshold() >= 250.0);
        let events = feed(&mut s, 200, 2000);
        assert!(finished(&events).is_empty());
    }

    #[test]
    fn flush_returns_the_utterance_in_progress() {
        let mut s = Segmenter::new(Options::default());
        feed(&mut s, 3000, 800);
        assert!(s.active());
        assert!(s.flush().is_some());
        assert!(s.flush().is_none());
    }

    #[test]
    fn reset_forgets_what_was_heard() {
        let mut s = Segmenter::new(Options::default());
        feed(&mut s, 3000, 800);
        s.reset();
        assert!(!s.active() && s.speech_ms() == 0 && s.flush().is_none());
    }

    #[test]
    fn levels_are_bounded_and_ordered() {
        assert_eq!(level(&[]), 0.0);
        assert!(level(&frame(100)) < level(&frame(2000)));
        assert!(level(&frame(i16::MAX)) <= 1.0);
        assert_eq!(rms(&frame(1234)).round(), 1234.0);
    }
}
