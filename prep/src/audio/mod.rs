//! Sound in and out: the microphone, the speakers, and the cutting of speech into turns.

pub mod capture;
pub mod playback;
pub mod resample;
pub mod vad;

pub use capture::Mic;
pub use playback::{Player, Speaker};
