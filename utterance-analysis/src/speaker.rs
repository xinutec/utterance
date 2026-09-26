//! The speaker profile: what stays true of a person across their recordings.
//!
//! A voiceprint describes one utterance; this describes the person — the range
//! their pitch moves in, the corners their vowel space reaches, which barely
//! change between takes. The speaker is the world a tuning and a lattice are
//! built from; the utterance is the piece. It is analysis because *how high does
//! this person's F2 go* has an answer that can be shown wrong.

use serde::{Deserialize, Serialize};

use crate::voiceprint::Voiceprint;

/// Identifies the profiling function; bump it for any change to the output, as
/// for [`crate::voiceprint::SCHEMA_VERSION`].
pub const PROFILE_VERSION: u32 = 2;

/// Percentiles taken as the low and high edge of a measured range.
///
/// ⚠ Not the minimum and maximum: per-frame formant assignment puts a few frames
/// of any take somewhere the speaker never was, and a true extreme would be
/// defined by them. A speaker spends far more than 5% of a take near their
/// corners, so trimming costs nothing real.
const LOW_PERCENTILE: f32 = 0.05;
const HIGH_PERCENTILE: f32 = 0.95;

/// Usable frames required before a range is reported at all — two seconds.
/// A caller can handle an absent range, but cannot detect a wrong one.
const MIN_FRAMES: usize = 200;

/// A measured range with somewhere to put a value inside it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Span {
    pub low_hz: f32,
    pub high_hz: f32,
}

impl Span {
    /// A span, or `None` if it has no extent — nothing can be placed in one.
    pub fn new(low_hz: f32, high_hz: f32) -> Option<Self> {
        (high_hz > low_hz).then_some(Self { low_hz, high_hz })
    }

    /// Where a value sits in this span, `0` at the low edge and `1` at the high,
    /// unclamped like [`VowelSpace::normalise`].
    pub fn place(&self, value: f32) -> f32 {
        (value - self.low_hz) / (self.high_hz - self.low_hz)
    }
}

/// The extent of a speaker's vowel space, in Hz. [`Self::new`] refuses a
/// degenerate span, so [`Self::normalise`] can divide without a guard.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VowelSpace {
    pub f1_low: f32,
    pub f1_high: f32,
    pub f2_low: f32,
    pub f2_high: f32,
    /// The third formant's range, when enough frames carried one. F3 separates
    /// mouth shapes the F1/F2 chart cannot — lip rounding, retroflexion — and is
    /// the first formant a noisy frame loses, hence optional.
    pub f3: Option<Span>,
}

impl VowelSpace {
    /// A vowel space with the given bounds, or `None` if either axis has no
    /// extent.
    pub fn new(f1_low: f32, f1_high: f32, f2_low: f32, f2_high: f32) -> Option<Self> {
        if f1_high <= f1_low || f2_high <= f2_low {
            return None;
        }
        Some(Self {
            f1_low,
            f1_high,
            f2_low,
            f2_high,
            f3: None,
        })
    }

    /// The same space with its third dimension measured.
    pub fn with_f3(self, f3: Option<Span>) -> Self {
        Self { f3, ..self }
    }

    /// Where one F3 measurement sits in this speaker's range, or `None` when F3
    /// was never measured well enough to have one.
    pub fn depth(&self, f3: f32) -> Option<f32> {
        self.f3.map(|span| span.place(f3))
    }

    /// Place one vowel measurement within this speaker's own space: `(0, 0)` is
    /// low-F1/low-F2, `(1, 1)` high-F1/high-F2.
    ///
    /// Not clamped: the edges are percentiles, so a value past one is a real
    /// measurement beyond the speaker's habit, and a mapping that folds it back
    /// should know it is doing so.
    pub fn normalise(&self, f1: f32, f2: f32) -> (f32, f32) {
        (
            (f1 - self.f1_low) / (self.f1_high - self.f1_low),
            (f2 - self.f2_low) / (self.f2_high - self.f2_low),
        )
    }
}

/// The spectral range a speaker's voiced tone moves through, in Hz.
///
/// Separate from the vowel: the same *ah* pressed and murmured is one point in
/// the vowel space and two tones. Measured per person, so bright means *bright
/// for them*. [`Self::new`] refuses a degenerate range.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Brightness {
    pub low_hz: f32,
    pub high_hz: f32,
}

impl Brightness {
    /// A brightness range, or `None` if it has no extent or reaches below zero.
    pub fn new(low_hz: f32, high_hz: f32) -> Option<Self> {
        if low_hz <= 0.0 || high_hz <= low_hz {
            return None;
        }
        Some(Self { low_hz, high_hz })
    }

    /// Place one measured centroid within this speaker's range, in log
    /// frequency — the midpoint of 500 and 2000 Hz sounds like 1000, not 1250.
    /// Unclamped, like [`VowelSpace::normalise`].
    pub fn place(&self, centroid_hz: f32) -> f32 {
        // No energy is no tone; the dark end is the only honest place for it,
        // and the level stream says whether anything sounds.
        if centroid_hz <= 0.0 {
            return 0.0;
        }
        (centroid_hz / self.low_hz).log2() / (self.high_hz / self.low_hz).log2()
    }
}

/// Frames a held vowel needs before its centre is reported: one second, half
/// what a range needs, since a centre settles long before the edges do. Below
/// the guided flow's own minimum, so a take it accepts is never silently unused.
const MIN_CORNER_FRAMES: usize = 100;

/// One corner of the vowel quadrilateral: *ee* close front, *ah* open, *oo* close
/// back — the extremes a tongue can reach, and so the ones a person can produce
/// on purpose.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub enum Corner {
    CloseFront,
    Open,
    CloseBack,
}

/// Where one speaker's held vowel actually sat, in Hz.
///
/// The centre is a median: a corner take glides in and out of the shape, and a
/// mean would be dragged toward neutral by those frames. The interquartile
/// spread is reported beside it, because a take held still and one that
/// wandered can share a centre, and a dot cannot tell them apart.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VowelCorner {
    pub f1_hz: f32,
    pub f2_hz: f32,
    /// Interquartile spread of F1 across the take, in Hz.
    pub f1_spread_hz: f32,
    /// Interquartile spread of F2 across the take, in Hz.
    pub f2_spread_hz: f32,
    /// Frames the centre was measured over.
    pub frames: usize,
}

/// Where one take's vowel sits, for a take that is one held vowel — or `None`
/// with too few frames to trust.
///
/// Nothing checks that the take really is one held vowel: a wide spread is
/// something true about the person, not a reason to refuse. The vowel's identity
/// comes from the step the take was recorded for.
pub fn corner(voiceprint: &Voiceprint) -> Option<VowelCorner> {
    let pairs = voiceprint.formants.vowel_space();
    if pairs.len() < MIN_CORNER_FRAMES {
        return None;
    }

    let mut f1: Vec<f32> = pairs.iter().map(|(a, _)| *a).collect();
    let mut f2: Vec<f32> = pairs.iter().map(|(_, b)| *b).collect();
    sort(&mut f1);
    sort(&mut f2);

    Some(VowelCorner {
        f1_hz: percentile(&f1, 0.5),
        f2_hz: percentile(&f2, 0.5),
        f1_spread_hz: percentile(&f1, 0.75) - percentile(&f1, 0.25),
        f2_spread_hz: percentile(&f2, 0.75) - percentile(&f2, 0.25),
        frames: pairs.len(),
    })
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
/// Every range is optional: a whispered take has no f0, and the formant fit can
/// fail often enough to leave too few frames.
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
    /// Where this speaker's voiced tone sits on the bright-to-dark axis. Voiced
    /// frames only: consonants would stretch the top to where no note reaches.
    pub brightness: Option<Brightness>,
}

/// Measure a speaker from the takes given. Frames are pooled across takes, so a
/// long recording counts for more — the right weighting for where this person's
/// articulation goes.
pub fn profile(voiceprints: &[&Voiceprint]) -> SpeakerProfile {
    // Both formants or neither: a frame knowing only F1 would skew the space
    // toward whichever axis the fit recovers more often.
    let mut f1: Vec<f32> = Vec::new();
    let mut f2: Vec<f32> = Vec::new();
    for (a, b) in voiceprints.iter().flat_map(|vp| vp.formants.vowel_space()) {
        f1.push(a);
        f2.push(b);
    }

    // F3 pooled on its own: requiring all three would throw away most of it.
    let mut f3: Vec<f32> = voiceprints
        .iter()
        .flat_map(|vp| vp.formants.f3.iter().flatten().copied())
        .collect();

    let mut f0: Vec<f32> = voiceprints
        .iter()
        .flat_map(|vp| vp.pitch.hz.iter().flatten().copied())
        .collect();

    // Voiced frames only, so a consonant never widens the tone's range.
    let mut centroid: Vec<f32> = voiceprints
        .iter()
        .flat_map(|vp| {
            vp.pitch
                .hz
                .iter()
                .zip(&vp.texture.centroid_hz)
                .filter(|(hz, c)| hz.is_some() && **c > 0.0)
                .map(|(_, c)| *c)
        })
        .collect();

    let vowel_frames = f1.len();
    let voiced_frames = f0.len();
    let bright_frames = centroid.len();

    SpeakerProfile {
        profile_version: PROFILE_VERSION,
        takes: voiceprints.len(),
        vowel_frames,
        voiced_frames,
        vowel_space: (vowel_frames >= MIN_FRAMES)
            .then(|| {
                sort(&mut f1);
                sort(&mut f2);
                let depth = (f3.len() >= MIN_FRAMES)
                    .then(|| {
                        sort(&mut f3);
                        Span::new(
                            percentile(&f3, LOW_PERCENTILE),
                            percentile(&f3, HIGH_PERCENTILE),
                        )
                    })
                    .flatten();
                VowelSpace::new(
                    percentile(&f1, LOW_PERCENTILE),
                    percentile(&f1, HIGH_PERCENTILE),
                    percentile(&f2, LOW_PERCENTILE),
                    percentile(&f2, HIGH_PERCENTILE),
                )
                .map(|space| space.with_f3(depth))
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
        brightness: (bright_frames >= MIN_FRAMES)
            .then(|| {
                sort(&mut centroid);
                Brightness::new(
                    percentile(&centroid, LOW_PERCENTILE),
                    percentile(&centroid, HIGH_PERCENTILE),
                )
            })
            .flatten(),
    }
}

/// Sort ascending with a total order, so a NaN is filed at an end, not a panic.
fn sort(values: &mut [f32]) {
    values.sort_by(f32::total_cmp);
}

/// Linear-interpolated percentile of an ascending slice, so one more frame moves
/// a bound slightly instead of stepping it.
fn percentile(sorted: &[f32], p: f32) -> f32 {
    debug_assert!(!sorted.is_empty(), "percentile of nothing");
    let rank = p * (sorted.len() - 1) as f32;
    let lo = rank.floor() as usize;
    let hi = rank.ceil() as usize;
    sorted[lo] + (sorted[hi] - sorted[lo]) * (rank - lo as f32)
}
