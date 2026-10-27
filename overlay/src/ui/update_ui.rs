//! The window that says a new version is there.

use egui::{RichText, ScrollArea};

use super::theme::{ACCENT, DANGER, GOOD, MUTED, ON_ACCENT, STRONG, semibold};
use super::{App, text};
use crate::update::{Available, System, pick_asset};

#[derive(Default)]
pub struct State {
    pub available: Option<Available>,
    pub dismissed: bool,
    pub requested: bool,
    pub checking: bool,
    /// The answer of a check the person asked for: the version that is current, or why it failed.
    pub last: Option<Result<String, String>>,
    pub progress: Option<(u64, u64)>,
    pub downloaded: Option<Result<std::path::PathBuf, String>>,
}

fn megabytes(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1_048_576.0)
}

impl App {
    pub fn update_window(&mut self, ctx: &egui::Context) {
        let lang = self.settings.ui_code();
        let t = |key: &str| text::t(&lang, key, &[]);
        let tf = |key: &str, values: &[(&str, &str)]| text::t(&lang, key, values);
        let _ = (&t, &tf);
        let Some(available) = self.update_ui.available.clone().filter(|_| !self.update_ui.dismissed) else { return };
        let version = available.release.version();
        let asset = pick_asset(&available.release, System::current()).cloned();
        let (mut later, mut skip, mut download, mut page, mut folder) = (false, false, false, false, false);
        egui::Modal::new(egui::Id::unique("update")).show(ctx, |ui| {
            ui.set_width(460.0);
            ui.label(RichText::new(tf("update_title", &[("version", &version)])).font(semibold(20.0)).color(STRONG));
            ui.label(RichText::new(tf("update_have", &[("version", &available.current)])).color(MUTED));
            if !available.release.notes.trim().is_empty() {
                ui.add_space(8.0);
                ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                    for line in notes_lines(&available.release.notes) {
                        ui.label(line);
                    }
                });
            }
            ui.add_space(10.0);
            if let Some((done, total)) = self.update_ui.progress {
                ui.label(
                    RichText::new(tf(
                        "update_downloading",
                        &[("done", &megabytes(done)), ("total", &megabytes(total))],
                    ))
                    .color(ACCENT),
                );
                ui.add(egui::ProgressBar::new(if total > 0 { done as f32 / total as f32 } else { 0.0 }));
            }
            match &self.update_ui.downloaded {
                Some(Ok(path)) => {
                    ui.label(
                        RichText::new(tf("update_verified", &[("path", &path.display().to_string())])).color(GOOD),
                    );
                    folder = ui.button(t("open_folder")).clicked();
                }
                Some(Err(why)) => {
                    ui.label(RichText::new(tf("update_error", &[("why", why)])).color(DANGER));
                }
                None => {
                    if asset.is_none() && System::current() == System::Linux {
                        ui.label(RichText::new(t("update_linux")).color(MUTED));
                    }
                }
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let downloading = self.update_ui.progress.is_some();
                if asset.is_some() && !matches!(self.update_ui.downloaded, Some(Ok(_))) {
                    download = ui
                        .add_enabled(
                            !downloading,
                            egui::Button::new(RichText::new(t("update_download")).color(ON_ACCENT)).fill(ACCENT),
                        )
                        .clicked();
                }
                page = ui.button(t("update_page")).clicked();
                later = ui.button(t("update_later")).clicked();
                skip = ui.add(egui::Button::new(RichText::new(t("update_skip")).color(MUTED)).frame(false)).clicked();
            });
        });
        if download {
            self.download_update(&available);
        }
        if page && !available.release.page.is_empty() {
            ctx.open_url(egui::OpenUrl::new_tab(&available.release.page));
        }
        if folder && let Some(Ok(path)) = &self.update_ui.downloaded {
            super::settings_ui::open_path(path.parent().unwrap_or(path));
        }
        if later {
            self.update_ui.dismissed = true;
        }
        if skip {
            self.app.skipped_version = version;
            self.app_changed();
            self.update_ui.dismissed = true;
        }
    }
}

/// The notes of a release without the Markdown marks: headings and `- ` bullets read as plain lines.
pub fn notes_lines(notes: &str) -> Vec<String> {
    notes
        .lines()
        .map(|line| {
            let line = line.trim();
            if let Some(heading) = line.strip_prefix('#') {
                return heading.trim_start_matches('#').trim().to_owned();
            }
            match line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
                Some(item) => format!("– {}", item.trim()),
                None => line.to_owned(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::notes_lines;

    #[test]
    fn markdown_marks_are_taken_off_the_notes() {
        assert_eq!(
            notes_lines("## What is new\n- A key\n* Another\nPlain line\n\n### Fixes"),
            ["What is new", "– A key", "– Another", "Plain line", "", "Fixes"]
        );
    }
}
