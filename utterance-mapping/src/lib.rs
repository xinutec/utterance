//! Voiceprint in, musical decisions out.
//!
//! The aesthetic layer (`docs/architecture.md`): code that can be *wrong about
//! nothing*. Calling a roughness minimum a note is a choice, and another choice
//! is another mapping, not a bug. Analysis never learns this crate exists, so a
//! discarded idea costs no DSP. What can be tested is the arithmetic and that a
//! derivation reads its input — never the taste.

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

pub mod compose;
pub mod dissonance;
pub mod field;
pub mod lattice;
pub mod mapping;
pub mod params;
pub mod score;
pub mod streams;
pub mod tonnetz;
pub mod tuning;
pub mod voice;
