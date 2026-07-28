//! Rendered samples into a WAV file.

use std::io::Cursor;

use crate::synth::RENDER_RATE;

/// Encode mono samples as 16-bit PCM WAV bytes.
///
/// Sixteen bits rather than float: this is for listening to, and every player
/// and browser takes it without argument. The rounding it costs is far below
/// anything audible in a render that was normalised to headroom.
pub fn encode(samples: &[f32]) -> Vec<u8> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: RENDER_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut buffer = Cursor::new(Vec::new());
    {
        let mut writer = hound::WavWriter::new(&mut buffer, spec).expect("in-memory writer");
        for &sample in samples {
            // Clamped, not wrapped: a sample past full scale that wrapped would
            // become a loud crack rather than the mild flattening clipping is.
            let clamped = sample.clamp(-1.0, 1.0);
            writer
                .write_sample((clamped * i16::MAX as f32) as i16)
                .expect("in-memory write");
        }
        writer.finalize().expect("in-memory finalize");
    }
    buffer.into_inner()
}
