//! Capture the screen for coding tasks and slides the interviewer shares.
//!
//! Linux asks a tool of the session (grim on Wayland, which writes a JPEG in about 30 ms; spectacle,
//! gnome-screenshot, maim, scrot); macOS has `screencapture`. Windows captures in-process and is not in this
//! build yet.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use base64::Engine;
use thiserror::Error;

pub const JPEG_QUALITY: u32 = 82;

#[derive(Debug, Error)]
pub enum ScreenshotError {
    #[error("No screenshot tool. Install grim (Wayland, e.g. niri or sway) or spectacle, or maim/scrot on X11.")]
    NoTool,
    #[error("{0} is not installed or cannot capture this session")]
    Unavailable(String),
    #[error("{tool} failed: {why}")]
    Failed { tool: String, why: String },
    #[error("Screenshots on Windows are not in this version of the Rust window yet")]
    Windows,
}

/// A captured screen: JPEG when it could be made, since it is ten times smaller than PNG.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shot {
    pub data: Vec<u8>,
    pub mime: &'static str,
}

impl Shot {
    pub fn data_url(&self) -> String {
        format!("data:{};base64,{}", self.mime, base64::engine::general_purpose::STANDARD.encode(&self.data))
    }
}

/// (name, arguments with `{path}`, works on Wayland). Wayland-capable tools first; ImageMagick's `import` and
/// the X11 tools see only X11 and XWayland windows.
const TOOLS: [(&str, &[&str], bool); 6] = [
    ("grim", &["{path}"], true),
    ("spectacle", &["--background", "--nonotify", "--fullscreen", "--output", "{path}"], true),
    ("gnome-screenshot", &["--file", "{path}"], true),
    ("maim", &["{path}"], false),
    ("scrot", &["{path}"], false),
    ("screencapture", &["-x", "{path}"], false), // built into macOS
];

/// Installed capture tools that can see this session's screen, best first. `has` says whether a program is
/// installed; the real caller looks it up on the PATH.
pub fn tools_with(has: impl Fn(&str) -> bool, wayland: bool) -> Vec<String> {
    TOOLS
        .iter()
        .filter(|(name, _, wayland_ok)| has(name) && (*wayland_ok || !wayland))
        .map(|(name, _, _)| (*name).to_owned())
        .collect()
}

pub fn on_path(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|folder| folder.join(program).is_file()))
}

pub fn available_tools() -> Vec<String> {
    if cfg!(target_os = "windows") {
        return vec!["primary-screen".to_owned(), "all-screens".to_owned()];
    }
    tools_with(on_path, std::env::var_os("WAYLAND_DISPLAY").is_some())
}

fn choose(tool: &str, installed: &[String]) -> Result<String, ScreenshotError> {
    if installed.is_empty() {
        return Err(ScreenshotError::NoTool);
    }
    if tool == "auto" || tool.is_empty() {
        return Ok(installed[0].clone());
    }
    installed.iter().find(|name| *name == tool).cloned().ok_or_else(|| ScreenshotError::Unavailable(tool.to_owned()))
}

/// The monitor that has the keyboard focus, when the compositor can say (niri). A task shown on one monitor is
/// then captured alone: half the pixels, so a faster answer.
pub fn focused_output() -> Option<String> {
    if std::env::var_os("NIRI_SOCKET").is_none() || !on_path("niri") {
        return None;
    }
    let output = Command::new("niri")
        .args(["msg", "--json", "focused-output"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let data: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    data.get("name")?.as_str().filter(|name| !name.is_empty()).map(str::to_owned)
}

fn is_jpeg(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xff, 0xd8])
}

/// Run `command` and give up after `limit`, so a stuck tool cannot hold the window.
fn run(mut command: Command, limit: Duration) -> std::io::Result<std::process::Output> {
    let mut child = command.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    let started = std::time::Instant::now();
    loop {
        if child.try_wait()?.is_some() {
            return child.wait_with_output();
        }
        if started.elapsed() > limit {
            let _ = child.kill();
            return child.wait_with_output();
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// The screen (or one `output`, where the tool can) as a [`Shot`], as fast as the tool allows.
pub fn capture(tool: &str, output: Option<&str>) -> Result<Shot, ScreenshotError> {
    if cfg!(target_os = "windows") {
        return Err(ScreenshotError::Windows);
    }
    let chosen = choose(tool, &available_tools())?;
    let limit = Duration::from_secs(20);
    if chosen == "grim" {
        let mut command = Command::new("grim");
        command.args(["-t", "jpeg", "-q", &JPEG_QUALITY.to_string()]);
        if let Some(output) = output {
            command.args(["-o", output]);
        }
        command.arg("-");
        let result = run(command, limit)
            .map_err(|error| ScreenshotError::Failed { tool: chosen.clone(), why: error.to_string() })?;
        if result.status.success() && is_jpeg(&result.stdout) {
            return Ok(Shot { data: result.stdout, mime: "image/jpeg" });
        }
        // an older grim without JPEG support: fall through to a PNG file
    }
    let directory = std::env::temp_dir().join(format!("careertwin-shot-{}", std::process::id()));
    std::fs::create_dir_all(&directory)
        .map_err(|error| ScreenshotError::Failed { tool: chosen.clone(), why: error.to_string() })?;
    let path = directory.join("screen.png");
    let result = capture_file(&chosen, output, &path, limit);
    let bytes = std::fs::read(&path).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&directory);
    result?;
    if bytes.is_empty() {
        return Err(ScreenshotError::Failed { tool: chosen, why: "no image written".into() });
    }
    Ok(Shot { data: bytes, mime: "image/png" })
}

fn capture_file(tool: &str, output: Option<&str>, path: &Path, limit: Duration) -> Result<(), ScreenshotError> {
    let (_, template, _) =
        TOOLS.iter().find(|(name, _, _)| *name == tool).ok_or_else(|| ScreenshotError::Unavailable(tool.to_owned()))?;
    let mut command = Command::new(tool);
    if tool == "grim"
        && let Some(output) = output
    {
        command.args(["-o", output]);
    }
    command.args(template.iter().map(|part| part.replace("{path}", &path.to_string_lossy())));
    let result = run(command, limit)
        .map_err(|error| ScreenshotError::Failed { tool: tool.to_owned(), why: error.to_string() })?;
    if result.status.success() {
        Ok(())
    } else {
        Err(ScreenshotError::Failed {
            tool: tool.to_owned(),
            why: String::from_utf8_lossy(&result.stderr).trim().chars().take(200).collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wayland_gets_the_tools_that_can_see_it_best_first() {
        let installed = |name: &str| ["grim", "maim", "scrot"].contains(&name);
        assert_eq!(tools_with(installed, true), ["grim"]);
        assert_eq!(tools_with(installed, false), ["grim", "maim", "scrot"]);
        assert!(tools_with(|_| false, true).is_empty());
        assert_eq!(tools_with(|name| name == "spectacle", true), ["spectacle"]);
    }

    #[test]
    fn a_tool_is_chosen_by_name_or_automatically() {
        let installed = vec!["grim".to_owned(), "maim".to_owned()];
        assert_eq!(choose("auto", &installed).unwrap(), "grim");
        assert_eq!(choose("maim", &installed).unwrap(), "maim");
        assert!(matches!(choose("scrot", &installed), Err(ScreenshotError::Unavailable(name)) if name == "scrot"));
        assert!(matches!(choose("auto", &[]), Err(ScreenshotError::NoTool)));
    }

    #[test]
    fn a_shot_is_sent_as_a_data_url() {
        assert_eq!(Shot { data: b"abc".to_vec(), mime: "image/jpeg" }.data_url(), "data:image/jpeg;base64,YWJj");
        assert_eq!(Shot { data: b"abc".to_vec(), mime: "image/png" }.data_url(), "data:image/png;base64,YWJj");
        assert!(is_jpeg(&[0xff, 0xd8, 0xff]) && !is_jpeg(b"\x89PNG"));
    }

    #[test]
    fn a_stuck_tool_is_stopped() {
        let mut command = Command::new("sleep");
        command.arg("5");
        let started = std::time::Instant::now();
        let result = run(command, Duration::from_millis(100)).unwrap();
        assert!(!result.status.success() && started.elapsed() < Duration::from_secs(2));
    }
}
