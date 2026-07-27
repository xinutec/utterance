//! WAV decoding.
//!
//! The only IO-shaped thing in this crate, and deliberately the thinnest layer
//! that exists: bytes in, samples out. Everything past this point is arithmetic.

use std::io::Cursor;

use crate::AnalysisError;

/// A decoded recording, before any rate normalisation.
#[derive(Clone, Debug)]
pub struct Decoded {
    /// Interleaved samples, normalised to roughly -1.0..1.0.
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
}

/// Absolute sample value at or above which a sample counts as pinned.
///
/// Just under 16-bit full scale (32767/32768 = 0.99997), so a genuinely
/// maximal sample is caught whatever the source bit depth.
const FULL_SCALE: f32 = 0.999;

impl Decoded {
    /// Duration in seconds of the source recording.
    pub fn duration_s(&self) -> f32 {
        let frames = self.samples.len() / usize::from(self.channels.max(1));
        frames as f32 / self.sample_rate as f32
    }

    /// Highest absolute sample, 0..1.
    pub fn peak(&self) -> f32 {
        self.samples.iter().fold(0.0f32, |m, s| m.max(s.abs()))
    }

    /// Fraction of samples pinned at full scale.
    ///
    /// Counted on the source samples, which is the only place it can be seen:
    /// clipping is a flat top on the waveform, and resampling rounds it off.
    pub fn clipped_fraction(&self) -> f32 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let pinned = self
            .samples
            .iter()
            .filter(|s| s.abs() >= FULL_SCALE)
            .count();
        pinned as f32 / self.samples.len() as f32
    }
}

/// Decode a WAV file.
///
/// Integer formats are scaled by their full-scale value rather than by the
/// maximum sample present: normalising to the loudest sample would make the
/// energy envelope depend on the recording's peak, so the same voice recorded
/// twice at different gains would produce different-looking voiceprints.
pub fn decode(bytes: &[u8]) -> Result<Decoded, AnalysisError> {
    let reader = hound::WavReader::new(Cursor::new(bytes))
        .map_err(|e| AnalysisError::Decode(e.to_string()))?;
    let spec = reader.spec();

    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .into_samples::<f32>()
            .collect::<Result<_, _>>()
            .map_err(|e| AnalysisError::Decode(e.to_string()))?,
        hound::SampleFormat::Int => {
            let full_scale = (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .into_samples::<i32>()
                .map(|s| s.map(|v| v as f32 / full_scale))
                .collect::<Result<_, _>>()
                .map_err(|e| AnalysisError::Decode(e.to_string()))?
        }
    };

    if spec.channels == 0 {
        return Err(AnalysisError::Decode("file declares zero channels".into()));
    }
    if samples.is_empty() {
        return Err(AnalysisError::Empty);
    }

    Ok(Decoded {
        samples,
        sample_rate: spec.sample_rate,
        channels: spec.channels,
    })
}
