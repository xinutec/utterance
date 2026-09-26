//! Score in, audio out — the mechanical layer (`docs/architecture.md`). Every
//! frequency arrives absolute, so nothing here knows what a scale is.
//!
//! Additive synthesis because it is forced: a derived tuning puts notes where
//! no sampled instrument can play, and only summed sinusoids carry the score's
//! own timbre, which the scale's consonance depends on. Nothing here shapes a
//! phrase or voices a chord; if it ever does, the split has failed.

// See `utterance-analysis/src/lib.rs` for what this bar is and what is
// deliberately left out of it.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented,
    clippy::unreachable,
    clippy::infinite_loop,
    clippy::while_float
)]

pub mod synth;
pub mod wav;
