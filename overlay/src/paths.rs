//! Where the overlay keeps its files, and where ABOUT.md is.
//!
//! The same folders as the Python overlay: `data/interview/settings.json`, `data/interview/context.json`,
//! `data/interviews/*.md` (transcripts). `CAREERTWIN_DATA_DIR` and `CAREERTWIN_ABOUT_PATH` mean what they mean
//! there; without them a `data` folder in the working directory (or above it) that already holds the
//! overlay's files is used, and last the platform's data folder.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Paths {
    pub data: PathBuf,
    pub about: PathBuf,
}

impl Paths {
    pub fn under(data: impl Into<PathBuf>) -> Self {
        let data = data.into();
        let about = data
            .parent()
            .map(|parent| parent.join("ABOUT.md"))
            .filter(|path| path.is_file())
            .unwrap_or_else(|| data.join("ABOUT.md"));
        Self { data, about }
    }

    pub fn discover() -> Self {
        let exe_folder = std::env::current_exe().ok().and_then(|exe| exe.parent().map(Path::to_path_buf));
        let data = choose_data(
            std::env::var_os("CAREERTWIN_DATA_DIR").map(PathBuf::from),
            exe_folder.as_deref(),
            find_checkout_data,
            platform_data,
        );
        let mut paths = Self::under(data);
        if let Some(about) = std::env::var_os("CAREERTWIN_ABOUT_PATH") {
            paths.about = PathBuf::from(about);
        }
        paths
    }

    pub fn interview_dir(&self) -> PathBuf {
        self.data.join("interview")
    }

    pub fn settings_file(&self) -> PathBuf {
        self.interview_dir().join("settings.json")
    }

    /// Settings only this program knows (the icon, the update check): kept apart so the Python overlay, which
    /// rewrites `settings.json` from its own list of fields, can never drop them.
    pub fn app_file(&self) -> PathBuf {
        self.interview_dir().join("app.json")
    }

    pub fn context_file(&self) -> PathBuf {
        self.interview_dir().join("context.json")
    }

    pub fn log_file(&self) -> PathBuf {
        self.interview_dir().join("interview.log")
    }

    pub fn transcripts_dir(&self) -> PathBuf {
        self.data.join("interviews")
    }

    /// Speech models downloaded for Whisper.
    pub fn models_dir(&self) -> PathBuf {
        self.data.join("models")
    }

    pub fn exports_dir(&self) -> PathBuf {
        self.data.join("exports")
    }
}

/// Where the data lives: the folder named by the environment; a `data` folder next to the program (the portable
/// zip, and an installation that came from the Python window, whose settings are kept there); a checkout's `data`
/// folder that already holds the window's files; else the platform's own data folder.
pub fn choose_data(
    environment: Option<PathBuf>,
    program_folder: Option<&Path>,
    checkout: impl FnOnce() -> Option<PathBuf>,
    platform: impl FnOnce() -> PathBuf,
) -> PathBuf {
    environment
        .or_else(|| program_folder.map(|folder| folder.join("data")).filter(|data| data.is_dir()))
        .or_else(checkout)
        .unwrap_or_else(platform)
}

fn find_checkout_data() -> Option<PathBuf> {
    let start = std::env::current_dir().ok()?;
    start.ancestors().take(4).map(|folder| folder.join("data")).find(|data: &PathBuf| has_overlay_files(data))
}

fn has_overlay_files(data: &Path) -> bool {
    data.join("interview").join("settings.json").is_file() || data.join("interview").join("context.json").is_file()
}

fn platform_data() -> PathBuf {
    directories::ProjectDirs::from("", "", "careertwin")
        .map_or_else(|| PathBuf::from("data"), |dirs| dirs.data_dir().to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_files_sit_where_the_python_overlay_keeps_them() {
        let paths = Paths::under("/x/data");
        assert_eq!(paths.settings_file(), Path::new("/x/data/interview/settings.json"));
        assert_eq!(paths.transcripts_dir(), Path::new("/x/data/interviews"));
        assert_eq!(paths.log_file(), Path::new("/x/data/interview/interview.log"));
        assert_ne!(paths.app_file(), paths.settings_file());
    }

    #[test]
    fn about_is_next_to_the_data_folder_when_it_is_there() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("data")).unwrap();
        std::fs::write(dir.path().join("ABOUT.md"), "# me").unwrap();
        assert_eq!(Paths::under(dir.path().join("data")).about, dir.path().join("ABOUT.md"));
        let alone = tempfile::tempdir().unwrap();
        assert_eq!(Paths::under(alone.path().join("data")).about, alone.path().join("data").join("ABOUT.md"));
    }

    #[test]
    fn data_is_taken_from_the_environment_then_next_to_the_program_then_the_checkout_then_the_platform() {
        let program = tempfile::tempdir().unwrap();
        let platform = || PathBuf::from("/platform");
        let checkout = || Some(PathBuf::from("/checkout/data"));
        let named = Some(PathBuf::from("/named"));
        assert_eq!(choose_data(named, Some(program.path()), checkout, platform), Path::new("/named"));
        // no `data` folder next to the program: the checkout, then the platform
        assert_eq!(choose_data(None, Some(program.path()), checkout, platform), Path::new("/checkout/data"));
        assert_eq!(choose_data(None, Some(program.path()), || None, platform), Path::new("/platform"));
        std::fs::create_dir(program.path().join("data")).unwrap();
        assert_eq!(choose_data(None, Some(program.path()), checkout, platform), program.path().join("data"));
        assert_eq!(choose_data(None, None, || None, platform), Path::new("/platform"));
    }
}
