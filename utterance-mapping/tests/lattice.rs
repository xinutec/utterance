//! The harmonic lattice, checked against its module's two testable claims: the
//! axes are read from the spectrum rather than assumed, and adjacency is the
//! near relation every musical consequence rests on.

use utterance_mapping::dissonance::Component;
use utterance_mapping::lattice::{
    Lattice, NoPlane, Triangle, Walk, generators, settle, triangle_at,
};
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

/// Partials at `k^1.4`, as a stiff bar has: if this lattice still comes out a
/// fifth and a third, the derivation is decoration.
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
    // Deep minima of this spectrum; that they are near the familiar pair is the
    // result.
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
    // The fifth, derived back from the spectrum rather than assumed.
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

/// A scale built by hand, as interior `(cents, depth)`; tonic and octave are
/// added as `tuning` adds them.
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
    // A scale of the fifth alone is a line; saying so beats folding it flat.
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
    // The message must name the knob and the intervals, so it can be checked
    // against the scale on screen.
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
    // Two intervals that sum to the octave: the second axis lies along the first.
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
    // A voice's two deepest minima, the fifth and the fourth, sum to the octave:
    // `(1,1)` lands on the tonic and both triangles of a cell are one chord.
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

    // ...and no triangle doubles a pitch.
    assert_eq!(up.len(), 3);
    assert!(
        up[0] != up[1] && up[1] != up[2],
        "a doubled pitch in {up:?}"
    );
}

#[test]
fn neighbouring_triangles_share_two_of_their_three_pitches() {
    // Moving to an adjacent chord holds two voices and steps one.
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
    // Jitter across a boundary must not change the harmony; a real move must.
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

/// Frames of settle used by the walk tests (frames, since [`Walk`] counts them).
const DWELL: usize = 5;

#[test]
fn a_chord_survives_a_departure_that_comes_straight_back() {
    // The artifact: the mouth really crossed — so `hold` lets go — and came back
    // two frames later.
    let mut walk = Walk::start(0.2, 0.2);
    let home = walk.step(0.2, 0.2, 0.0, DWELL);

    // Well past the boundary, so `hold` at any setting would have yielded.
    for _ in 0..DWELL - 1 {
        assert_eq!(
            walk.step(1.5, 0.2, 0.0, DWELL),
            home,
            "the chord followed a departure shorter than the settle time"
        );
    }
    assert_eq!(
        walk.step(0.2, 0.2, 0.0, DWELL),
        home,
        "coming back did not restore the chord"
    );
}

#[test]
fn a_departure_that_lasts_moves_the_chord() {
    // A delay, not a lockout: a flicker is refused, a move is not.
    let mut walk = Walk::start(0.2, 0.2);
    let home = walk.step(0.2, 0.2, 0.0, DWELL);

    let mut moved = home;
    for _ in 0..DWELL {
        moved = walk.step(1.5, 0.2, 0.0, DWELL);
    }
    assert_ne!(moved, home, "a sustained move never committed");
    assert_eq!(moved, triangle_at(1.5, 0.2), "committed to the wrong cell");
}

#[test]
fn the_count_restarts_when_the_mouth_comes_home() {
    // Consecutive frames, not a total, or dips would accumulate into a move.
    let mut walk = Walk::start(0.2, 0.2);
    let home = walk.step(0.2, 0.2, 0.0, DWELL);

    for _ in 0..DWELL * 3 {
        // Asserted during the departure too, or a walk with no clock would pass.
        assert_eq!(
            walk.step(1.5, 0.2, 0.0, DWELL),
            home,
            "one frame away was enough to move the chord"
        );
        assert_eq!(
            walk.step(0.2, 0.2, 0.0, DWELL),
            home,
            "single-frame departures added up to a chord change"
        );
    }
}

#[test]
fn a_glide_keeps_moving_rather_than_freezing() {
    // Counting departures, not rests in one triangle: a glide rests nowhere, and
    // the harmony must follow it rather than freeze.
    let mut walk = Walk::start(0.0, 0.2);
    let start = walk.step(0.0, 0.2, 0.0, DWELL);

    // Fast enough to rest in no triangle for `DWELL` frames, or both designs
    // would commit.
    let mut seen: Vec<Triangle> = Vec::new();
    for frame in 0..40 {
        let x = frame as f32 * 0.3;
        let here = walk.step(x, 0.2, 0.0, DWELL);
        if !seen.contains(&here) {
            seen.push(here);
        }
    }
    assert!(
        seen.len() > 3,
        "a glide across twelve cells settled on {} triangles",
        seen.len()
    );
    assert!(seen.contains(&start), "the walk skipped where it began");
}

#[test]
fn no_settle_time_is_the_walk_that_has_no_clock_in_it() {
    // The default changes nothing: 0 and 1 frame both commit at once.
    for frames in [0, 1] {
        let mut walk = Walk::start(0.2, 0.2);
        walk.step(0.2, 0.2, 0.5, frames);
        assert_eq!(
            walk.step(0.45, 0.45, 0.5, frames),
            settle(triangle_at(0.2, 0.2), 0.45, 0.45, 0.5),
            "settle over {frames} frames disagreed with the spatial rule alone"
        );
        assert_eq!(
            walk.step(1.6, 0.2, 0.5, frames),
            triangle_at(1.6, 0.2),
            "settle over {frames} frames refused a move the spatial rule allows"
        );
    }
}

#[test]
fn a_triangle_is_judged_by_its_worst_interval_not_its_best() {
    // A real scale from a held *ah*: its two deepest minima, 884 and 702, differ
    // by 182 cents — near the roughness peak, in every chord.
    let measured = scale(&[
        (316.0, 0.09),
        (386.0, 0.12),
        (582.0, 0.05),
        (702.0, 0.138),
        (813.0, 0.06),
        (884.0, 0.155),
    ]);

    let (a, b) = generators(&measured).expect("this scale spans a plane");

    // Inversions count as the same interval: octave placement comes later.
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

    // All three of the triangle's intervals are consonant, not just the axes.
    for interval in [a.cents, b.cents, a.cents - b.cents] {
        assert!(
            is_degree(interval),
            "{:.0} cents is in no degree of the scale",
            fold(interval)
        );
    }

    // And the deepest pair is specifically *not* chosen.
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
    // No pair has a consonant difference (450, 600, 150 are not degrees): the
    // rough lattice is taken rather than the mapping vanishing.
    let awkward = scale(&[(100.0, 0.10), (550.0, 0.08), (700.0, 0.12)]);
    let (a, b) = generators(&awkward).expect("a lattice is still spanned");
    assert_ne!(a.cents, b.cents);
}

#[test]
fn a_point_a_hair_below_the_tonic_folds_to_the_tonic() {
    // A pitch class of 1200 would sound the tonic an octave up. `rem_euclid` of a
    // tiny negative rounds to exactly 1200.0 in f32, so two nearly cancelling
    // axes reach it.
    let cancelling = Lattice {
        a_cents: 1e-5,
        b_cents: -2e-5,
    };
    assert!(
        cancelling.cents(1, 1) < 0.0,
        "the fixture is meant to sit just below the tonic, not on or above it"
    );
    assert_eq!(
        cancelling.pitch_class(1, 1),
        0.0,
        "a point a hair below the tonic came back as a whole octave above it"
    );

    // The general statement, over a lattice built the way a real one is.
    let real = Lattice::from_tuning(&harmonic()).expect("a lattice");
    for x in -12..=12 {
        for y in -12..=12 {
            let pc = real.pitch_class(x, y);
            assert!(
                (0.0..1200.0).contains(&pc),
                "({x}, {y}) has a pitch class of {pc}, outside the octave it is defined in"
            );
        }
    }
}
