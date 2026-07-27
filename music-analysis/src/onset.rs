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
//!
//! # What flux cannot tell apart
//!
//! Spectral flux measures *the spectrum changed*, and reads that as *a sound
//! started*. In speech the two coincide — a new syllable is a new articulation —
//! which is why the measure works at all. They come apart whenever a single
//! continuous sound changes shape.
//!
//! The clean demonstration is a glided vowel: *ee → ah → oo* on one unbroken
//! breath contains no events whatsoever, yet produces large flux wherever the
//! articulators move quickly between targets. Nothing in the flux curve
//! distinguishes that from a genuine onset, because in purely spectral terms
//! there is no difference.
//!
//! The consequence for tuning: **onset thresholds must be judged on speech, not
//! on sustained material.** A held or glided vowel can bound how badly the
//! detector over-fires, and `tests/onset_real.rs` uses one for exactly that, but
//! it cannot say what the right count is — the question has no answer there.
//! Resolving the ambiguity properly needs a cue flux does not carry: the stress
//! hierarchy, which is where the metrical work in `docs/architecture.md` starts.

use rustfft::FftPlanner;
use rustfft::num_complex::Complex32;

use crate::frame::{self, SPECTRAL_WINDOW};

/// Minimum gap between reported onsets, in frames (50 ms).
///
/// Below the fastest syllable rate anyone speaks at, so this only merges the
/// multiple flux peaks a single articulation produces — a plosive burst followed
/// by its vowel onset is one event, not two.
const MIN_SEPARATION: usize = 5;

/// How much recent history the adaptive threshold is measured over, in frames
/// (~250 ms). Long enough to characterise a stretch of speech, short enough to
/// follow it as it changes.
const HISTORY_FRAMES: usize = 25;

/// Frames either side of a candidate excluded from its own threshold statistics.
///
/// Wide enough to cover a flux peak and the skirt around it, so an event never
/// contributes to the estimate of what "quiet round here" means.
const GUARD_FRAMES: usize = 5;

/// Frames either side that a candidate must dominate to count as a peak (50 ms).
///
/// Below the shortest gap between two separately articulated events, so a real
/// pair is never merged, but far wider than the wobble of a steady sound.
const PEAK_WINDOW: usize = 5;

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

/// Frames examined on each side of a candidate when deciding whether the level
/// is rising or falling through it (100 ms each way).
///
/// Long enough to span a natural vocal release. At 30 ms it was not: a voice
/// stops over a couple of hundred milliseconds, so the level barely moves across
/// any single 30 ms step and the gate read the whole decay as "steady" —
/// leaving three phantom events at the tail of a real sustained vowel.
const GATE_SPAN: usize = 10;

/// How many local median-absolute-deviations above the local median a peak must
/// sit. Raise to report fewer, more confident onsets.
///
/// The dominant sensitivity knob. See [`threshold`] for why the units are MADs.
const THRESHOLD_MADS: f32 = 6.0;

/// How far above the noise floor a frame must sit before its flux counts fully,
/// in dB. Below the floor it is ignored; it ramps in across this range.
const SILENCE_MARGIN_DB: f32 = 15.0;

/// Floor on the threshold, as a fraction of the take's peak flux.
///
/// Where the flux is genuinely flat — digital silence, or a perfectly steady
/// synthetic tone — the MAD collapses to nearly zero and any wobble at all would
/// clear a purely relative threshold. This keeps a floor under it.
const THRESHOLD_FLOOR: f32 = 0.06;

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
        *slot = sum * offset_gate(&level_db, i) * silence_gate(&level_db, i);
    }

    // Frame 0 has no predecessor, so its flux is the whole spectrum appearing at
    // once. That is an artefact of where the recording starts, not an onset.
    out[0] = 0.0;
    normalise(out)
}

/// Weight in 0..1 that suppresses flux in frames near the noise floor.
///
/// Spectral flux is a *relative* measure — it is normalised by the take's own
/// maximum — so room tone shuffling between bins produces a flux value on the
/// same scale as a real attack, and the local threshold is at its most permissive
/// in exactly those quiet stretches. But there is no such thing as an onset in
/// silence: whatever the spectrum did there, no sound started.
///
/// Judged against the take's own noise floor rather than an absolute dBFS
/// number, because a quiet recording is not a recording of nothing.
///
/// Measured over a short window *starting* at the candidate rather than at the
/// candidate frame alone: the beginning of a sound is precisely the moment its
/// level is still crossing up from the floor, so testing that one frame would
/// attenuate every real onset. What matters is whether sound is present just
/// after.
fn silence_gate(level_db: &[f32], i: usize) -> f32 {
    let floor = noise_floor(level_db);
    let hi = (i + GATE_SPAN).min(level_db.len());
    let present = level_db[i..hi]
        .iter()
        .copied()
        .fold(f32::NEG_INFINITY, f32::max);
    ((present - floor) / SILENCE_MARGIN_DB).clamp(0.0, 1.0)
}

/// Estimated noise floor: the 10th percentile of the take's frame levels.
///
/// A percentile rather than the minimum, which would latch onto a single
/// anomalously quiet frame and put the floor below anything real.
fn noise_floor(level_db: &[f32]) -> f32 {
    let mut sorted: Vec<f32> = level_db.to_vec();
    sorted.sort_by(f32::total_cmp);
    sorted[sorted.len() / 10]
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
    // Means on both sides. Taking the loudest frame after the candidate instead
    // looks appealing — it would protect a phrase-final syllable — but it reads
    // the decaying tail of the very sound being suppressed and lets every offset
    // back through.
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
/// A peak qualifies when it is a local maximum, clears the local threshold, and
/// is at least [`MIN_SEPARATION`] frames from the last one accepted.
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

/// Whether `i` is the largest flux value within [`PEAK_WINDOW`] frames either side.
///
/// The condition that does most of the work, and the one the first version got
/// wrong: it asked only whether a frame exceeded its two immediate neighbours,
/// which every small wobble on a noisy curve satisfies. Sustained phonation is
/// full of such wobbles — cycle-to-cycle jitter — so the detector generated a
/// candidate every few frames and left the threshold to sort them out, which no
/// threshold can do reliably.
///
/// Requiring dominance over a real span asks the right question: an onset is a
/// spike that stands out from its surroundings, not merely a point that happens
/// to sit above the two samples touching it.
fn is_local_maximum(flux: &[f32], i: usize) -> bool {
    let lo = i.saturating_sub(PEAK_WINDOW);
    let hi = (i + PEAK_WINDOW + 1).min(flux.len());
    // Strictly greater going back, so a flat run reports its first frame rather
    // than every frame in it.
    flux[lo..i].iter().all(|&v| v < flux[i]) && flux[i + 1..hi].iter().all(|&v| v <= flux[i])
}

/// The value a peak at `i` must exceed to count as an onset.
///
/// `median + k · MAD`, floored. Adapting to the local *level* alone is not
/// enough — that was the original mistake. A sustained vowel sits at a low flux
/// level but is constantly jittery (cycle-to-cycle pitch and amplitude
/// variation, slow drift in the vowel), so a fixed offset above the local median
/// is cleared by noise dozens of times over a few seconds. Measured on a real
/// seven-second sustained vowel, a fixed offset reported 22 onsets where there
/// is exactly one event.
///
/// Scaling by the median absolute deviation asks the right question instead: not
/// "is this peak bigger than usual round here", but "is it bigger than the
/// variation round here". A jittery stretch demands a proportionally larger peak.
fn threshold(flux: &[f32], i: usize) -> f32 {
    let (median, mad) = local_spread(flux, i);
    median + (THRESHOLD_MADS * mad).max(THRESHOLD_FLOOR)
}

/// Median and median-absolute-deviation of the flux curve *preceding* `i`.
///
/// Backward-looking, not centred. A centred window contains the very peak being
/// judged along with its aftermath, which inflates both statistics exactly where
/// a real event is — the attack of a sound then has to clear a threshold its own
/// arrival raised, and drops out. Measured on the sustained-vowel fixture, a
/// centred window lost the attack entirely while keeping mid-vowel jitter.
///
/// Asking "is this bigger than what was happening just before" is also the
/// better question on its own terms: that is what an onset *is*.
///
/// MAD rather than standard deviation because one outlier in the history moves a
/// standard deviation a long way, and the threshold would rise to meet whatever
/// it was supposed to detect.
fn local_spread(flux: &[f32], i: usize) -> (f32, f32) {
    // Centred, with a guard band excluded around the candidate — the standard
    // constant-false-alarm-rate arrangement. Two separate problems force it:
    //
    // A purely backward window collapses after any quiet stretch (median and MAD
    // both reach zero, the threshold drops to its floor, and the first wobble
    // after a pause is admitted), so the window has to straddle the candidate.
    // But a plain centred window then includes the candidate's own peak and the
    // skirt around it, inflating the statistics exactly where a real event is —
    // which cost the attack of the sustained-vowel fixture entirely.
    //
    // Excluding a guard band solves both: the statistics describe the
    // surroundings on each side without the event contaminating them.
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
