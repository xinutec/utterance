//! Scales derived from spectra whose right answer is known independently.
//!
//! Nothing here can check that a scale sounds good. What it can check is that
//! the procedure reproduces results the literature already establishes, and —
//! more importantly — that the scale genuinely comes out of the spectrum rather
//! than out of the code. Two tests carry that weight: a stretched spectrum must
//! not make the octave consonant, and changing only the amplitudes of a harmonic
//! series must change the scale.

use music_mapping::dissonance::{self, Component};
use music_mapping::tuning::{self, Tuning, ratio_to_cents};

/// A harmonic spectrum with amplitudes given by a rolloff function of `k`.
fn harmonic(f0: f32, count: usize, amplitude: impl Fn(usize) -> f32) -> Vec<Component> {
    (1..=count)
        .map(|k| Component {
            hz: f0 * k as f32,
            amplitude: amplitude(k),
        })
        .collect()
}

/// Steeply falling partials — a dull timbre, the twelfth 21 dB down.
fn dull(f0: f32) -> Vec<Component> {
    harmonic(f0, 12, |k| 1.0 / k as f32)
}

/// Gently falling partials — a bright timbre, the twelfth only 11 dB down.
fn bright(f0: f32) -> Vec<Component> {
    harmonic(f0, 12, |k| 0.9f32.powi(k as i32))
}

/// Partials at `k^1.4` rather than `k`: emphatically not a harmonic series.
fn stretched(f0: f32) -> Vec<Component> {
    (1..=12)
        .map(|k| Component {
            hz: f0 * (k as f32).powf(1.4),
            amplitude: 1.0 / k as f32,
        })
        .collect()
}

/// Distance from `cents` to the nearest derived degree.
fn distance_to_degree(t: &Tuning, cents: f32) -> f32 {
    t.degrees
        .iter()
        .map(|d| (d.cents - cents).abs())
        .fold(f32::INFINITY, f32::min)
}

#[test]
fn roughness_vanishes_at_unison_and_at_a_distance() {
    // Both limits of the Plomp-Levelt curve. Without the first nothing would
    // make a unison consonant; without the second every wide interval would be.
    let a = Component {
        hz: 440.0,
        amplitude: 1.0,
    };
    assert_eq!(dissonance::between(a, a), 0.0);

    let far = Component {
        hz: 4400.0,
        amplitude: 1.0,
    };
    assert!(dissonance::between(a, far) < 0.001);
}

#[test]
fn roughness_peaks_at_a_small_separation() {
    let a = Component {
        hz: 440.0,
        amplitude: 1.0,
    };
    let beating = Component {
        hz: 455.0,
        amplitude: 1.0,
    };
    let apart = Component {
        hz: 660.0,
        amplitude: 1.0,
    };

    let rough = dissonance::between(a, beating);
    assert!(rough > dissonance::between(a, apart));
    assert!(rough > 0.1, "expected a pronounced peak, got {rough}");
}

#[test]
fn a_harmonic_spectrum_puts_the_fifth_and_fourth_where_they_belong() {
    let t = tuning::from_spectrum(&dull(200.0)).unwrap();
    for (name, ratio) in [("fifth", 3.0 / 2.0), ("fourth", 4.0 / 3.0)] {
        let miss = distance_to_degree(&t, ratio_to_cents(ratio));
        assert!(miss < 6.0, "{name} missed by {miss:.0} cents");
    }
}

#[test]
fn the_fifth_is_the_deepest_note_in_any_harmonic_scale() {
    // 3:2 aligns more partials than any other interval inside the octave, so it
    // should be the firmest place to rest whatever the amplitudes do.
    for spectrum in [dull(200.0), bright(200.0)] {
        let t = tuning::from_spectrum(&spectrum).unwrap();
        let deepest = t
            .degrees
            .iter()
            .max_by(|a, b| a.depth.total_cmp(&b.depth))
            .unwrap();
        assert!(
            (deepest.cents - 702.0).abs() < 6.0,
            "deepest degree was at {:.0} cents, not the fifth",
            deepest.cents
        );
    }
}

#[test]
fn the_amplitude_profile_decides_what_the_scale_contains() {
    // The claim the whole project rests on. These two spectra have partials at
    // identical frequencies and differ *only* in how loud each one is — so any
    // difference in the scale comes from the timbre and nowhere else.
    let dull_scale = tuning::from_spectrum(&dull(200.0)).unwrap();
    let bright_scale = tuning::from_spectrum(&bright(200.0)).unwrap();

    assert!(
        bright_scale.degrees.len() > dull_scale.degrees.len() + 2,
        "bright gave {} degrees, dull gave {} — the rolloff should matter more than that",
        bright_scale.degrees.len(),
        dull_scale.degrees.len()
    );

    // Concretely: the major third is a note in one and not in the other. In the
    // dull spectrum the dip at 386 cents exists but is 0.001 deep, which is a
    // technicality rather than somewhere a listener rests.
    let third = ratio_to_cents(5.0 / 4.0);
    assert!(
        distance_to_degree(&bright_scale, third) < 6.0,
        "a bright harmonic timbre should make a major third consonant"
    );
    assert!(
        distance_to_degree(&dull_scale, third) > 50.0,
        "a dull one should not"
    );
}

#[test]
fn a_stretched_spectrum_does_not_make_the_octave_consonant() {
    // The test that stops this being a tautology, and the classic result:
    // octaves are consonant *because* partials are harmonic, not because 2:1 is
    // arithmetically special. Move the partials and the octave roughens.
    let harmonic_octave = tuning::from_spectrum(&dull(200.0))
        .unwrap()
        .degrees
        .last()
        .unwrap()
        .dissonance;
    let stretched_octave = tuning::from_spectrum(&stretched(200.0))
        .unwrap()
        .degrees
        .last()
        .unwrap()
        .dissonance;

    assert!(
        harmonic_octave < 0.1,
        "a harmonic octave should be near-silent, measured {harmonic_octave:.3}"
    );
    assert!(
        stretched_octave > 3.0 * harmonic_octave,
        "stretched octave {stretched_octave:.3} against harmonic {harmonic_octave:.3} — \
         the procedure is not reading the spectrum"
    );
}

#[test]
fn every_scale_opens_at_the_tonic_and_closes_at_the_octave() {
    let t = tuning::from_spectrum(&dull(200.0)).unwrap();
    assert_eq!(t.degrees.first().unwrap().cents, 0.0);
    assert_eq!(t.degrees.last().unwrap().cents, 1200.0);
}

#[test]
fn degrees_ascend_and_none_repeats() {
    let t = tuning::from_spectrum(&bright(180.0)).unwrap();
    for pair in t.degrees.windows(2) {
        assert!(
            pair[1].cents > pair[0].cents,
            "degrees out of order: {} then {}",
            pair[0].cents,
            pair[1].cents
        );
    }
}

#[test]
fn a_scale_has_a_workable_number_of_notes() {
    // Not a claim about the right number — only that the depth threshold does
    // something. Without it every wobble is a note: the bright spectrum alone
    // has twenty local minima, most of them a thousandth deep.
    for spectrum in [dull(200.0), bright(200.0)] {
        let t = tuning::from_spectrum(&spectrum).unwrap();
        assert!(
            (3..=20).contains(&t.degrees.len()),
            "derived {} degrees, which is not a scale anyone can use",
            t.degrees.len()
        );
    }
}

#[test]
fn says_nothing_when_there_is_nothing_to_collide() {
    // One partial has nothing to beat against, so its curve is flat and every
    // interval is equally consonant — true, and useless.
    assert!(
        tuning::from_spectrum(&[Component {
            hz: 440.0,
            amplitude: 1.0
        }])
        .is_none()
    );
    assert!(tuning::from_spectrum(&[]).is_none());
}

#[test]
fn is_a_pure_function_of_its_input() {
    let spectrum = bright(200.0);
    let a = tuning::from_spectrum(&spectrum).unwrap();
    let b = tuning::from_spectrum(&spectrum).unwrap();
    assert_eq!(a.degrees, b.degrees);
}
