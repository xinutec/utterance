//! WAV decoding — the one door audio comes in through.

mod common;

use common::wav_bytes;
use music_analysis::AnalysisError;
use music_analysis::wav::decode;

/// Encode with an explicit channel count, interleaving the same signal.
fn stereo_wav(samples: &[f32], rate: u32) -> Vec<u8> {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut w = hound::WavWriter::new(&mut buf, spec).expect("wav writer");
        for &s in samples {
            let v = (s.clamp(-1.0, 1.0) * 32_767.0) as i16;
            w.write_sample(v).expect("write left");
            w.write_sample(v).expect("write right");
        }
        w.finalize().expect("finalize wav");
    }
    buf.into_inner()
}

#[test]
fn round_trips_16_bit_pcm() {
    let want = [0.0f32, 0.5, -0.5, 0.25];
    let got = decode(&wav_bytes(&want, 44_100)).unwrap();
    assert_eq!(got.sample_rate, 44_100);
    assert_eq!(got.channels, 1);
    for (g, w) in got.samples.iter().zip(want) {
        assert!((g - w).abs() < 1e-3, "{g} != {w}");
    }
}

#[test]
fn reports_duration_from_frames_not_samples() {
    // Two channels of 8000 frames at 8 kHz is one second, not two.
    let d = decode(&stereo_wav(&vec![0.1f32; 8_000], 8_000)).unwrap();
    assert_eq!(d.channels, 2);
    assert!(
        (d.duration_s() - 1.0).abs() < 1e-6,
        "got {}",
        d.duration_s()
    );
}

#[test]
fn normalises_by_full_scale_not_by_the_loudest_sample() {
    // Peak normalisation would make the energy envelope depend on the take's
    // loudest moment, so the same voice recorded twice at different gains would
    // produce different-looking voiceprints.
    let quiet = decode(&wav_bytes(&[0.25f32, -0.25], 16_000)).unwrap();
    assert!(
        quiet.samples.iter().all(|s| s.abs() < 0.3),
        "got {:?}",
        quiet.samples
    );
}

#[test]
fn rejects_non_wav_bytes() {
    assert!(matches!(
        decode(b"this is not a wav file"),
        Err(AnalysisError::Decode(_))
    ));
}

#[test]
fn rejects_an_empty_recording() {
    assert!(matches!(
        decode(&wav_bytes(&[], 16_000)),
        Err(AnalysisError::Empty)
    ));
}
