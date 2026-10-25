//! Concept checks: a question, a few answers, and how sure you are. Being sure and wrong teaches
//! the most, so that is what comes back soonest.

use egui::{Align2, Color32, CornerRadius, RichText, Sense, Stroke, StrokeKind, TextEdit, Ui, vec2};

use super::theme::{self, BERRY, FAINT, GOOD, LINE, MUTED, RAISED, SNOW, SURFACE};
use super::widgets::{self, Icon, card_frame, chip, ghost_button, icon_button, primary_button};
use super::{Core, Nav, scrolling_column};
use crate::i18n::K;
use crate::prompts::Lang;
use crate::prompts::cards::{self, ConceptCard};
use crate::session::{Kind, Session};
use crate::srs::{self, Calibration, Card, CardKind};

pub enum Phase {
    Choose,
    Making,
    Playing,
    Done,
    Failed(String),
}

pub struct ConceptsView {
    pub topic: String,
    pub count: usize,
    pub difficulty: u8,
    pub phase: Phase,
    pub cards: Vec<ConceptCard>,
    pub index: usize,
    /// `None` until something is picked; `Some(None)` is "I don't know".
    pub choice: Option<Option<usize>>,
    pub confidence: f32,
    pub thinking: String,
    pub answered: bool,
    pub saved: usize,
    pub session: Option<Session>,
}

impl ConceptsView {
    pub fn new(core: &Core) -> Self {
        let topic = core
            .brief
            .as_ref()
            .map(cards::topics)
            .and_then(|t| t.into_iter().next())
            .unwrap_or_else(|| "SQL and indexes".into());
        Self {
            topic,
            count: 5,
            difficulty: 2,
            phase: Phase::Choose,
            cards: Vec::new(),
            index: 0,
            choice: None,
            confidence: 50.0,
            thinking: String::new(),
            answered: false,
            saved: 0,
            session: None,
        }
    }

    /// A finished set, opened again from the history.
    pub fn from_session(session: Session) -> Self {
        let cards: Vec<ConceptCard> = serde_json::from_value(session.payload["cards"].clone()).unwrap_or_default();
        let saved = session.payload["saved"].as_u64().unwrap_or(0) as usize;
        Self {
            topic: session.payload["topic"].as_str().unwrap_or_default().to_owned(),
            count: cards.len(),
            difficulty: 2,
            phase: Phase::Done,
            cards,
            index: 0,
            choice: None,
            confidence: 50.0,
            thinking: String::new(),
            answered: true,
            saved,
            session: Some(session),
        }
    }

    pub fn current(&self) -> Option<&ConceptCard> {
        self.cards.get(self.index)
    }

    /// `(right, confidence)` for every question answered so far.
    pub fn results(&self) -> Vec<(bool, u8)> {
        self.cards.iter().filter_map(|c| c.answered.as_ref().map(|a| (a.correct, a.confidence))).collect()
    }

    pub fn right(&self) -> usize {
        self.results().iter().filter(|(right, _)| *right).count()
    }

    /// Records the answer to the current question; returns whether it was right.
    pub fn submit(&mut self, core: &mut Core) -> bool {
        let now = core.now();
        let confidence = self.confidence.round().clamp(0.0, 100.0) as u8;
        let choice = self.choice.unwrap_or(None);
        let thinking = self.thinking.clone();
        let Some(card) = self.cards.get_mut(self.index) else { return false };
        let correct = card.answer(choice, confidence, &thinking, now);
        let note = srs::calibration(correct, confidence);
        // what you got wrong, or got right only by luck, comes back; being sure and wrong comes back at once
        if !correct || note == Calibration::Underconfident {
            let right = card.options[card.answer_index].clone();
            let back = format!("{right}\n\n{}", card.explanation);
            let mut review = Card::new(CardKind::Question, &card.question, &back, &card.topic, now);
            if correct {
                review.due = now + srs::DAY;
            }
            if core.deck.add(review) {
                self.saved += 1;
                let _ = core.deck.save(&core.paths.deck_file());
            }
        }
        self.answered = true;
        correct
    }

    pub fn next(&mut self, core: &mut Core) {
        if self.index + 1 < self.cards.len() {
            self.index += 1;
            self.choice = None;
            self.confidence = 50.0;
            self.thinking.clear();
            self.answered = false;
        } else {
            self.phase = Phase::Done;
            self.keep(core);
        }
    }

    fn keep(&mut self, core: &mut Core) {
        let title = format!("{} · {}", core.tr.get(K::CardConcepts), self.topic);
        let mut session = self.session.take().unwrap_or_else(|| Session::new(core.track, Kind::Cards, &title));
        session.payload = serde_json::json!({ "topic": self.topic, "cards": self.cards, "saved": self.saved });
        session.finish();
        let _ = core.store.save(&session);
        self.session = Some(session);
    }
}

/// One answer to pick, drawn as a rounded row with a circle.
fn option_row(ui: &mut Ui, text: &str, selected: bool, dashed: bool, state: Option<bool>) -> egui::Response {
    let wrap = ui.available_width() - 64.0;
    let galley = ui.painter().layout(text.to_owned(), theme::sans(16.0), SNOW, wrap);
    let height = (galley.size().y + 26.0).max(52.0);
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
    let response = widgets::pointer(response);
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::RadioButton, true, selected, text));
    let hovered = response.hovered() && state.is_none();
    let (stroke, fill) = match state {
        Some(true) => (GOOD, theme::alpha(GOOD, 0.10)),
        Some(false) => (BERRY, theme::alpha(BERRY, 0.10)),
        None if selected => (BERRY, theme::alpha(BERRY, 0.08)),
        None => (if hovered { BERRY } else { LINE }, if hovered { RAISED } else { Color32::TRANSPARENT }),
    };
    ui.painter().rect(rect, CornerRadius::same(14), fill, Stroke::new(1.0, stroke), StrokeKind::Inside);
    let circle = egui::pos2(rect.left() + 28.0, rect.center().y);
    ui.painter().circle_stroke(circle, 9.0, Stroke::new(1.4, if selected { BERRY } else { FAINT }));
    if selected {
        ui.painter().circle_filled(circle, 5.0, BERRY);
    }
    let _ = dashed;
    let color = if dashed { MUTED } else { SNOW };
    ui.painter().galley(egui::pos2(rect.left() + 52.0, rect.center().y - galley.size().y / 2.0), galley, color);
    response
}

pub fn ui(core: &mut Core, view: &mut ConceptsView, ui: &mut Ui) -> Option<Nav> {
    let tr = core.tr;
    let mut nav = None;
    let mut make = false;
    let mut speak: Option<String> = None;
    scrolling_column(ui, 760.0, |ui| {
        ui.add_space(20.0);
        if ghost_button(ui, tr.get(K::BackHome)).clicked() {
            nav = Some(Nav::Home);
        }
        ui.add_space(14.0);
        widgets::title(ui, tr.get(K::CardConcepts));
        ui.add_space(8.0);
        match &view.phase {
            Phase::Choose | Phase::Failed(_) => {
                if let Phase::Failed(message) = &view.phase {
                    card_frame().show(ui, |ui| {
                        ui.label(RichText::new(message).color(MUTED));
                    });
                    ui.add_space(8.0);
                }
                widgets::caps(ui, tr.get(K::CardsTopic));
                let topics = core
                    .brief
                    .as_ref()
                    .map(cards::topics)
                    .unwrap_or_else(|| cards::topics(&crate::brief::Brief::default()));
                ui.horizontal_wrapped(|ui| {
                    for topic in &topics {
                        if chip(ui, topic, view.topic == *topic).clicked() {
                            view.topic = topic.clone();
                        }
                    }
                });
                ui.add(TextEdit::singleline(&mut view.topic).desired_width(f32::INFINITY).margin(vec2(12.0, 9.0)));
                ui.add_space(10.0);
                widgets::caps(ui, tr.get(K::CardsCount));
                ui.horizontal(|ui| {
                    for count in [3usize, 5, 8] {
                        if chip(ui, &count.to_string(), view.count == count).clicked() {
                            view.count = count;
                        }
                    }
                    ui.add_space(14.0);
                    for (level, key) in [(1u8, K::Easy), (2, K::Medium), (3, K::Hard)] {
                        if chip(ui, tr.get(key), view.difficulty == level).clicked() {
                            view.difficulty = level;
                        }
                    }
                });
                ui.add_space(18.0);
                if primary_button(ui, tr.get(K::CardsMake), !view.topic.trim().is_empty()).clicked() {
                    make = true;
                }
            }
            Phase::Making => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new(tr.get(K::CardsMaking)).font(theme::serif_italic(19.0)).color(MUTED));
                });
            }
            Phase::Playing => {
                let total = view.cards.len();
                let Some(card) = view.current().cloned() else { return };
                ui.label(
                    RichText::new(format!(
                        "{} · {} · {}",
                        tr.get(K::CardsLabel),
                        card.topic,
                        tr.fill(
                            K::CardsProgress,
                            &[("n", &(view.index + 1).to_string()), ("total", &total.to_string())]
                        )
                    ))
                    .font(theme::mono(11.5))
                    .color(FAINT),
                );
                widgets::progress(ui, ui.available_width(), view.index as f32 / total.max(1) as f32);
                ui.add_space(12.0);
                card_frame().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(&card.question).font(theme::serif(23.0)).color(SNOW));
                    ui.add_space(12.0);
                    for (index, option) in card.options.iter().enumerate() {
                        let state = view.answered.then(|| {
                            if index == card.answer_index {
                                Some(true)
                            } else if view.choice == Some(Some(index)) {
                                Some(false)
                            } else {
                                None
                            }
                        });
                        let state = state.flatten();
                        let selected = view.choice == Some(Some(index));
                        if option_row(ui, option, selected, false, state).clicked() && !view.answered {
                            view.choice = Some(Some(index));
                            ui.ctx().request_repaint();
                        }
                        ui.add_space(2.0);
                    }
                    let selected = view.choice == Some(None);
                    if option_row(ui, tr.get(K::CardsDontKnow), selected, true, None).clicked() && !view.answered {
                        view.choice = Some(None);
                        ui.ctx().request_repaint();
                    }
                    if !view.answered {
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            let confidence_label = widgets::caps(ui, tr.get(K::CardsConfidence));
                            ui.add_space(8.0);
                            ui.style_mut().visuals.selection.bg_fill = BERRY;
                            ui.style_mut().visuals.widgets.inactive.bg_fill = LINE;
                            ui.style_mut().visuals.widgets.hovered.bg_fill = LINE;
                            ui.style_mut().spacing.slider_width = ui.available_width() - 120.0;
                            ui.add(egui::Slider::new(&mut view.confidence, 0.0..=100.0).show_value(false).step_by(5.0))
                                .labelled_by(confidence_label.id);
                            ui.label(
                                RichText::new(format!("{}%", view.confidence.round() as i32))
                                    .font(theme::mono(13.0))
                                    .color(MUTED),
                            );
                        });
                        ui.add_space(6.0);
                        let thinking_label = widgets::caps(ui, tr.get(K::CardsThinking));
                        ui.add(
                            TextEdit::multiline(&mut view.thinking)
                                .hint_text(tr.get(K::CardsThinkingHint))
                                .desired_rows(3)
                                .desired_width(f32::INFINITY)
                                .margin(vec2(12.0, 10.0)),
                        )
                        .labelled_by(thinking_label.id);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(tr.get(K::CardsSubmitHint)).font(theme::mono(11.5)).color(FAINT));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if primary_button(ui, tr.get(K::CardsSubmit), view.choice.is_some()).clicked() {
                                    view.submit(core);
                                    ui.ctx().request_repaint();
                                }
                            });
                        });
                    } else if let Some(answer) =
                        card.answered.clone().or_else(|| view.cards[view.index].answered.clone())
                    {
                        ui.add_space(10.0);
                        let (word, color) =
                            if answer.correct { (K::CardsCorrect, GOOD) } else { (K::CardsWrong, BERRY) };
                        ui.label(RichText::new(tr.get(word)).font(theme::semibold(17.0)).color(color));
                        let note = match srs::calibration(answer.correct, answer.confidence) {
                            Calibration::Overconfident => K::CardsOverconfident,
                            Calibration::Underconfident => K::CardsUnderconfident,
                            Calibration::WellCalibrated => K::CardsWellJudged,
                        };
                        ui.label(
                            RichText::new(format!("{} ({}%)", tr.get(note), answer.confidence))
                                .font(theme::sans(14.5))
                                .color(MUTED),
                        );
                        ui.add_space(8.0);
                        ui.label(RichText::new(&card.explanation).font(theme::serif(18.0)).color(SNOW));
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if icon_button(ui, Icon::Play, tr.get(K::Listen), 36.0, None).clicked() {
                                speak = Some(format!("{} {}", card.options[card.answer_index], card.explanation));
                            }
                            let last = view.index + 1 >= total;
                            if primary_button(ui, tr.get(if last { K::CardsFinish } else { K::CardsNext }), true)
                                .clicked()
                            {
                                view.next(core);
                            }
                        });
                    }
                });
            }
            Phase::Done => {
                let total = view.cards.len();
                let results = view.results();
                ui.label(
                    RichText::new(
                        tr.fill(K::CardsScore, &[("right", &view.right().to_string()), ("total", &total.to_string())]),
                    )
                    .font(theme::serif(26.0))
                    .color(SNOW),
                );
                if let Some(score) = srs::brier(&results) {
                    ui.label(
                        RichText::new(tr.fill(K::CardsBrier, &[("score", &format!("{score:.2}"))]))
                            .font(theme::mono(12.5))
                            .color(MUTED),
                    );
                }
                if view.saved > 0 {
                    ui.label(
                        RichText::new(tr.fill(K::CardsSaved, &[("n", &view.saved.to_string())]))
                            .font(theme::sans(15.0))
                            .color(BERRY),
                    );
                }
                ui.add_space(14.0);
                for card in &view.cards {
                    let Some(answer) = &card.answered else { continue };
                    ui.horizontal_top(|ui| {
                        ui.label(
                            RichText::new(if answer.correct { "+" } else { "–" })
                                .font(theme::mono_bold(15.0))
                                .color(if answer.correct { GOOD } else { BERRY }),
                        );
                        ui.add(widgets::wrapping(RichText::new(&card.question).font(theme::sans(15.0)).color(SNOW)));
                    });
                }
                ui.add_space(18.0);
                if primary_button(ui, tr.get(K::PractiseAgain), true).clicked() {
                    nav = Some(Nav::Concepts);
                }
            }
        }
        ui.add_space(40.0);
    });
    let _ = (Align2::CENTER_CENTER, SURFACE);
    if make {
        view.phase = Phase::Making;
        core.make_cards(view.topic.trim().to_owned(), view.count, view.difficulty);
        ui.ctx().request_repaint();
    }
    if let Some(text) = speak {
        core.speak(text, Lang::En);
    }
    nav
}
