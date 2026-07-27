//! The score: the artefact between mapping and realisation.
//!
//! The second stable interface in the project, alongside the voiceprint, and it
//! earns its keep the same way — realisation can be rewritten without touching a
//! mapping, and a mapping can be replaced without touching a synthesiser.
//!
//! **Frequencies are absolute, in hertz.** No degrees, no scale, no key. This is
//! the mirror image of the rule that keeps analysis from knowing what a scale is:
//! realisation must not know either, or the choice of tuning leaks into the
//! synthesiser and the two stop being separable. By the time a score exists,
//! every musical decision has already been made.

use serde::{Deserialize, Serialize};

/// One sounded note.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub start_s: f32,
    pub duration_s: f32,
    /// Absolute pitch. Whatever tuning produced it is already resolved.
    pub hz: f32,
    /// Relative loudness, 0..1.
    pub amplitude: f32,
}

/// Everything needed to render a piece.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Score {
    pub duration_s: f32,
    /// Relative amplitude of each harmonic, starting at the fundamental.
    ///
    /// Carried in the score rather than chosen by the synthesiser, because a
    /// derived tuning is only consonant for tones that actually have the
    /// spectrum it was derived from. Tune to one spectrum and play another and
    /// the minima no longer line up with the notes — the scale keeps its
    /// numbers and loses its justification.
    pub timbre: Vec<f32>,
    /// Ascending by start time.
    pub events: Vec<Event>,
}
