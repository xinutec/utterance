//! Fundamental-frequency tracking by the YIN algorithm.
//!
//! Prosody, not melody. What comes out is the shape of a speaking voice — glides,
//! declination, the rise at the end of a question — sampled on the frame grid.
//! Mapping layers should read it as a gesture; quantising it straight to a scale
//! is the obvious move and the wrong one.
//!
//! De Cheveigné & Kawahara (2002), "YIN, a fundamental frequency estimator for
//! speech and music", JASA 111(4).

use crate::frame::{self, PITCH_WINDOW};
use crate::resample::ANALYSIS_RATE;

/// Lowest tracked f0. Below a typical bass speaking range; going lower costs a
/// longer window, which smears the fast contour movements we care about.
pub const F0_MIN_HZ: f32 = 70.0;

/// Highest tracked f0, above a typical soprano speaking range.
pub const F0_MAX_HZ: f32 = 500.0;

/// YIN's absolute threshold on the normalised difference. A frame whose best
/// candidate does not get under this is not periodic enough to call voiced.
///
/// 0.15 rather than the paper's 0.10: speech recorded on a room microphone is
/// noisier than the paper's material, and at 0.10 the tail of every vowel drops
/// out — which reads as a gap in the contour rather than the decay it is.
const THRESHOLD: f32 = 0.15;

/// One frame's pitch estimate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct F0Frame {
    /// Estimated fundamental, or `None` where the frame is not voiced.
    ///
    /// `None`, never a sentinel 0.0: an unvoiced frame has no fundamental, and a
    /// downstream mean over zeros would be quietly wrong.
    pub hz: Option<f32>,
    /// YIN's normalised difference at the chosen lag, in roughly 0..1. Low is
    /// strongly periodic. Kept even for unvoiced frames — it is the continuous
    /// measurement, and `hz` is just it thresholded.
    pub aperiodicity: f32,
}

/// Track f0 across every frame of `samples` (mono, at [`ANALYSIS_RATE`]).
pub fn track(samples: &[f32]) -> Vec<F0Frame> {
    let tau_min = (ANALYSIS_RATE as f32 / F0_MAX_HZ).floor() as usize;
    let tau_max = (ANALYSIS_RATE as f32 / F0_MIN_HZ).ceil() as usize;

    (0..frame::count(samples.len()))
        .map(|i| estimate(&frame::windowed(samples, i, PITCH_WINDOW), tau_min, tau_max))
        .collect()
}

/// YIN on a single window.
fn estimate(window: &[f32], tau_min: usize, tau_max: usize) -> F0Frame {
    let tau_max = tau_max.min(window.len() / 2);
    if tau_max <= tau_min {
        return F0Frame {
            hz: None,
            aperiodicity: 1.0,
        };
    }

    let diff = difference(window, tau_max);
    let norm = cumulative_mean_normalised(&diff);

    // Step 4 of the paper: take the FIRST lag that dips below the threshold, not
    // the global minimum. The global minimum is often an octave down — a signal
    // periodic at T is also periodic at 2T, and it usually scores marginally
    // better. Preferring the earliest qualifying dip is what stops octave errors.
    let mut best = None;
    for tau in tau_min..tau_max {
        if norm[tau] < THRESHOLD {
            // Walk to the bottom of this dip rather than taking its leading edge.
            let mut t = tau;
            while t + 1 < tau_max && norm[t + 1] < norm[t] {
                t += 1;
            }
            best = Some(t);
            break;
        }
    }

    // Nothing crossed the threshold: report the best candidate anyway, flagged
    // unvoiced. The lag is still the most likely period if a caller wants it,
    // and the aperiodicity says how much to trust it.
    let voiced = best.is_some();
    let tau = best.unwrap_or_else(|| {
        (tau_min..tau_max)
            .min_by(|&a, &b| norm[a].total_cmp(&norm[b]))
            .unwrap_or(tau_min)
    });

    let refined = parabolic_refine(&norm, tau);
    let aperiodicity = norm[tau].clamp(0.0, 1.0);
    let hz = (ANALYSIS_RATE as f32) / refined;

    F0Frame {
        // A refined lag can land just outside the tracked band; that is a
        // rejected estimate, not a clamped one.
        hz: (voiced && (F0_MIN_HZ..=F0_MAX_HZ).contains(&hz)).then_some(hz),
        aperiodicity,
    }
}

/// YIN step 1: the squared-difference function d(tau).
fn difference(x: &[f32], tau_max: usize) -> Vec<f32> {
    // O(W * tau_max) as written — about 230k operations per frame at our window
    // and range, which is not the bottleneck. It can become an FFT-based
    // autocorrelation if it ever is.
    let n = x.len() - tau_max;
    (0..tau_max)
        .map(|tau| (0..n).map(|j| (x[j] - x[j + tau]).powi(2)).sum())
        .collect()
}

/// YIN step 2: the cumulative mean normalised difference d'(tau).
///
/// This is the step that makes the threshold absolute. Raw d(tau) is smallest at
/// tau = 0 and scales with signal level, so no fixed cutoff works; dividing by
/// the running mean removes both problems.
fn cumulative_mean_normalised(diff: &[f32]) -> Vec<f32> {
    let mut out = vec![1.0f32; diff.len()];
    let mut running = 0.0f32;
    for tau in 1..diff.len() {
        running += diff[tau];
        out[tau] = if running <= f32::EPSILON {
            1.0
        } else {
            diff[tau] * (tau as f32) / running
        };
    }
    out
}

/// Fit a parabola through the minimum and its neighbours for sub-sample lag.
///
/// Without this the estimate is quantised to whole samples: at 16 kHz a lag of
/// 40 vs 41 samples is 400 vs 390 Hz, a 43-cent step. The contour would move in
/// visible stairs.
fn parabolic_refine(norm: &[f32], tau: usize) -> f32 {
    if tau == 0 || tau + 1 >= norm.len() {
        return tau as f32;
    }
    let (a, b, c) = (norm[tau - 1], norm[tau], norm[tau + 1]);
    let denom = 2.0 * (2.0 * b - a - c);
    if denom.abs() < f32::EPSILON {
        return tau as f32;
    }
    tau as f32 + (c - a) / denom
}
