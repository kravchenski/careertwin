//! What you did last, in a list that opens from the top bar, with the work running in the background.

use egui::{
    Align2, Color32, CornerRadius, Frame, Id, Margin, Order, RichText, ScrollArea, Sense, Stroke, StrokeKind, TextEdit,
    Ui, vec2,
};

use super::theme::{self, BERRY, FAINT, LINE, MUTED, RAISED, SNOW, SURFACE};
use super::widgets::{self, Icon, icon_button};
use super::{Core, Nav};
use crate::i18n::K;
use crate::session::{Kind, Session, Status, Track, ago};

#[derive(Default)]
pub struct HistoryState {
    pub query: String,
}

fn tag(session: &Session) -> &'static str {
    match (session.track, session.kind) {
        (_, Kind::Cards) => "CARDS",
        (_, Kind::Translate) => "SAY",
        (_, Kind::Prepare) => "PREP",
        (Track::English, _) => "EN",
        (Track::Technical, _) => "TECH",
        (Track::Interview, _) => "HR",
    }
}

pub fn ui(core: &mut Core, state: &mut HistoryState, ctx: &egui::Context) -> Option<Nav> {
    if !core.history_open {
        return None;
    }
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        core.history_open = false;
        return None;
    }
    let tr = core.tr;
    let now = core.now();
    let mut nav = None;
    let mut delete: Option<String> = None;
    let mut stop: Option<u64> = None;
    let height = (ctx.content_rect().height() - 110.0).max(240.0);
    egui::Area::new(Id::unique("history")).order(Order::Foreground).anchor(Align2::LEFT_TOP, vec2(20.0, 72.0)).show(
        ctx,
        |ui| {
            Frame::new()
                .fill(SURFACE)
                .stroke(Stroke::new(1.0, LINE))
                .corner_radius(CornerRadius::same(20))
                .inner_margin(Margin::same(16))
                .shadow(egui::Shadow { offset: [0, 12], blur: 40, spread: 0, color: Color32::from_black_alpha(150) })
                .show(ui, |ui| {
                    ui.set_width(330.0);
                    ui.set_max_height(height);
                    ScrollArea::vertical().auto_shrink([false, true]).max_height(height - 60.0).show(ui, |ui| {
                        widgets::caps(ui, tr.get(K::ContinueLabel));
                        ui.add_space(4.0);
                        ui.add(
                            TextEdit::singleline(&mut state.query)
                                .hint_text(tr.get(K::Search))
                                .desired_width(f32::INFINITY)
                                .margin(vec2(10.0, 8.0)),
                        );
                        ui.add_space(8.0);
                        let sessions = core.store.search(&state.query);
                        if sessions.is_empty() {
                            widgets::muted(ui, tr.get(K::NoSessions));
                        }
                        for session in &sessions {
                            row(core, ui, session, now, &mut nav, &mut delete);
                        }
                    });
                    if core.jobs.is_busy() {
                        ui.add_space(10.0);
                        widgets::caps(ui, tr.get(K::Background));
                        for job in core.jobs.running() {
                            // a download shows how far it is, not the 0% it started with
                            let label = match core.downloads.iter().find(|d| d.job == job.id) {
                                Some(download) => tr.fill(
                                    K::Downloading,
                                    &[("name", &download.name), ("percent", &download.progress.percent().to_string())],
                                ),
                                None => job.label.clone(),
                            };
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(RichText::new(label).font(theme::sans(14.0)).color(MUTED));
                                if job.cancellable
                                    && widgets::quiet_icon(ui, widgets::Icon::Close, tr.get(K::Cancel), 22.0).clicked()
                                {
                                    stop = Some(job.id);
                                }
                            });
                        }
                    }
                });
        },
    );
    if let Some(id) = delete {
        let _ = core.store.delete(&id);
        ctx.request_repaint();
    }
    if let Some(job) = stop {
        core.cancel_job(job);
        ctx.request_repaint();
    }
    if nav.is_some() {
        core.history_open = false;
    }
    nav
}

fn row(core: &Core, ui: &mut Ui, session: &Session, now: u64, nav: &mut Option<Nav>, delete: &mut Option<String>) {
    let tr = core.tr;
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 58.0), Sense::click());
    let response = widgets::pointer(response);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &session.title));
    if response.hovered() {
        ui.painter().rect(rect, CornerRadius::same(12), RAISED, Stroke::new(1.0, BERRY), StrokeKind::Inside);
    }
    ui.painter().text(
        rect.left_top() + vec2(12.0, 10.0),
        Align2::LEFT_TOP,
        &session.title,
        theme::semibold(14.5),
        SNOW,
    );
    let status = match session.status {
        Status::Active => tr.get(K::StatusActive),
        Status::Done => tr.get(K::StatusDone),
    };
    ui.painter().text(
        rect.left_top() + vec2(12.0, 33.0),
        Align2::LEFT_TOP,
        format!("{} · {} · {}", tag(session), status, ago(session.updated, now)),
        theme::mono(11.5),
        FAINT,
    );
    let trash = egui::Rect::from_center_size(egui::pos2(rect.right() - 20.0, rect.center().y), vec2(26.0, 26.0));
    let trash_response = ui.interact(trash, ui.scope_id().with(("trash", &session.id)), Sense::click());
    trash_response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::Button, true, format!("{} {}", tr.get(K::Delete), session.title))
    });
    if response.hovered() || trash_response.hovered() {
        widgets::paint_icon(
            ui.painter(),
            Icon::Trash,
            trash.center(),
            18.0,
            if trash_response.hovered() { BERRY } else { FAINT },
        );
    }
    if trash_response.clicked() {
        *delete = Some(session.id.clone());
    } else if response.clicked() {
        *nav = Some(Nav::Open(session.id.clone()));
    }
    let _ = icon_button;
}
