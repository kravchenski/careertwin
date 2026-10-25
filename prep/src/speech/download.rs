//! Fetching model files, and refusing any that is not exactly the file that was expected.
//!
//! Every file has a size and a SHA-256 written down in this program. A file that is already on
//! disk is used only if it matches; one that is downloaded is checked before it gets its name, so a
//! damaged, truncated or swapped download never reaches a model loader.

use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone)]
pub struct Asset {
    pub url: String,
    /// Where the file goes under the model folder, with `/` between parts.
    pub relative: String,
    pub bytes: u64,
    pub sha256: String,
}

impl Asset {
    pub fn new(url: impl Into<String>, relative: &str, bytes: u64, sha256: &str) -> Self {
        Self { url: url.into(), relative: relative.to_owned(), bytes, sha256: sha256.to_owned() }
    }
}

#[derive(Debug, Error)]
pub enum DownloadError {
    #[error("{0}")]
    Network(String),
    #[error("{file} is not the file that was expected (its checksum differs), so it was thrown away")]
    Mismatch { file: String },
    #[error("{0}")]
    Disk(String),
    #[error("{0}")]
    Refused(String),
}

/// How far a download has got; the window reads it while the work runs.
#[derive(Debug, Default)]
pub struct Progress {
    pub done: AtomicU64,
    pub total: AtomicU64,
}

impl Progress {
    pub fn percent(&self) -> u8 {
        let total = self.total.load(Ordering::Relaxed);
        if total == 0 {
            return 0;
        }
        ((self.done.load(Ordering::Relaxed) * 100) / total).min(100) as u8
    }
}

/// The SHA-256 of a file, as lower-case hex.
pub fn sha256_of(path: &Path) -> std::io::Result<String> {
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

/// Whether the file is there and is exactly the expected one.
pub fn is_good(path: &Path, asset: &Asset) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.len() == asset.bytes)
        && sha256_of(path).is_ok_and(|h| h == asset.sha256)
}

fn safe_relative(relative: &str) -> Option<PathBuf> {
    let path = Path::new(relative);
    let clean = !relative.is_empty() && path.components().all(|c| matches!(c, Component::Normal(_)));
    clean.then(|| path.to_path_buf())
}

fn is_loopback_http(url: &reqwest::Url) -> bool {
    url.scheme() == "http" && matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]" | "::1"))
}

fn client() -> Result<reqwest::Client, DownloadError> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            let allowed = attempt.url().scheme() == "https" || is_loopback_http(attempt.url());
            if allowed && attempt.previous().len() < 6 { attempt.follow() } else { attempt.stop() }
        }))
        .user_agent(concat!("careertwin-prep/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| DownloadError::Network(e.to_string()))
}

/// The half-written files a download of `assets` leaves under `root` while it runs.
pub fn partial_files(assets: &[Asset], root: &Path) -> Vec<PathBuf> {
    assets.iter().filter_map(|a| safe_relative(&a.relative)).map(|p| root.join(p).with_extension("part")).collect()
}

/// Removes the half-written files an earlier run left behind (it was closed or stopped while it
/// downloaded). Nothing is downloading when the program starts, so none of them is wanted.
pub fn remove_stale_partials(root: &Path) -> usize {
    let mut removed = 0;
    let Ok(entries) = std::fs::read_dir(root) else { return 0 };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            removed += remove_stale_partials(&path);
        } else if path.extension().is_some_and(|e| e == "part") && std::fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// Makes sure every asset is under `root`, downloading what is missing or wrong.
pub async fn ensure(assets: &[Asset], root: &Path, progress: &Progress) -> Result<(), DownloadError> {
    let mut missing = Vec::new();
    for asset in assets {
        let relative = safe_relative(&asset.relative)
            .ok_or_else(|| DownloadError::Refused(format!("{} is not a safe file name", asset.relative)))?;
        let url = reqwest::Url::parse(&asset.url)
            .map_err(|_| DownloadError::Refused(format!("{} is not an address", asset.url)))?;
        if url.scheme() != "https" && !is_loopback_http(&url) {
            return Err(DownloadError::Refused(format!("{} is not an https address", asset.url)));
        }
        if !is_good(&root.join(&relative), asset) {
            missing.push((asset, root.join(relative)));
        }
    }
    progress.done.store(0, Ordering::Relaxed);
    progress.total.store(missing.iter().map(|(a, _)| a.bytes).sum(), Ordering::Relaxed);
    if missing.is_empty() {
        return Ok(());
    }
    let client = client()?;
    for (asset, path) in missing {
        fetch(&client, asset, &path, progress).await?;
    }
    Ok(())
}

async fn fetch(client: &reqwest::Client, asset: &Asset, path: &Path, progress: &Progress) -> Result<(), DownloadError> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| DownloadError::Disk(format!("could not create {}: {e}", parent.display())))?;
    }
    let partial = path.with_extension("part");
    let result = download_to(client, asset, &partial, progress).await;
    match result {
        Ok(()) => tokio::fs::rename(&partial, path)
            .await
            .map_err(|e| DownloadError::Disk(format!("could not save {}: {e}", path.display()))),
        Err(error) => {
            let _ = tokio::fs::remove_file(&partial).await;
            Err(error)
        }
    }
}

async fn download_to(
    client: &reqwest::Client,
    asset: &Asset,
    partial: &Path,
    progress: &Progress,
) -> Result<(), DownloadError> {
    let response = client
        .get(&asset.url)
        .send()
        .await
        .map_err(|e| DownloadError::Network(format!("could not reach {}: {e}", host(&asset.url))))?;
    if !response.status().is_success() {
        return Err(DownloadError::Network(format!("{} answered {}", host(&asset.url), response.status().as_u16())));
    }
    let mut file = tokio::fs::File::create(partial)
        .await
        .map_err(|e| DownloadError::Disk(format!("could not write {}: {e}", partial.display())))?;
    let mut hasher = Sha256::new();
    let mut received = 0u64;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| DownloadError::Network(format!("the download broke off: {e}")))?;
        received += chunk.len() as u64;
        if received > asset.bytes {
            return Err(DownloadError::Mismatch { file: asset.relative.clone() });
        }
        hasher.update(&chunk);
        file.write_all(&chunk)
            .await
            .map_err(|e| DownloadError::Disk(format!("could not write {}: {e}", partial.display())))?;
        progress.done.fetch_add(chunk.len() as u64, Ordering::Relaxed);
    }
    file.flush().await.map_err(|e| DownloadError::Disk(e.to_string()))?;
    drop(file);
    if received != asset.bytes || hex::encode(hasher.finalize()) != asset.sha256 {
        return Err(DownloadError::Mismatch { file: asset.relative.clone() });
    }
    Ok(())
}

fn host(url: &str) -> String {
    reqwest::Url::parse(url).ok().and_then(|u| u.host_str().map(str::to_owned)).unwrap_or_else(|| "the server".into())
}

#[cfg(test)]
mod tests {
    #[test]
    fn half_written_files_from_an_earlier_run_are_removed_and_nothing_else() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("kokoro")).unwrap();
        std::fs::write(dir.path().join("ggml-large.part"), b"half").unwrap();
        std::fs::write(dir.path().join("kokoro/voice.part"), b"half").unwrap();
        std::fs::write(dir.path().join("ggml-small.bin"), b"whole").unwrap();
        assert_eq!(remove_stale_partials(dir.path()), 2);
        assert!(dir.path().join("ggml-small.bin").exists());
        assert!(!dir.path().join("ggml-large.part").exists());
        assert_eq!(remove_stale_partials(&dir.path().join("missing")), 0);
    }

    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn asset(server: &MockServer, relative: &str, content: &[u8]) -> Asset {
        Asset::new(
            format!("{}/{relative}", server.uri()),
            relative,
            content.len() as u64,
            &hex::encode(Sha256::digest(content)),
        )
    }

    async fn serving(server: &MockServer, relative: &str, content: &[u8]) {
        Mock::given(method("GET"))
            .and(path(format!("/{relative}")))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(content.to_vec()))
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn a_missing_file_is_downloaded_checked_and_named() {
        let server = MockServer::start().await;
        let content = vec![7u8; 300_000];
        serving(&server, "model.bin", &content).await;
        let dir = tempfile::tempdir().unwrap();
        let progress = Progress::default();
        ensure(&[asset(&server, "model.bin", &content)], dir.path(), &progress).await.unwrap();
        assert_eq!(std::fs::read(dir.path().join("model.bin")).unwrap(), content);
        assert!(!dir.path().join("model.part").exists());
        assert_eq!(progress.percent(), 100);
    }

    #[tokio::test]
    async fn a_file_with_the_wrong_checksum_is_refused_and_leaves_nothing_behind() {
        let server = MockServer::start().await;
        serving(&server, "model.bin", b"not what was promised....").await;
        let dir = tempfile::tempdir().unwrap();
        let wanted = Asset::new(format!("{}/model.bin", server.uri()), "model.bin", 25, &"0".repeat(64));
        let error = ensure(&[wanted], dir.path(), &Progress::default()).await.unwrap_err();
        assert!(matches!(error, DownloadError::Mismatch { .. }), "{error}");
        assert!(!dir.path().join("model.bin").exists() && !dir.path().join("model.part").exists());
    }

    #[tokio::test]
    async fn a_truncated_or_oversized_download_is_refused() {
        let server = MockServer::start().await;
        serving(&server, "short.bin", b"abc").await;
        serving(&server, "long.bin", b"abcdefghij").await;
        let dir = tempfile::tempdir().unwrap();
        let short = Asset::new(
            format!("{}/short.bin", server.uri()),
            "short.bin",
            10,
            &hex::encode(Sha256::digest(b"abcdefghij")),
        );
        assert!(matches!(
            ensure(&[short], dir.path(), &Progress::default()).await,
            Err(DownloadError::Mismatch { .. })
        ));
        let long =
            Asset::new(format!("{}/long.bin", server.uri()), "long.bin", 3, &hex::encode(Sha256::digest(b"abc")));
        assert!(matches!(ensure(&[long], dir.path(), &Progress::default()).await, Err(DownloadError::Mismatch { .. })));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn a_good_file_on_disk_is_not_fetched_again() {
        let server = MockServer::start().await;
        let content = b"already here".to_vec();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("model.bin"), &content).unwrap();
        ensure(&[asset(&server, "model.bin", &content)], dir.path(), &Progress::default()).await.unwrap();
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_damaged_file_on_disk_is_replaced() {
        let server = MockServer::start().await;
        let content = b"the right bytes".to_vec();
        serving(&server, "model.bin", &content).await;
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("model.bin"), b"the wrong bytes").unwrap();
        ensure(&[asset(&server, "model.bin", &content)], dir.path(), &Progress::default()).await.unwrap();
        assert_eq!(std::fs::read(dir.path().join("model.bin")).unwrap(), content);
    }

    #[tokio::test]
    async fn files_in_subfolders_and_several_at_once() {
        let server = MockServer::start().await;
        serving(&server, "voices/a.pt", b"voice a").await;
        serving(&server, "config.json", b"{}").await;
        let dir = tempfile::tempdir().unwrap();
        let assets = [asset(&server, "voices/a.pt", b"voice a"), asset(&server, "config.json", b"{}")];
        ensure(&assets, dir.path(), &Progress::default()).await.unwrap();
        assert!(dir.path().join("voices/a.pt").is_file() && dir.path().join("config.json").is_file());
    }

    #[tokio::test]
    async fn a_server_error_is_reported_with_its_name_and_leaves_nothing() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).respond_with(ResponseTemplate::new(503)).mount(&server).await;
        let dir = tempfile::tempdir().unwrap();
        let error = ensure(&[asset(&server, "model.bin", b"x")], dir.path(), &Progress::default())
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("503") && error.contains("127.0.0.1"), "{error}");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn names_that_could_escape_the_folder_and_plain_http_elsewhere_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        for bad in ["../evil", "/etc/passwd", "a/../../b", ""] {
            let a = Asset::new("https://huggingface.co/x", bad, 1, &"0".repeat(64));
            assert!(
                matches!(ensure(&[a], dir.path(), &Progress::default()).await, Err(DownloadError::Refused(_))),
                "{bad}"
            );
        }
        let plain = Asset::new("http://example.com/x", "x", 1, &"0".repeat(64));
        assert!(matches!(ensure(&[plain], dir.path(), &Progress::default()).await, Err(DownloadError::Refused(_))));
        let not_an_address = Asset::new("not an address", "x", 1, &"0".repeat(64));
        assert!(matches!(
            ensure(&[not_an_address], dir.path(), &Progress::default()).await,
            Err(DownloadError::Refused(_))
        ));
    }

    #[test]
    fn the_checksum_helper_matches_a_known_value() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("f");
        std::fs::write(&file, b"abc").unwrap();
        assert_eq!(sha256_of(&file).unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert!(sha256_of(&dir.path().join("none")).is_err());
    }

    #[test]
    fn progress_reports_a_percentage_and_never_divides_by_zero() {
        let progress = Progress::default();
        assert_eq!(progress.percent(), 0);
        progress.total.store(200, Ordering::Relaxed);
        progress.done.store(50, Ordering::Relaxed);
        assert_eq!(progress.percent(), 25);
        progress.done.store(900, Ordering::Relaxed);
        assert_eq!(progress.percent(), 100);
    }
}
