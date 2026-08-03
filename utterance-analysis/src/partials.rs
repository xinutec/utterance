//! The measured harmonic series of a voice.
//!
//! A voiced sound is a periodic glottal source filtered by the vocal tract. The
//! source puts energy at every integer multiple of f0; the tract's resonances
//! then decide which of those multiples come out loud and which are all but
//! absent. This measures the result: for each harmonic, where it actually sat
//! and how strong it was.
//!
//! **The amplitudes are the payload, not the frequencies.** A voice is very
//! nearly harmonic — unlike a bell or a struck string, its partials really do
//! land on integer multiples — so the ratios measured here should come out close
//! to whole numbers, and a large deviation is far more likely to be measurement
//! error than a discovery. What differs between people is the *profile*: which
//! partials their vocal tract emphasises. That profile is what a tuning system
//! can be derived from, because consonance between two tones depends on which of
//! their partials collide.
//!
//! **A harmonic series belongs to a vowel, not only to a speaker.** The tract
//! shape that emphasises partials 2 and 3 in one vowel emphasises 6 and 7 in
//! another. That is why calibration asks for a *steady* vowel: a glide measures
//! the average of several mouths.
//!
//! Only frames worth trusting are used — voiced, and close to the take's own
//! median pitch — so a take that is not sustained phonation yields few frames
//! and says so through [`Partials::frames_used`] rather than returning a
//! confident answer built from nothing.

use rustfft::FftPlanner;
use rustfft::num_complex::Complex32;
use serde::{Deserialize, Serialize};

use crate::frame;
use crate::resample::ANALYSIS_RATE;

/// Highest harmonic looked for.
///
/// At a typical male f0 the 24th harmonic is near 3 kHz, comfortably inside the
/// 8 kHz the analysis rate can represent, and past the point where a partial
/// still contributes usefully to whether two tones beat against each other.
/// Going higher mostly collects noise that the presence gate then discards.
pub const MAX_PARTIAL: usize = 24;

/// Window for the harmonic measurement, 128 ms.
///
/// Much longer than the spectral window onset detection uses, and for the
/// opposite reason: this wants frequency resolution, not time resolution. At
/// 2048 samples the bins are 7.8 Hz apart, so neighbouring harmonics of even a
/// low voice sit several bins apart and can be told from each other. A sustained
/// vowel is stationary, so the long window costs nothing.
pub const PARTIAL_WINDOW: usize = 2048;

/// How far a frame's pitch may sit from the take's median and still be used,
/// in semitones.
///
/// The measurement assumes every frame is describing the same note. A frame a
/// tone away is describing a different one, and averaging it in smears every
/// partial by that interval — multiplied by the harmonic number, so the top of
/// the series smears worst.
const PITCH_TOLERANCE_SEMITONES: f32 = 1.0;

/// Fraction of the search band, either side of a harmonic's predicted position,
/// that is searched for its peak.
///
/// A third of the spacing to the next harmonic. Wider would let harmonic *k*
/// lock onto its neighbour when f0 is slightly misestimated, which produces a
/// beautifully clean and completely wrong series.
const SEARCH_FRACTION: f32 = 0.33;

/// Amplitude below the frame's strongest partial at which a peak stops counting
/// as found, in dB.
///
/// Sixty decibels down is a millionth of the power of the loudest partial, which
/// in a real recording is the noise floor rather than the voice.
const FLOOR_DB: f32 = -60.0;

/// Fraction of usable frames a harmonic must appear in to be reported.
///
/// A partial found in a fifth of frames has a median amplitude computed from
/// almost nothing, and reporting it alongside genuinely measured ones would
/// present the two as equally solid. Those below the bar are dropped rather
/// than reported weakly, and the ones above carry their own
/// [`Partial::presence`] so a consumer can still weight them.
const MIN_PRESENCE: f32 = 0.5;

/// One harmonic of the measured series.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Partial {
    /// Which harmonic this is. 1 is the fundamental.
    pub number: u32,
    /// Measured frequency over measured f0, median across frames.
    ///
    /// Should sit close to `number`. How close is bounded by the pitch
    /// tracker's own accuracy, so this measures agreement between two
    /// estimates rather than proving the voice harmonic.
    pub ratio: f32,
    /// Median amplitude, relative to the strongest partial in the take.
    pub amplitude: f32,
    /// Fraction of usable frames this harmonic was found in.
    pub presence: f32,
}

/// The harmonic series of one recording.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Partials {
    /// Frames that were voiced and close enough to the median pitch to use.
    ///
    /// The honest measure of how much this series is worth. Sustained phonation
    /// yields hundreds; connected speech yields few, because its pitch moves.
    pub frames_used: usize,
    /// Median f0 across those frames, the reference every ratio is against.
    pub f0_hz: Option<f32>,
    /// Harmonics found often enough to report, ascending by number.
    pub partials: Vec<Partial>,
}

/// Measure the harmonic series of `samples`, guided by an existing pitch track.
///
/// The pitch track comes from the caller rather than being re-derived here so
/// that one recording has exactly one f0 answer, and the ratios below are
/// against the same fundamental everything else in the voiceprint is.
pub fn measure(samples: &[f32], pitch: &[Option<f32>]) -> Partials {
    let Some(median_f0) = median(&pitch.iter().flatten().copied().collect::<Vec<_>>()) else {
        return Partials {
            frames_used: 0,
            f0_hz: None,
            partials: Vec::new(),
        };
    };

    let tolerance = 2f32.powf(PITCH_TOLERANCE_SEMITONES / 12.0);
    let usable: Vec<(usize, f32)> = pitch
        .iter()
        .enumerate()
        .filter_map(|(i, hz)| Some((i, (*hz)?)))
        .filter(|(_, hz)| *hz < median_f0 * tolerance && *hz > median_f0 / tolerance)
        .collect();

    if usable.is_empty() {
        return Partials {
            frames_used: 0,
            f0_hz: Some(median_f0),
            partials: Vec::new(),
        };
    }

    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(PARTIAL_WINDOW);
    let window = blackman(PARTIAL_WINDOW);
    let bin_hz = ANALYSIS_RATE as f32 / PARTIAL_WINDOW as f32;

    // Per harmonic, every frame's observation of it.
    let mut ratios: Vec<Vec<f32>> = vec![Vec::new(); MAX_PARTIAL + 1];
    let mut amplitudes: Vec<Vec<f32>> = vec![Vec::new(); MAX_PARTIAL + 1];

    let mut buf = vec![Complex32::new(0.0, 0.0); PARTIAL_WINDOW];
    for &(index, f0) in &usable {
        let block = frame::windowed(samples, index, PARTIAL_WINDOW);
        for (b, (s, w)) in buf.iter_mut().zip(block.iter().zip(&window)) {
            *b = Complex32::new(s * w, 0.0);
        }
        fft.process(&mut buf);

        // Real input, so the upper half mirrors the lower and carries nothing.
        let magnitude: Vec<f32> = buf[..PARTIAL_WINDOW / 2].iter().map(|c| c.norm()).collect();

        let observed = harmonics(&magnitude, f0, bin_hz);
        let Some(loudest) = observed
            .iter()
            .flatten()
            .map(|(_, a)| *a)
            .max_by(f32::total_cmp)
        else {
            continue;
        };
        let floor = loudest * 10f32.powf(FLOOR_DB / 20.0);

        for (k, found) in observed.iter().enumerate() {
            if let Some((hz, amplitude)) = *found
                && amplitude >= floor
            {
                ratios[k].push(hz / f0);
                amplitudes[k].push(amplitude);
            }
        }
    }

    // Normalised against the loudest harmonic overall rather than per frame, so
    // the reported profile is one spectrum's shape and not an average of shapes
    // each scaled by however loud that instant happened to be.
    let peak = amplitudes
        .iter()
        .filter_map(|a| median(a))
        .max_by(f32::total_cmp)
        .unwrap_or(1.0);

    let partials = (1..=MAX_PARTIAL)
        .filter_map(|k| {
            let presence = amplitudes[k].len() as f32 / usable.len() as f32;
            if presence < MIN_PRESENCE {
                return None;
            }
            Some(Partial {
                number: k as u32,
                ratio: median(&ratios[k])?,
                amplitude: median(&amplitudes[k])? / peak,
                presence,
            })
        })
        .collect();

    Partials {
        frames_used: usable.len(),
        f0_hz: Some(median_f0),
        partials,
    }
}

/// Peak frequency and amplitude for each harmonic of `f0`, index by harmonic
/// number. Index 0 is always `None` so `k` indexes harmonic `k`.
fn harmonics(magnitude: &[f32], f0: f32, bin_hz: f32) -> Vec<Option<(f32, f32)>> {
    let mut found = vec![None; MAX_PARTIAL + 1];
    let half_band = (f0 * SEARCH_FRACTION / bin_hz).max(1.0);

    for (k, slot) in found.iter_mut().enumerate().skip(1) {
        let centre = k as f32 * f0 / bin_hz;
        let lo = (centre - half_band).floor().max(1.0) as usize;
        let hi = ((centre + half_band).ceil() as usize).min(magnitude.len() - 2);
        if lo > hi {
            break;
        }

        let peak = (lo..=hi).fold(lo, |best, i| {
            if magnitude[i] > magnitude[best] {
                i
            } else {
                best
            }
        });
        // A peak pinned to the edge of its band is the shoulder of something
        // else, not this harmonic.
        if peak == lo || peak == hi {
            continue;
        }
        *slot = Some(interpolate(magnitude, peak, bin_hz));
    }
    found
}

/// Refine a magnitude peak by fitting a parabola through it and its neighbours.
///
/// The true peak almost never lands on a bin centre. Without this, a measured
/// ratio is quantised to the bin spacing, which at the fundamental is several
/// percent — enough to swamp the deviation from integer that is being looked at.
fn interpolate(magnitude: &[f32], peak: usize, bin_hz: f32) -> (f32, f32) {
    let (a, b, c) = (magnitude[peak - 1], magnitude[peak], magnitude[peak + 1]);
    let denominator = a - 2.0 * b + c;
    // Flat or perfectly symmetric: the bin centre is already the best answer.
    let offset = if denominator.abs() < f32::EPSILON {
        0.0
    } else {
        0.5 * (a - c) / denominator
    };
    let amplitude = b - 0.25 * (a - c) * offset;
    ((peak as f32 + offset) * bin_hz, amplitude)
}

/// Blackman window of length `n`.
///
/// Chosen over the Hamming used for linear prediction because its sidelobes fall
/// away far faster. Here the quantity of interest is one partial's amplitude
/// beside another's, and a strong harmonic leaking into its neighbour's bins
/// would be read as that neighbour being louder than it is.
fn blackman(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let x = 2.0 * std::f32::consts::PI * i as f32 / n as f32;
            0.42 - 0.5 * x.cos() + 0.08 * (2.0 * x).cos()
        })
        .collect()
}

/// Median of an unsorted slice, or `None` when there is nothing to take.
fn median(values: &[f32]) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f32::total_cmp);
    Some(sorted[sorted.len() / 2])
}
