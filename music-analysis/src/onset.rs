//! Event detection by spectral flux.
//!
//! These are *events*, not beats. A syllable onset, a plosive release, the start
//! of a vowel. Turning them into a metrical structure — grouping them into feet,
//! finding the strong/weak alternation — is a mapping-layer job that needs the
//! stress hierarchy this does not yet compute.
//!
//! Flux rather than raw energy rise, because a vowel-to-vowel transition at a
//! constant level is an onset a listener hears and an energy tracker misses: the
//! spectrum changes even though the loudness does not.

use rustfft::FftPlanner;
use rustfft::num_complex::Complex32;

use crate::frame::{self, SPECTRAL_WINDOW};

/// Minimum gap between reported onsets, in frames (50 ms).
///
/// Below the fastest syllable rate anyone speaks at, so this only merges the
/// multiple flux peaks a single articulation produces — a plosive burst followed
/// by its vowel onset is one event, not two.
const MIN_SEPARATION: usize = 5;

/// Half-width of the moving median window used for the adaptive threshold, in
/// frames (~150 ms each side).
const MEDIAN_HALF_WIDTH: usize = 15;

/// Level drop, in dB between adjacent frames, at which flux is fully suppressed.
///
/// A sound *stopping* also produces positive spectral flux: truncating a
/// steady tone widens its mainlobe, so neighbouring bins gain magnitude even as
/// the tone loses it, and half-wave rectification counts that as an increase.
/// Without this, every burst reports two events — one where it starts and one
/// where it stops.
///
/// The gate is graded rather than binary so that a vowel-to-vowel transition at
/// a constant level, which is a real onset with no level rise at all, still
/// passes at full weight.
const OFFSET_SUPPRESSION_DB: f32 = 3.0;

/// Frames averaged on each side of a candidate when deciding whether the level
/// is rising or falling through it (30 ms each way).
const GATE_SPAN: usize = 3;

/// How far above the local median a peak must sit to count, as a fraction of the
/// overall flux range. Raise to report fewer, more confident onsets.
const THRESHOLD_DELTA: f32 = 0.08;

/// Half-wave-rectified spectral flux per frame, normalised to 0..1.
///
/// Kept in the voiceprint alongside the picked onsets: the continuous curve is
/// the measurement, and the onset list is one thresholding of it. A mapping that
/// wants different sensitivity should re-pick from the curve rather than ask the
/// analyser to re-run.
pub fn flux(samples: &[f32]) -> Vec<f32> {
    let n = frame::count(samples.len());
    if n == 0 {
        return Vec::new();
    }

    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(SPECTRAL_WINDOW);
    let window = frame::hann(SPECTRAL_WINDOW);
    let bins = SPECTRAL_WINDOW / 2 + 1; // real input: the upper half mirrors.

    let mut out = vec![0.0f32; n];
    let mut prev = vec![0.0f32; bins];
    let mut buf = vec![Complex32::new(0.0, 0.0); SPECTRAL_WINDOW];
    let level_db = crate::energy::track(samples);

    for (i, slot) in out.iter_mut().enumerate() {
        let frame_samples = frame::windowed(samples, i, SPECTRAL_WINDOW);
        for (b, (s, w)) in buf.iter_mut().zip(frame_samples.iter().zip(&window)) {
            *b = Complex32::new(s * w, 0.0);
        }
        fft.process(&mut buf);

        // Half-wave rectified: only increases in a bin signal an onset. A
        // decrease is a sound ending, which is a different event.
        let mut sum = 0.0f32;
        for (k, p) in prev.iter_mut().enumerate().take(bins) {
            let mag = buf[k].norm();
            sum += (mag - *p).max(0.0);
            *p = mag;
        }
        *slot = sum * offset_gate(&level_db, i);
    }

    // Frame 0 has no predecessor, so its flux is the whole spectrum appearing at
    // once. That is an artefact of where the recording starts, not an onset.
    out[0] = 0.0;
    normalise(out)
}

/// Weight in 0..1 that suppresses flux caused by a sound stopping.
///
/// Compares the mean level over the frames *after* the candidate against the
/// frames *before* it: an onset leaves more sound behind than it found, an
/// offset leaves less. 1.0 when the level is flat or rising, tapering to 0.0
/// once the drop reaches [`OFFSET_SUPPRESSION_DB`].
///
/// Straddling the candidate rather than differencing adjacent frames, because
/// the energy and spectral windows are both 32 ms: a truncation smears its level
/// drop across three frames while the flux spike from it is sharp, so an
/// adjacent-frame test still reads flat at the exact frame that spikes.
fn offset_gate(level_db: &[f32], i: usize) -> f32 {
    let before = mean_level(level_db, i.saturating_sub(GATE_SPAN), i);
    let after = mean_level(level_db, i + 1, i + 1 + GATE_SPAN);
    match (before, after) {
        (Some(b), Some(a)) if a < b => (1.0 + (a - b) / OFFSET_SUPPRESSION_DB).clamp(0.0, 1.0),
        // Nothing to compare against at the very edges of the recording, and a
        // rising or flat level is exactly what an onset looks like.
        _ => 1.0,
    }
}

/// Mean level over `[lo, hi)`, clamped to the series; `None` if that is empty.
fn mean_level(level_db: &[f32], lo: usize, hi: usize) -> Option<f32> {
    let hi = hi.min(level_db.len());
    if lo >= hi {
        return None;
    }
    Some(level_db[lo..hi].iter().sum::<f32>() / (hi - lo) as f32)
}

/// Scale to 0..1 by the maximum. Flux is unitless, and every threshold below is
/// expressed relative to the signal's own range.
fn normalise(mut x: Vec<f32>) -> Vec<f32> {
    let max = x.iter().copied().fold(0.0f32, f32::max);
    if max > 0.0 {
        for v in &mut x {
            *v /= max;
        }
    }
    x
}

/// Pick onset frames from a flux curve.
///
/// A peak qualifies when it is a local maximum, sits [`THRESHOLD_DELTA`] above
/// the local median, and is at least [`MIN_SEPARATION`] frames from the last one
/// accepted. The median is local rather than global because speech changes level
/// constantly — a fixed threshold tuned on a loud phrase goes deaf on a quiet one.
pub fn pick(flux: &[f32]) -> Vec<usize> {
    let mut picked: Vec<usize> = Vec::new();
    for i in 1..flux.len().saturating_sub(1) {
        if flux[i] < flux[i - 1] || flux[i] < flux[i + 1] {
            continue;
        }
        if flux[i] < local_median(flux, i) + THRESHOLD_DELTA {
            continue;
        }
        match picked.last() {
            // Within the refractory window: keep whichever peak is stronger,
            // rather than always the earlier one.
            Some(&last) if i - last < MIN_SEPARATION => {
                if flux[i] > flux[last] {
                    let n = picked.len();
                    picked[n - 1] = i;
                }
            }
            _ => picked.push(i),
        }
    }
    picked
}

/// Median of the flux curve in a window centred on `i`.
fn local_median(flux: &[f32], i: usize) -> f32 {
    let lo = i.saturating_sub(MEDIAN_HALF_WIDTH);
    let hi = (i + MEDIAN_HALF_WIDTH + 1).min(flux.len());
    let mut w: Vec<f32> = flux[lo..hi].to_vec();
    w.sort_by(f32::total_cmp);
    w[w.len() / 2]
}
