//! CareerTwin Prep: spoken practice for English, technical and general interviews with an
//! interviewer who talks like a person.
//!
//! The library holds everything that is not a window: settings, the facts about the candidate,
//! the model client, the prompts, sessions and spaced repetition.

#![cfg_attr(test, allow(clippy::field_reassign_with_default))]

pub mod audio;
pub mod brief;
pub mod config;
pub mod conversation;
pub mod llm;
pub mod prompts;
pub mod secrets;
pub mod sentences;
pub mod session;
pub mod speech;
pub mod srs;
pub mod testing;
pub mod textdiff;
