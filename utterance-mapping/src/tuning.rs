//! A scale read out of a measured spectrum.
//!
//! Sweep one interval upward from unison to the octave, ask at every point how
//! rough the spectrum sounds against a copy of itself at that distance, and take
//! the places where the answer is locally lowest. For a harmonic spectrum those
//! places land close to the simple frequency ratios — which is a result rather
//! than an assumption, and the reason this is worth doing at all: the same
//! procedure applied to a bell or a gamelan metallophone produces a scale that
//! has nothing to do with just intonation, because those spectra are not
//! harmonic.
//!
//! A voice *is* harmonic, so the interesting variation is not in where the
//! minima roughly are — they will be near 3:2 and 4:3 for everyone — but in
//! which ones are deep and which barely dent the curve, because that follows
//! from which partials the speaker's vocal tract makes loud.
//!
//! **The choices made here, all of them arguable:**
//! - the octave is the repeat interval, so the sweep stops at 2:1
//! - a minimum counts as a note when it is deep enough, by [`MIN_DEPTH`]
//! - unison and octave are degrees by fiat rather than by measurement
//!
//! A different mapping would answer these differently and would not be wrong.

use serde::{Deserialize, Serialize};
use utterance_analysis::partials::Partials;

use crate::dissonance::{self, Component};

/// Steps per octave the curve is sampled at — one per cent.
///
/// A cent is roughly the finest pitch difference a trained listener can hear, so
/// sampling finer would locate minima to a precision no one could act on.
pub const RESOLUTION: usize = 1200;

/// How deep a dip must be to count as a note, as a fraction of the curve's own
/// range.
///
/// Every wobble in a dissonance curve is a local minimum, and most are the
/// arithmetic of two partials sliding past each other rather than anywhere a
/// listener would rest. This is the most arguable number in the crate: raise it
/// and you get a pentatonic-ish handful of very stable intervals, lower it and
/// you get a dense microtonal set.
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
    /// How far the curve climbs either side before turning back down.
    ///
    /// The measure of how firmly a note is a note. A degree with a depth of 0.5
    /// is somewhere a listener could rest; one at 0.02 is a technicality.
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

/// Derive a scale from a measured harmonic series.
///
/// Returns `None` when there is not enough spectrum to say anything: a single
/// partial has nothing to collide with, so its curve is flat and every point on
/// it is equally consonant, which is true and useless.
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

/// Derive a scale from any spectrum, harmonic or not.
///
/// Separate from [`from_partials`] because the interesting test of this code is
/// a spectrum that is *not* a voice — a stretched or inharmonic one, where the
/// answer must come out somewhere other than just intonation or the procedure is
/// only rediscovering its own assumptions.
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

/// Unison and octave, which are degrees by decision rather than by measurement.
///
/// Both really are minima for a harmonic spectrum, so including them changes
/// nothing there. For an inharmonic one the octave may genuinely be rough, and
/// this asserts it as a degree anyway — a choice, made so that every scale
/// repeats at the octave and can be handled uniformly downstream.
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
        // Strict on one side and weak on the other, so a flat-bottomed valley
        // reports its last sample once rather than every sample in it.
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

/// How far the curve rises either side of a minimum before turning back down.
///
/// The smaller of the two climbs, which is what makes this a measure of how
/// isolated the dip is rather than of how far the curve happens to travel on one
/// side. A dip halfway down a long slope has a large climb one way and almost
/// none the other, and is not a place anything rests.
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
