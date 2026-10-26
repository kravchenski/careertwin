//! WAV files in and out of memory.

use std::io::Cursor;

use anyhow::{Context, Result, bail};

use crate::audio::resample::to_mono;

/// 16-bit mono WAV bytes of `samples` at `rate` Hz.
pub fn encode_i16(samples: &[i16], rate: u32) -> Result<Vec<u8>> {
    let spec =
        hound::WavSpec { channels: 1, sample_rate: rate, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let mut cursor = Cursor::new(Vec::with_capacity(samples.len() * 2 + 44));
    {
        let mut writer = hound::WavWriter::new(&mut cursor, spec).context("could not start the WAV file")?;
        for &sample in samples {
            writer.write_sample(sample)?;
        }
        writer.finalize()?;
    }
    Ok(cursor.into_inner())
}

/// Mono float samples and the rate of a WAV file (16, 24 or 32-bit integer, or 32-bit float).
pub fn decode(bytes: &[u8]) -> Result<(Vec<f32>, u32)> {
    let mut reader = hound::WavReader::new(Cursor::new(bytes)).context("the audio is not a WAV file")?;
    let spec = reader.spec();
    if spec.channels == 0 || spec.sample_rate == 0 {
        bail!("the WAV file has no audio format");
    }
    let samples: Vec<f32> = match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Float, 32) => reader.samples::<f32>().collect::<Result<_, _>>()?,
        (hound::SampleFormat::Int, bits @ (8 | 16 | 24 | 32)) => {
            let scale = (1i64 << (bits - 1)) as f32;
            reader.samples::<i32>().map(|s| s.map(|v| v as f32 / scale)).collect::<Result<_, _>>()?
        }
        (format, bits) => bail!("unsupported WAV format {format:?} with {bits} bits"),
    };
    Ok((to_mono(&samples, usize::from(spec.channels)), spec.sample_rate))
}

/// Raw 16-bit little-endian mono PCM (what ElevenLabs sends) as float samples.
#[allow(clippy::chunks_exact_to_as_chunks)]
pub fn from_pcm16le(bytes: &[u8]) -> Vec<f32> {
    bytes.chunks_exact(2).map(|pair| f32::from(i16::from_le_bytes([pair[0], pair[1]])) / 32768.0).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wav_round_trips() {
        let samples: Vec<i16> = (0..1600).map(|i| ((i as f32 / 20.0).sin() * 8000.0) as i16).collect();
        let bytes = encode_i16(&samples, 16_000).unwrap();
        assert_eq!(&bytes[..4], b"RIFF");
        let (decoded, rate) = decode(&bytes).unwrap();
        assert_eq!(rate, 16_000);
        assert_eq!(decoded.len(), samples.len());
        assert!((decoded[100] - f32::from(samples[100]) / 32768.0).abs() < 0.001);
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        assert!(decode(b"not a wav").is_err());
        assert!(decode(&[]).is_err());
        let mut truncated = encode_i16(&[1, 2, 3, 4], 16_000).unwrap();
        truncated.truncate(30);
        let _ = decode(&truncated);
    }

    #[test]
    fn stereo_is_mixed_down() {
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 8000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer = hound::WavWriter::new(&mut cursor, spec).unwrap();
            for _ in 0..10 {
                writer.write_sample(16384i16).unwrap();
                writer.write_sample(0i16).unwrap();
            }
            writer.finalize().unwrap();
        }
        let (mono, rate) = decode(&cursor.into_inner()).unwrap();
        assert_eq!((mono.len(), rate), (10, 8000));
        assert!((mono[0] - 0.25).abs() < 0.001);
    }

    #[test]
    fn float_wavs_decode() {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 24_000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer = hound::WavWriter::new(&mut cursor, spec).unwrap();
            writer.write_sample(0.5f32).unwrap();
            writer.finalize().unwrap();
        }
        let (samples, rate) = decode(&cursor.into_inner()).unwrap();
        assert_eq!((samples, rate), (vec![0.5], 24_000));
    }

    #[test]
    fn raw_pcm_is_read_little_endian_and_an_odd_byte_is_ignored() {
        let samples = from_pcm16le(&[0x00, 0x40, 0x00, 0xC0, 0x7F]);
        assert_eq!(samples.len(), 2);
        assert!((samples[0] - 0.5).abs() < 0.001 && (samples[1] + 0.5).abs() < 0.001);
    }
}
