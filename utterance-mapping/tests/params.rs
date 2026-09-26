//! The knobs, checked for changing what they name and nothing else.

use utterance_mapping::dissonance::Component;
use utterance_mapping::params::{KnobName, Params, bind_toward_equal};
use utterance_mapping::tuning::{self, ratio_to_cents};

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
    // What conventional tuning does to a spectrum that did not ask for it.
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
    // Interpolated in cents: halfway from 386 to 400 is 393.
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
    // Degrees that snap to one note would double a voice.
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
    // Taking no knobs must change nothing, so renders stay comparable.
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
    // Out of range is someone exploring: answer with the nearest sound.
    let wild = Params {
        bind: 4.0,
        density: -1.0,
        voices: 900,
        spacing: 0,
        drift: -3.0,
        reach: 50.0,
        hold: 7.0,
        settle: 9.0,
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
    assert!(wild.hold <= 1.0);
    assert!(wild.settle <= 0.5);
    assert!(wild.voicing <= 1.0);
    assert!(wild.articulation >= 0.0);
    assert!(wild.consonants >= 0.0);
}

/// Invariants of the knob table, which the UI builds a slider per row from.
mod table {
    use utterance_mapping::params::{
        ARTICULATION, BIND, CONSONANTS, DENSITY, DRIFT, HOLD, KNOBS, KnobName, KnobQuery, Params,
        REACH, SETTLE, SPACING, VOICES, VOICING,
    };

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
        // A clamped default would make the parameterless render differ from the
        // table.
        assert_eq!(Params::default().sane(), Params::default());
    }

    #[test]
    fn no_two_knobs_share_a_name() {
        // Query parameters: a duplicate name is unreachable.
        for (i, a) in KNOBS.iter().enumerate() {
            for b in &KNOBS[i + 1..] {
                assert_ne!(a.name, b.name);
            }
        }
    }

    #[test]
    fn every_knob_sets_the_field_its_name_promises() {
        // Spelled out, as a second statement of which field each knob sets, and
        // asserting nothing else moved.
        let d = Params::default();
        assert_eq!(d.with(BIND.name, 0.0), Params { bind: 0.0, ..d });
        assert_eq!(d.with(DENSITY.name, 0.4), Params { density: 0.4, ..d });
        assert_eq!(d.with(VOICES.name, 7.0), Params { voices: 7, ..d });
        assert_eq!(d.with(SPACING.name, 4.0), Params { spacing: 4, ..d });
        assert_eq!(d.with(DRIFT.name, 1.5), Params { drift: 1.5, ..d });
        assert_eq!(d.with(REACH.name, 2.5), Params { reach: 2.5, ..d });
        assert_eq!(d.with(HOLD.name, 0.9), Params { hold: 0.9, ..d });
        assert_eq!(d.with(SETTLE.name, 0.2), Params { settle: 0.2, ..d });
        assert_eq!(d.with(VOICING.name, 0.1), Params { voicing: 0.1, ..d });
        assert_eq!(
            d.with(ARTICULATION.name, 1.2),
            Params {
                articulation: 1.2,
                ..d
            }
        );
        assert_eq!(
            d.with(CONSONANTS.name, 0.0),
            Params {
                consonants: 0.0,
                ..d
            }
        );
    }

    #[test]
    fn every_published_knob_is_reachable_by_name() {
        // Each knob's field must actually move — to the end furthest from the
        // default, since `bind` starts at its maximum.
        for knob in KNOBS {
            let far = if (knob.max - knob.default) >= (knob.default - knob.min) {
                knob.max
            } else {
                knob.min
            };
            assert_ne!(
                Params::default().with(knob.name, far),
                Params::default(),
                "{} does not move when set to {far}",
                knob.name
            );
        }
    }

    #[test]
    fn setting_a_knob_clamps_rather_than_escaping_its_range() {
        // Past the end lands at the end.
        for knob in KNOBS {
            let over = Params::default().with(knob.name, knob.max + 1000.0);
            assert_eq!(over, over.sane(), "{}", knob.name);
        }
    }

    /// [`KnobName::name`] and the serde attribute are one spelling, so a query
    /// parameter, a field and a wire value are the same word.
    #[test]
    fn name_round_trips_through_serde() {
        for knob in KNOBS {
            assert_eq!(
                KnobName::from_name(knob.name.name()),
                Some(knob.name),
                "{:?} spells itself {:?}, which serde does not read back",
                knob.name,
                knob.name.name()
            );
        }
        assert_eq!(KnobName::from_name("bnid"), None);
    }

    /// A query naming no knob leaves every knob at its default.
    #[test]
    fn an_empty_query_is_the_defaults() {
        assert_eq!(KnobQuery::default().params(), Params::default());
    }
}

#[test]
fn a_count_knob_rounds_to_the_nearest_rather_than_down() {
    // A slider stopped just under 5 means 5 voices, not 4.
    assert_eq!(Params::default().with(KnobName::VOICES, 4.7).voices, 5);
    assert_eq!(Params::default().with(KnobName::VOICES, 4.4).voices, 4);
    assert_eq!(Params::default().with(KnobName::VOICES, 5.0).voices, 5);
    // The other count knob, so the property is the conversion's.
    assert_eq!(Params::default().with(KnobName::SPACING, 2.7).spacing, 3);
}
