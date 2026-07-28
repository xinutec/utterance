//! Audio in, voiceprint out.
//!
//! The objective layer of the three described in `docs/architecture.md`. Every
//! question this crate answers has a right answer that can be demonstrated wrong
//! — is this frame voiced, what is f0 here, where are the events. It holds no
//! musical opinions and must never grow any: it does not know what a scale is,
//! and the moment it does, the mapping layer stops being replaceable.
//!
//! Analysis is a pure function of the audio bytes. No clock, no randomness, no
//! ambient configuration. The same input yields the same voiceprint on any
//! machine, which is what makes fixtures meaningful.

pub mod energy;
pub mod f0;
pub mod formant;
pub mod frame;
pub mod lpc;
pub mod onset;
pub mod partials;
pub mod resample;
pub mod speaker;
pub mod texture;
pub mod voiceprint;
pub mod wav;

use resample::ANALYSIS_RATE;
use voiceprint::{Events, Formants, FrameGrid, Pitch, Source, Voiceprint};

/// Everything that can go wrong turning bytes into a voiceprint.
#[derive(Debug, thiserror::Error)]
pub enum AnalysisError {
    #[error("could not decode audio: {0}")]
    Decode(String),
    #[error("recording contains no audio")]
    Empty,
    #[error("recording is too short to analyse: {duration_s:.2}s, need at least {minimum_s:.2}s")]
    TooShort { duration_s: f32, minimum_s: f32 },
}

/// Shortest recording worth analysing.
///
/// About four pitch windows. Below this the frame grid is a couple of dozen
/// entries, a large share of them edge-padded, and the medians and percentiles
/// everything downstream takes are computed over too few values to mean much.
pub const MIN_DURATION_S: f32 = 0.25;

/// Decode a WAV file and analyse it.
pub fn analyse_wav(bytes: &[u8]) -> Result<Voiceprint, AnalysisError> {
    let decoded = wav::decode(bytes)?;
    let duration_s = decoded.duration_s();
    if duration_s < MIN_DURATION_S {
        return Err(AnalysisError::TooShort {
            duration_s,
            minimum_s: MIN_DURATION_S,
        });
    }

    let mono = resample::to_mono(&decoded.samples, decoded.channels);
    let normalised = resample::resample(&mono, decoded.sample_rate, ANALYSIS_RATE);

    Ok(analyse(
        &normalised,
        Source {
            sample_rate_hz: decoded.sample_rate,
            channels: decoded.channels,
            duration_s,
            peak: decoded.peak(),
            clipped_fraction: decoded.clipped_fraction(),
        },
    ))
}

/// Analyse mono samples already at [`ANALYSIS_RATE`].
///
/// Public so a caller holding samples from somewhere other than a WAV file — a
/// live capture, a generated signal, a test — can reach the same analysis
/// without a round trip through a container format.
pub fn analyse(samples: &[f32], source: Source) -> Voiceprint {
    let count = frame::count(samples.len());

    let pitch_frames = f0::track(samples);
    // Formants are gated on voicing: linear prediction assumes a source driving
    // a filter, and an unvoiced frame has no periodic source to drive it.
    let voiced: Vec<bool> = pitch_frames.iter().map(|f| f.hz.is_some()).collect();
    let formant_frames = formant::track(samples, &voiced);

    let flux = onset::flux(samples);
    let onset_frames = onset::pick(&flux);
    let hop_s = frame::HOP as f32 / ANALYSIS_RATE as f32;

    // Guided by the pitch track above rather than re-deriving f0, so one
    // recording has exactly one answer about its fundamental.
    let pitch_hz: Vec<Option<f32>> = pitch_frames.iter().map(|f| f.hz).collect();
    let partials = partials::measure(samples, &pitch_hz);
    let texture = texture::track(samples);

    Voiceprint {
        schema_version: voiceprint::SCHEMA_VERSION,
        source,
        frame: FrameGrid {
            analysis_rate_hz: ANALYSIS_RATE,
            hop_s,
            count,
        },
        pitch: Pitch {
            hz: pitch_hz,
            aperiodicity: pitch_frames.iter().map(|f| f.aperiodicity).collect(),
        },
        formants: Formants {
            f1: formant_frames.iter().map(|f| f.f1).collect(),
            f2: formant_frames.iter().map(|f| f.f2).collect(),
            f3: formant_frames.iter().map(|f| f.f3).collect(),
        },
        rms_db: energy::track(samples),
        events: Events {
            onset_times_s: onset_frames.iter().map(|&i| i as f32 * hop_s).collect(),
            onset_frames,
            flux,
        },
        partials,
        texture,
    }
}
