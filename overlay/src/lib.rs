//! The interview window of CareerTwin: hears the call, writes what to say, answers what is on the screen.
//!
//! This crate is the Rust port of the Python overlay (`src/careertwin/interview`). It reads and writes the
//! same files (`data/interview/settings.json`, the saved transcripts), so the two can be swapped one for
//! the other while the port is finished.

pub mod accounts;
pub mod brief;
pub mod coach;
pub mod export;
pub mod languages;
pub mod lexicon;
pub mod paths;
pub mod prompt_text;
pub mod prompts;
pub mod providers;
pub mod questions;
pub mod screenshot;
pub mod settings;
pub mod transcript;
pub mod update;

#[cfg(feature = "gui")]
pub mod ui;
