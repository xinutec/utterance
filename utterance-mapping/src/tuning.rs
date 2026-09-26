//! A scale read out of a measured spectrum.
//!
//! Sweep an interval from unison to the octave, ask how rough the spectrum is
//! against a copy of itself at each distance, and take the local minima. For a
//! harmonic spectrum they land near simple ratios — a result, since a bell's
//! spectrum gives a scale unrelated to just intonation. For voices the minima are
//! all near 3:2 and 4:3; what differs is which are deep.
//!
//! Arguable choices: the octave repeats, a minimum is a note when deeper than
//! [`MIN_DEPTH`], and unison and octave are degrees by fiat.

use serde::{Deserialize, Serialize};
use utterance_analysis::partials::Partials;

use crate::dissonance::{self, Component};

/// Steps per octave the curve is sampled at — one per cent, about the finest
/// pitch difference anyone hears.
pub const RESOLUTION: usize = 1200;

/// How deep a dip must be to count as a note, as a fraction of the curve's
/// range. Most minima are two partials sliding past each other; this is the most
/// arguable number in the crate.
pub const MIN_DEPTH: f32 = 0.02;

/// One note of a derived scale.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Degree {
    /// Distance above the tonic in cents. 0 is the tonic, 1200 the octave.
    pub cents: f32,
    /// The same interval as a frequency ratio.
    pub ratio: f32,
    /// Roughness at this minimum, on the curve's normalised 0..1 scale.
    pub dissonance: f32,
    /// How far the curve climbs either side before turning back down: how firmly
    /// this is somewhere a listener could rest.
    pub depth: f32,
}

/// A scale, and the curve it was read from.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tuning {
    /// Ascending by pitch, always opening at 0 cents and closing at 1200.
    pub degrees: Vec<Degree>,
    /// The normalised dissonance curve, one sample per cent, for plotting.
    pub curve: Vec<f32>,
}

/// Derive a scale from a measured harmonic series, or `None` with fewer than two
/// partials — nothing to collide, a flat curve.
pub fn from_partials(partials: &Partials) -> Option<Tuning> {
    from_partials_with(partials, MIN_DEPTH)
}

/// Derive a scale, choosing how deep a dip has to be to count as a note.
pub fn from_partials_with(partials: &Partials, min_depth: f32) -> Option<Tuning> {
    let f0 = partials.f0_hz?;
    let spectrum: Vec<Component> = partials
        .partials
        .iter()
        .map(|p| Component {
            hz: p.ratio * f0,
            amplitude: p.amplitude,
        })
        .collect();
    from_spectrum_with(&spectrum, min_depth)
}

/// Derive a scale from any spectrum — for testing against spectra that are not a
/// voice, where the answer must differ from just intonation.
pub fn from_spectrum(spectrum: &[Component]) -> Option<Tuning> {
    from_spectrum_with(spectrum, MIN_DEPTH)
}

/// Derive a scale from any spectrum at a chosen depth threshold.
pub fn from_spectrum_with(spectrum: &[Component], min_depth: f32) -> Option<Tuning> {
    if spectrum.len() < 2 {
        return None;
    }

    let raw: Vec<f32> = (0..=RESOLUTION)
        .map(|c| dissonance::at_interval(spectrum, cents_to_ratio(c as f32)))
        .collect();

    let peak = raw.iter().copied().fold(0.0f32, f32::max);
    if peak <= 0.0 {
        return None;
    }
    let curve: Vec<f32> = raw.iter().map(|v| v / peak).collect();

    let mut degrees = vec![endpoint(&curve, 0)];
    degrees.extend(interior_minima(&curve, min_depth));
    degrees.push(endpoint(&curve, RESOLUTION));

    Some(Tuning { degrees, curve })
}

/// Unison and octave, which are degrees by decision: for an inharmonic spectrum
/// the octave may be rough, and is kept so every scale repeats at it.
fn endpoint(curve: &[f32], index: usize) -> Degree {
    Degree {
        cents: index as f32,
        ratio: cents_to_ratio(index as f32),
        dissonance: curve[index],
        depth: 0.0,
    }
}

/// Every local minimum deep enough to call a note.
fn interior_minima(curve: &[f32], min_depth: f32) -> Vec<Degree> {
    let mut found = Vec::new();
    for i in 1..curve.len() - 1 {
        // Strict one side, weak the other: a flat valley reports once.
        if !(curve[i] < curve[i - 1] && curve[i] <= curve[i + 1]) {
            continue;
        }
        let depth = prominence(curve, i);
        if depth >= min_depth {
            found.push(Degree {
                cents: i as f32,
                ratio: cents_to_ratio(i as f32),
                dissonance: curve[i],
                depth,
            });
        }
    }
    found
}

/// How far the curve rises either side of a minimum — the smaller climb, so a
/// dip halfway down a slope does not count as isolated.
fn prominence(curve: &[f32], index: usize) -> f32 {
    let mut left = 0.0f32;
    for i in (0..index).rev() {
        left = left.max(curve[i] - curve[index]);
        if curve[i] < curve[index] {
            break;
        }
    }

    let mut right = 0.0f32;
    for (i, _) in curve.iter().enumerate().skip(index + 1) {
        right = right.max(curve[i] - curve[index]);
        if curve[i] < curve[index] {
            break;
        }
    }

    left.min(right)
}

/// Frequency ratio of an interval given in cents.
pub fn cents_to_ratio(cents: f32) -> f32 {
    2f32.powf(cents / 1200.0)
}

/// Interval in cents between two frequency ratios.
pub fn ratio_to_cents(ratio: f32) -> f32 {
    1200.0 * ratio.log2()
}
