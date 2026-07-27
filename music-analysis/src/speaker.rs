//! The speaker profile: what stays true of a person across their recordings.
//!
//! A voiceprint describes one utterance. This describes the person who produced
//! it — the range their pitch moves in, the corners their vowel space reaches.
//! Those barely change between takes, because they are anatomy and habit rather
//! than a function of what was said.
//!
//! The split earns its keep downstream. Stable per-person facts are what a tuning
//! system and a harmonic lattice get built from, while the utterance decides what
//! happens inside them — the speaker is the world, the utterance is the piece.
//! Keeping them in separate documents also stops a mapping quietly deriving a
//! speaker's range from one short take that never reached it.
//!
//! This is measurement rather than aesthetics, which is why it belongs to the
//! analysis layer: *how high does this person's F2 go* has an answer that can be
//! demonstrated wrong. What to do with that range is the mapping layer's problem.

use serde::{Deserialize, Serialize};

use crate::voiceprint::Voiceprint;

/// Bumped whenever the meaning of a field changes.
///
/// Same contract as [`crate::voiceprint::SCHEMA_VERSION`], and for the same
/// reason: a profile is a cache of a pure function of the voiceprints it was
/// built from, so this number identifies the function, and an algorithm change
/// invalidates a stored profile exactly as thoroughly as a shape change does.
pub const PROFILE_VERSION: u32 = 1;

/// Percentiles taken as the low and high edge of a measured range.
///
/// Deliberately not the minimum and maximum. Formant assignment is per-frame with
/// no continuity tracking, so a handful of frames in any take place a formant
/// somewhere it never actually was — and a true extreme would be defined entirely
/// by those frames. Trimming a twentieth from each end costs nothing real, since
/// a speaker spends far more than 5% of a take near their own corners, and makes
/// the bound reproducible across takes instead of hostage to the worst frame in
/// each.
const LOW_PERCENTILE: f32 = 0.05;
const HIGH_PERCENTILE: f32 = 0.95;

/// Usable frames required before a range is reported at all.
///
/// Two seconds at the 10 ms hop. Below this the percentiles are taken over too
/// few values to be stable between takes, and a profile confidently reporting a
/// speaker's range from half a second of speech is worse than one reporting
/// nothing — the caller can handle an absent range, but cannot detect a wrong one.
const MIN_FRAMES: usize = 200;

/// The extent of a speaker's vowel space, in Hz.
///
/// Build one with [`Self::new`], which refuses a degenerate span so
/// [`Self::normalise`] can divide without a guard.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VowelSpace {
    pub f1_low: f32,
    pub f1_high: f32,
    pub f2_low: f32,
    pub f2_high: f32,
}

impl VowelSpace {
    /// A vowel space with the given bounds, or `None` if either axis has no
    /// extent — a space of zero width is not one anything can be placed in.
    pub fn new(f1_low: f32, f1_high: f32, f2_low: f32, f2_high: f32) -> Option<Self> {
        if f1_high <= f1_low || f2_high <= f2_low {
            return None;
        }
        Some(Self {
            f1_low,
            f1_high,
            f2_low,
            f2_high,
        })
    }

    /// Place one vowel measurement within this speaker's own space.
    ///
    /// `(0, 0)` is the low-F1/low-F2 corner and `(1, 1)` the high-F1/high-F2 one.
    ///
    /// Values outside `0..1` are expected and deliberately not clamped. The edges
    /// are percentiles, so a frame beyond one is a real measurement past the
    /// speaker's usual reach; clamping here would silently discard the loudest
    /// evidence that a speaker exceeded their own habit, and a mapping that wants
    /// to fold such a frame back can do it knowing what it is folding.
    pub fn normalise(&self, f1: f32, f2: f32) -> (f32, f32) {
        (
            (f1 - self.f1_low) / (self.f1_high - self.f1_low),
            (f2 - self.f2_low) / (self.f2_high - self.f2_low),
        )
    }
}

/// The pitch range a speaker actually uses.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct F0Range {
    pub low_hz: f32,
    pub median_hz: f32,
    pub high_hz: f32,
}

/// Everything measured about a speaker rather than about one thing they said.
///
/// Both ranges are optional because either can be unmeasurable in material that
/// is otherwise fine: a whispered take has no f0 at all, and a take can be voiced
/// throughout while the formant fit fails often enough to leave too few frames.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeakerProfile {
    pub profile_version: u32,
    /// How many voiceprints went into this profile.
    pub takes: usize,
    /// Frames carrying both F1 and F2, across all takes.
    pub vowel_frames: usize,
    /// Frames carrying a fundamental, across all takes.
    pub voiced_frames: usize,
    pub vowel_space: Option<VowelSpace>,
    pub f0: Option<F0Range>,
}

/// Measure a speaker from everything they have recorded.
///
/// Takes a slice rather than one voiceprint because the profile improves with
/// material: a speaker reaches the corners of their vowel space over minutes of
/// varied speech, not reliably within any one take. Frames are pooled across
/// takes rather than averaged per take, so a long recording contributes more than
/// a short one — which is the right weighting when what is being estimated is
/// where this person's articulation actually goes.
pub fn profile(voiceprints: &[&Voiceprint]) -> SpeakerProfile {
    // Both formants or neither: a frame that knows only F1 is a point on no
    // plane, and letting it widen the F1 range but not the F2 range would skew
    // the space toward whichever axis the fit happens to recover more often.
    let mut f1: Vec<f32> = Vec::new();
    let mut f2: Vec<f32> = Vec::new();
    for (a, b) in voiceprints.iter().flat_map(|vp| vp.formants.vowel_space()) {
        f1.push(a);
        f2.push(b);
    }

    let mut f0: Vec<f32> = voiceprints
        .iter()
        .flat_map(|vp| vp.pitch.hz.iter().flatten().copied())
        .collect();

    let vowel_frames = f1.len();
    let voiced_frames = f0.len();

    SpeakerProfile {
        profile_version: PROFILE_VERSION,
        takes: voiceprints.len(),
        vowel_frames,
        voiced_frames,
        vowel_space: (vowel_frames >= MIN_FRAMES)
            .then(|| {
                sort(&mut f1);
                sort(&mut f2);
                VowelSpace::new(
                    percentile(&f1, LOW_PERCENTILE),
                    percentile(&f1, HIGH_PERCENTILE),
                    percentile(&f2, LOW_PERCENTILE),
                    percentile(&f2, HIGH_PERCENTILE),
                )
            })
            .flatten(),
        f0: (voiced_frames >= MIN_FRAMES).then(|| {
            sort(&mut f0);
            F0Range {
                low_hz: percentile(&f0, LOW_PERCENTILE),
                median_hz: percentile(&f0, 0.5),
                high_hz: percentile(&f0, HIGH_PERCENTILE),
            }
        }),
    }
}

/// Sort ascending, total order over floats.
///
/// `total_cmp` rather than `partial_cmp().unwrap()`: the inputs are measured
/// frequencies and should never be NaN, but a sort that panics on the day one
/// appears is a worse failure than one that files it at an end.
fn sort(values: &mut [f32]) {
    values.sort_by(f32::total_cmp);
}

/// Linear-interpolated percentile of an ascending slice.
///
/// Interpolating rather than taking the nearest rank keeps the result a
/// continuous function of the input, so adding one frame to a take moves a bound
/// slightly instead of stepping it.
fn percentile(sorted: &[f32], p: f32) -> f32 {
    debug_assert!(!sorted.is_empty(), "percentile of nothing");
    let rank = p * (sorted.len() - 1) as f32;
    let lo = rank.floor() as usize;
    let hi = rank.ceil() as usize;
    sorted[lo] + (sorted[hi] - sorted[lo]) * (rank - lo as f32)
}
