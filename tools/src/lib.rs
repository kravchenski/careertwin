//! Small tools that the repository's own workflows and maintainers run. They stand alone: nothing here is
//! part of CareerTwin the program.

pub mod release;

#[cfg(feature = "links")]
pub mod links;
