//! Per-frame loudness.
//!
//! The envelope is what phrasing is read from: breath groups show up as the
//! troughs between sustained regions, and stress shows up as local peaks that
//! line up with syllable nuclei.

use crate::frame::{self, SPECTRAL_WINDOW};

/// Floor for the dB conversion. Digital silence is negative infinity, which
/// serialises to `null` in JSON and poisons every plot and average downstream.
pub const SILENCE_DB: f32 = -100.0;

/// Root-mean-square level per frame, in dBFS.
pub fn track(samples: &[f32]) -> Vec<f32> {
    (0..frame::count(samples.len()))
        .map(|i| {
            let w = frame::windowed(samples, i, SPECTRAL_WINDOW);
            let rms = (w.iter().map(|s| s * s).sum::<f32>() / (w.len() as f32)).sqrt();
            to_db(rms)
        })
        .collect()
}

/// Amplitude ratio to dBFS, floored at [`SILENCE_DB`].
pub fn to_db(amplitude: f32) -> f32 {
    if amplitude <= 0.0 {
        return SILENCE_DB;
    }
    (20.0 * amplitude.log10()).max(SILENCE_DB)
}
