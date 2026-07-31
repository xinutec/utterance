//! What each knob actually changes, measured on the mapping being listened to.
//!
//! **Why the published figures needed redoing.** The numbers in
//! `docs/roadmap.md` — `density` 1516 cents, `spacing` 1200, `bind` 18 — are
//! how far each knob moves the *pitch* of the field mapping at its widest. Two
//! things are wrong with using them now. They were measured on `field`, and the
//! Tonnetz is a different geometry. And a single scalar under-reports any knob
//! that does not work by moving pitch: `bind` scored 18 cents not because it is
//! feeble but because pitch travel is the wrong ruler for it. Its whole effect
//! is whether partials of different voices lock or beat, which a measure of how
//! far the notes moved cannot see at all.
//!
//! That is the same error as reporting the Tonnetz's held-chord *fraction*
//! instead of its ring durations (see `dwell.rs`). So this reports several axes
//! side by side and refuses to rank them into one number: a knob is loud if it
//! is loud on *any* of them.
//!
//! | axis | what it sees | the knobs it is the right ruler for |
//! | --- | --- | --- |
//! | pitch | how far the voices move, in cents | density, spacing, reach, drift |
//! | roughness | beating between voices' partials | bind |
//! | balance | how the chord's loudness is distributed | voicing, articulation |
//! | colour | position on the timbre palette | articulation |
//! | noise | loudness of the unpitched material | consonants |
//! | ring | how long one chord holds, in seconds | hold, reach |
//!
//! **Every axis here was added because its absence produced a false zero**, and
//! the count is now three: the field-only comparison reported `consonants` as
//! doing nothing, holding one derived voice fixed reported `density` as doing
//! nothing, and pitch-travel-alone reported `bind` as nearly doing nothing. A
//! knob measured on the wrong axis is indistinguishable from a knob that does
//! not work, which is the reading that gets one deleted.
//!
//! ```text
//! cargo run --bin authority                  # the default take, both mappings
//! cargo run --bin authority -- vowel-ah      # one take by label
//! ```

use std::collections::BTreeMap;

use utterance::store::Store;
use utterance::voice;
use utterance_analysis::voiceprint::Voiceprint;
use utterance_mapping::dissonance::{self, Component};
use utterance_mapping::mapping::{CONTINUOUS, Mapping};
use utterance_mapping::params::{KNOBS, Params};
use utterance_mapping::score::{Field, NoiseEvent};
use utterance_mapping::tonnetz;
use utterance_mapping::voice::Voice;

/// Harmonics per voice when estimating how rough the chord is.
///
/// Six, with amplitude falling as 1/n — a plain harmonic stack rather than the
/// speaker's measured spectrum. The measure is only ever used to compare two
/// settings against each other, and both sides get the same proxy, so what it
/// costs is the absolute value and not the comparison. Using the real partials
/// would cost an FFT per frame per knob per setting.
const PARTIALS: usize = 6;

/// Frames to sample when measuring, at most.
///
/// Every knob is rendered at both ends on every mapping, so this is the
/// difference between a measurement that takes a second and one that takes a
/// minute. Spread across the take rather than taken from the front, because the
/// beginning of a take is the part least like the rest of it.
const SAMPLES: usize = 400;

/// How different two renders are, along axes that do not reduce to each other.
#[derive(Default, Clone, Copy)]
struct Change {
    /// Widest pitch move of any voice, in cents.
    ///
    /// A maximum, so it says what the knob *can* do and not what it usually
    /// does. Read beside [`Change::pitch_typical`], which is the median over
    /// every voice and frame: the two differ by an order of magnitude wherever
    /// a knob mostly nudges and occasionally re-registers a voice by an octave,
    /// and those are different things to listen for.
    pitch_cents: f32,
    /// Median pitch move across every voice and frame, in cents.
    pitch_typical: f32,
    /// Change in chord roughness, as a fraction of the quieter setting's.
    roughness: f32,
    /// Change in how loudness sits across the voices, 0..1.
    balance: f32,
    /// Change in position on the timbre palette, 0..1.
    colour: f32,
    /// Change in how long one chord holds, in seconds.
    ring_s: f32,
    /// Change in the loudness of the unpitched material, 0..1.
    ///
    /// Its own axis because the consonants are not in the field at all: they
    /// are a separate list of events on the score, and a measurement that
    /// compared only fields reported `consonants` as a knob that does nothing.
    /// It does nothing *to the field*, which is not the same sentence.
    noise: f32,
}

impl Change {
    /// Whether this knob does anything a listener could notice, on any axis.
    ///
    /// Deliberately a disjunction and deliberately not a weighted sum. A sum
    /// needs weights, weights are a claim about what matters, and that claim is
    /// exactly the thing nobody has settled — it is what the listening is for.
    fn audible(&self) -> bool {
        self.pitch_cents > 5.0
            || self.roughness > 0.01
            || self.balance > 0.01
            || self.colour > 0.01
            || self.ring_s.abs() > 0.05
            || self.noise > 0.01
    }
}

/// Indices of the frames to measure, spread evenly across the take.
fn sampled(frames: usize) -> Vec<usize> {
    if frames <= SAMPLES {
        return (0..frames).collect();
    }
    (0..SAMPLES).map(|i| i * frames / SAMPLES).collect()
}

/// Roughness within one chord: every voice against every other.
fn roughness(f: &Field, i: usize) -> f32 {
    let voices: Vec<Vec<Component>> = (0..f.voices.len())
        .map(|v| {
            let (hz, gain) = (f.voices[v][i], f.gains[v][i]);
            (1..=PARTIALS)
                .map(|k| Component {
                    hz: hz * k as f32,
                    amplitude: gain / k as f32,
                })
                .collect()
        })
        .collect();
    let mut total = 0.0;
    for (a, one) in voices.iter().enumerate() {
        for other in &voices[a + 1..] {
            total += dissonance::between_spectra(one, other);
        }
    }
    total
}

/// Mean over sampled frames, guarding an empty take.
fn mean(values: impl Iterator<Item = f32>) -> f32 {
    let (sum, n) = values.fold((0.0, 0usize), |(s, n), v| (s + v, n + 1));
    if n == 0 { 0.0 } else { sum / n as f32 }
}

/// How far apart two renders of the same take are.
fn difference(a: &Field, b: &Field) -> Change {
    let frames = a.colour.len().min(b.colour.len());
    let at = sampled(frames);
    let voices = a.voices.len().min(b.voices.len());

    // The widest move of any voice, not the average. A knob that re-registers
    // one voice by an octave and leaves four alone has done something a
    // listener hears, and a mean across five voices would report a fifth of it.
    let pitch_cents = at
        .iter()
        .flat_map(|&i| {
            (0..voices).map(move |v| {
                let (x, y) = (a.voices[v][i], b.voices[v][i]);
                if x > 0.0 && y > 0.0 {
                    1200.0 * (y / x).log2().abs()
                } else {
                    0.0
                }
            })
        })
        .fold(0.0f32, f32::max);

    let mut moves: Vec<f32> = at
        .iter()
        .flat_map(|&i| {
            (0..voices).map(move |v| {
                let (x, y) = (a.voices[v][i], b.voices[v][i]);
                if x > 0.0 && y > 0.0 {
                    1200.0 * (y / x).log2().abs()
                } else {
                    0.0
                }
            })
        })
        .collect();
    moves.sort_by(f32::total_cmp);
    let pitch_typical = moves.get(moves.len() / 2).copied().unwrap_or(0.0);

    let rough_a = mean(at.iter().map(|&i| roughness(a, i)));
    let rough_b = mean(at.iter().map(|&i| roughness(b, i)));
    let roughness = if rough_a.max(rough_b) > 0.0 {
        (rough_a - rough_b).abs() / rough_a.max(rough_b)
    } else {
        0.0
    };

    // Balance compares the *shape* of the chord's loudness rather than its
    // level, so a knob that only makes everything quieter does not read as one
    // that rearranged the chord.
    let share = |f: &Field, i: usize| {
        let total: f32 = (0..voices).map(|v| f.gains[v][i]).sum();
        let total = if total > 0.0 { total } else { 1.0 };
        (0..voices)
            .map(|v| f.gains[v][i] / total)
            .collect::<Vec<_>>()
    };
    let balance = mean(at.iter().map(|&i| {
        let (x, y) = (share(a, i), share(b, i));
        x.iter().zip(&y).map(|(p, q)| (p - q).abs()).sum::<f32>() / 2.0
    }));

    let colour = mean(at.iter().map(|&i| (a.colour[i] - b.colour[i]).abs()));

    Change {
        pitch_cents,
        pitch_typical,
        roughness,
        balance,
        colour,
        ring_s: 0.0,
        noise: 0.0,
    }
}

/// How far apart two takes' unpitched material is.
///
/// Mean amplitude across the events, compared as a fraction of the louder — the
/// consonants keep their positions and change only their level, so a difference
/// in *when* they happen would be a different bug entirely.
fn noise_change(a: &[NoiseEvent], b: &[NoiseEvent]) -> f32 {
    let level = |events: &[NoiseEvent]| mean(events.iter().map(|e| e.amplitude));
    let (x, y) = (level(a), level(b));
    if x.max(y) > 0.0 {
        (x - y).abs() / x.max(y)
    } else {
        0.0
    }
}

/// Median duration of one held chord, for the mappings that hold one.
fn ring_s(vp: &Voiceprint, voice: &Voice, params: Params) -> f32 {
    let Some(path) = tonnetz::harmonic_path(vp, voice, params) else {
        return 0.0;
    };
    let peak = vp.rms_db.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let mut runs = Vec::new();
    let mut len = 0usize;
    for (i, here) in path.iter().enumerate() {
        let sounding = vp.rms_db.get(i).is_some_and(|db| *db > peak - 40.0);
        let same = i > 0 && path[i - 1] == *here;
        if sounding && same {
            len += 1;
        } else {
            if len > 0 {
                runs.push(len as f32 * vp.frame.hop_s);
            }
            len = usize::from(sounding);
        }
    }
    if len > 0 {
        runs.push(len as f32 * vp.frame.hop_s);
    }
    runs.sort_by(f32::total_cmp);
    runs.get(runs.len() / 2).copied().unwrap_or(0.0)
}

/// Whether a mapping quantises its harmony, and so has a ring worth timing.
///
/// The only thing this measurement needs to know about a mapping that the
/// mapping crate does not already say. It kept a whole local enum to hold it —
/// three variants' worth of name and dispatch restated to carry one predicate —
/// until `Mapping` existed to be asked instead.
fn holds_a_chord(mapping: Mapping) -> bool {
    matches!(mapping, Mapping::Tonnetz)
}

/// The speaker's voice as it would be derived at these settings.
///
/// Not a constant across a sweep, which is the trap this measurement fell into
/// first: `density` is the depth a dip in the roughness curve must clear to
/// count as a note, so it decides what the *scale* is before any mapping runs.
/// Held fixed, it measures as a knob that changes nothing.
fn voice_at(store: &Store, params: Params) -> anyhow::Result<Voice> {
    Ok(voice::calibrate_with(store, None, params.density)
        .map_err(|e| anyhow::anyhow!("{e}"))?
        .voice)
}

fn main() -> anyhow::Result<()> {
    let data_dir = std::env::var("DATA_DIR").unwrap_or_else(|_| "data".into());
    let store = Store::open(&data_dir)?;
    let calibrated = voice::calibrate(&store, None).map_err(|e| anyhow::anyhow!("{e}"))?;

    let wanted = std::env::args().nth(1);
    let takes = store.list()?;
    let meta = match &wanted {
        Some(label) => takes
            .iter()
            .find(|m| m.label == *label || m.id == *label)
            .ok_or_else(|| anyhow::anyhow!("no take called {label}"))?,
        // The calibration take by default, which is what the published field
        // figures were measured on — so the two tables can be read together.
        None => takes
            .iter()
            .find(|m| m.id == calibrated.source.id)
            .ok_or_else(|| anyhow::anyhow!("the calibration take is not in the store"))?,
    };
    let vp = store.voiceprint(&meta.id)?;
    println!(
        "take: {} ({:.1}s)   scale: {} degrees from {}\n",
        meta.label,
        meta.duration_s,
        calibrated.voice.tuning.degrees.len(),
        calibrated.source.label,
    );

    for mapping in CONTINUOUS.iter().copied() {
        println!("{}", mapping.name());
        println!(
            "  {:<14} {:>9} {:>9} {:>10} {:>8} {:>7} {:>7} {:>8}",
            "knob", "pitch", "typical", "roughness", "balance", "colour", "noise", "ring"
        );

        let mut silent = Vec::new();
        // Sorted by name so two runs can be diffed; the table's own order is
        // how it was written, which is not a fact about the measurement.
        let mut rows: BTreeMap<&str, Change> = BTreeMap::new();

        for knob in KNOBS.iter().filter(|k| k.reaches(mapping)) {
            let low = Params::default().with(knob.name, knob.min);
            let high = Params::default().with(knob.name, knob.max);

            // **`density` acts on the calibration, not on the composition**, so
            // a sweep holding one derived voice fixed measured it at exactly
            // zero on every axis — and reported the loudest knob in the
            // published table as one that does nothing. The scale is re-derived
            // at each end, which is what the render route does too.
            let (Ok(va), Ok(vb)) = (voice_at(&store, low), voice_at(&store, high)) else {
                println!("  {:<14} {:>10}", knob.name, "no scale");
                continue;
            };

            let (lo, hi) = (
                mapping.score_with(&vp, &va, low),
                mapping.score_with(&vp, &vb, high),
            );
            let (Some(a), Some(b)) = (&lo.field, &hi.field) else {
                // A refusal is a real answer — see `Lattice::from_tuning` — and
                // the honest report is that this end of the range has no sound
                // to compare rather than that the knob changed nothing.
                println!("  {:<14} {:>10}", knob.name, "refused");
                continue;
            };

            let mut change = difference(a, b);
            change.noise = noise_change(&lo.noise, &hi.noise);
            if holds_a_chord(mapping) {
                change.ring_s = ring_s(&vp, &vb, high) - ring_s(&vp, &va, low);
            }
            if !change.audible() {
                silent.push(knob.name.name());
            }
            rows.insert(knob.name.name(), change);
        }

        for (name, c) in &rows {
            println!(
                "  {:<14} {:>8.0}¢ {:>8.0}¢ {:>9.0}% {:>7.0}% {:>6.0}% {:>6.0}% {:>+7.2}s  {}",
                name,
                c.pitch_cents,
                c.pitch_typical,
                c.roughness * 100.0,
                c.balance * 100.0,
                c.colour * 100.0,
                c.noise * 100.0,
                c.ring_s,
                if c.audible() { "" } else { "<- nothing" },
            );
        }
        if !silent.is_empty() {
            println!("  nothing on any axis: {}", silent.join(", "));
        }
        println!();
    }

    Ok(())
}
