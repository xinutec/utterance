//! Onset detection against real recorded audio.
//!
//! Synthetic signals establish that the detector is *correct* — an onset where a
//! burst starts, none in silence. They cannot establish that it is *usable*: a
//! generated tone is perfectly steady, and real phonation is not. This file is
//! the other half.
//!
//! **These tests deliberately assert bounds, not exact counts.** How many events
//! this recording truly contains is not known — it depends on what the speaker
//! articulated, which the audio alone does not settle. Asserting an exact number
//! would be writing down a guess and calling it ground truth. What *is* certain
//! is the shape of the failure the first implementation had, and that is what is
//! pinned here.

use music_analysis::analyse_wav;

/// A real held vowel — see `tests/fixtures/README.md`.
const SUSTAINED_VOWEL: &[u8] = include_bytes!("fixtures/sustained-vowel.wav");

/// Onsets the first implementation reported on this fixture. Kept as the number
/// to stay far below.
const ORIGINAL_FALSE_POSITIVES: usize = 22;

#[test]
fn a_sustained_vowel_does_not_dissolve_into_events() {
    // The regression this file exists for. A fixed offset above the local median,
    // with "greater than its two neighbours" as the peak test, reported 22 onsets
    // scattered through one continuous sound.
    let onsets = &analyse_wav(SUSTAINED_VOWEL).unwrap().events.onset_times_s;

    assert!(
        onsets.len() < ORIGINAL_FALSE_POSITIVES / 2,
        "{} onsets in a single sustained vowel (was {ORIGINAL_FALSE_POSITIVES}): {:?}",
        onsets.len(),
        onsets
            .iter()
            .map(|t| (t * 100.0).round() / 100.0)
            .collect::<Vec<_>>()
    );
}

#[test]
fn nothing_is_detected_after_the_sound_stops() {
    // Phonation ends at about 7.2s in this fixture; the rest is room tone. Flux
    // is meaningless there — a sound that has stopped cannot start — and the
    // original reported three events in it, artefacts of the release.
    let onsets = &analyse_wav(SUSTAINED_VOWEL).unwrap().events.onset_times_s;

    for &t in onsets {
        assert!(t < 7.3, "onset at {t:.2}s, after the phonation has ended");
    }
}

#[test]
fn the_fixture_really_is_a_steady_sustained_vowel() {
    // Guards the guard. If a future edit swaps the fixture for something else,
    // the tests above would still pass while measuring nothing.
    let vp = analyse_wav(SUSTAINED_VOWEL).unwrap();

    assert!(
        vp.pitch.voiced_fraction() > 0.7,
        "only {:.0}% voiced",
        vp.pitch.voiced_fraction() * 100.0
    );

    let mut hz: Vec<f32> = vp.pitch.hz.iter().flatten().copied().collect();
    hz.sort_by(f32::total_cmp);
    let semitones = 12.0 * (hz[9 * hz.len() / 10] / hz[hz.len() / 10]).log2();
    assert!(
        semitones < 2.0,
        "f0 spans {semitones:.1} semitones — not a steady vowel"
    );
}

#[test]
fn the_loudest_spectral_change_is_still_reported() {
    // The failure mode of over-correcting. This recording's largest spectral
    // change by a wide margin sits around 2.4s; whatever tuning the detector
    // carries, an event that dominant must survive it.
    let vp = analyse_wav(SUSTAINED_VOWEL).unwrap();

    let (peak_frame, _) = vp
        .events
        .flux
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.total_cmp(b))
        .expect("flux curve is empty");

    assert!(
        vp.events.onset_frames.contains(&peak_frame),
        "the largest flux peak (frame {peak_frame}) was not reported as an onset: {:?}",
        vp.events.onset_frames
    );
}
