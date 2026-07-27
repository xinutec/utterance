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

impl Decoded {
    /// Duration in seconds of the source recording.
    pub fn duration_s(&self) -> f32 {
        let frames = self.samples.len() / usize::from(self.channels.max(1));
        frames as f32 / self.sample_rate as f32
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
