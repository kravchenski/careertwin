//! The review after a call: what was good, what was wrong, and how to say it better, with a way
//! to hear the better version and to say it yourself and see which words went missing.

use std::collections::HashMap;

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, RichText, Stroke, Ui};

use super::theme::{self, BERRY, FAINT, GOOD, LINE, MUTED, SNOW};
use super::widgets::{self, Icon, card_frame, ghost_button, icon_button, primary_button};
use super::{Core, Nav, scrolling_column};
use crate::i18n::K;
use crate::prompts::Lang;
use crate::prompts::feedback::Feedback;
use crate::prompts::research::Digest;
use crate::session::{Session, date};
use crate::srs::{Card, CardKind};
use crate::textdiff::{Comparison, Mark};

/// The room the play button and the add button of a phrase need at the right of its words.
const PHRASE_BUTTONS: f32 = 270.0;

pub enum State {
    Working,
    Ready,
    Failed(String),
}

/// What happened when you said an answer yourself.
pub struct Drill {
    pub heard: String,
    pub comparison: Comparison,
}

pub struct ReviewView {
    pub session: Session,
    pub state: State,
    pub added: Vec<String>,
    pub drills: HashMap<usize, Drill>,
    pub listening_slot: Option<usize>,
    pub drill_error: Option<String>,
}

impl ReviewView {
    pub fn new(session: Session) -> Self {
        let state = if session.feedback.is_some() { State::Ready } else { State::Working };
        Self { session, state, added: Vec::new(), drills: HashMap::new(), listening_slot: None, drill_error: None }
    }

    pub fn feedback(&self) -> Option<&Feedback> {
        self.session.feedback.as_ref()
    }

    /// The notes taken from the call, once they are ready.
    pub fn digest(&self) -> Option<Digest> {
        serde_json::from_value(self.session.payload.get("digest")?.clone()).ok()
    }
}

/// The few lines to keep from the call, and what to do next time.
fn digest_card(ui: &mut Ui, tr: crate::i18n::Tr, digest: &Digest, nav: &mut Option<Nav>) {
    card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            widgets::caps(ui, tr.get(K::DigestTitle));
            ui.label(RichText::new(tr.get(K::DigestSaved)).font(theme::mono(11.0)).color(GOOD));
        });
        bullets(ui, &digest.vital, BERRY);
        if !digest.mistakes.is_empty() {
            ui.add_space(6.0);
            bullets(ui, &digest.mistakes, FAINT);
        }
        if !digest.next.is_empty() {
            ui.add_space(8.0);
            ui.label(
                RichText::new(format!("{}: {}", tr.get(K::DigestNext), digest.next))
                    .font(theme::serif_italic(16.5))
                    .color(MUTED),
            );
        }
        if !digest.hook.is_empty() {
            ui.label(RichText::new(&digest.hook).font(theme::sans(14.5)).color(MUTED));
        }
        ui.add_space(6.0);
        if ghost_button(ui, tr.get(K::OpenNotes)).clicked() {
            *nav = Some(Nav::Notes);
        }
    });
}

fn bullets(ui: &mut Ui, items: &[String], color: Color32) {
    for item in items {
        ui.horizontal_top(|ui| {
            ui.label(RichText::new("•").color(color));
            ui.add(widgets::wrapping(RichText::new(item).font(theme::sans(15.0)).color(SNOW)));
        });
    }
}

/// The words you said, with the ones that went missing marked.
fn drill_job(drill: &Drill) -> LayoutJob {
    let mut job = LayoutJob::default();
    for (index, mark) in drill.comparison.marks.iter().enumerate() {
        let format = match mark.mark {
            Mark::Match => TextFormat { font_id: theme::sans(15.5), color: GOOD, ..Default::default() },
            Mark::Missed => TextFormat {
                font_id: theme::sans(15.5),
                color: BERRY,
                underline: Stroke::new(1.5, BERRY),
                ..Default::default()
            },
            Mark::Extra => TextFormat {
                font_id: theme::sans(15.5),
                color: FAINT,
                strikethrough: Stroke::new(1.0, FAINT),
                ..Default::default()
            },
        };
        let text = if index == 0 { mark.word.clone() } else { format!(" {}", mark.word) };
        job.append(&text, 0.0, format);
    }
    job
}

pub fn ui(core: &mut Core, view: &mut ReviewView, ui: &mut Ui) -> Option<Nav> {
    let tr = core.tr;
    let mut nav = None;
    let mut speak: Option<String> = None;
    let mut hear: Option<usize> = None;
    let mut retry = false;
    let mut add_phrase: Option<(String, String)> = None;
    scrolling_column(ui, 780.0, |ui| {
        ui.add_space(20.0);
        ui.horizontal(|ui| {
            if ghost_button(ui, tr.get(K::BackHome)).clicked() {
                nav = Some(Nav::Home);
            }
            if ghost_button(ui, tr.get(K::PractiseAgain)).clicked() {
                nav = Some(Nav::SetupCall);
            }
        });
        ui.add_space(14.0);
        widgets::title(ui, tr.get(K::ReviewTitle));
        ui.label(
            RichText::new(format!("{} · {}", view.session.title, date(view.session.created)))
                .font(theme::mono(12.5))
                .color(FAINT),
        );
        ui.add_space(14.0);
        if let Some(digest) = view.digest() {
            digest_card(ui, tr, &digest, &mut nav);
            ui.add_space(14.0);
        }
        match &view.state {
            State::Working => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new(tr.get(K::ReviewPreparing)).font(theme::serif_italic(19.0)).color(MUTED));
                });
                return;
            }
            State::Failed(message) => {
                card_frame().show(ui, |ui| {
                    ui.label(RichText::new(tr.get(K::ReviewFailed)).font(theme::semibold(16.0)).color(SNOW));
                    ui.label(RichText::new(message).color(MUTED));
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ghost_button(ui, tr.get(K::Retry)).clicked() {
                            retry = true;
                        }
                        if ghost_button(ui, tr.get(K::OpenSettings)).clicked() {
                            core.settings_open = true;
                        }
                    });
                });
                return;
            }
            State::Ready => {}
        }
        let Some(feedback) = view.session.feedback.clone() else { return };
        if !feedback.summary.is_empty() {
            ui.label(RichText::new(&feedback.summary).font(theme::serif(20.0)).color(SNOW));
        }
        if !feedback.scores.is_empty() {
            ui.add_space(22.0);
            widgets::caps(ui, tr.get(K::Scores));
            for score in &feedback.scores {
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [130.0, 20.0],
                        egui::Label::new(RichText::new(&score.name).font(theme::semibold(15.0)).color(SNOW)),
                    );
                    widgets::score_bar(ui, score.value);
                    ui.label(RichText::new(format!("{}/5", score.value)).font(theme::mono(12.5)).color(MUTED));
                    if !score.note.is_empty() {
                        ui.add(widgets::wrapping(RichText::new(&score.note).font(theme::sans(14.0)).color(FAINT)));
                    }
                });
            }
        }
        if !feedback.priorities.is_empty() {
            ui.add_space(22.0);
            widgets::caps(ui, tr.get(K::Priorities));
            for (index, priority) in feedback.priorities.iter().enumerate() {
                ui.horizontal_top(|ui| {
                    ui.label(RichText::new(format!("{}", index + 1)).font(theme::mono_bold(13.0)).color(BERRY));
                    ui.add(widgets::wrapping(RichText::new(priority).font(theme::sans(16.0)).color(SNOW)));
                });
            }
        }
        if !feedback.answers.is_empty() {
            ui.add_space(26.0);
            widgets::caps(ui, tr.get(K::YourAnswers));
            for (slot, answer) in feedback.answers.iter().enumerate() {
                ui.add_space(6.0);
                card_frame().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    if !answer.question.is_empty() {
                        ui.label(RichText::new(&answer.question).font(theme::serif_italic(17.0)).color(MUTED));
                        ui.add_space(6.0);
                    }
                    widgets::caps(ui, tr.get(K::YouSaid));
                    ui.label(RichText::new(&answer.you_said).font(theme::sans(15.5)).color(SNOW));
                    if !answer.good.is_empty() {
                        ui.add_space(8.0);
                        widgets::caps(ui, tr.get(K::WentWell));
                        bullets(ui, &answer.good, GOOD);
                    }
                    if !answer.issues.is_empty() {
                        ui.add_space(8.0);
                        widgets::caps(ui, tr.get(K::ToFix));
                        bullets(ui, &answer.issues, BERRY);
                    }
                    if !answer.missed_points.is_empty() {
                        ui.add_space(8.0);
                        widgets::caps(ui, tr.get(K::MissedPoints));
                        bullets(ui, &answer.missed_points, MUTED);
                    }
                    if !answer.better_answer.is_empty() {
                        ui.add_space(10.0);
                        crate::ui::rule(ui);
                        ui.add_space(8.0);
                        widgets::caps(ui, tr.get(K::SayItLike));
                        ui.label(RichText::new(&answer.better_answer).font(theme::serif(18.5)).color(SNOW));
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            if icon_button(ui, Icon::Play, tr.get(K::Listen), 36.0, None).clicked() {
                                speak = Some(answer.better_answer.clone());
                            }
                            let busy = view.listening_slot == Some(slot);
                            let label = if busy { tr.get(K::Listening) } else { tr.get(K::SayItYourself) };
                            if ghost_button(ui, label).clicked() && !busy {
                                hear = Some(slot);
                            }
                        });
                        if let Some(drill) = view.drills.get(&slot) {
                            ui.add_space(6.0);
                            widgets::caps(ui, tr.get(K::Heard));
                            ui.label(drill_job(drill));
                            let percent = (drill.comparison.accuracy * 100.0).round() as i32;
                            ui.label(
                                RichText::new(tr.fill(K::WordsMatched, &[("n", &percent.to_string())]))
                                    .font(theme::mono(12.5))
                                    .color(MUTED),
                            );
                            let missed = drill.comparison.missed();
                            if !missed.is_empty() {
                                ui.label(
                                    RichText::new(tr.fill(K::MissedWords, &[("words", &missed.join(", "))]))
                                        .font(theme::sans(14.0))
                                        .color(BERRY),
                                );
                            }
                        }
                    }
                });
            }
        }
        if let Some(error) = &view.drill_error {
            ui.add_space(6.0);
            ui.label(RichText::new(error).font(theme::sans(14.0)).color(BERRY));
        }
        if !feedback.language_notes.is_empty() {
            ui.add_space(26.0);
            widgets::caps(ui, tr.get(K::LanguageNotes));
            for note in &feedback.language_notes {
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(&note.said).font(theme::sans(15.5)).color(BERRY).strikethrough());
                    ui.label(RichText::new("→").color(FAINT));
                    ui.label(RichText::new(&note.better).font(theme::semibold(15.5)).color(GOOD));
                });
                if !note.why.is_empty() {
                    ui.label(RichText::new(&note.why).font(theme::sans(14.0)).color(MUTED));
                }
            }
        }
        if !feedback.phrases.is_empty() {
            ui.add_space(26.0);
            widgets::caps(ui, tr.get(K::PhrasesToLearn));
            for phrase in &feedback.phrases {
                ui.add_space(4.0);
                // the words take what is left of the row after the two buttons and wrap in it, so a long
                // phrase never runs under them
                let words = (ui.available_width() - PHRASE_BUTTONS).max(200.0);
                ui.horizontal_top(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(words, 0.0),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            ui.set_width(words);
                            ui.add(widgets::wrapping(
                                RichText::new(&phrase.english).font(theme::semibold(16.0)).color(SNOW),
                            ));
                            if !phrase.meaning.is_empty() {
                                ui.add(widgets::wrapping(
                                    RichText::new(&phrase.meaning).font(theme::sans(14.0)).color(MUTED),
                                ));
                            }
                        },
                    );
                    if icon_button(ui, Icon::Play, tr.get(K::Listen), 34.0, None).clicked() {
                        speak = Some(phrase.english.clone());
                    }
                    if view.added.contains(&phrase.english) {
                        ui.label(RichText::new(tr.get(K::Added)).font(theme::mono(12.5)).color(GOOD));
                    } else if ghost_button(ui, tr.get(K::AddToDeck)).clicked() {
                        add_phrase = Some((phrase.english.clone(), phrase.meaning.clone()));
                    }
                });
                let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 0.0, LINE);
            }
        }
        ui.add_space(30.0);
        if primary_button(ui, tr.get(K::PractiseAgain), true).clicked() {
            nav = Some(Nav::SetupCall);
        }
        ui.add_space(40.0);
    });

    // a change made while handling a click is drawn on the next frame, which must be asked for
    if speak.is_some() || hear.is_some() || retry || add_phrase.is_some() {
        ui.ctx().request_repaint();
    }
    if let Some(text) = speak {
        core.speak(text, Lang::En);
    }
    if let Some((english, meaning)) = add_phrase {
        let card = Card::new(CardKind::Phrase, &english, &meaning, "", core.now());
        core.deck.add(card);
        let _ = core.deck.save(&core.paths.deck_file());
        view.added.push(english);
    }
    if let Some(slot) = hear {
        view.listening_slot = Some(slot);
        view.drill_error = None;
        core.hear(slot as u32);
    }
    if retry {
        view.state = State::Working;
        core.spawn_review(&view.session);
    }
    nav
}
