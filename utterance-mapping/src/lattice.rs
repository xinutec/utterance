//! A harmonic lattice with both of its axes read out of the speaker's spectrum.
//!
//! The classical Tonnetz is a plane of pitches spanned by the fifth and the
//! major third, where every triangle is a triad and neighbouring triangles share
//! two notes — so voice leading falls out of moving between neighbours.
//!
//! **The axes are derived, not assumed.** 3:2 and 5:4 are what a harmonic
//! spectrum makes least rough; taking them as given would assume what this
//! project re-derives. The generators are minima of the speaker's own roughness
//! curve. They must be independent: a voice's two deepest minima, the fifth and
//! the fourth, sum to the octave and span a plane that is secretly a line. A
//! scale with no independent pair is refused rather than folded flat.

use std::fmt;

use crate::tuning::{Degree, Tuning};

/// How near two intervals may be before they count as the same one, in cents:
/// a quarter-tone.
const SAME_INTERVAL_CENTS: f32 = 50.0;

/// Why a scale spans no lattice. A reason rather than an absence: a declining
/// mapping otherwise sounds like consonants over silence, indistinguishable from
/// a broken build, when usually one knob has pruned the scale too hard.
#[derive(Clone, Debug, PartialEq)]
pub enum NoPlane {
    /// Fewer than two intervals to choose axes from.
    TooFewIntervals {
        /// The interior degrees there were, in cents.
        interior: Vec<f32>,
    },
    /// Every interval is the first one again or its complement, so the second
    /// axis would lie along the first.
    NoIndependentPair {
        /// The deepest interval, which would have been the first axis.
        first: f32,
        /// The ones tried against it and refused, in cents.
        rejected: Vec<f32>,
    },
}

impl fmt::Display for NoPlane {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Every message names the density knob: lowering it undoes this.
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

/// Two independent intervals, and the plane they span. See [`generators`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lattice {
    /// The first axis, in cents.
    pub a_cents: f32,
    /// The second axis, in cents.
    pub b_cents: f32,
}

impl Lattice {
    /// Lay a lattice out over a derived scale, or say why it has no plane.
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
    /// Which octave it sounds in is decided where the chord is voiced.
    pub fn pitch_class(&self, x: i32, y: i32) -> f32 {
        let c = self.cents(x, y).rem_euclid(1200.0);
        // `rem_euclid` of a tiny negative can round to exactly 1200.0.
        if c >= 1200.0 { 0.0 } else { c }
    }
}

/// Whether an interval, folded into the octave, is one of the scale's degrees
/// to within [`SAME_INTERVAL_CENTS`].
fn is_consonance(candidates: &[Degree], cents: f32) -> bool {
    let wrapped = cents.rem_euclid(1200.0);
    let interval = wrapped.min(1200.0 - wrapped);
    candidates
        .iter()
        .any(|d| (d.cents.min(1200.0 - d.cents) - interval).abs() <= SAME_INTERVAL_CENTS)
}

/// The two intervals a scale is spanned by.
///
/// ⚠ **A triangle has three intervals, and the third, `a - b`, was never
/// measured**: the roughness curve rates each degree against the tonic only.
/// The two deepest minima can put a rough interval inside every chord (axes of
/// 884 and 702 give 182 cents). So a pair is admitted only if its difference is
/// also a consonance, and the one whose shallower axis is deepest wins. On a real
/// voice that gives a fifth and a third: the classical Tonnetz, derived.
///
/// Falls back to the deepest independent pair when no pair qualifies — a rough
/// interval in every chord beats no mapping. [`NoPlane`] when even that fails.
pub fn generators(tuning: &Tuning) -> Result<(Degree, Degree), NoPlane> {
    // Interior degrees only: the tonic and the octave span nothing.
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
            // Ties broken by pitch, so the answer is deterministic.
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
    // Whole pairs are searched, because the best triangle is not always built
    // on the best single interval.
    let mut best: Option<(Degree, Degree, f32)> = None;
    for (i, &p) in candidates.iter().enumerate() {
        for &q in candidates.iter().skip(i + 1) {
            if !independent(p, q) {
                continue;
            }
            if !is_consonance(&candidates, p.cents - q.cents) {
                continue;
            }
            // The difference may sit slightly off a measured degree, so it is
            // only checked, and the pair is scored on its two axes.
            let worst = p.depth.min(q.depth);
            if best.is_none_or(|(.., b)| worst > b) {
                best = Some((p, q, worst));
            }
        }
    }
    if let Some((p, q, _)) = best {
        return Ok((p, q));
    }

    // No pair has a consonant difference: take the rough lattice over none.
    match candidates.iter().skip(1).find(|d| independent(a, **d)) {
        Some(&b) => Ok((a, b)),
        // Every one was the first axis again — the fifth-and-fourth trap.
        None => Err(NoPlane::NoIndependentPair {
            first: a.cents,
            rejected: interior(1),
        }),
    }
}

/// Whether a second interval spans a direction the first does not: the axes
/// must differ (or a triangle doubles a pitch) and must not sum to the octave (or
/// both triangles of a cell are the same chord).
///
/// Deliberately local. Any plane folds onto itself somewhere far out — three
/// just major thirds fall 42 cents short of the octave — and those folds are the
/// ordinary ambiguities of just intonation, further than a vowel walks.
fn independent(a: Degree, b: Degree) -> bool {
    let apart = |cents: f32| {
        let wrapped = cents.rem_euclid(1200.0);
        wrapped.min(1200.0 - wrapped)
    };
    apart(b.cents - a.cents) > SAME_INTERVAL_CENTS && apart(a.cents + b.cents) > SAME_INTERVAL_CENTS
}

/// A triangle of the lattice: three mutually adjacent points, the unit of
/// harmony. Two triangles sharing an edge share two pitches, so a move between
/// them holds two voices and steps one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Triangle {
    /// The lattice point at the triangle's lower-left corner.
    pub x: i32,
    pub y: i32,
    /// Whether this is the upward triangle of the cell, `(x,y) (x+1,y) (x,y+1)`,
    /// or the downward one, `(x+1,y) (x,y+1) (x+1,y+1)` — on a just lattice, the
    /// major and minor triads on the same axes.
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

    /// Points around this triangle, nearest its middle first: its corners, then
    /// outward, so extra voices thicken the chord it already is.
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
            // Coordinates break distance ties, so the chord is deterministic.
            d(p).total_cmp(&d(q)).then(p.cmp(q))
        });
        points.extend(extra);
        points.truncate(wanted.max(1));
        points
    }

    /// How many points this triangle shares with another: three is the same
    /// chord, two a step to a neighbour, zero a jump.
    pub fn shared_with(&self, other: &Triangle) -> usize {
        let theirs = other.corners();
        self.corners().iter().filter(|c| theirs.contains(c)).count()
    }
}

/// Which triangle a continuous position falls in: the integer grid cuts the
/// cells, and the diagonal splits each.
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
/// Without this the harmony changes whenever a formant estimate wobbles across
/// a line — several times a second on speech — and no chord rings long enough to
/// be heard in a tuning. `hold` is how far past the boundary the mouth must go,
/// as a fraction of a cell: 0 is [`triangle_at`], 1 a whole further cell.
pub fn settle(previous: Triangle, x: f32, y: f32, hold: f32) -> Triangle {
    let candidate = triangle_at(x, y);
    if candidate == previous {
        return previous;
    }
    let margin = hold.clamp(0.0, 1.0);
    if margin <= 0.0 {
        return candidate;
    }

    // Stay while the position is within `margin` of the triangle it is leaving,
    // so only a mouth that hovers keeps the chord; one that keeps going moves.
    if depth_inside(previous, x, y) < margin {
        return previous;
    }
    candidate
}

/// The harmony's walk across the lattice, holding in space *and* in time.
///
/// [`settle`] alone leaves a chord that holds for seconds flicking to a
/// neighbour for two frames and back: the mouth really crossed, then returned.
/// So a departure must last `frames` in a row before the harmony follows.
///
/// ⚠ The count is of consecutive frames wanting to leave, not frames in one new
/// triangle: a glide rests in no cell, and waiting for one would freeze the
/// harmony for the whole gesture. Counting departures, the walk follows the
/// mouth, lagging by `frames`.
pub struct Walk {
    here: Triangle,
    /// Consecutive frames the position has wanted to leave `here`.
    leaving: usize,
}

impl Walk {
    /// Start wherever the first frame lands.
    pub fn start(x: f32, y: f32) -> Self {
        Walk {
            here: triangle_at(x, y),
            leaving: 0,
        }
    }

    /// Advance one frame and report the triangle the harmony is in. `frames` is
    /// the minimum dwell; 0 and 1 both commit as soon as `settle` allows.
    pub fn step(&mut self, x: f32, y: f32, hold: f32, frames: usize) -> Triangle {
        let candidate = settle(self.here, x, y, hold);
        if candidate == self.here {
            // Back inside, or never out: the departure did not happen.
            self.leaving = 0;
            return self.here;
        }

        self.leaving += 1;
        if self.leaving >= frames.max(1) {
            self.here = candidate;
            self.leaving = 0;
        }
        self.here
    }
}

/// How far outside a triangle a position has strayed, in cells.
fn depth_inside(t: Triangle, x: f32, y: f32) -> f32 {
    let (fx, fy) = (x - t.x as f32, y - t.y as f32);
    // A triangle is three half-planes; the distance outside is the worst one.
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
