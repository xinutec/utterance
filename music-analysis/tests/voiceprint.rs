//! The whole pipeline: audio in, a coherent voiceprint out.

mod common;

use common::{sine, vowel, wav_bytes};
use music_analysis::resample::ANALYSIS_RATE;
use music_analysis::voiceprint::{Source, Voiceprint};
use music_analysis::{AnalysisError, analyse, analyse_wav, energy};

fn source(secs: f32) -> Source {
    Source {
        sample_rate_hz: ANALYSIS_RATE,
        channels: 1,
        duration_s: secs,
    }
}

#[test]
fn every_series_is_the_length_the_grid_declares() {
    // The invariant the whole document rests on: series are read side by side by
    // frame index, so a length mismatch is a silent misalignment.
    let vp = analyse(&vowel(120.0, 2.0), source(2.0));
    assert_eq!(vp.pitch.hz.len(), vp.frame.count);
    assert_eq!(vp.pitch.aperiodicity.len(), vp.frame.count);
    assert_eq!(vp.rms_db.len(), vp.frame.count);
    assert_eq!(vp.events.flux.len(), vp.frame.count);
    assert_eq!(vp.frame_times_s().len(), vp.frame.count);
}

#[test]
fn onset_frames_and_times_agree() {
    let vp = analyse(&vowel(120.0, 2.0), source(2.0));
    assert_eq!(vp.events.onset_frames.len(), vp.events.onset_times_s.len());
    for (&f, &t) in vp.events.onset_frames.iter().zip(&vp.events.onset_times_s) {
        assert!((t - f as f32 * vp.frame.hop_s).abs() < 1e-6);
        assert!(
            f < vp.frame.count,
            "onset frame {f} is off the end of the grid"
        );
    }
}

#[test]
fn analysis_is_deterministic() {
    // Byte for byte, not approximately. Fixtures are worthless otherwise, and so
    // is telling "the mapping changed" apart from "the analyser drifted".
    let x = vowel(140.0, 1.0);
    let a = serde_json::to_string(&analyse(&x, source(1.0))).unwrap();
    let b = serde_json::to_string(&analyse(&x, source(1.0))).unwrap();
    assert_eq!(a, b);
}

#[test]
fn a_sustained_vowel_is_mostly_voiced() {
    let vp = analyse(&vowel(120.0, 2.0), source(2.0));
    assert!(
        vp.pitch.voiced_fraction() > 0.9,
        "got {}",
        vp.pitch.voiced_fraction()
    );
}

#[test]
fn silence_is_entirely_unvoiced() {
    let vp = analyse(&vec![0.0f32; ANALYSIS_RATE as usize], source(1.0));
    assert_eq!(vp.pitch.voiced_fraction(), 0.0);
    assert!(vp.events.onset_frames.is_empty());
    assert!(vp.rms_db.iter().all(|&d| d == energy::SILENCE_DB));
}

#[test]
fn voiceprint_survives_a_json_round_trip() {
    let vp = analyse(&vowel(120.0, 0.5), source(0.5));
    let json = serde_json::to_string(&vp).unwrap();
    let back: Voiceprint = serde_json::from_str(&json).unwrap();
    assert_eq!(serde_json::to_string(&back).unwrap(), json);
}

#[test]
fn a_recording_shorter_than_the_minimum_is_rejected() {
    // Rejected loudly rather than analysed into a grid of edge artefacts.
    let short = wav_bytes(&vowel(120.0, 0.1), ANALYSIS_RATE);
    assert!(matches!(
        analyse_wav(&short),
        Err(AnalysisError::TooShort { .. })
    ));
}

#[test]
fn the_wav_path_and_the_sample_path_agree() {
    let x = vowel(150.0, 1.0);
    let from_wav = analyse_wav(&wav_bytes(&x, ANALYSIS_RATE)).unwrap();
    let direct = analyse(&x, source(1.0));
    // The WAV path round-trips through 16-bit ints, so pitch shifts by fractions
    // of a hertz; the frame grid must match exactly.
    assert_eq!(from_wav.frame.count, direct.frame.count);
    assert!((from_wav.pitch.voiced_fraction() - direct.pitch.voiced_fraction()).abs() < 0.05);
}

#[test]
fn a_recording_at_another_rate_lands_on_the_same_grid() {
    // The reason resampling exists: frame indices must mean the same thing
    // whatever the recording device produced. A 48 kHz take and a 16 kHz take of
    // the same duration must produce the same number of frames, and report the
    // same pitch, while remembering the rate they arrived at.
    let seconds = 1.0;
    let at_48k = analyse_wav(&wav_bytes(&sine(150.0, 48_000, seconds), 48_000)).unwrap();
    let at_16k = analyse_wav(&wav_bytes(
        &sine(150.0, ANALYSIS_RATE, seconds),
        ANALYSIS_RATE,
    ))
    .unwrap();

    assert_eq!(at_48k.source.sample_rate_hz, 48_000);
    assert_eq!(at_48k.frame.analysis_rate_hz, ANALYSIS_RATE);
    assert_eq!(at_48k.frame.count, at_16k.frame.count);
    assert!((at_48k.frame.hop_s - 0.01).abs() < 1e-6);

    let median = |vp: &Voiceprint| {
        let mut hz: Vec<f32> = vp.pitch.hz.iter().flatten().copied().collect();
        hz.sort_by(f32::total_cmp);
        hz[hz.len() / 2]
    };
    assert!((median(&at_48k) - median(&at_16k)).abs() < 1.0);
}
