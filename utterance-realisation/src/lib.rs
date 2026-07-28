//! Score in, audio out.
//!
//! The mechanical layer of the three in `docs/architecture.md`. It makes no
//! decisions: every frequency in a score is already absolute, so this crate does
//! not know what a scale is, what a key is, or that anything here came from a
//! voice. One score, rendered the same way every time.
//!
//! **Additive synthesis, not samples.** Forced rather than chosen: a derived
//! tuning puts notes wherever the speaker's spectrum says they belong, and a
//! sampled instrument cannot play 582 cents. Summing sinusoids can play
//! anything, and it can carry the score's own timbre — which matters, because a
//! scale derived from one spectrum is only consonant for tones that have it.
//!
//! **What this is not.** Nothing here shapes a phrase, voices a chord or decides
//! an articulation. Those are musical judgements and they belong to mapping. If
//! this crate ever starts making one, the split has failed and every aesthetic
//! experiment starts costing a synthesiser rewrite.

pub mod synth;
pub mod wav;
