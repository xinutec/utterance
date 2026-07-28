//! The analysis frame grid.
//!
//! One grid for the whole voiceprint: every per-frame series (f0, energy, flux)
//! is indexed by the same frame number, so they can be read side by side without
//! interpolation. Analyses that need different amounts of context around a frame
//! vary their *window*, never their hop.

use crate::resample::ANALYSIS_RATE;

/// Samples between consecutive frames — 10 ms, i.e. 100 frames per second.
///
/// The speech-analysis convention: short enough to place a plosive burst, long
/// enough that 30 seconds stays a few thousand frames rather than a few hundred
/// thousand.
pub const HOP: usize = ANALYSIS_RATE as usize / 100;

/// Window for pitch estimation, 64 ms.
///
/// Set by the lowest f0 we track. YIN searches lags out to one period of the
/// lowest voice — 229 samples at 70 Hz — and its difference function needs the
/// window to be at least twice the longest lag it examines, so the search is
/// capped at `window / 2` (see `f0::estimate`). 1024 leaves that cap comfortably
/// above 229. Anything shorter silently loses the bottom of a low male range.
pub const PITCH_WINDOW: usize = 1024;

/// Window for spectral analysis, 32 ms. Shorter than the pitch window because
/// onset detection wants time resolution, not frequency resolution.
pub const SPECTRAL_WINDOW: usize = 512;

/// Number of frames covering `len` samples.
pub fn count(len: usize) -> usize {
    if len == 0 { 0 } else { len.div_ceil(HOP) }
}

/// Start time of frame `i`, in seconds.
pub fn time_s(i: usize) -> f32 {
    (i * HOP) as f32 / ANALYSIS_RATE as f32
}

/// Copy the `window`-sample window centred on frame `i`, zero-padded at the
/// signal edges.
///
/// Centred rather than left-aligned so a frame's measurements describe the audio
/// *at* its timestamp. A left-aligned window reports every event half a window
/// late, which is invisible in a plot and fatal once onsets drive rhythm.
pub fn windowed(samples: &[f32], i: usize, window: usize) -> Vec<f32> {
    let center = (i * HOP) as isize;
    let start = center - (window as isize) / 2;
    (0..window)
        .map(|k| {
            let idx = start + k as isize;
            if idx < 0 || idx as usize >= samples.len() {
                0.0
            } else {
                samples[idx as usize]
            }
        })
        .collect()
}

/// Periodic Hann window of length `n`.
///
/// Periodic (divisor `n`) rather than symmetric (`n - 1`): these windows feed an
/// FFT, where the periodic form is the one that sums to a constant under overlap.
pub fn hann(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let x = 2.0 * std::f32::consts::PI * (i as f32) / (n as f32);
            0.5 * (1.0 - x.cos())
        })
        .collect()
}

/// Hamming window of length `n`.
///
/// Used for linear prediction rather than the Hann above: its lower first
/// sidelobe keeps energy from one harmonic of the source out of the
/// autocorrelation of its neighbours, which is what the pole fit is trying to
/// see past.
pub fn hamming(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let x = 2.0 * std::f32::consts::PI * (i as f32) / (n as f32);
            0.54 - 0.46 * x.cos()
        })
        .collect()
}
