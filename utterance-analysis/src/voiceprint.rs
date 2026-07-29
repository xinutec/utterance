//! The voiceprint: everything analysis knows about a recording.
//!
//! This is the interface between analysis and every layer downstream, and the
//! one artefact whose shape needs to stay stable. It is a plain serialisable
//! document on purpose — it can be diffed, committed as a fixture, and plotted
//! in the browser without anyone running the analyser.
//!
//! It holds no musical opinions. There are no notes here, no scale, no key, no
//! beat. Those are decisions, and decisions belong to the mapping layer.

use serde::{Deserialize, Serialize};

/// Bumped whenever the meaning of a field changes, so a stored voiceprint is
/// never silently reinterpreted under a newer analyser.
///
/// **Bump this for any change that alters the output — the algorithm as much as
/// the shape.** A stored voiceprint is a cache of a pure function of the audio,
/// and this number identifies the function. Changing how onsets are picked
/// invalidates every stored voiceprint exactly as thoroughly as adding a field
/// does; the difference is that the shape change fails loudly on deserialise
/// while the algorithm change is silent, so only this makes it visible.
///
/// Caught the hard way: an onset-detector rewrite left every stored take
/// reporting its old counts, because the shape still parsed.
///
/// - 2: `Source` gained `peak` and `clippedFraction`.
/// - 3: onset detection reworked — peak dominance, CFAR threshold, silence gate.
/// - 4: added `formants` (F1/F2/F3 by linear prediction).
/// - 5: added `partials` (the measured harmonic series).
/// - 6: added `texture` (spectral centroid and flatness per frame).
/// - 7: `texture` measured above 300 Hz — below it, room rumble dominated
///   both series and every consonant read as tonal.
/// - 8: `texture` gained `tiltDbPerOctave` — the slope of the spectrum, which
///   the centroid alone cannot express.
pub const SCHEMA_VERSION: u32 = 8;

/// What the recording was before analysis normalised it.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub duration_s: f32,
    /// Highest absolute sample in the source, 0..1.
    pub peak: f32,
    /// Fraction of source samples pinned at full scale.
    ///
    /// Measured on the decoded samples *before* resampling: a band-limited
    /// resampler rounds off the flat tops that clipping produces, so a
    /// conversion first would hide the very thing this measures.
    pub clipped_fraction: f32,
}

/// Fraction of pinned samples above which a recording is called clipped.
///
/// A single sample touching full scale is a peak that happened to land there and
/// says nothing. A tenth of a percent of the take pinned is flat-topping, which
/// no microphone produces and only a too-hot input does.
pub const CLIPPING_FRACTION: f32 = 0.001;

impl Source {
    /// Whether the recording was driven into the rails.
    ///
    /// Worth acting on rather than noting: clipping is harmonic distortion, and
    /// the measured amplitudes of a speaker's partials are what the tuning
    /// mapping is meant to be derived from. A clipped take corrupts precisely
    /// the measurement this project exists to make.
    pub fn is_clipped(&self) -> bool {
        self.clipped_fraction > CLIPPING_FRACTION
    }
}

/// The frame grid every per-frame series below is indexed by.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct FrameGrid {
    /// Rate the analysis ran at, after resampling.
    pub analysis_rate_hz: u32,
    /// Seconds between consecutive frames.
    pub hop_s: f32,
    /// Length of every per-frame series in this document.
    pub count: usize,
}

/// Prosodic contour, one entry per frame.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Pitch {
    /// Fundamental per frame; `null` where the frame is unvoiced.
    ///
    /// Nullable rather than zero-filled so a consumer cannot average an unvoiced
    /// frame into a phrase's mean pitch without noticing.
    pub hz: Vec<Option<f32>>,
    /// YIN's normalised difference at the chosen lag. Present for every frame,
    /// voiced or not — it is the continuous measurement behind the decision.
    pub aperiodicity: Vec<f32>,
}

impl Pitch {
    /// Fraction of frames that carry a fundamental at all.
    ///
    /// A useful sanity number when a recording disappoints: a voiced fraction
    /// near zero usually means the input is noise, not that the tracker failed.
    pub fn voiced_fraction(&self) -> f32 {
        if self.hz.is_empty() {
            return 0.0;
        }
        self.hz.iter().filter(|h| h.is_some()).count() as f32 / self.hz.len() as f32
    }
}

/// Vocal-tract resonances, one entry per frame.
///
/// Two dimensions that matter and a third for context. F1 against F2 is the
/// space vowels live in: every vowel of a language occupies a region of it, and
/// a vowel sequence is a path through it. That geometry is the input the harmony
/// mapping is meant to be derived from.
///
/// `null` wherever the frame gives no usable estimate — unvoiced, silent, or the
/// fit found nothing in range. There is no such thing as a formant in silence.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Formants {
    pub f1: Vec<Option<f32>>,
    pub f2: Vec<Option<f32>>,
    pub f3: Vec<Option<f32>>,
}

impl Formants {
    /// Frames where both F1 and F2 are known, as `(f1, f2)` pairs.
    ///
    /// The vowel-space trajectory, which is what a consumer almost always wants:
    /// a frame carrying only one of the two is a point on no plane.
    pub fn vowel_space(&self) -> Vec<(f32, f32)> {
        self.f1
            .iter()
            .zip(&self.f2)
            .filter_map(|(a, b)| Some(((*a)?, (*b)?)))
            .collect()
    }
}

/// Event structure. Not yet a rhythm — see `docs/architecture.md`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Events {
    /// Normalised spectral flux per frame, 0..1 — the continuous curve.
    pub flux: Vec<f32>,
    /// Frame indices picked as onsets from that curve.
    pub onset_frames: Vec<usize>,
    /// The same onsets in seconds, so a consumer does not have to know the hop.
    pub onset_times_s: Vec<f32>,
}

/// Everything the analyser extracted from one recording.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Voiceprint {
    pub schema_version: u32,
    pub source: Source,
    pub frame: FrameGrid,
    pub pitch: Pitch,
    pub formants: Formants,
    /// Per-frame RMS in dBFS, floored at -100.
    pub rms_db: Vec<f32>,
    pub events: Events,
    /// The take's harmonic series, where it held a pitch long enough to have one.
    ///
    /// Not a per-frame series like the fields above: it describes the recording
    /// as a whole, measured over whichever frames were steady enough to use.
    /// `framesUsed` says how many those were, which on connected speech is few.
    pub partials: crate::partials::Partials,
    /// The shape of the noise, per frame.
    ///
    /// Defined everywhere but interesting mostly where the voice is unvoiced —
    /// the consonants, which are most of ordinary speech and which every other
    /// field here gates away.
    pub texture: crate::texture::Texture,
}

impl Voiceprint {
    /// Timestamp of each frame, in seconds. Derived rather than stored — it is
    /// `hop_s * i`, and storing it would be a second copy of the grid that could
    /// disagree with the first.
    pub fn frame_times_s(&self) -> Vec<f32> {
        (0..self.frame.count)
            .map(|i| i as f32 * self.frame.hop_s)
            .collect()
    }
}
