//! The harmonic lattice, checked against the claims its module makes.
//!
//! Two of them can be tested and both matter. That the axes are *read* from the
//! spectrum rather than assumed — the same test the tuning has, for the same
//! reason: a derivation that restates its own assumptions is worth nothing. And
//! that adjacency on the lattice really is the near relation it is claimed to
//! be, since every musical consequence here rests on that geometry.

use music_mapping::dissonance::Component;
use music_mapping::lattice::{Lattice, Triangle, generators, settle, triangle_at};
use music_mapping::tuning::{self, Tuning};

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

#[test]
fn a_scale_with_one_interval_spans_no_plane() {
    // A steady *ee* in the calibration set gave the fifth and nothing else. That
    // is a line, and saying so beats folding the plane flat and letting one of
    // the two vowel dimensions reach nothing.
    let line = Tuning {
        degrees: vec![
            tuning::from_spectrum(&[
                Component {
                    hz: 100.0,
                    amplitude: 1.0,
                },
                Component {
                    hz: 200.0,
                    amplitude: 1.0,
                },
            ])
            .unwrap()
            .degrees[0],
        ],
        curve: vec![0.0; tuning::RESOLUTION + 1],
    };
    assert!(Lattice::from_tuning(&line).is_none());
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
