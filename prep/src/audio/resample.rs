//! Sample-rate and sample-format conversions for speech.

/// Interleaved frames to one channel by averaging.
pub fn to_mono(samples: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return samples.to_vec();
    }
    samples.chunks(channels).map(|frame| frame.iter().sum::<f32>() / frame.len() as f32).collect()
}

/// Converts `input` from `from` Hz to `to` Hz. Downsampling averages the samples it folds
/// together, which keeps speech clean enough for recognition.
pub fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if input.is_empty() || from == 0 || to == 0 {
        return Vec::new();
    }
    if from == to {
        return input.to_vec();
    }
    let ratio = f64::from(from) / f64::from(to);
    let out_len = ((input.len() as f64) / ratio).floor() as usize;
    let mut out = Vec::with_capacity(out_len);
    for n in 0..out_len {
        let position = n as f64 * ratio;
        let value = if ratio > 1.0 {
            let start = position.floor() as usize;
            let end = ((position + ratio).ceil() as usize).min(input.len()).max(start + 1);
            input[start..end].iter().sum::<f32>() / (end - start) as f32
        } else {
            let index = position.floor() as usize;
            let fraction = (position - index as f64) as f32;
            let a = input[index];
            let b = input.get(index + 1).copied().unwrap_or(a);
            a + (b - a) * fraction
        };
        out.push(value);
    }
    out
}

/// Converts a live stream chunk by chunk, keeping its place between chunks so there is no click
/// where one chunk ends and the next begins.
pub struct StreamResampler {
    ratio: f64,
    position: f64,
    pending: Vec<f32>,
}

impl StreamResampler {
    pub fn new(from: u32, to: u32) -> Self {
        Self { ratio: f64::from(from.max(1)) / f64::from(to.max(1)), position: 0.0, pending: Vec::new() }
    }

    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        self.pending.extend_from_slice(input);
        let mut out = Vec::new();
        loop {
            let start = self.position.floor() as usize;
            if self.ratio >= 1.0 {
                let end = (self.position + self.ratio).ceil() as usize;
                if end > self.pending.len() {
                    break;
                }
                let end = end.max(start + 1);
                out.push(self.pending[start..end].iter().sum::<f32>() / (end - start) as f32);
            } else {
                if start + 1 >= self.pending.len() {
                    break;
                }
                let fraction = (self.position - start as f64) as f32;
                let (a, b) = (self.pending[start], self.pending[start + 1]);
                out.push(a + (b - a) * fraction);
            }
            self.position += self.ratio;
        }
        let consumed = (self.position.floor() as usize).min(self.pending.len());
        self.pending.drain(..consumed);
        self.position -= consumed as f64;
        out
    }
}

pub fn f32_to_i16(samples: &[f32]) -> Vec<i16> {
    samples.iter().map(|&s| (s.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16).collect()
}

pub fn i16_to_f32(samples: &[i16]) -> Vec<f32> {
    samples.iter().map(|&s| f32::from(s) / 32768.0).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(rate: u32, hz: f32, seconds: f32) -> Vec<f32> {
        (0..(rate as f32 * seconds) as usize)
            .map(|i| (i as f32 / rate as f32 * hz * std::f32::consts::TAU).sin() * 0.5)
            .collect()
    }

    #[test]
    fn stereo_is_averaged_to_mono() {
        assert_eq!(to_mono(&[1.0, 0.0, 0.5, 0.5], 2), vec![0.5, 0.5]);
        assert_eq!(to_mono(&[0.1, 0.2], 1), vec![0.1, 0.2]);
    }

    #[test]
    fn the_length_follows_the_ratio() {
        let one_second = sine(48_000, 440.0, 1.0);
        assert_eq!(resample(&one_second, 48_000, 16_000).len(), 16_000);
        let up = resample(&sine(16_000, 440.0, 1.0), 16_000, 48_000);
        assert!((47_990..=48_000).contains(&up.len()), "{}", up.len());
        assert_eq!(resample(&one_second, 48_000, 48_000).len(), 48_000);
    }

    #[test]
    fn a_tone_keeps_its_pitch_and_loudness() {
        let down = resample(&sine(48_000, 440.0, 1.0), 48_000, 16_000);
        let peak = down.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!((0.4..=0.51).contains(&peak), "{peak}");
        let crossings = down.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
        assert!((435..=445).contains(&crossings), "{crossings}");
    }

    #[test]
    fn streaming_in_chunks_matches_one_go() {
        let input = sine(44_100, 300.0, 1.0);
        let whole = resample(&input, 44_100, 16_000);
        let mut stream = StreamResampler::new(44_100, 16_000);
        let mut chunked = Vec::new();
        for chunk in input.chunks(441) {
            chunked.extend(stream.process(chunk));
        }
        assert!((chunked.len() as i64 - whole.len() as i64).abs() <= 2, "{} vs {}", chunked.len(), whole.len());
        let n = chunked.len().min(whole.len()) - 2;
        let worst = (0..n).map(|i| (chunked[i] - whole[i]).abs()).fold(0.0f32, f32::max);
        assert!(worst < 0.02, "{worst}");
    }

    #[test]
    fn streaming_upsampling_has_no_gaps_at_chunk_edges() {
        let input = sine(16_000, 200.0, 0.5);
        let mut stream = StreamResampler::new(16_000, 48_000);
        let mut out = Vec::new();
        for chunk in input.chunks(100) {
            out.extend(stream.process(chunk));
        }
        assert!((out.len() as i64 - 24_000).abs() < 8, "{}", out.len());
        let jump = out.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
        assert!(jump < 0.05, "{jump}");
    }

    #[test]
    fn empty_and_zero_rates_are_safe() {
        assert!(resample(&[], 48_000, 16_000).is_empty());
        assert!(resample(&[0.1], 0, 16_000).is_empty());
        assert!(resample(&[0.1], 16_000, 0).is_empty());
    }

    #[test]
    fn a_single_sample_does_not_panic() {
        let _ = resample(&[0.3], 16_000, 48_000);
        let _ = resample(&[0.3], 48_000, 16_000);
    }

    #[test]
    fn format_conversions_round_trip_and_clip() {
        assert_eq!(f32_to_i16(&[0.0, 1.0, -1.0, 2.0, -2.0]), vec![0, 32767, -32767, 32767, -32767]);
        let back = i16_to_f32(&f32_to_i16(&[0.25, -0.5]));
        assert!((back[0] - 0.25).abs() < 0.001 && (back[1] + 0.5).abs() < 0.001);
    }
}
