//! The speech models that can be downloaded, and the download: streamed to a file next to its name, checked against
//! its SHA256, and kept only when they agree.

use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SttModel {
    /// What the settings call it: `tiny`, `base`, `small`, `medium`, `large-v3-turbo`.
    pub id: &'static str,
    pub file: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
}

const BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

impl SttModel {
    pub fn url(&self) -> String {
        format!("{BASE_URL}/{}", self.file)
    }

    pub fn path_in(&self, dir: &Path) -> PathBuf {
        dir.join(self.file)
    }

    pub fn megabytes(&self) -> u64 {
        self.bytes / 1_000_000
    }
}

/// Quantized models: all of them understand many languages, so a slip into Russian is still heard.
pub const STT_MODELS: [SttModel; 5] = [
    SttModel {
        id: "tiny",
        file: "ggml-tiny-q5_1.bin",
        bytes: 32_152_673,
        sha256: "818710568da3ca15689e31a743197b520007872ff9576237bda97bd1b469c3d7",
    },
    SttModel {
        id: "base",
        file: "ggml-base-q5_1.bin",
        bytes: 59_707_625,
        sha256: "422f1ae452ade6f30a004d7e5c6a43195e4433bc370bf23fac9cc591f01a8898",
    },
    SttModel {
        id: "small",
        file: "ggml-small-q5_1.bin",
        bytes: 190_085_487,
        sha256: "ae85e4a935d7a567bd102fe55afc16bb595bdb618e11b2fc7591bc08120411bb",
    },
    SttModel {
        id: "medium",
        file: "ggml-medium-q5_0.bin",
        bytes: 539_212_467,
        sha256: "19fea4b380c3a618ec4723c3eef2eb785ffba0d0538cf43f8f235e7b3b34220f",
    },
    SttModel {
        id: "large-v3-turbo",
        file: "ggml-large-v3-turbo-q5_0.bin",
        bytes: 574_041_195,
        sha256: "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
    },
];

pub fn stt_model(id: &str) -> Option<&'static SttModel> {
    STT_MODELS.iter().find(|model| model.id == id)
}

#[derive(Debug, Error)]
pub enum DownloadError {
    #[error("Could not download the speech model: {0}")]
    Network(String),
    #[error("Could not save the speech model: {0}")]
    Disk(String),
    #[error("The downloaded speech model does not match its SHA256 and was deleted")]
    Mismatch,
}

pub fn sha256_of(path: &Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Whether the model is already on disk, whole and as it should be.
pub fn is_present(model: &SttModel, dir: &Path) -> bool {
    let path = model.path_in(dir);
    std::fs::metadata(&path).is_ok_and(|meta| meta.len() == model.bytes)
        && sha256_of(&path).is_ok_and(|hash| hash == model.sha256)
}

/// Download `model` into `dir` unless it is there. `progress(done, total)` is told as it goes. `base` is
/// [`BASE_URL`] outside tests.
pub async fn ensure(
    model: &SttModel,
    dir: &Path,
    base: Option<&str>,
    mut progress: impl FnMut(u64, u64),
) -> Result<PathBuf, DownloadError> {
    let target = model.path_in(dir);
    if is_present(model, dir) {
        return Ok(target);
    }
    std::fs::create_dir_all(dir).map_err(|error| DownloadError::Disk(error.to_string()))?;
    let part = dir.join(format!("{}.part", model.file));
    let url = base.map_or_else(|| model.url(), |base| format!("{base}/{}", model.file));
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .user_agent(concat!("careertwin-overlay/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| DownloadError::Network(error.to_string()))?;
    let response = client
        .get(url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|error| DownloadError::Network(error.to_string()))?;
    let total = response.content_length().unwrap_or(model.bytes);
    let mut file = tokio::fs::File::create(&part).await.map_err(|error| DownloadError::Disk(error.to_string()))?;
    let mut hasher = Sha256::new();
    let mut done = 0u64;
    let mut stream = response.bytes_stream();
    while let Some(piece) = stream.next().await {
        let piece = piece.map_err(|error| DownloadError::Network(error.to_string()))?;
        hasher.update(&piece);
        file.write_all(&piece).await.map_err(|error| DownloadError::Disk(error.to_string()))?;
        done += piece.len() as u64;
        progress(done, total);
    }
    file.flush().await.map_err(|error| DownloadError::Disk(error.to_string()))?;
    drop(file);
    if hex::encode(hasher.finalize()) != model.sha256 {
        let _ = std::fs::remove_file(&part);
        return Err(DownloadError::Mismatch);
    }
    std::fs::rename(&part, &target).map_err(|error| DownloadError::Disk(error.to_string()))?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::path;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn every_setting_has_a_model_with_a_file_a_size_and_a_hash() {
        for id in crate::settings::WHISPER_MODELS {
            let model = stt_model(id).unwrap_or_else(|| panic!("no model for {id}"));
            assert!(model.file.starts_with("ggml-") && model.sha256.len() == 64 && model.bytes > 1_000_000, "{id}");
            assert!(model.url().ends_with(model.file));
        }
        assert!(stt_model("huge").is_none());
        assert_eq!(stt_model("small").unwrap().megabytes(), 190);
    }

    fn tiny(bytes: &'static [u8]) -> SttModel {
        // a model that is a few bytes, for the test server
        let hash: &'static str = Box::leak(hex::encode(Sha256::digest(bytes)).into_boxed_str());
        SttModel { id: "tiny", file: "ggml-test.bin", bytes: bytes.len() as u64, sha256: hash }
    }

    #[tokio::test]
    async fn a_model_is_downloaded_checked_and_not_downloaded_twice() {
        let server = MockServer::start().await;
        Mock::given(path("/ggml-test.bin"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(&b"model-bytes"[..]))
            .expect(1)
            .mount(&server)
            .await;
        let model = tiny(b"model-bytes");
        let dir = tempfile::tempdir().unwrap();
        let mut seen = Vec::new();
        let path =
            ensure(&model, dir.path(), Some(&server.uri()), |done, total| seen.push((done, total))).await.unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"model-bytes");
        assert_eq!(seen.last(), Some(&(11, 11)));
        assert!(is_present(&model, dir.path()));
        ensure(&model, dir.path(), Some(&server.uri()), |_, _| {}).await.unwrap(); // the second call asks for nothing (expect(1))
    }

    #[tokio::test]
    async fn a_download_that_does_not_match_is_deleted() {
        let server = MockServer::start().await;
        Mock::given(path("/ggml-test.bin"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(&b"other-bytes"[..]))
            .mount(&server)
            .await;
        let model = tiny(b"model-bytes");
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            ensure(&model, dir.path(), Some(&server.uri()), |_, _| {}).await,
            Err(DownloadError::Mismatch)
        ));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
        let missing = MockServer::start().await;
        assert!(matches!(
            ensure(&model, dir.path(), Some(&missing.uri()), |_, _| {}).await,
            Err(DownloadError::Network(_))
        ));
    }
}
