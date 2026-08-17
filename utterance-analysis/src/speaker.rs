//! The speaker profile: what stays true of a person across their recordings.
//!
//! A voiceprint describes one utterance; this describes the person who produced it — the
//! range their pitch moves in, the corners their vowel space reaches. Those barely change
//! between takes, being anatomy and habit rather than a function of what was said.
//!
//! The split earns its keep downstream: stable per-person facts are what a tuning system
//! and a harmonic lattice are built from, while the utterance decides what happens inside
//! them. The speaker is the world, the utterance is the piece — and separate documents
//! stop a mapping quietly deriving a speaker's range from one short take that never
//! reached it.
//!
//! Measurement rather than aesthetics, which is why it is analysis: *how high does this
//! person's F2 go* has an answer that can be shown wrong. What to do with that range is
//! the mapping layer's problem.

use serde::{Deserialize, Serialize};

use crate::voiceprint::Voiceprint;

/// Bumped whenever the meaning of a field changes.
///
/// Same contract as [`crate::voiceprint::SCHEMA_VERSION`]: a profile is a cache of a pure
/// function of the voiceprints it was built from, so this number identifies the function,
/// and an algorithm change invalidates a stored profile as thoroughly as a shape change.
/// - 2: added `brightness` (the spectral range this speaker's voiced tone
///   moves through).
pub const PROFILE_VERSION: u32 = 2;

/// Percentiles taken as the low and high edge of a measured range.
///
/// ⚠ Deliberately not the minimum and maximum. Formant assignment is per-frame with no
/// continuity tracking, so a handful of frames in any take place a formant somewhere it
/// never was — and a true extreme would be defined entirely by those frames. Trimming a
/// twentieth from each end costs nothing real, a speaker spending far more than 5% of a
/// take near their own corners, and makes the bound reproducible across takes.
const LOW_PERCENTILE: f32 = 0.05;
const HIGH_PERCENTILE: f32 = 0.95;

/// Usable frames required before a range is reported at all.
///
/// Two seconds at the 10 ms hop. Below this the percentiles are taken over too
/// few values to be stable between takes, and a profile confidently reporting a
/// speaker's range from half a second of speech is worse than one reporting
/// nothing — the caller can handle an absent range, but cannot detect a wrong one.
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

    /// Where a value sits in this span, `0` at the low edge and `1` at the high.
    ///
    /// Unclamped, like [`VowelSpace::normalise`] and for the same reason.
    pub fn place(&self, value: f32) -> f32 {
        (value - self.low_hz) / (self.high_hz - self.low_hz)
    }
}

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
    /// The third formant's range, when enough frames carried one.
    ///
    /// The same space's third dimension rather than a separate measurement. F1
    /// and F2 place a vowel on the chart everyone draws; F3 is what distinguishes
    /// mouth shapes that chart cannot tell apart — lip rounding and tongue
    /// retroflexion, which move it and leave the other two where they were.
    ///
    /// Optional because the formant fit recovers F3 far less reliably than the
    /// two below it: it is the highest pole and the first to be lost to a noisy
    /// frame.
    pub f3: Option<Span>,
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
            f3: None,
        })
    }

    /// The same space with its third dimension measured.
    pub fn with_f3(self, f3: Option<Span>) -> Self {
        Self { f3, ..self }
    }

    /// Where one F3 measurement sits in this speaker's third-formant range.
    ///
    /// `None` when F3 was never measured well enough to have a range, which is
    /// the state a caller has to handle rather than paper over: there is no
    /// sensible stand-in for a dimension nobody measured.
    pub fn depth(&self, f3: f32) -> Option<f32> {
        self.f3.map(|span| span.place(f3))
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

/// The spectral range a speaker's voiced tone moves through, in Hz.
///
/// Brightness is a dimension of a voice quite separate from which vowel is being
/// said: the same *ah* pressed hard and murmured are the same point in the vowel
/// space and nowhere near each other in tone. Measuring it per person, like the
/// vowel space next door, is what makes a bright frame mean *bright for them*.
///
/// Build with [`Self::new`], which refuses a degenerate range so [`Self::place`]
/// can divide without a guard.
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

    /// Place one measured centroid within this speaker's range.
    ///
    /// Interpolated in log frequency, because that is how brightness is heard:
    /// the midpoint between 500 Hz and 2000 Hz sounds like 1000 Hz, not like the
    /// arithmetic 1250. A linear axis would spend most of its length on the
    /// bright end, where a listener hears the least difference.
    ///
    /// Outside `0..1` is expected and not clamped, for the same reason
    /// [`VowelSpace::normalise`] does not clamp: the edges are percentiles, so a
    /// frame past one is real evidence rather than an error.
    pub fn place(&self, centroid_hz: f32) -> f32 {
        // No energy is not a dark tone, it is no tone — but the darkest end of
        // the axis is the only honest place to put it, and the caller's own
        // level stream is what says whether anything sounds there at all.
        if centroid_hz <= 0.0 {
            return 0.0;
        }
        (centroid_hz / self.low_hz).log2() / (self.high_hz / self.low_hz).log2()
    }
}

/// Frames a held vowel needs before its centre is reported.
///
/// One second at the 10 ms hop, half what a *range* needs. A range is only as
/// good as its tails, so it wants enough frames for the speaker to have reached
/// their own extremes; a corner is one shape held still, and its centre is
/// stable long before its edges are. The guided flow asks for two or three
/// seconds and accepts one and a half, so this bar sits below what it accepts —
/// a take the person was told was usable must not then be silently unused.
const MIN_CORNER_FRAMES: usize = 100;

/// One corner of the vowel quadrilateral, as a held vowel rather than as a name.
///
/// The three the guided calibration asks for, chosen because they are the
/// extremes a tongue can reach and therefore the ones a person can produce
/// deliberately: *ee* at the close front, *ah* open, *oo* at the close back.
/// Which sound realises a corner is a fact about a language; where a corner
/// *is* is a fact about a mouth, and only the second is measured here.
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
/// The centre is a median rather than a mean: a corner take begins and ends by
/// gliding in and out of the shape, and those frames are real measurements of
/// something that is not the vowel. A mean is moved by them in proportion to how
/// far off they are, which is exactly backwards.
///
/// **The spread is reported because a single point would claim more than was
/// measured.** Two takes can share a centre while one held still and the other
/// wandered through half the vowel space, and a dot on a chart cannot tell them
/// apart. Quartiles rather than the full extent, for the reason the percentiles
/// above are trimmed: the glide frames are at the ends.
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

/// Where one take's vowel sits, for a take that is one held vowel.
///
/// `None` when too few frames carried both formants — an absent corner is
/// something a caller can show as "not recorded yet"; a corner measured from
/// twenty frames is a number nobody can tell is wrong.
///
/// **Nothing here checks that the take really is one held vowel.** It cannot:
/// the only evidence would be the spread, and refusing a wide one would throw
/// away the case worth seeing — a person whose *ee* wanders is being told
/// something true about their *ee*. The identity of the vowel comes from the
/// step the take was recorded for, which is why this takes a voiceprint and not
/// a name.
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
    /// Where this speaker's voiced tone sits on the bright-to-dark axis.
    ///
    /// Measured over voiced frames only. Unvoiced frames are consonants, which
    /// are far brighter than any tone and would stretch the top of the range to
    /// somewhere no sustained note ever reaches — leaving every vowel crowded
    /// into the bottom of an axis mostly describing sibilance.
    pub brightness: Option<Brightness>,
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

    // F3 pooled on its own rather than gated on the two below it. It is the
    // first formant lost to a noisy frame, so requiring all three would throw
    // away most of the evidence there is for the range it moves in.
    let mut f3: Vec<f32> = voiceprints
        .iter()
        .flat_map(|vp| vp.formants.f3.iter().flatten().copied())
        .collect();

    let mut f0: Vec<f32> = voiceprints
        .iter()
        .flat_map(|vp| vp.pitch.hz.iter().flatten().copied())
        .collect();

    // Brightness of the voiced material only, take by take, so an unvoiced frame
    // never contributes its centroid to the range a tone is placed in.
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
