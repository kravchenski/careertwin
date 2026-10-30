//! The interview window of CareerTwin: hears the call, writes what to say, answers what is on the screen.
//!
//! This crate is the Rust port of the Python overlay (`src/careertwin/interview`). It reads and writes the
//! same files (`data/interview/settings.json`, the saved transcripts), so the two can be swapped one for
//! the other while the port is finished.

pub mod accounts;
pub mod audio;
pub mod brief;
pub mod capture_exclude;
pub mod coach;
pub mod control;
pub mod export;
pub mod hearing;
pub mod hotkey;
pub mod languages;
pub mod lexicon;
pub mod library;
pub mod log;
pub mod look;
pub mod models;
pub mod paths;
pub mod prepare;
pub mod project;
pub mod project_text;
pub mod prompt_text;
pub mod prompts;
pub mod providers;
pub mod questions;
pub mod screenshot;
pub mod session;
pub mod settings;
pub mod speech;
pub mod transcript;
pub mod tray;
pub mod update;
pub mod vad;

#[cfg(feature = "gui")]
pub mod ui;
