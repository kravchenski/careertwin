//! The offline Russian voice: Piper, the project's own release, run as a program for each sentence.
//! Kokoro does not speak Russian, so this is what says the Russian half of a lesson without the cloud.
//! The program and the voice files are fetched and checked like every other model (see
//! [`crate::models::piper_assets`]); the archive is unpacked once, on first use.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::FutureExt;
use futures_util::future::BoxFuture;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

use super::{Speech, SpeechError, Tts, Voice};
use crate::models::{PIPER_ARCHIVE, piper_voice_or_default};
use crate::prompts::Lang;

/// A sentence that takes longer than this to say is a program that has hung.
const LONGEST_WAIT: Duration = Duration::from_secs(30);

pub struct PiperTts {
    folder: PathBuf,
    unpacking: Arc<Mutex<()>>,
}

fn not_downloaded(folder: &Path) -> SpeechError {
    SpeechError::Status(format!("The offline Russian voice is not downloaded yet ({}).", folder.display()))
}

impl PiperTts {
    /// `folder` holds the archive and the voice files (see [`crate::models::piper_assets`]).
    pub fn new(folder: PathBuf) -> Self {
        Self { folder, unpacking: Arc::new(Mutex::new(())) }
    }

    fn program(folder: &Path) -> PathBuf {
        folder.join("engine").join("piper").join("piper")
    }

    /// The program, unpacked from the archive the first time it is needed.
    fn unpack(folder: &Path, lock: &Mutex<()>) -> Result<PathBuf, SpeechError> {
        let _once = lock.lock().map_err(|_| SpeechError::Status("the offline Russian voice stopped working".into()))?;
        let program = Self::program(folder);
        if program.is_file() {
            return Ok(program);
        }
        let archive = folder.join(PIPER_ARCHIVE);
        if !archive.is_file() {
            return Err(not_downloaded(folder));
        }
        let staging = folder.join("engine.tmp");
        let _ = std::fs::remove_dir_all(&staging);
        let failed =
            |why: String| SpeechError::Status(format!("The offline Russian voice could not be unpacked: {why}"));
        std::fs::create_dir_all(&staging).map_err(|e| failed(e.to_string()))?;
        let status = std::process::Command::new("tar")
            .arg("-xzf")
            .arg(&archive)
            .arg("-C")
            .arg(&staging)
            .status()
            .map_err(|e| failed(format!("tar did not start: {e}")))?;
        if !status.success() {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(failed(format!("tar stopped with {status}")));
        }
        std::fs::rename(&staging, folder.join("engine")).map_err(|e| failed(e.to_string()))?;
        Ok(program)
    }

    /// The sample rate the voice was trained at, from its config file.
    fn sample_rate(config: &Path) -> Result<u32, SpeechError> {
        let text = std::fs::read_to_string(config)
            .map_err(|e| SpeechError::Status(format!("The offline Russian voice has no config: {e}")))?;
        let json: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| SpeechError::Status(format!("The offline Russian voice has a broken config: {e}")))?;
        json["audio"]["sample_rate"]
            .as_u64()
            .and_then(|rate| u32::try_from(rate).ok())
            .filter(|rate| (8_000..=48_000).contains(rate))
            .ok_or_else(|| SpeechError::Status("The offline Russian voice config does not say its sample rate.".into()))
    }
}

/// Signed 16-bit little-endian samples, as Piper writes them, as floats.
fn samples(raw: &[u8]) -> Vec<f32> {
    raw.as_chunks::<2>().0.iter().map(|pair| f32::from(i16::from_le_bytes(*pair)) / 32_768.0).collect()
}

impl Tts for PiperTts {
    fn id(&self) -> String {
        "piper-local|ru|2023.11.14-2".into()
    }

    fn warm_up(&self) -> BoxFuture<'static, ()> {
        let folder = self.folder.clone();
        let lock = Arc::clone(&self.unpacking);
        async move {
            let _ = tokio::task::spawn_blocking(move || Self::unpack(&folder, &lock)).await;
        }
        .boxed()
    }

    fn synthesize(&self, text: String, voice: Voice) -> BoxFuture<'static, Result<Speech, SpeechError>> {
        let folder = self.folder.clone();
        let lock = Arc::clone(&self.unpacking);
        async move {
            if voice.language != Lang::Ru {
                return Err(SpeechError::Status("The offline Russian voice speaks Russian only.".into()));
            }
            let name = piper_voice_or_default(&voice.name).to_owned();
            let model = folder.join(format!("{name}.onnx"));
            let config = folder.join(format!("{name}.onnx.json"));
            if !model.is_file() || !config.is_file() {
                return Err(not_downloaded(&folder));
            }
            let (unpack_from, rate_from) = (folder.clone(), config.clone());
            let (program, rate) = tokio::task::spawn_blocking(move || {
                Ok::<_, SpeechError>((Self::unpack(&unpack_from, &lock)?, Self::sample_rate(&rate_from)?))
            })
            .await
            .map_err(|e| SpeechError::Status(format!("the offline Russian voice stopped: {e}")))??;

            let slower = 1.0 / f64::from(voice.speed.clamp(0.5, 2.0));
            let mut child = Command::new(&program)
                .arg("--model")
                .arg(&model)
                .arg("--config")
                .arg(&config)
                .arg("--output_raw")
                .arg("--length_scale")
                .arg(format!("{slower:.2}"))
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .spawn()
                .map_err(|e| SpeechError::Status(format!("The offline Russian voice did not start: {e}")))?;
            if let Some(pid) = child.id() {
                crate::polite::lower_process(pid);
            }
            let broken = |e: std::io::Error| SpeechError::Status(format!("The offline Russian voice failed: {e}"));
            let mut stdin = child.stdin.take().ok_or_else(|| SpeechError::Status("no input to the voice".into()))?;
            let mut stdout =
                child.stdout.take().ok_or_else(|| SpeechError::Status("no output from the voice".into()))?;
            // one line is one utterance: a line break inside a sentence would cut it in two
            let line = format!("{}\n", text.replace(['\n', '\r'], " "));
            stdin.write_all(line.as_bytes()).await.map_err(broken)?;
            drop(stdin);
            let mut raw = Vec::new();
            tokio::time::timeout(LONGEST_WAIT, stdout.read_to_end(&mut raw))
                .await
                .map_err(|_| SpeechError::Status("The offline Russian voice took too long.".into()))?
                .map_err(broken)?;
            let _ = child.wait().await;
            if raw.len() < 2 {
                return Err(SpeechError::BadAudio("The offline Russian voice made no sound.".into()));
            }
            Ok(Speech { pcm: samples(&raw), sample_rate: rate })
        }
        .boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_samples_become_floats_in_range() {
        let raw = [0x00, 0x00, 0xFF, 0x7F, 0x00, 0x80];
        let pcm = samples(&raw);
        assert_eq!(pcm.len(), 3);
        assert_eq!(pcm[0], 0.0);
        assert!((pcm[1] - 1.0).abs() < 0.001 && pcm[2] == -1.0);
    }

    #[test]
    fn the_sample_rate_comes_from_the_voice_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v.onnx.json");
        std::fs::write(&path, r#"{"audio":{"sample_rate":22050}}"#).unwrap();
        assert_eq!(PiperTts::sample_rate(&path).unwrap(), 22_050);
        std::fs::write(&path, r#"{"audio":{"sample_rate":5}}"#).unwrap();
        assert!(PiperTts::sample_rate(&path).is_err());
        std::fs::write(&path, "not json").unwrap();
        assert!(PiperTts::sample_rate(&path).is_err());
    }

    #[tokio::test]
    async fn a_voice_that_is_not_downloaded_says_so_and_english_is_not_its_job() {
        let dir = tempfile::tempdir().unwrap();
        let tts = PiperTts::new(dir.path().to_path_buf());
        let russian =
            Voice { name: "ru_RU-irina-medium".into(), speed: 1.0, instructions: String::new(), language: Lang::Ru };
        let error = tts.synthesize("Привет.".into(), russian.clone()).await.unwrap_err().to_string();
        assert!(error.contains("not downloaded yet"), "{error}");
        let english = Voice { language: Lang::En, ..russian };
        let error = tts.synthesize("Hello.".into(), english).await.unwrap_err().to_string();
        assert!(error.contains("Russian only"), "{error}");
    }

    #[test]
    fn a_name_that_is_not_one_of_ours_gets_the_default_voice() {
        assert_eq!(piper_voice_or_default("alloy"), "ru_RU-irina-medium");
        assert_eq!(piper_voice_or_default("ru_RU-denis-medium"), "ru_RU-denis-medium");
        assert_eq!(piper_voice_or_default("../../etc/passwd"), "ru_RU-irina-medium");
    }
}
