//! Formant tracking: where the vocal tract resonates, frame by frame.
//!
//! Formants are what makes one vowel a different vowel from another, and they
//! are close to independent of pitch — the same person saying the same vowel high
//! or low moves f0 a long way and F1/F2 barely at all. That independence is why
//! this measurement is worth having: it is a description of what the speaker is
//! *doing* with their mouth, separable from the note they are on.
//!
//! For this project specifically, F1 against F2 is a two-dimensional space in
//! which every vowel of a language occupies a region, and vowel sequences are
//! trajectories through it — the raw geometry the harmony mapping is meant to be
//! built on (see `docs/architecture.md`).

use crate::frame::{self, SPECTRAL_WINDOW};
use crate::lpc;
use crate::resample::ANALYSIS_RATE;

/// Lowest frequency accepted as a formant. Below any F1, and above the region
/// where the residual spectral tilt puts spurious poles.
const F_MIN_HZ: f32 = 90.0;

/// Highest frequency accepted as a formant. Above any F3, comfortably below the
/// 8 kHz Nyquist where poles are unreliable.
const F_MAX_HZ: f32 = 5_000.0;

/// Widest pole accepted as a formant.
///
/// A vocal-tract resonance is narrow — a few tens of hertz to a couple of
/// hundred. A very wide pole is the fit describing the general shape of the
/// spectrum rather than a resonance in it, and admitting those is how spurious
/// formants appear in silence and in fricatives.
const BANDWIDTH_MAX_HZ: f32 = 400.0;

/// The first three formants of one frame.
///
/// `None` where the frame gives no usable estimate — unvoiced, silent, or the
/// fit simply produced nothing in range. Never a sentinel: a frame with no
/// second formant must not average into a mapping as 0 Hz.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FormantFrame {
    pub f1: Option<f32>,
    pub f2: Option<f32>,
    pub f3: Option<f32>,
}

/// A single resonance recovered from the fit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Resonance {
    pub frequency_hz: f32,
    pub bandwidth_hz: f32,
}

/// Track formants across every frame.
///
/// `voiced` gates the estimate: linear prediction assumes a source driving a
/// filter, and in an unvoiced frame there is no periodic source, so whatever
/// poles come back describe noise. Reporting them would be inventing vowels in
/// the gaps between them.
pub fn track(samples: &[f32], voiced: &[bool]) -> Vec<FormantFrame> {
    (0..frame::count(samples.len()))
        .map(|i| {
            if !voiced.get(i).copied().unwrap_or(false) {
                return FormantFrame::default();
            }
            let window = frame::windowed(samples, i, SPECTRAL_WINDOW);
            estimate(&window)
        })
        .collect()
}

/// Plausible range for each formant, in Hz, across adult speakers.
///
/// Wide enough to cover any speaker and any vowel, narrow enough to be
/// informative. These are anatomy: F1 is set mostly by how open the jaw is and
/// F2 by where the tongue sits, and neither can reach far outside these bounds
/// on a human vocal tract.
const RANGES: [(f32, f32); 3] = [(200.0, 1_100.0), (600.0, 3_000.0), (1_500.0, 4_000.0)];

/// Formants of a single windowed frame.
pub fn estimate(window: &[f32]) -> FormantFrame {
    assign(&resonances(window))
}

/// Fit resonances to formant slots, lowest first, respecting each slot's range.
///
/// Taking the three lowest resonances in order is the obvious rule and it fails
/// in a specific, visible way: when a genuine formant is missed for a frame —
/// merged with its neighbour, or too damped to survive the bandwidth filter —
/// every formant above it shifts down a slot, and F2 is reported at a frequency
/// no F2 can occupy. Measured on a real glided vowel, that put F2 at 3.4 kHz in
/// a fifth of frames.
///
/// Requiring each slot's candidate to lie in that formant's anatomical range
/// turns those into `None`. Reporting nothing where the fit failed is worth more
/// than reporting a number known to be impossible, because a mapping downstream
/// can skip a gap but cannot detect a plausible-looking lie.
fn assign(resonances: &[Resonance]) -> FormantFrame {
    let mut slots: [Option<f32>; 3] = [None; 3];
    let mut next = 0;

    for resonance in resonances {
        // Advance past slots this resonance is already too high for, so a missing
        // F1 does not consume the F2 slot with an F2-range value.
        while next < RANGES.len() && resonance.frequency_hz > RANGES[next].1 {
            next += 1;
        }
        if next >= RANGES.len() {
            break;
        }
        if resonance.frequency_hz >= RANGES[next].0 {
            slots[next] = Some(resonance.frequency_hz);
            next += 1;
        }
    }
    FormantFrame {
        f1: slots[0],
        f2: slots[1],
        f3: slots[2],
    }
}

/// Every resonance in the frame, in increasing frequency order.
pub fn resonances(window: &[f32]) -> Vec<Resonance> {
    let emphasised = lpc::pre_emphasise(window);
    let windowed: Vec<f32> = emphasised
        .iter()
        .zip(frame::hamming(emphasised.len()))
        .map(|(s, w)| s * w)
        .collect();

    let Some(coefficients) = lpc::coefficients(&windowed, lpc::ORDER) else {
        return Vec::new();
    };

    let rate = f64::from(ANALYSIS_RATE);
    let mut found: Vec<Resonance> = lpc::roots(&coefficients)
        .into_iter()
        // One of each conjugate pair. A pole on the real axis is not a
        // resonance — it is the fit describing spectral slope.
        .filter(|z| z.im > 0.0)
        // Outside the unit circle means an unstable fit, numerically rather than
        // physically; those poles describe nothing real.
        .filter(|z| z.norm() < 1.0)
        .map(|z| Resonance {
            frequency_hz: (z.arg() * rate / (2.0 * std::f64::consts::PI)) as f32,
            bandwidth_hz: (-z.norm().ln() * rate / std::f64::consts::PI) as f32,
        })
        .filter(|r| (F_MIN_HZ..=F_MAX_HZ).contains(&r.frequency_hz))
        .filter(|r| r.bandwidth_hz <= BANDWIDTH_MAX_HZ)
        .collect();

    found.sort_by(|a, b| a.frequency_hz.total_cmp(&b.frequency_hz));
    found
}
