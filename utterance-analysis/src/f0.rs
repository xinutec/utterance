//! Fundamental-frequency tracking by YIN (de Cheveigné & Kawahara, 2002).
//!
//! Prosody, not melody: glides and declination, to be read as gesture —
//! quantising it straight to a scale is the obvious move and the wrong one.

use crate::frame::{self, PITCH_WINDOW};
use crate::resample::ANALYSIS_RATE;

/// Lowest tracked f0. Below a typical bass speaking range; going lower costs a
/// longer window, which smears the fast contour movements we care about.
pub const F0_MIN_HZ: f32 = 70.0;

/// Highest tracked f0, above a typical soprano speaking range.
pub const F0_MAX_HZ: f32 = 500.0;

/// YIN's threshold on the normalised difference for calling a frame voiced.
/// 0.15 rather than the paper's 0.10: on a room microphone 0.10 drops every
/// vowel's tail out of the contour.
const THRESHOLD: f32 = 0.15;

/// One frame's pitch estimate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct F0Frame {
    /// Estimated fundamental, or `None` where unvoiced — never a sentinel 0.
    pub hz: Option<f32>,
    /// YIN's normalised difference at the chosen lag, roughly 0..1, low when
    /// periodic — kept for every frame; `hz` is it thresholded.
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

    // Step 4 of the paper: the FIRST lag under the threshold, not the global
    // minimum, which is often an octave down.
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

    // Nothing crossed the threshold: report the best lag anyway, unvoiced.
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
        // A refined lag just outside the band is rejected, not clamped.
        hz: (voiced && (F0_MIN_HZ..=F0_MAX_HZ).contains(&hz)).then_some(hz),
        aperiodicity,
    }
}

/// YIN step 1: the squared-difference function d(tau).
fn difference(x: &[f32], tau_max: usize) -> Vec<f32> {
    // O(W·tau_max), about 180k operations per frame.
    let n = x.len() - tau_max;
    (0..tau_max)
        .map(|tau| (0..n).map(|j| (x[j] - x[j + tau]).powi(2)).sum())
        .collect()
}

/// YIN step 2: the cumulative mean normalised difference d'(tau), which makes
/// the threshold independent of lag and level.
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

/// Fit a parabola through the minimum for a sub-sample lag — whole samples step
/// by 43 cents at 400 Hz.
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
