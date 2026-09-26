//! What each knob actually changes, on each continuous mapping, along several
//! axes that are reported side by side and never summed: a knob is loud if it is
//! loud on *any* of them.
//!
//! | axis | what it sees | the right ruler for |
//! | --- | --- | --- |
//! | pitch | how far the voices move, in cents | density, spacing, reach, drift |
//! | roughness | beating between voices' partials | bind |
//! | balance | how the chord's loudness is distributed | voicing, articulation |
//! | colour | position on the timbre palette | articulation |
//! | noise | loudness of the unpitched material | consonants |
//! | ring | how long one chord holds, in seconds | hold, reach |
//!
//! Every axis exists because its absence produces a false zero — pitch travel
//! alone says `bind` does almost nothing, when its effect is whether partials
//! lock or beat. A knob measured on the wrong ruler looks broken.
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

/// Harmonics per voice when estimating chord roughness: a plain 1/n stack, not
/// the measured spectrum. Only ever used to compare two settings, both with the
/// same proxy, and far cheaper than an FFT per frame.
const PARTIALS: usize = 6;

/// Frames to sample when measuring, at most, spread across the take — the start
/// is the least typical part.
const SAMPLES: usize = 400;

/// How different two renders are, along axes that do not reduce to each other.
#[derive(Default, Clone, Copy)]
struct Change {
    /// Widest pitch move of any voice, in cents — what the knob *can* do. Read
    /// beside [`Change::pitch_typical`]: a knob that mostly nudges and sometimes
    /// re-registers a voice an octave does two things.
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
    /// Change in the loudness of the unpitched material, 0..1 — its own axis,
    /// since consonants are separate events, not part of the field.
    noise: f32,
}

impl Change {
    /// Whether this knob does anything a listener could notice, on any axis. A
    /// disjunction, not a weighted sum: weights would claim what matters, which
    /// is what the listening is for.
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

    // The widest move of any voice: a mean would dilute one voice re-registered.
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

    // The *shape* of the chord's loudness, so plain quieter is not rearranged.
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

/// How far apart two takes' unpitched material is: mean event amplitude, as a
/// fraction of the louder.
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
fn holds_a_chord(mapping: Mapping) -> bool {
    matches!(mapping, Mapping::Tonnetz)
}

/// The speaker's voice as derived at these settings — re-derived per setting,
/// because `density` decides the scale before any mapping runs.
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
        // The calibration take by default: the one the scale comes from.
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
        // Sorted by name, so two runs can be diffed.
        let mut rows: BTreeMap<&str, Change> = BTreeMap::new();

        for knob in KNOBS.iter().filter(|k| k.reaches(mapping)) {
            let low = Params::default().with(knob.name, knob.min);
            let high = Params::default().with(knob.name, knob.max);

            // `density` acts on the calibration, so the voice is re-derived at
            // each end, as the render route does.
            let (Ok(va), Ok(vb)) = (voice_at(&store, low), voice_at(&store, high)) else {
                println!("  {:<14} {:>10}", knob.name, "no scale");
                continue;
            };

            let (lo, hi) = (
                mapping.score_with(&vp, &va, low),
                mapping.score_with(&vp, &vb, high),
            );
            let (Some(a), Some(b)) = (&lo.field, &hi.field) else {
                // A refusal is an answer: this end has no sound to compare.
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
