//! Voiceprint in, musical decisions out.
//!
//! The aesthetic layer of the three described in `docs/architecture.md`, and the
//! first code in this repo that can be *wrong about nothing*. Everything in
//! `utterance-analysis` answers a question with a right answer — is this frame
//! voiced, where is F2. Nothing here does. Whether the minima of a dissonance
//! curve should be called the notes of a scale is a choice, and a different
//! choice would be a different mapping rather than a bug in this one.
//!
//! That is why it is a separate crate. The dependency runs one way — mapping
//! reads what analysis measured, and analysis must never learn this crate
//! exists — so a discarded aesthetic idea can be deleted without touching a line
//! of DSP.
//!
//! What is testable here is narrower than next door, and worth being honest
//! about: the arithmetic (does the dissonance model reproduce its published
//! curve, does a scale derived from a harmonic spectrum land near just
//! intonation), and that a derivation reads its input rather than restating its
//! own assumptions. Never the taste.

pub mod compose;
pub mod dissonance;
pub mod field;
pub mod lattice;
pub mod params;
pub mod score;
pub mod streams;
pub mod tonnetz;
pub mod tuning;
pub mod voice;
