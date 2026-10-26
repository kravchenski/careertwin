//! CareerTwin Prep: spoken practice for English, technical and general interviews with an
//! interviewer who talks like a person.
//!
//! The library holds everything that is not a window: settings, the facts about the candidate,
//! the model client, the prompts, sessions and spaced repetition.

#![cfg_attr(test, allow(clippy::field_reassign_with_default))]

pub mod audio;
pub mod brief;
pub mod coding;
pub mod config;
pub mod conversation;
pub mod doctor;
pub mod explain;
pub mod i18n;
pub mod jobs;
pub mod llm;
pub mod models;
pub mod notes;
pub mod ops;
pub mod polite;
pub mod pomodoro;
pub mod prompts;
pub mod secrets;
pub mod sentences;
pub mod services;
pub mod session;
pub mod speech;
pub mod srs;
pub mod stats;
pub mod syllabus;
pub mod tags;
pub mod testing;
pub mod textdiff;
#[cfg(feature = "gui")]
pub mod ui;
pub mod watch;
