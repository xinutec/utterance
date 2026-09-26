//! The voiceprint: everything analysis knows about a recording, and the one
//! artefact whose shape must stay stable. A plain document, so it can be diffed,
//! kept as a fixture and plotted without the analyser. No notes, no scale, no
//! beat: those are the mapping layer's decisions.

use serde::{Deserialize, Serialize};

/// Identifies the analyser. **Bump it for any change that alters the output —
/// the algorithm as much as the shape**: a shape change fails on deserialise, an
/// algorithm change is silent.
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
    /// Fraction of source samples pinned at full scale, measured before
    /// resampling rounds the flat tops off.
    pub clipped_fraction: f32,
}

/// Fraction of pinned samples above which a recording is called clipped: one
/// sample at full scale is chance, a tenth of a percent is a too-hot input.
pub const CLIPPING_FRACTION: f32 = 0.001;

impl Source {
    /// Whether the recording was driven into the rails — distortion in exactly
    /// the partial amplitudes a tuning is derived from.
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
    /// Fundamental per frame; `null` where unvoiced, so a mean cannot silently
    /// include it.
    pub hz: Vec<Option<f32>>,
    /// YIN's normalised difference at the chosen lag, for every frame — the
    /// continuous measurement behind the voicing decision.
    pub aperiodicity: Vec<f32>,
}

impl Pitch {
    /// Fraction of frames that carry a fundamental: near zero usually means the
    /// input is noise.
    pub fn voiced_fraction(&self) -> f32 {
        if self.hz.is_empty() {
            return 0.0;
        }
        self.hz.iter().filter(|h| h.is_some()).count() as f32 / self.hz.len() as f32
    }
}

/// Vocal-tract resonances, one entry per frame. F1 against F2 is the space vowels
/// live in, and what the harmony mappings read. `null` where the frame gives no
/// usable estimate.
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
    /// Frames where both F1 and F2 are known, as `(f1, f2)` pairs: the vowel-space
    /// trajectory.
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
    /// The take's harmonic series, where it held a pitch long enough to have one
    /// — for the recording as a whole, not per frame.
    pub partials: crate::partials::Partials,
    /// The shape of the noise, per frame — mostly interesting where unvoiced.
    pub texture: crate::texture::Texture,
}

impl Voiceprint {
    /// Timestamp of each frame, in seconds: derived, so it cannot disagree with
    /// the grid.
    pub fn frame_times_s(&self) -> Vec<f32> {
        (0..self.frame.count)
            .map(|i| i as f32 * self.frame.hop_s)
            .collect()
    }
}
