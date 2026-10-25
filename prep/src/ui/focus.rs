//! The focus timer in the top bar, and the short messages that appear under it.

use std::time::{Duration, Instant};

use egui::{
    Align2, CornerRadius, Frame, Id, Margin, Order, Rect, RichText, Role, Sense, Stroke, StrokeKind, Ui, WidgetInfo,
    pos2, vec2,
};

use super::Core;
use super::theme::{self, BERRY, FAINT, GOOD, LINE, MUTED, RAISED, SNOW, SURFACE};
use super::widgets::{self, Icon, paint_icon, pointer};
use crate::i18n::K;
use crate::pomodoro::{self, Phase, Signal};

pub const PILL_WIDTH: f32 = 168.0;
/// The narrow form for a small window: the ring and the time, no label and no skip button.
pub const PILL_COMPACT: f32 = 112.0;
const PILL_HEIGHT: f32 = 44.0;
/// How long a message stays before it goes away on its own.
const TOAST_SECONDS: u64 = 14;

fn phase_label(core: &Core) -> &'static str {
    let tr = core.tr;
    match core.pomodoro.phase() {
        Phase::Focus => tr.get(K::PhaseFocus),
        Phase::ShortBreak => tr.get(K::PhaseShortBreak),
        Phase::LongBreak => tr.get(K::PhaseLongBreak),
    }
}

/// The ring and the time left. A click starts or pauses; a right click resets; the small arrow
/// skips to the next phase.
pub fn pill(core: &mut Core, ui: &mut Ui, compact: bool) {
    let tr = core.tr;
    let now = Instant::now();
    let width = if compact { PILL_COMPACT } else { PILL_WIDTH };
    let (rect, response) = ui.allocate_exact_size(vec2(width, PILL_HEIGHT), Sense::click());
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
    painter.rect(
        rect,
        CornerRadius::same((PILL_HEIGHT / 2.0) as u8),
        if hovered { RAISED } else { SURFACE },
        Stroke::new(1.0, if running { theme::mix(LINE, accent, 0.55) } else { LINE }),
        StrokeKind::Inside,
    );
    let ring = pos2(rect.left() + 24.0, rect.center().y);
    painter.circle_stroke(ring, 12.5, Stroke::new(2.4, LINE));
    let fraction = core.pomodoro.fraction(now);
    if fraction > 0.0 {
        let points = widgets::arc(
            ring,
            12.5,
            -std::f32::consts::FRAC_PI_2,
            -std::f32::consts::FRAC_PI_2 + fraction * std::f32::consts::TAU,
            40,
        );
        painter.add(egui::Shape::line(points, Stroke::new(2.4, accent)));
    }
    paint_icon(painter, if running { Icon::Pause } else { Icon::Play }, ring, 14.0, SNOW);
    let time = pomodoro::clock(core.pomodoro.remaining(now));
    painter.text(
        pos2(rect.left() + 46.0, rect.center().y - 7.0),
        Align2::LEFT_CENTER,
        time,
        theme::mono_bold(15.0),
        SNOW,
    );
    if !compact {
        painter.text(
            pos2(rect.left() + 46.0, rect.center().y + 9.0),
            Align2::LEFT_CENTER,
            phase_label(core).to_uppercase(),
            theme::mono(10.0),
            if running { accent } else { FAINT },
        );
    }
    if response.clicked() {
        core.pomodoro.toggle(now);
    }
    if response.secondary_clicked() {
        core.pomodoro.reset();
    }
    if !compact && core.pomodoro.in_progress(now) {
        let skip = Rect::from_center_size(pos2(rect.right() - 22.0, rect.center().y), vec2(30.0, 30.0));
        let skipped = pointer(ui.interact(skip, ui.scope_id().with("timer-skip"), Sense::click()));
        skipped.widget_info(|| WidgetInfo::labeled(Role::Button, true, tr.get(K::TimerSkip)));
        let skipped = skipped.on_hover_text(tr.get(K::TimerSkip));
        paint_icon(ui.painter(), Icon::Skip, skip.center(), 15.0, if skipped.hovered() { SNOW } else { MUTED });
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
