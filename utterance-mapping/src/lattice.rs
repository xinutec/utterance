//! A harmonic lattice with both of its axes read out of the speaker's spectrum.
//!
//! The classical Tonnetz is a plane of pitches spanned by two intervals — the
//! fifth one way, the major third the other — in which every triangle is a triad
//! and adjacent triangles share two of their three notes. That adjacency is the
//! useful part: it makes *related* a geometric fact rather than a stylistic one,
//! and voice leading falls out of moving between neighbours instead of being
//! composed.
//!
//! **The axes here are derived, not assumed.** 3:2 and 5:4 are the two intervals
//! a harmonic spectrum makes least rough, which is *why* western harmony settled
//! on them — so taking them as given would be assuming the conclusion this
//! project exists to re-derive. Instead the generators are the two deepest
//! minima of the speaker's own dissonance curve, subject to being independent of
//! each other. For a voice they will land near the familiar pair, which is a
//! result; for a bell they would not, and neither would the harmony built on
//! them.
//!
//! **Why independence has to be checked.** A harmonic spectrum's two deepest
//! minima are the fifth and the fourth, and those sum to the octave — so the
//! obvious pair spans a plane that is secretly a line, where the two triangles
//! of every cell are the same three pitches and half of all harmonic motion does
//! nothing. Taking the deepest two would walk into that every time. A scale
//! offering no second independent interval genuinely cannot be laid out this
//! way, and the honest answer is to say so rather than to fold the plane flat
//! and pretend.

use std::fmt;

use crate::tuning::{Degree, Tuning};

/// How near two intervals may be before they count as the same one, in cents.
///
/// A quarter-tone. Wide enough that a generator and a slightly-mistuned copy of
/// it are recognised as one interval rather than two; narrow enough to keep
/// genuinely distinct neighbouring degrees apart.
const SAME_INTERVAL_CENTS: f32 = 50.0;

/// Why a scale spans no lattice.
///
/// A reason rather than an absence, because of how the refusal presents. What
/// reaches a listener when this mapping declines is a player that makes
/// consonants and silence, and *nothing happened* is indistinguishable from a
/// broken build. The scale is usually fine and one knob has pruned it too hard —
/// which is recoverable, and only if someone is told.
#[derive(Clone, Debug, PartialEq)]
pub enum NoPlane {
    /// Fewer than two intervals to choose axes from. One is a line; none is a
    /// point.
    TooFewIntervals {
        /// The interior degrees there were, in cents.
        interior: Vec<f32>,
    },
    /// Intervals enough, but every one of them is the deepest one over again or
    /// its complement, so the second axis would lie along the first.
    NoIndependentPair {
        /// The deepest interval, which would have been the first axis.
        first: f32,
        /// The ones tried against it and refused, in cents.
        rejected: Vec<f32>,
    },
}

impl fmt::Display for NoPlane {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Every message names the density knob, because it is the one that
        // caused this in every case seen so far and the only one that undoes it.
        match self {
            NoPlane::TooFewIntervals { interior } => write!(
                f,
                "this voice's scale has {} besides the tonic and the octave, and a \
                 lattice is spanned by two intervals pointing different ways. \
                 Lowering the scale density keeps more of them.",
                match interior.as_slice() {
                    [] => "nothing".to_string(),
                    [one] => format!("one interval ({})", cents_list(&[*one])),
                    many => format!("only {} intervals", many.len()),
                }
            ),
            NoPlane::NoIndependentPair { first, rejected } => write!(
                f,
                "this voice's scale points one way only: beside {}, every interval \
                 in it ({}) is that same interval again or the rest of the octave \
                 after it, so both axes would lie along one line. Lowering the \
                 scale density keeps more of them.",
                cents_list(&[*first]),
                cents_list(rejected)
            ),
        }
    }
}

impl std::error::Error for NoPlane {}

/// Intervals as someone reads them, in the unit the rest of the UI uses.
fn cents_list(cents: &[f32]) -> String {
    cents
        .iter()
        .map(|c| format!("{c:.0}¢"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Two independent intervals, and the plane they span.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lattice {
    /// The first axis, in cents. The deepest minimum in the speaker's curve.
    pub a_cents: f32,
    /// The second axis, in cents. The deepest one independent of the first.
    pub b_cents: f32,
}

impl Lattice {
    /// Lay a lattice out over a derived scale.
    ///
    /// Fails when the scale has no two independent interior degrees — one steady
    /// *ee* in the calibration set gave a scale of the fifth and nothing else,
    /// and there is no plane to be had from that. The failure carries its reason
    /// rather than being an empty answer, for the reason on [`NoPlane`].
    pub fn from_tuning(tuning: &Tuning) -> Result<Self, NoPlane> {
        let (a, b) = generators(tuning)?;
        Ok(Lattice {
            a_cents: a.cents,
            b_cents: b.cents,
        })
    }

    /// Where a lattice point sits above the tonic, in cents, before folding.
    pub fn cents(&self, x: i32, y: i32) -> f32 {
        x as f32 * self.a_cents + y as f32 * self.b_cents
    }

    /// The pitch class of a lattice point: its position within one octave.
    ///
    /// Octave equivalence is what makes the lattice finite to the ear — a point
    /// four fifths out is a major third away from the tonic and heard as one,
    /// whatever register it is written in. Registration is a separate decision,
    /// taken where the chord is voiced.
    pub fn pitch_class(&self, x: i32, y: i32) -> f32 {
        let c = self.cents(x, y).rem_euclid(1200.0);
        // `rem_euclid` on a negative value near zero can return exactly 1200.0
        // once rounded, which would place a tonic an octave up.
        if c >= 1200.0 { 0.0 } else { c }
    }
}

/// Whether an interval is one this speaker's spectrum rests on.
///
/// Folded into the octave and compared against the scale, within the same
/// quarter-tone that decides whether two intervals are the same one.
fn is_consonance(candidates: &[Degree], cents: f32) -> bool {
    let wrapped = cents.rem_euclid(1200.0);
    let interval = wrapped.min(1200.0 - wrapped);
    candidates
        .iter()
        .any(|d| (d.cents.min(1200.0 - d.cents) - interval).abs() <= SAME_INTERVAL_CENTS)
}

/// The two intervals a scale is spanned by.
///
/// Depth rather than shallowness of the minimum: `Degree::depth` is how far the
/// roughness curve climbs either side before turning back down, which is the
/// measure of how firmly a note is somewhere a listener rests.
///
/// **A triangle has three intervals and the third is `a - b`, which the scale
/// never measured.** The roughness curve is swept as one spectrum against a
/// shifted copy of itself, so every degree is a good interval *from the tonic*
/// and nothing in it says how two degrees sound against each other. Picking the
/// two deepest and stopping — which this did until 2026-07-29 — gave this
/// speaker axes of 884 and 702 and so put **182 cents inside every chord the
/// mapping has ever played**: not a degree, not near one, and close to where the
/// roughness curve peaks. A tuning difference of sixteen cents was being looked
/// for underneath a whole-tone clash.
///
/// So a pair is judged by its worst interval rather than its best, because a
/// chord is as rough as the roughest thing in it. Pairs whose difference is also
/// a consonance are preferred, and among those the one whose *shallowest* of the
/// three minima is deepest.
///
/// On the voice this was found with, that changes the answer from a major sixth
/// and a fifth to **a fifth and a major third** — the classical Tonnetz, arrived
/// at from one speaker's spectrum rather than assumed. Its triangles are then
/// just major and minor triads whose every internal interval is a degree of the
/// speaker's own scale.
///
/// **Falls back to the deepest independent pair** when no pair has a consonant
/// difference, rather than refusing: a lattice with a rough interval in every
/// chord is worse than one without, and still better than no mapping at all.
/// That case is worth knowing about, so it is what `NoPlane` would report if the
/// fallback also fails.
pub fn generators(tuning: &Tuning) -> Result<(Degree, Degree), NoPlane> {
    // Interior degrees only. The tonic and the octave are degrees by decision
    // rather than by measurement and carry a depth of zero, and neither spans
    // anything: an axis of 0 cents is a point and one of 1200 is the octave the
    // pitch classes already fold into.
    let mut candidates: Vec<Degree> = tuning
        .degrees
        .iter()
        .copied()
        .filter(|d| d.depth > 0.0 && d.cents > SAME_INTERVAL_CENTS)
        .filter(|d| d.cents < 1200.0 - SAME_INTERVAL_CENTS)
        .collect();
    candidates.sort_by(|p, q| {
        q.depth
            .total_cmp(&p.depth)
            // Deterministic where two minima are equally deep, which happens on
            // a synthetic spectrum far more often than on a voice.
            .then(p.cents.total_cmp(&q.cents))
    });

    let interior = |from: usize| candidates[from..].iter().map(|d| d.cents).collect();
    let Some(&a) = candidates.first() else {
        return Err(NoPlane::TooFewIntervals {
            interior: Vec::new(),
        });
    };
    if candidates.len() < 2 {
        return Err(NoPlane::TooFewIntervals {
            interior: interior(0),
        });
    }
    // Every independent pair, judged by the shallowest of its three intervals —
    // the two axes and the difference between them. Whole pairs are searched
    // rather than the deepest axis being fixed first, because the best triangle
    // is not always built on the best single interval.
    let mut best: Option<(Degree, Degree, f32)> = None;
    for (i, &p) in candidates.iter().enumerate() {
        for &q in candidates.iter().skip(i + 1) {
            if !independent(p, q) {
                continue;
            }
            if !is_consonance(&candidates, p.cents - q.cents) {
                continue;
            }
            // The difference's own depth is not read from the table — it may be
            // a degree the sweep found at a slightly different place — so the
            // pair is scored on the two axes and admitted on the third.
            let worst = p.depth.min(q.depth);
            if best.is_none_or(|(.., b)| worst > b) {
                best = Some((p, q, worst));
            }
        }
    }
    if let Some((p, q, _)) = best {
        return Ok((p, q));
    }

    // Nothing had a consonant difference. Every chord this lattice builds will
    // carry an interval the speaker's spectrum finds rough; that is a real loss
    // and it is still a mapping, so it is taken rather than refused.
    match candidates.iter().skip(1).find(|d| independent(a, **d)) {
        Some(&b) => Ok((a, b)),
        // Every one of them was the first axis wearing a different name. The
        // pair that does this to a voice is the fifth and the fourth, which are
        // its two deepest minima and which sum to the octave.
        None => Err(NoPlane::NoIndependentPair {
            first: a.cents,
            rejected: interior(1),
        }),
    }
}

/// Whether a second interval spans a direction the first does not.
///
/// Two conditions, each naming a chord that would come out wrong. The axes must
/// differ, or a triangle has two of its three voices on one pitch. And they must
/// not sum to the octave, or the two triangles of a cell are the *same* chord —
/// which is the trap a harmonic spectrum lays, because the fifth and the fourth
/// are the two deepest minima any voice has and they add up to 1200 cents. A
/// lattice spanned by both looks two-dimensional and has half its moves do
/// nothing.
///
/// Deliberately local. A plane spanned by two intervals will eventually fold
/// onto itself somewhere — three of a just major third is 42 cents shy of the
/// octave, twelve fifths a comma over — and testing far enough out rejects every
/// pair there is. Those folds are the ordinary ambiguities of just intonation
/// rather than defects, and they are further away than a vowel ever walks.
fn independent(a: Degree, b: Degree) -> bool {
    let apart = |cents: f32| {
        let wrapped = cents.rem_euclid(1200.0);
        wrapped.min(1200.0 - wrapped)
    };
    apart(b.cents - a.cents) > SAME_INTERVAL_CENTS && apart(a.cents + b.cents) > SAME_INTERVAL_CENTS
}

/// A triangle of the lattice: three mutually adjacent points.
///
/// The unit of harmony here, in the same way a triad is the unit of the
/// classical Tonnetz. Two triangles sharing an edge share two of their three
/// pitches, so a move between neighbours holds two voices still and steps the
/// third — voice leading as a consequence of the geometry rather than a rule
/// applied on top of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Triangle {
    /// The lattice point at the triangle's lower-left corner.
    pub x: i32,
    pub y: i32,
    /// Whether this is the upward triangle of the cell or the downward one.
    ///
    /// A parallelogram cell of the lattice splits into two, and they are
    /// different chords: the upward one is `(x,y) (x+1,y) (x,y+1)`, the downward
    /// one `(x+1,y) (x,y+1) (x+1,y+1)`. On a just lattice those are the major
    /// and minor triads on the same pair of axes, which is the closest relation
    /// two chords have and the shortest move anything here can make.
    pub up: bool,
}

impl Triangle {
    /// The three lattice points, as offsets from the tonic.
    pub fn corners(&self) -> [(i32, i32); 3] {
        if self.up {
            [(self.x, self.y), (self.x + 1, self.y), (self.x, self.y + 1)]
        } else {
            [
                (self.x + 1, self.y),
                (self.x, self.y + 1),
                (self.x + 1, self.y + 1),
            ]
        }
    }

    /// Points around this triangle, nearest first, for more voices than three.
    ///
    /// The triangle's own corners come first, then the points that complete the
    /// cell and the cells touching it, ordered by how far they sit from the
    /// triangle's middle. Extra voices thicken the chord outward from what it
    /// already is rather than starting a second one somewhere else.
    ///
    pub fn ring(&self, wanted: usize) -> Vec<(i32, i32)> {
        let corners = self.corners();
        let centre = (
            corners.iter().map(|c| c.0 as f32).sum::<f32>() / 3.0,
            corners.iter().map(|c| c.1 as f32).sum::<f32>() / 3.0,
        );
        let mut points: Vec<(i32, i32)> = corners.to_vec();
        let mut extra: Vec<(i32, i32)> = (self.x - 2..=self.x + 3)
            .flat_map(|x| (self.y - 2..=self.y + 3).map(move |y| (x, y)))
            .filter(|p| !corners.contains(p))
            .collect();
        extra.sort_by(|p, q| {
            let d =
                |c: &(i32, i32)| (c.0 as f32 - centre.0).powi(2) + (c.1 as f32 - centre.1).powi(2);
            // Distance decides; the coordinates break ties, because a lattice is
            // full of points equally far from a centre and the chord must not
            // depend on which order they were generated in.
            d(p).total_cmp(&d(q)).then(p.cmp(q))
        });
        points.extend(extra);
        points.truncate(wanted.max(1));
        points
    }

    /// How many points this triangle shares with another.
    ///
    /// The measure of how small a move between them is: three is the same chord,
    /// two is a step to a neighbour, zero is a jump.
    pub fn shared_with(&self, other: &Triangle) -> usize {
        let theirs = other.corners();
        self.corners().iter().filter(|c| theirs.contains(c)).count()
    }
}

/// Which triangle a continuous position falls in.
///
/// The plane is cut into cells by the integer grid and each cell across its
/// diagonal, so the answer is the whole-number part of the position plus which
/// side of the diagonal the fraction lands on.
pub fn triangle_at(x: f32, y: f32) -> Triangle {
    let (cx, cy) = (x.floor(), y.floor());
    Triangle {
        x: cx as i32,
        y: cy as i32,
        up: (x - cx) + (y - cy) < 1.0,
    }
}

/// The triangle a position falls in, unless it is barely past the boundary.
///
/// **What makes a chord ring long enough to have a tuning.** Without this the
/// harmony changes the instant a formant estimate wobbles across a line, which
/// is several times a second on real speech — and a chord that never holds still
/// for a second cannot be heard as being in one tuning rather than another. That
/// is measured rather than supposed: `docs/roadmap.md` records the derived scale
/// as real and currently inaudible for exactly this reason.
///
/// `hold` is how far past a boundary the mouth must travel before the harmony
/// follows, as a fraction of a cell. At 0 this is [`triangle_at`]; at 1 the
/// voice must cross a whole further cell to commit, so the chord holds through
/// anything short of a deliberate move.
pub fn settle(previous: Triangle, x: f32, y: f32, hold: f32) -> Triangle {
    let candidate = triangle_at(x, y);
    if candidate == previous {
        return previous;
    }
    let margin = hold.clamp(0.0, 1.0);
    if margin <= 0.0 {
        return candidate;
    }

    // Distance from the position to the nearest point still inside the triangle
    // it is leaving. Staying while that distance is small is what makes the
    // boundary sticky rather than making it move, so a mouth that keeps going
    // still changes the chord and only a mouth that hovers does not.
    if depth_inside(previous, x, y) < margin {
        return previous;
    }
    candidate
}

/// How far outside a triangle a position has strayed, in cells.
fn depth_inside(t: Triangle, x: f32, y: f32) -> f32 {
    let (fx, fy) = (x - t.x as f32, y - t.y as f32);
    // Each triangle is the intersection of three half-planes; how far outside
    // the triangle a point is, is how far it is past the worst of them.
    let diagonal = if t.up {
        1.0 - (fx + fy)
    } else {
        (fx + fy) - 1.0
    };
    let outside = [fx, fy, 1.0 - fx, 1.0 - fy, diagonal]
        .into_iter()
        .fold(f32::INFINITY, f32::min);
    (-outside).max(0.0)
}
