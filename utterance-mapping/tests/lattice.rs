//! The harmonic lattice, checked against the claims its module makes.
//!
//! Two of them can be tested and both matter. That the axes are *read* from the
//! spectrum rather than assumed — the same test the tuning has, for the same
//! reason: a derivation that restates its own assumptions is worth nothing. And
//! that adjacency on the lattice really is the near relation it is claimed to
//! be, since every musical consequence here rests on that geometry.

use utterance_mapping::dissonance::Component;
use utterance_mapping::lattice::{Lattice, NoPlane, Triangle, generators, settle, triangle_at};
use utterance_mapping::tuning::{self, Degree, Tuning};

/// A harmonic spectrum, as a voice makes.
fn harmonic() -> Tuning {
    let spectrum: Vec<Component> = (1..=16)
        .map(|k| Component {
            hz: k as f32 * 120.0,
            amplitude: 0.9f32.powi(k),
        })
        .collect();
    tuning::from_spectrum(&spectrum).expect("a scale")
}

/// A spectrum stretched until it is nothing a throat could make.
///
/// Partials at `k^1.4` rather than at `k`, which is roughly what a stiff bar
/// does. Its consonances are somewhere else entirely, and if the lattice comes
/// out spanned by a fifth and a third anyway then the derivation is decoration.
fn stretched() -> Tuning {
    let spectrum: Vec<Component> = (1..=16)
        .map(|k| Component {
            hz: 120.0 * (k as f32).powf(1.4),
            amplitude: 0.9f32.powi(k),
        })
        .collect();
    tuning::from_spectrum(&spectrum).expect("a scale")
}

#[test]
fn a_voice_is_spanned_by_intervals_it_actually_makes_consonant() {
    // Not a claim that they are the fifth and the third — a claim that they are
    // deep minima of this speaker's own curve. That they land near the familiar
    // pair for a harmonic spectrum is the result, not the input.
    let t = harmonic();
    let (a, b) = generators(&t).expect("two generators");

    for g in [a, b] {
        let index = g.cents.round() as usize;
        let neighbours = [index - 1, index + 1];
        for n in neighbours {
            assert!(
                t.curve[index] <= t.curve[n],
                "generator at {} cents is not a minimum of the curve",
                g.cents
            );
        }
    }
    assert!(a.depth >= b.depth, "generators are not ordered by depth");
}

#[test]
fn the_first_axis_of_a_voice_is_the_fifth() {
    // The classical Tonnetz's first axis, arrived at from the spectrum rather
    // than from the theory: the fifth is what a harmonic series makes least
    // rough, which is why harmony settled on it. Deriving it back is the check
    // that the derivation does the work the theory says it does.
    let (a, _) = generators(&harmonic()).unwrap();
    assert!(
        (a.cents - 702.0).abs() < 30.0,
        "the deepest axis of a voice is at {} cents, not the fifth",
        a.cents
    );
}

#[test]
fn an_inharmonic_spectrum_is_spanned_by_something_else() {
    let voice = generators(&harmonic()).unwrap();
    let bar = generators(&stretched()).unwrap();
    let moved =
        (voice.0.cents - bar.0.cents).abs() > 30.0 || (voice.1.cents - bar.1.cents).abs() > 30.0;
    assert!(
        moved,
        "a stretched spectrum gave the same axes as a voice: {:?} and {:?}",
        (voice.0.cents, voice.1.cents),
        (bar.0.cents, bar.1.cents)
    );
}

/// A scale built by hand, to reach shapes a real spectrum reaches only rarely.
///
/// Interior degrees as `(cents, depth)`; the tonic and the octave are added the
/// way `tuning` adds them, with no depth.
fn scale(interior: &[(f32, f32)]) -> Tuning {
    let degree = |cents: f32, depth: f32| Degree {
        cents,
        ratio: tuning::cents_to_ratio(cents),
        dissonance: 0.0,
        depth,
    };
    let mut degrees = vec![degree(0.0, 0.0)];
    degrees.extend(interior.iter().map(|(c, d)| degree(*c, *d)));
    degrees.push(degree(1200.0, 0.0));
    Tuning {
        degrees,
        curve: vec![0.0; tuning::RESOLUTION + 1],
    }
}

#[test]
fn a_scale_with_one_interval_spans_no_plane() {
    // A steady *ee* in the calibration set gave the fifth and nothing else. That
    // is a line, and saying so beats folding the plane flat and letting one of
    // the two vowel dimensions reach nothing.
    let refused = Lattice::from_tuning(&scale(&[(702.0, 0.3)])).expect_err("a plane from a line");
    assert_eq!(
        refused,
        NoPlane::TooFewIntervals {
            interior: vec![702.0]
        }
    );
}

#[test]
fn a_refusal_says_which_intervals_there_were_and_what_to_move() {
    // The whole reason the failure carries a reason. Someone who raised one
    // slider until the music stopped needs to be told *that* slider, and a
    // message that leaves out the intervals cannot be checked against the scale
    // shown on the same screen.
    let refused = Lattice::from_tuning(&scale(&[(702.0, 0.3)])).expect_err("a plane from a line");
    let said = refused.to_string();
    assert!(
        said.contains("702"),
        "the interval it had is not named: {said}"
    );
    assert!(
        said.contains("density"),
        "the knob that undoes this is not named: {said}"
    );
}

#[test]
fn a_scale_of_the_fifth_and_the_fourth_is_refused_as_one_direction() {
    // Two intervals, and still no plane: they sum to the octave, so the second
    // axis lies along the first and the two triangles of every cell would be the
    // same three pitches. The trap a harmonic spectrum lays, since these are the
    // two deepest minima any voice has.
    let refused = Lattice::from_tuning(&scale(&[(498.0, 0.4), (702.0, 0.5)]))
        .expect_err("a plane from the fifth and the fourth");
    assert_eq!(
        refused,
        NoPlane::NoIndependentPair {
            first: 702.0,
            rejected: vec![498.0]
        }
    );
    let said = refused.to_string();
    assert!(said.contains("702") && said.contains("498"), "{said}");
}

#[test]
fn a_scale_of_nothing_but_the_tonic_and_the_octave_is_refused_too() {
    let refused = Lattice::from_tuning(&scale(&[])).expect_err("a plane from a point");
    assert_eq!(
        refused,
        NoPlane::TooFewIntervals {
            interior: Vec::new()
        }
    );
}

#[test]
fn the_two_halves_of_a_cell_are_different_chords() {
    // The degeneracy a voice's own spectrum lays a trap for. Its two deepest
    // minima are the fifth and the fourth, which sum to the octave — and a
    // lattice spanned by both has `(1,1)` back on the tonic, so the upward and
    // downward triangles of every cell are the same three pitches. It would look
    // two-dimensional and have half its moves do nothing.
    let lattice = Lattice::from_tuning(&harmonic()).unwrap();
    let pitches = |t: Triangle| {
        let mut cents: Vec<i32> = t
            .corners()
            .iter()
            .map(|(x, y)| lattice.pitch_class(*x, *y).round() as i32)
            .collect();
        cents.sort_unstable();
        cents
    };

    let up = pitches(Triangle {
        x: 0,
        y: 0,
        up: true,
    });
    let down = pitches(Triangle {
        x: 0,
        y: 0,
        up: false,
    });
    assert_ne!(up, down, "both halves of a cell are the same chord");

    // ...and neither has two voices sitting on one pitch, which is the other way
    // an axis can fail to add a dimension.
    assert_eq!(up.len(), 3);
    assert!(
        up[0] != up[1] && up[1] != up[2],
        "a doubled pitch in {up:?}"
    );
}

#[test]
fn neighbouring_triangles_share_two_of_their_three_pitches() {
    // The claim the whole mapping rests on: moving to an adjacent chord holds
    // two voices still and steps one. Nothing enforces that — it is what
    // adjacency on this lattice is.
    let up = Triangle {
        x: 0,
        y: 0,
        up: true,
    };
    let down = Triangle {
        x: 0,
        y: 0,
        up: false,
    };
    assert_eq!(up.shared_with(&down), 2, "the two halves of one cell");

    let next = Triangle {
        x: 1,
        y: 0,
        up: true,
    };
    assert_eq!(down.shared_with(&next), 2, "across a cell boundary");
}

#[test]
fn a_position_is_in_the_triangle_it_is_in() {
    assert_eq!(
        triangle_at(0.2, 0.2),
        Triangle {
            x: 0,
            y: 0,
            up: true
        }
    );
    assert_eq!(
        triangle_at(0.8, 0.8),
        Triangle {
            x: 0,
            y: 0,
            up: false
        }
    );
    assert_eq!(
        triangle_at(-0.8, 0.2),
        Triangle {
            x: -1,
            y: 0,
            up: true
        }
    );
}

#[test]
fn holding_keeps_a_chord_through_a_wobble_and_yields_to_a_move() {
    // What makes a chord ring: a formant estimate that jitters across a boundary
    // must not change the harmony, and a mouth that genuinely goes somewhere
    // must.
    let start = triangle_at(0.2, 0.2);
    let wobbled = settle(start, 0.45, 0.45, 0.5);
    assert_eq!(
        wobbled, start,
        "a wobble across the diagonal moved the chord"
    );

    let moved = settle(start, 1.6, 0.2, 0.5);
    assert_ne!(
        moved, start,
        "a whole cell of travel did not move the chord"
    );
}

#[test]
fn holding_at_zero_follows_every_boundary() {
    let start = triangle_at(0.2, 0.2);
    assert_eq!(settle(start, 0.45, 0.45, 0.0), triangle_at(0.45, 0.45));
}

#[test]
fn a_triangle_is_judged_by_its_worst_interval_not_its_best() {
    // The real scale this was found on, measured from a held *ah*, with its two
    // deepest minima at their measured depths. Their difference is 182 cents,
    // which is not a degree, is not near one, and sits close to where the
    // roughness curve peaks — so spanning the lattice by the deepest pair put a
    // whole-tone clash inside *every chord the mapping could play*, and a
    // sixteen-cent tuning question was being asked underneath it.
    let measured = scale(&[
        (316.0, 0.09),
        (386.0, 0.12),
        (582.0, 0.05),
        (702.0, 0.138),
        (813.0, 0.06),
        (884.0, 0.155),
    ]);

    let (a, b) = generators(&measured).expect("this scale spans a plane");

    // Inversions count as the same interval, as they must: these are pitch
    // classes and which octave each voice takes is decided later, so a fourth
    // in the lattice can sound as a fifth in the chord.
    let fold = |cents: f32| {
        let wrapped: f32 = cents.rem_euclid(1200.0);
        wrapped.min(1200.0 - wrapped)
    };
    let is_degree = |cents: f32| {
        measured
            .degrees
            .iter()
            .any(|d| (fold(d.cents) - fold(cents)).abs() <= 50.0)
    };

    // The property that matters: all three of the triangle's intervals are
    // things this speaker's spectrum rests on, not just the two axes.
    for interval in [a.cents, b.cents, a.cents - b.cents] {
        assert!(
            is_degree(interval),
            "{:.0} cents is in no degree of the scale",
            fold(interval)
        );
    }

    // And the deepest pair is specifically *not* what comes out, which is the
    // whole change: 884 and 702 are the two deepest minima here and their
    // difference is 182, which is nothing.
    let deepest_pair = (a.cents - 884.0).abs() < 1.0 && (b.cents - 702.0).abs() < 1.0;
    assert!(
        !deepest_pair,
        "picked the deepest pair despite its 182-cent difference"
    );
    assert!(
        !is_degree(182.0),
        "the fixture no longer demonstrates the problem"
    );
}

#[test]
fn a_scale_whose_intervals_never_agree_still_gets_a_lattice() {
    // No pair here has a consonant difference: 100 and 550 differ by 450, 100
    // and 700 by 600, 550 and 700 by 150, and none of those is a degree. A
    // lattice with a rough interval in every chord is worse than one without
    // and better than no mapping at all, so it is taken rather than refused —
    // the alternative is a speaker for whom the Tonnetz silently disappears.
    let awkward = scale(&[(100.0, 0.10), (550.0, 0.08), (700.0, 0.12)]);
    let (a, b) = generators(&awkward).expect("a lattice is still spanned");
    assert_ne!(a.cents, b.cents);
}
