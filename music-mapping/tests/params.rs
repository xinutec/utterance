//! The knobs, checked for doing what they claim.
//!
//! Every one of these was a constant until someone needed to hear it moved, so
//! what matters is that turning each changes the thing it names and nothing
//! else. A knob that silently does nothing is worse than no knob.

use music_mapping::dissonance::Component;
use music_mapping::params::{Params, bind_toward_equal};
use music_mapping::tuning::{self, ratio_to_cents};

/// A bright harmonic spectrum — rich enough to give a scale worth thinning.
fn spectrum() -> Vec<Component> {
    (1..=12)
        .map(|k| Component {
            hz: 200.0 * k as f32,
            amplitude: 0.9f32.powi(k),
        })
        .collect()
}

/// Distance from `cents` to the nearest degree.
fn miss(t: &tuning::Tuning, cents: f32) -> f32 {
    t.degrees
        .iter()
        .map(|d| (d.cents - cents).abs())
        .fold(f32::INFINITY, f32::min)
}

#[test]
fn full_bind_leaves_the_speakers_scale_alone() {
    let t = tuning::from_spectrum(&spectrum()).unwrap();
    assert_eq!(bind_toward_equal(&t, 1.0).degrees, t.degrees);
}

#[test]
fn no_bind_lands_every_degree_on_a_tempered_note() {
    // The other end of the axis: this is what conventional tuning does to a
    // spectrum that did not ask for it.
    let t = bind_toward_equal(&tuning::from_spectrum(&spectrum()).unwrap(), 0.0);
    for d in &t.degrees {
        let off = d.cents - (d.cents / 100.0).round() * 100.0;
        assert!(
            off.abs() < 0.01,
            "degree at {} cents is not tempered",
            d.cents
        );
    }
}

#[test]
fn half_bind_sits_between_the_two() {
    // Interpolated in cents, because that is where the perceptual midpoint is:
    // halfway between a just third at 386 and a tempered one at 400 is 393.
    let t = tuning::from_spectrum(&spectrum()).unwrap();
    let just = ratio_to_cents(5.0 / 4.0);
    assert!(miss(&t, just) < 6.0, "no major third to bind");

    let half = bind_toward_equal(&t, 0.5);
    assert!(
        miss(&half, 393.0) < 6.0,
        "a half-bound third should sit near 393 cents"
    );
}

#[test]
fn binding_never_leaves_two_degrees_on_the_same_note() {
    // Neighbours can snap to the same tempered pitch. Keeping both would double
    // a voice in the field and change the balance for a reason nothing reports.
    for bind in [0.0f32, 0.1, 0.3] {
        let t = bind_toward_equal(&tuning::from_spectrum(&spectrum()).unwrap(), bind);
        for pair in t.degrees.windows(2) {
            assert!(
                (pair[1].cents - pair[0].cents).abs() >= 1.0,
                "bind {bind} left degrees at {} and {}",
                pair[0].cents,
                pair[1].cents
            );
        }
    }
}

#[test]
fn density_decides_how_many_notes_the_scale_keeps() {
    let sparse = tuning::from_spectrum_with(&spectrum(), 0.15).unwrap();
    let dense = tuning::from_spectrum_with(&spectrum(), 0.005).unwrap();
    assert!(
        dense.degrees.len() > sparse.degrees.len() + 2,
        "density did nothing: {} against {}",
        dense.degrees.len(),
        sparse.degrees.len()
    );
}

#[test]
fn defaults_reproduce_the_unparameterised_mapping() {
    // Taking no knobs must change nothing, or every earlier render stops being
    // comparable with every later one.
    let d = Params::default();
    assert_eq!(d.bind, 1.0);
    assert_eq!(d.density, tuning::MIN_DEPTH);
    assert_eq!(
        tuning::from_spectrum_with(&spectrum(), d.density)
            .unwrap()
            .degrees,
        tuning::from_spectrum(&spectrum()).unwrap().degrees
    );
}

#[test]
fn a_knob_out_of_range_is_brought_back_rather_than_refused() {
    // Someone exploring is not a bug, and the useful answer is the nearest thing
    // that makes a sound.
    let wild = Params {
        bind: 4.0,
        density: -1.0,
        voices: 900,
        spacing: 0,
        drift: -3.0,
        reach: 50.0,
        voicing: 9.0,
        articulation: -4.0,
        consonants: -2.0,
    }
    .sane();

    assert!((0.0..=1.0).contains(&wild.bind));
    assert!(wild.density > 0.0);
    assert!((1..=12).contains(&wild.voices));
    assert!(wild.spacing >= 1);
    assert!(wild.drift >= 0.0);
    assert!(wild.reach <= 3.0);
    assert!(wild.voicing <= 1.0);
    assert!(wild.articulation >= 0.0);
    assert!(wild.consonants >= 0.0);
}

/// Invariants of the knob table itself.
///
/// It is published to the UI, which builds a slider per row from the range and
/// the starting value — so a row that contradicts itself becomes a control that
/// cannot be used, and one that says nothing becomes a control nobody can
/// interpret.
mod table {
    use music_mapping::params::{KNOBS, Params};

    #[test]
    fn every_knob_starts_somewhere_it_is_allowed_to_be() {
        for knob in KNOBS {
            assert!(
                knob.min <= knob.default && knob.default <= knob.max,
                "{} starts at {} outside {}..{}",
                knob.name,
                knob.default,
                knob.min,
                knob.max
            );
            assert!(knob.step > 0.0, "{} has no step", knob.name);
            assert!(
                knob.step <= knob.max - knob.min,
                "{} steps past its own range",
                knob.name
            );
            assert!(!knob.about.is_empty(), "{} explains nothing", knob.name);
            assert!(
                !knob.label.is_empty(),
                "{} has no name for a person",
                knob.name
            );
        }
    }

    #[test]
    fn the_defaults_are_already_sane() {
        // If clamping moved a default, the table and the ranges disagree — and
        // every render taking no parameters would quietly be a different render
        // from the one the table describes.
        assert_eq!(Params::default().sane(), Params::default());
    }

    #[test]
    fn no_two_knobs_share_a_name() {
        // They are query parameters; two of a name means one is unreachable.
        for (i, a) in KNOBS.iter().enumerate() {
            for b in &KNOBS[i + 1..] {
                assert_ne!(a.name, b.name);
            }
        }
    }
}
