//! The words the program says to the model, and how it reads the answers back.
//!
//! One rule runs through all of them: a claim about the candidate's own experience may come only
//! from the context the candidate wrote. General technical knowledge is fine, but it is never
//! passed off as something the candidate did.

pub mod cards;
pub mod explain;
pub mod feedback;
pub mod interviewer;
pub mod json;
pub mod prepare;
pub mod research;
pub mod translate;
pub mod tutor;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PromptError {
    #[error("The model did not return {0}. Try again or pick another model.")]
    Unusable(&'static str),
}

/// The language a session is conducted in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    #[default]
    En,
    Ru,
}

impl Lang {
    pub fn name(self) -> &'static str {
        match self {
            Self::En => "English",
            Self::Ru => "Russian",
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Ru => "ru",
        }
    }
}

/// The honesty rule, repeated wherever the model writes in the candidate's voice.
pub const HONESTY: &str = "Statements about the candidate's own experience, employers, dates, numbers, tools and results \
must come ONLY from the candidate context and the CV. Never invent any of them. If the context does not support a claim, \
say so plainly (for example \"I haven't used X in production\") and bridge to the closest real experience. General \
technical knowledge is fine as a fact about the technology, never as something the candidate did. Respect the sections \
\"Honest limits\" and \"Never claim\".";
