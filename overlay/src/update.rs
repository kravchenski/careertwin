//! A new version of the app: ask GitHub for the latest release, say so, and download the installer for this
//! system next to a SHA256 check. Nothing is run: starting the installer stays the person's decision.

use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_util::StreamExt;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::io::AsyncWriteExt;

use crate::settings::AppSettings;

pub const REPO: &str = "kravchenski/careertwin";
pub const API: &str = "https://api.github.com";
/// How often the app looks for a new version by itself.
pub const CHECK_EVERY_SECONDS: i64 = 24 * 60 * 60;
const SUMS: &str = "SHA256SUMS.txt";
const NOTES_CHARS: usize = 1500;

#[derive(Debug, Error)]
pub enum UpdateError {
    #[error("Could not ask GitHub for the latest version: {0}")]
    Network(String),
    #[error("GitHub's answer about the latest version was unreadable")]
    Unreadable,
    #[error("This release has no installer for this system")]
    NoInstaller,
    #[error("The release has no SHA256 list, so the download could not be checked")]
    NoChecksum,
    #[error("The download does not match its SHA256 and was deleted")]
    Mismatch,
    #[error("The download address is not a GitHub release address: {0}")]
    Address(String),
    #[error("Could not save the installer: {0}")]
    Disk(String),
}

/// `0.5.0` or `v0.5.0`, as numbers; a pre-release suffix is ignored.
pub fn parse_version(text: &str) -> Option<(u32, u32, u32)> {
    let core = text.trim().trim_start_matches(['v', 'V']).split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|part| part.parse::<u32>().ok());
    Some((parts.next()??, parts.next()??, parts.next()??))
}

/// Whether `latest` is a higher version than `current`; a version that cannot be read is never newer.
pub fn is_newer(current: &str, latest: &str) -> bool {
    matches!((parse_version(current), parse_version(latest)), (Some(current), Some(latest)) if latest > current)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    /// As tagged: `v0.5.0`.
    pub tag: String,
    pub notes: String,
    /// The release page, to read about it or to download by hand.
    pub page: String,
    pub assets: Vec<Asset>,
}

impl Release {
    pub fn version(&self) -> String {
        self.tag.trim_start_matches(['v', 'V']).to_owned()
    }
}

#[derive(Deserialize)]
struct RawAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
}

#[derive(Deserialize)]
struct RawRelease {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    assets: Vec<RawAsset>,
}

pub fn parse_release(json: &str) -> Result<Release, UpdateError> {
    let raw: RawRelease = serde_json::from_str(json).map_err(|_| UpdateError::Unreadable)?;
    Ok(Release {
        tag: raw.tag_name,
        notes: raw.body.unwrap_or_default().chars().take(NOTES_CHARS).collect(),
        page: raw.html_url,
        assets: raw.assets.into_iter().map(|asset| Asset { name: asset.name, url: asset.browser_download_url, size: asset.size }).collect(),
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum System {
    Windows,
    MacOs,
    Linux,
}

impl System {
    pub fn current() -> Self {
        if cfg!(target_os = "windows") {
            System::Windows
        } else if cfg!(target_os = "macos") {
            System::MacOs
        } else {
            System::Linux
        }
    }
}

/// The installer for this system: the setup program on Windows (the zip when there is none), the disk image
/// on macOS. Linux has none: the app is installed with `uv tool` or from the repository.
pub fn pick_asset(release: &Release, system: System) -> Option<&Asset> {
    let find = |needle: &str, ext: &str| release.assets.iter().find(|asset| asset.name.contains(needle) && asset.name.ends_with(ext));
    match system {
        System::Windows => find("windows-x64-setup", ".exe").or_else(|| find("windows-x64", ".zip")),
        System::MacOs => find("macos", ".dmg"),
        System::Linux => None,
    }
}

/// The SHA256 of `file` from a `sha256sum` list (`<hex>  <name>` per line).
pub fn checksum_for(list: &str, file: &str) -> Option<String> {
    list.lines().find_map(|line| {
        let (hash, name) = line.split_once(char::is_whitespace)?;
        (name.trim().trim_start_matches('*') == file && hash.len() == 64).then(|| hash.to_lowercase())
    })
}

/// What the window tells the person.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Available {
    pub current: String,
    pub release: Release,
}

/// `Some` when the release is newer than this version and the person has not chosen to skip it.
pub fn decide(current: &str, release: Release, skipped: &str) -> Option<Available> {
    let latest = release.version();
    (is_newer(current, &latest) && latest != skipped.trim_start_matches(['v', 'V'])).then(|| Available { current: current.to_owned(), release })
}

/// Whether the app should look for a new version now: it is allowed to, and it has been a day.
pub fn due(app: &AppSettings, now_seconds: i64) -> bool {
    app.check_updates && now_seconds - app.last_update_check >= CHECK_EVERY_SECONDS
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(20))
        .user_agent(concat!("careertwin-overlay/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("the network client has fixed settings")
}

/// The latest release of the project. `api` is `API` outside tests.
pub async fn latest(api: &str) -> Result<Release, UpdateError> {
    let response = client()
        .get(format!("{api}/repos/{REPO}/releases/latest"))
        .header("accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|error| UpdateError::Network(if error.is_timeout() { "timed out".into() } else { "no connection".into() }))?;
    if !response.status().is_success() {
        return Err(UpdateError::Network(format!("GitHub answered {}", response.status().as_u16())));
    }
    parse_release(&response.text().await.map_err(|_| UpdateError::Unreadable)?)
}

/// Only GitHub's own release files are downloaded.
fn trusted(url: &str, allow_local: bool) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return allow_local && (url.starts_with("http://127.0.0.1") || url.starts_with("http://localhost"));
    };
    let host = rest.split(['/', ':']).next().unwrap_or("");
    matches!(host, "github.com" | "objects.githubusercontent.com" | "release-assets.githubusercontent.com")
}

/// The folder an installer is saved in: Downloads, or the temporary folder when there is none.
pub fn downloads_dir() -> PathBuf {
    directories::UserDirs::new().and_then(|dirs| dirs.download_dir().map(Path::to_path_buf)).unwrap_or_else(std::env::temp_dir)
}

/// Download the installer of `release` into `folder`, check its SHA256 against the release's list, and keep it
/// only when they agree. `progress(done, total)` is told as it goes. The file is never started.
pub async fn download(release: &Release, system: System, folder: &Path, allow_local: bool, mut progress: impl FnMut(u64, u64)) -> Result<PathBuf, UpdateError> {
    let asset = pick_asset(release, system).ok_or(UpdateError::NoInstaller)?;
    let sums = release.assets.iter().find(|asset| asset.name == SUMS).ok_or(UpdateError::NoChecksum)?;
    for address in [&asset.url, &sums.url] {
        if !trusted(address, allow_local) {
            return Err(UpdateError::Address(address.clone()));
        }
    }
    let http = reqwest::Client::builder().connect_timeout(Duration::from_secs(8)).user_agent(concat!("careertwin-overlay/", env!("CARGO_PKG_VERSION"))).build().expect("the network client has fixed settings");
    let list = http.get(&sums.url).send().await.and_then(reqwest::Response::error_for_status).map_err(|error| UpdateError::Network(error.to_string()))?.text().await.map_err(|error| UpdateError::Network(error.to_string()))?;
    let wanted = checksum_for(&list, &asset.name).ok_or(UpdateError::NoChecksum)?;
    std::fs::create_dir_all(folder).map_err(|error| UpdateError::Disk(error.to_string()))?;
    let target = folder.join(&asset.name);
    let part = folder.join(format!("{}.part", asset.name));
    let response = http.get(&asset.url).send().await.and_then(reqwest::Response::error_for_status).map_err(|error| UpdateError::Network(error.to_string()))?;
    let total = response.content_length().unwrap_or(asset.size);
    let mut file = tokio::fs::File::create(&part).await.map_err(|error| UpdateError::Disk(error.to_string()))?;
    let mut hasher = Sha256::new();
    let mut done = 0u64;
    let mut stream = response.bytes_stream();
    while let Some(piece) = stream.next().await {
        let piece = piece.map_err(|error| UpdateError::Network(error.to_string()))?;
        hasher.update(&piece);
        file.write_all(&piece).await.map_err(|error| UpdateError::Disk(error.to_string()))?;
        done += piece.len() as u64;
        progress(done, total);
    }
    file.flush().await.map_err(|error| UpdateError::Disk(error.to_string()))?;
    drop(file);
    if hex::encode(hasher.finalize()) != wanted {
        let _ = std::fs::remove_file(&part);
        return Err(UpdateError::Mismatch);
    }
    std::fs::rename(&part, &target).map_err(|error| UpdateError::Disk(error.to_string()))?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::path;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn release(base: &str, tag: &str) -> Release {
        let asset = |name: &str| Asset { name: name.into(), url: format!("{base}/{name}"), size: 4 };
        Release {
            tag: tag.into(),
            notes: "Notes".into(),
            page: "https://github.com/kravchenski/careertwin/releases/tag/v0.6.0".into(),
            assets: vec![
                asset("CareerTwin-Interview-v0.6.0-macos-arm64.dmg"),
                asset("CareerTwin-Interview-v0.6.0-windows-x64-setup.exe"),
                asset("CareerTwin-Interview-v0.6.0-windows-x64.zip"),
                asset("SHA256SUMS.txt"),
            ],
        }
    }

    #[test]
    fn versions_are_read_and_compared_as_numbers() {
        assert_eq!(parse_version("v0.5.0"), Some((0, 5, 0)));
        assert_eq!(parse_version("0.10.2-rc1"), Some((0, 10, 2)));
        assert_eq!(parse_version("latest"), None);
        assert!(is_newer("0.5.0", "v0.5.1") && is_newer("0.9.9", "0.10.0") && is_newer("0.5.0", "1.0.0"));
        assert!(!is_newer("0.5.0", "0.5.0") && !is_newer("0.5.1", "0.5.0") && !is_newer("0.5.0", "nonsense") && !is_newer("x", "0.6.0"));
    }

    #[test]
    fn a_github_release_is_read() {
        let text = r#"{"tag_name": "v0.6.0", "body": "What is new", "html_url": "https://github.com/kravchenski/careertwin/releases/tag/v0.6.0",
            "assets": [{"name": "SHA256SUMS.txt", "browser_download_url": "https://github.com/x/SHA256SUMS.txt", "size": 300},
                       {"name": "other", "browser_download_url": "https://github.com/x/other"}]}"#;
        let release = parse_release(text).unwrap();
        assert_eq!((release.version().as_str(), release.notes.as_str(), release.assets.len(), release.assets[1].size), ("0.6.0", "What is new", 2, 0));
        assert!(matches!(parse_release("{}"), Err(UpdateError::Unreadable)));
        assert_eq!(parse_release(&format!(r#"{{"tag_name": "v1", "body": "{}"}}"#, "x".repeat(5000))).unwrap().notes.len(), NOTES_CHARS);
    }

    #[test]
    fn the_installer_depends_on_the_system() {
        let release = release("https://github.com/x", "v0.6.0");
        assert!(pick_asset(&release, System::Windows).unwrap().name.ends_with("windows-x64-setup.exe"));
        assert!(pick_asset(&release, System::MacOs).unwrap().name.ends_with(".dmg"));
        assert!(pick_asset(&release, System::Linux).is_none());
        let mut zip_only = release.clone();
        zip_only.assets.retain(|asset| !asset.name.ends_with(".exe"));
        assert!(pick_asset(&zip_only, System::Windows).unwrap().name.ends_with("windows-x64.zip"));
    }

    #[test]
    fn a_sha256_list_gives_the_hash_of_a_file() {
        let hash = "a".repeat(64);
        let list = format!("{hash}  one.exe\n{}  *two.zip\nshort  three\n", "B".repeat(64));
        assert_eq!(checksum_for(&list, "one.exe"), Some(hash));
        assert_eq!(checksum_for(&list, "two.zip"), Some("b".repeat(64))); // a binary-mode star and capitals are fine
        assert_eq!(checksum_for(&list, "three"), None);
        assert_eq!(checksum_for(&list, "missing"), None);
    }

    #[test]
    fn a_newer_release_is_announced_unless_it_was_skipped() {
        let found = release("https://github.com/x", "v0.6.0");
        assert!(decide("0.5.0", found.clone(), "").is_some());
        assert!(decide("0.5.0", found.clone(), "0.6.0").is_none() && decide("0.5.0", found.clone(), "v0.6.0").is_none());
        assert!(decide("0.6.0", found.clone(), "").is_none() && decide("0.7.0", found, "").is_none());
    }

    #[test]
    fn the_app_looks_once_a_day_and_only_when_allowed() {
        let mut app = AppSettings::default();
        assert!(due(&app, 1_000_000));
        app.last_update_check = 1_000_000 - 3600;
        assert!(!due(&app, 1_000_000));
        app.last_update_check = 1_000_000 - CHECK_EVERY_SECONDS;
        assert!(due(&app, 1_000_000));
        app.check_updates = false;
        assert!(!due(&app, 10_000_000));
    }

    #[test]
    fn only_github_release_files_are_trusted() {
        assert!(trusted("https://github.com/kravchenski/careertwin/releases/download/v1/x.exe", false));
        assert!(trusted("https://objects.githubusercontent.com/x", false));
        assert!(!trusted("https://github.com.evil.example/x", false) && !trusted("http://github.com/x", false) && !trusted("https://example.com/x.exe", false));
        assert!(!trusted("http://127.0.0.1:1/x", false) && trusted("http://127.0.0.1:1/x", true));
    }

    #[tokio::test]
    async fn the_latest_release_is_asked_from_github() {
        let server = MockServer::start().await;
        Mock::given(path(format!("/repos/{REPO}/releases/latest"))).respond_with(ResponseTemplate::new(200).set_body_string(r#"{"tag_name": "v0.6.0", "assets": []}"#)).mount(&server).await;
        assert_eq!(latest(&server.uri()).await.unwrap().tag, "v0.6.0");
        let down = MockServer::start().await;
        Mock::given(path(format!("/repos/{REPO}/releases/latest"))).respond_with(ResponseTemplate::new(403)).mount(&down).await;
        assert!(matches!(latest(&down.uri()).await, Err(UpdateError::Network(text)) if text.contains("403")));
        assert!(matches!(latest("http://127.0.0.1:1").await, Err(UpdateError::Network(_))));
    }

    async fn serve(body: &'static [u8], listed: &str) -> (MockServer, Release) {
        let server = MockServer::start().await;
        let release = release(&server.uri(), "v0.6.0");
        let name = "CareerTwin-Interview-v0.6.0-windows-x64-setup.exe";
        Mock::given(path(format!("/{name}"))).respond_with(ResponseTemplate::new(200).set_body_bytes(body)).mount(&server).await;
        Mock::given(path("/SHA256SUMS.txt")).respond_with(ResponseTemplate::new(200).set_body_string(format!("{listed}  {name}\n"))).mount(&server).await;
        (server, release)
    }

    #[tokio::test]
    async fn an_installer_is_downloaded_and_checked_and_never_started() {
        let (_server, release) = serve(b"data", &hex::encode(Sha256::digest(b"data"))).await;
        let dir = tempfile::tempdir().unwrap();
        let mut seen = Vec::new();
        let path = download(&release, System::Windows, dir.path(), true, |done, total| seen.push((done, total))).await.unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"data");
        assert_eq!(seen.last(), Some(&(4, 4)));
        assert!(!dir.path().join("CareerTwin-Interview-v0.6.0-windows-x64-setup.exe.part").exists());
    }

    #[tokio::test]
    async fn a_download_that_does_not_match_is_deleted() {
        let (_server, release) = serve(b"data", &"0".repeat(64)).await;
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(download(&release, System::Windows, dir.path(), true, |_, _| {}).await, Err(UpdateError::Mismatch)));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn nothing_is_downloaded_without_a_checksum_an_installer_or_a_github_address() {
        let dir = tempfile::tempdir().unwrap();
        let mut bare = release("https://github.com/x", "v0.6.0");
        bare.assets.retain(|asset| asset.name != SUMS);
        assert!(matches!(download(&bare, System::Windows, dir.path(), false, |_, _| {}).await, Err(UpdateError::NoChecksum)));
        assert!(matches!(download(&release("https://github.com/x", "v0.6.0"), System::Linux, dir.path(), false, |_, _| {}).await, Err(UpdateError::NoInstaller)));
        assert!(matches!(download(&release("https://example.com", "v0.6.0"), System::Windows, dir.path(), false, |_, _| {}).await, Err(UpdateError::Address(_))));
    }
}
