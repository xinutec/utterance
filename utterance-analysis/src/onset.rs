//! Event detection by spectral flux.
//!
//! Events, not beats: grouping them into meter needs the stress hierarchy, a
//! mapping-layer job. Flux rather than energy rise, because a vowel-to-vowel
//! transition at constant level is an onset a listener hears.
//!
//! ⚠ **Flux measures *the spectrum changed*, not *a sound started*.** In speech
//! they mostly coincide; a glided *ee → ah → oo* has no events and plenty of
//! flux. So thresholds must be judged on speech: sustained material can bound
//! over-firing (`tests/onset_real.rs`) but has no right count. The real fix is
//! the stress hierarchy.

use rustfft::num_complex::Complex32;

/// Minimum gap between reported onsets, in frames (50 ms) — below any syllable
/// rate, so it only merges the peaks one articulation makes, like a plosive
/// burst and its vowel.
const MIN_SEPARATION: usize = 5;

/// Span of the local window the adaptive threshold is measured over, in frames
/// (~250 ms, straddling the candidate, minus the guard band below).
const HISTORY_FRAMES: usize = 25;

/// Frames either side of a candidate excluded from its own threshold, so an
/// event never raises the bar it is judged against.
const GUARD_FRAMES: usize = 5;

/// Frames either side that a candidate must dominate to count as a peak (50 ms):
/// shorter than the gap between separate events, wider than a steady sound's
/// wobble.
const PEAK_WINDOW: usize = 5;

/// Level drop, in dB, at which flux is fully suppressed.
///
/// A sound *stopping* also produces positive flux (truncation widens the
/// mainlobe), so without this every burst reports two events. Graded, so a
/// vowel-to-vowel transition at constant level still passes at full weight.
const OFFSET_SUPPRESSION_DB: f32 = 3.0;

/// Frames examined each side of a candidate to decide whether the level rises or
/// falls through it (100 ms): a voice's release takes a couple of hundred
/// milliseconds, and a shorter span reads the decay as steady.
const GATE_SPAN: usize = 10;

/// How many local median-absolute-deviations above the local median a peak must
/// sit — the main sensitivity knob (see [`threshold`]).
///
/// **This, [`THRESHOLD_FLOOR`] and [`SILENCE_MARGIN_DB`] were fitted on one
/// sustained-vowel fixture and are unvalidated on speech**: a starting point, not
/// a result.
const THRESHOLD_MADS: f32 = 6.0;

/// How far above the noise floor a frame must sit before its flux counts fully,
/// in dB. Below the floor it is ignored; it ramps in across this range.
const SILENCE_MARGIN_DB: f32 = 15.0;

/// Floor on the threshold, as a fraction of the take's peak flux: where flux is
/// flat the MAD collapses and any wobble would clear a purely relative bar.
const THRESHOLD_FLOOR: f32 = 0.06;

/// Half-wave-rectified spectral flux per frame, normalised to 0..1, from the
/// frames' spectra and levels.
///
/// Kept in the voiceprint beside the onsets: the curve is the measurement, the
/// onsets one thresholding of it, and a mapping can re-pick from it.
pub fn flux(spectra: &[Vec<Complex32>], level_db: &[f32]) -> Vec<f32> {
    if spectra.is_empty() {
        return Vec::new();
    }
    let floor = noise_floor(level_db);
    let mut prev: Vec<f32> = vec![0.0; spectra[0].len()];
    let mut out: Vec<f32> = spectra
        .iter()
        .enumerate()
        .map(|(i, spectrum)| {
            // Only increases count: a decrease is a sound ending.
            let mut sum = 0.0f32;
            for (bin, p) in spectrum.iter().zip(prev.iter_mut()) {
                let mag = bin.norm();
                sum += (mag - *p).max(0.0);
                *p = mag;
            }
            sum * offset_gate(level_db, i) * silence_gate(level_db, floor, i)
        })
        .collect();

    // Frame 0 has no predecessor: its "flux" is where the recording starts.
    out[0] = 0.0;
    normalise(out)
}

/// Weight in 0..1 that suppresses flux near the noise floor.
///
/// Flux is relative, so room tone shuffling between bins scores like an attack,
/// and the local threshold is most permissive exactly there. Judged against the
/// take's own floor. ⚠ Over a short window *starting* at the candidate: an onset
/// is the moment the level is still climbing, so the frame alone would
/// attenuate every real one.
fn silence_gate(level_db: &[f32], floor: f32, i: usize) -> f32 {
    let hi = (i + GATE_SPAN).min(level_db.len());
    let present = level_db[i..hi]
        .iter()
        .copied()
        .fold(f32::NEG_INFINITY, f32::max);
    ((present - floor) / SILENCE_MARGIN_DB).clamp(0.0, 1.0)
}

/// Estimated noise floor: the 10th percentile of the take's frame levels, not
/// the minimum, which one anomalous frame would decide.
fn noise_floor(level_db: &[f32]) -> f32 {
    let mut sorted: Vec<f32> = level_db.to_vec();
    sorted.sort_by(f32::total_cmp);
    sorted[sorted.len() / 10]
}

/// Weight in 0..1 that suppresses flux caused by a sound stopping: 1 when the
/// mean level after the candidate is at least the mean before, tapering to 0 at
/// [`OFFSET_SUPPRESSION_DB`] of drop. Straddling rather than adjacent frames,
/// because the 32 ms windows smear a truncation's drop over three frames while
/// its flux spike is sharp.
fn offset_gate(level_db: &[f32], i: usize) -> f32 {
    let before = mean_level(level_db, i.saturating_sub(GATE_SPAN), i);
    // Means, not the loudest frame after: that reads the decaying tail of the
    // very sound being suppressed.
    let after = mean_level(level_db, i + 1, i + 1 + GATE_SPAN);
    match (before, after) {
        (Some(b), Some(a)) if a < b => (1.0 + (a - b) / OFFSET_SUPPRESSION_DB).clamp(0.0, 1.0),
        // Nothing to compare at the edges; flat or rising is what an onset is.
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

/// Scale to 0..1 by the maximum; every threshold here is relative.
fn normalise(mut x: Vec<f32>) -> Vec<f32> {
    let max = x.iter().copied().fold(0.0f32, f32::max);
    if max > 0.0 {
        for v in &mut x {
            *v /= max;
        }
    }
    x
}

/// Pick onset frames from a flux curve: local maxima that clear the local
/// threshold, at least `MIN_SEPARATION` apart.
pub fn pick(flux: &[f32]) -> Vec<usize> {
    let mut picked: Vec<usize> = Vec::new();
    for i in 1..flux.len().saturating_sub(1) {
        if !is_local_maximum(flux, i) {
            continue;
        }
        if flux[i] < threshold(flux, i) {
            continue;
        }
        match picked.last() {
            // Within the refractory window, keep the stronger peak.
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

/// Whether `i` is the largest flux value within [`PEAK_WINDOW`] frames either
/// side. Beating the two neighbours is not enough: sustained phonation's jitter
/// does that every few frames, and no threshold can sort those out.
fn is_local_maximum(flux: &[f32], i: usize) -> bool {
    let lo = i.saturating_sub(PEAK_WINDOW);
    let hi = (i + PEAK_WINDOW + 1).min(flux.len());
    // Strictly greater going back, so a flat run reports its first frame.
    flux[lo..i].iter().all(|&v| v < flux[i]) && flux[i + 1..hi].iter().all(|&v| v <= flux[i])
}

/// The value a peak at `i` must exceed: `median + k · MAD`, floored.
///
/// Scaled by the spread, not only offset from the level: a sustained vowel is
/// low but jittery, and a fixed offset reported 22 onsets in seven seconds of
/// one held vowel. A jittery stretch demands a proportionally larger peak.
fn threshold(flux: &[f32], i: usize) -> f32 {
    let (median, mad) = local_spread(flux, i);
    median + (THRESHOLD_MADS * mad).max(THRESHOLD_FLOOR)
}

/// Median and median-absolute-deviation of the flux around `i`.
///
/// MAD, because one outlier moves a standard deviation a long way. The window
/// is centred, since a backward one collapses after a pause, and excludes a
/// guard band, since the candidate's own peak would inflate its bar — the usual
/// constant-false-alarm-rate arrangement.
fn local_spread(flux: &[f32], i: usize) -> (f32, f32) {
    let half = HISTORY_FRAMES / 2;
    let lo = i.saturating_sub(half);
    let hi = (i + half + 1).min(flux.len());
    let guard_lo = i.saturating_sub(GUARD_FRAMES);
    let guard_hi = (i + GUARD_FRAMES + 1).min(flux.len());

    let mut w: Vec<f32> = flux[lo..guard_lo]
        .iter()
        .chain(&flux[guard_hi..hi])
        .copied()
        .collect();
    if w.is_empty() {
        return (0.0, 0.0);
    }
    w.sort_by(f32::total_cmp);
    let median = w[w.len() / 2];

    let mut deviations: Vec<f32> = w.iter().map(|v| (v - median).abs()).collect();
    deviations.sort_by(f32::total_cmp);
    (median, deviations[deviations.len() / 2])
}
