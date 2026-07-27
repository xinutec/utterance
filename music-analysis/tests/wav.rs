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
fn reports_the_peak_of_the_source() {
    let quiet = decode(&wav_bytes(&[0.3f32, -0.5, 0.2], 16_000)).unwrap();
    assert!((quiet.peak() - 0.5).abs() < 1e-3, "got {}", quiet.peak());
}

#[test]
fn a_clean_recording_reports_no_clipping() {
    // A single sample touching full scale is a peak that happened to land
    // there, not distortion — it must not raise the flag on its own.
    let mut samples = vec![0.4f32; 1_000];
    samples[500] = 1.0;
    let d = decode(&wav_bytes(&samples, 16_000)).unwrap();
    assert!(
        d.clipped_fraction() <= 0.001,
        "got {}",
        d.clipped_fraction()
    );
}

#[test]
fn a_flat_topped_recording_reports_clipping() {
    // What a too-hot input produces: runs of samples pinned at the rail.
    let mut samples = vec![0.4f32; 1_000];
    for s in samples.iter_mut().take(60) {
        *s = 1.0;
    }
    let d = decode(&wav_bytes(&samples, 16_000)).unwrap();
    assert!(
        (d.clipped_fraction() - 0.06).abs() < 0.01,
        "got {}",
        d.clipped_fraction()
    );
}

#[test]
fn clipping_is_measured_before_resampling() {
    // The measurement has to survive the trip through analyse_wav: a
    // band-limited resampler rounds the flat tops off, so a conversion first
    // would quietly hide it.
    let mut samples = vec![0.4f32; 48_000];
    for (i, s) in samples.iter_mut().enumerate() {
        if i % 100 < 40 {
            *s = 1.0;
        }
    }
    let vp = music_analysis::analyse_wav(&wav_bytes(&samples, 48_000)).unwrap();
    assert!(
        vp.source.is_clipped(),
        "clipping was lost: {}",
        vp.source.clipped_fraction
    );
    assert!((vp.source.peak - 1.0).abs() < 1e-3);
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
