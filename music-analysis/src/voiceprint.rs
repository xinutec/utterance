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
pub const SCHEMA_VERSION: u32 = 1;

/// What the recording was before analysis normalised it.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub duration_s: f32,
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
    /// Per-frame RMS in dBFS, floored at -100.
    pub rms_db: Vec<f32>,
    pub events: Events,
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
