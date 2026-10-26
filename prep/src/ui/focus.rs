//! The focus timer in the top bar, and the short messages that appear under it.

use std::time::{Duration, Instant};

use egui::{
    Align2, CornerRadius, Frame, Id, Margin, Order, Rect, RichText, Role, Sense, Stroke, Ui, WidgetInfo, pos2, vec2,
};

use super::Core;
use super::theme::{self, BERRY, GOOD, LINE, MUTED, RAISED, SNOW, SURFACE};
use super::widgets::{self, Icon, paint_icon, pointer};
use crate::i18n::K;
use crate::pomodoro::{self, Phase, Signal};

/// The width the timer takes at the top right.
pub const PILL_WIDTH: f32 = 128.0;
const PILL_HEIGHT: f32 = 44.0;
/// How long a message stays before it goes away on its own.
const TOAST_SECONDS: u64 = 14;

/// A ring that fills as the block goes, and the time left beside it; nothing around them. A
/// click starts or pauses; a right click resets; the arrow that appears under the pointer skips
/// to the next phase.
pub fn pill(core: &mut Core, ui: &mut Ui) {
    let tr = core.tr;
    let now = Instant::now();
    let (rect, response) = ui.allocate_exact_size(vec2(PILL_WIDTH, PILL_HEIGHT), Sense::click());
    let response = pointer(response);
    let running = core.pomodoro.running();
    let label = if running { tr.get(K::TimerPause) } else { tr.get(K::TimerStart) };
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, label));
    let explain = tr.fill(
        K::TimerExplain,
        &[
            ("focus", &core.settings.pomodoro.focus_minutes.to_string()),
            ("rest", &core.settings.pomodoro.short_minutes.to_string()),
        ],
    );
    let response = response.on_hover_text(explain);
    let focus = core.pomodoro.phase() == Phase::Focus;
    let accent = if focus { BERRY } else { GOOD };
    let hovered = response.hovered();
    let painter = ui.painter();
    if hovered {
        painter.rect_filled(rect, CornerRadius::same((PILL_HEIGHT / 2.0) as u8), RAISED);
    }
    let ring = pos2(rect.left() + 24.0, rect.center().y);
    painter.circle_stroke(ring, 11.0, Stroke::new(2.2, LINE));
    let fraction = core.pomodoro.fraction(now);
    if fraction > 0.0 {
        let points = widgets::arc(
            ring,
            11.0,
            -std::f32::consts::FRAC_PI_2,
            -std::f32::consts::FRAC_PI_2 + fraction * std::f32::consts::TAU,
            40,
        );
        painter.add(egui::Shape::line(points, Stroke::new(2.2, accent)));
    }
    if hovered {
        paint_icon(painter, if running { Icon::Pause } else { Icon::Play }, ring, 12.0, SNOW);
    }
    let time = pomodoro::clock(core.pomodoro.remaining(now));
    painter.text(
        pos2(rect.left() + 44.0, rect.center().y),
        Align2::LEFT_CENTER,
        time,
        theme::mono_bold(16.0),
        if running { SNOW } else { MUTED },
    );
    if response.clicked() {
        core.pomodoro.toggle(now);
    }
    if response.secondary_clicked() {
        core.pomodoro.reset();
    }
    if core.pomodoro.in_progress(now) {
        let skip = Rect::from_center_size(pos2(rect.right() - 14.0, rect.center().y), vec2(26.0, 30.0));
        let skipped = pointer(ui.interact(skip, ui.scope_id().with("timer-skip"), Sense::click()));
        skipped.widget_info(|| WidgetInfo::labeled(Role::Button, true, tr.get(K::TimerSkip)));
        let skipped = skipped.on_hover_text(tr.get(K::TimerSkip));
        if hovered || skipped.hovered() {
            paint_icon(ui.painter(), Icon::Skip, skip.center(), 14.0, if skipped.hovered() { SNOW } else { MUTED });
        }
        if skipped.clicked() {
            core.pomodoro.skip(now);
        }
    }
    if running {
        ui.ctx().request_repaint_after(Duration::from_millis(500));
    }
}

impl Core {
    /// Shows a short message under the top bar. It goes away by itself.
    pub fn notify(&mut self, message: impl Into<String>) {
        self.notice = Some(message.into());
        self.notice_at = Some(Instant::now());
    }

    /// What to do when the timer says a phase has ended.
    pub fn on_timer(&mut self, signal: Signal) {
        let now = self.now();
        match signal {
            Signal::FocusDone { next } => {
                self.stats.add_focus_block(now);
                self.save_stats();
                let minutes = match next {
                    Phase::LongBreak => self.settings.pomodoro.long_minutes,
                    _ => self.settings.pomodoro.short_minutes,
                };
                let text = self.tr.fill(K::FocusDone, &[("n", &minutes.to_string())]);
                self.notify(text);
            }
            Signal::BreakDone => {
                let text = self.tr.get(K::BreakDone).to_owned();
                self.notify(text);
            }
        }
        self.chime();
    }

    /// A soft sound for the end of a phase, unless somebody is talking.
    fn chime(&self) {
        let speaker = &self.services.speaker;
        if speaker.queued().is_zero() {
            speaker.enqueue(pomodoro::chime(24_000), 24_000);
        }
    }
}

/// The message under the top bar, if there is one.
pub fn toast(core: &mut Core, ctx: &egui::Context) {
    let Some(text) = core.notice.clone() else { return };
    if core.notice_at.is_some_and(|at| at.elapsed() > Duration::from_secs(TOAST_SECONDS)) {
        core.notice = None;
        return;
    }
    let tr = core.tr;
    let mut dismiss = false;
    egui::Area::new(Id::unique("toast")).order(Order::Foreground).anchor(Align2::CENTER_TOP, vec2(0.0, 78.0)).show(
        ctx,
        |ui| {
            Frame::new()
                .fill(SURFACE)
                .stroke(Stroke::new(1.0, theme::mix(LINE, BERRY, 0.6)))
                .corner_radius(CornerRadius::same(16))
                .inner_margin(Margin::symmetric(16, 10))
                .shadow(egui::Shadow {
                    offset: [0, 8],
                    blur: 28,
                    spread: 0,
                    color: egui::Color32::from_black_alpha(130),
                })
                .show(ui, |ui| {
                    ui.set_max_width(520.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&text).font(theme::sans(14.5)).color(SNOW));
                        if widgets::ghost_button(ui, tr.get(K::Dismiss)).clicked() {
                            dismiss = true;
                        }
                    });
                });
        },
    );
    if dismiss {
        core.notice = None;
    }
    ctx.request_repaint_after(Duration::from_secs(1));
}
