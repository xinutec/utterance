//! How long a chord actually rings, across every take in the store.
//!
//! Durations, one per ring, not the fraction of a take spent holding a chord:
//! a tuning takes about a second of stable chord to hear, and a fraction cannot
//! tell eight held seconds from eighty flickers. Reads the mapping's own walk
//! ([`utterance_mapping::tonnetz::harmonic_path`]) so it cannot drift from it.
//! Silence splits a ring: stopping and resuming on one triangle is two chords.
//!
//! ```text
//! cargo run --bin dwell             # the default hold, plus a sweep
//! cargo run --bin dwell -- 0.9      # one specific hold value
//! cargo run --bin dwell -- 0.9 0.2  # …and a settle time, in seconds
//! ```

use std::collections::BTreeSet;

use utterance::store::Store;
use utterance::voice;
use utterance_mapping::lattice::Triangle;
use utterance_mapping::params::Params;
use utterance_mapping::tonnetz;

/// How far below a take's loudest frame still counts as sounding: 40 dB. The
/// field never quite falls silent, so without a gate a pause is one long chord.
const PEAK_DROP_DB: f32 = 40.0;

/// How long one chord must ring before its tuning is perceptible, in seconds:
/// the tempered and derived beats `beating.rs` measured (4.8–14.3 Hz against
/// 0.01–0.26 Hz) take about a second of stable chord to tell apart.
const RING_S: f32 = 1.0;

/// One take's rings at one setting.
struct Dwells {
    /// Every ring's duration, ascending.
    durations: Vec<f32>,
    /// Seconds the take spent sounding at all.
    sounding_s: f32,
}

impl Dwells {
    fn quantile(&self, q: f32) -> f32 {
        if self.durations.is_empty() {
            return 0.0;
        }
        let at = ((self.durations.len() - 1) as f32 * q).round() as usize;
        self.durations[at]
    }

    /// Share of sounding time inside rings long enough to have a tuning —
    /// weighted by duration, since a hundred flickers and one long ring are not
    /// half-and-half to a listener.
    fn ring_share(&self) -> f32 {
        if self.sounding_s <= 0.0 {
            return 0.0;
        }
        let held: f32 = self.durations.iter().filter(|d| **d >= RING_S).sum();
        held / self.sounding_s
    }
}

/// Split a take's harmonic walk into rings.
fn dwells(path: &[Triangle], sounding: &[bool], hop_s: f32) -> Dwells {
    let mut durations = Vec::new();
    let mut run: Option<(Triangle, usize)> = None;

    for (i, here) in path.iter().enumerate() {
        match run {
            // Still the same chord, still audible: the ring goes on.
            Some((was, len)) if sounding[i] && was == *here => run = Some((was, len + 1)),
            _ => {
                if let Some((_, len)) = run.take() {
                    durations.push(len as f32 * hop_s);
                }
                if sounding[i] {
                    run = Some((*here, 1));
                }
            }
        }
    }
    if let Some((_, len)) = run {
        durations.push(len as f32 * hop_s);
    }

    durations.sort_by(f32::total_cmp);
    let sounding_s = sounding.iter().filter(|s| **s).count() as f32 * hop_s;
    Dwells {
        durations,
        sounding_s,
    }
}

fn main() -> anyhow::Result<()> {
    let data_dir = std::env::var("DATA_DIR").unwrap_or_else(|_| "data".into());
    let store = Store::open(&data_dir)?;
    let calibrated = voice::calibrate(&store, None).map_err(|e| anyhow::anyhow!("{e}"))?;
    println!(
        "calibration: {} ({} degrees)\n",
        calibrated.source.id,
        calibrated.voice.tuning.degrees.len()
    );

    // The range's ends and middle plus the default: a knob is judged across its
    // travel.
    let holds: Vec<f32> = if let Some(one) = std::env::args().nth(1) {
        vec![one.parse()?]
    } else {
        let mut set: BTreeSet<u32> = [0.0f32, 0.25, 0.5, 0.75, 0.9, 1.0]
            .iter()
            .map(|h| (h * 1000.0) as u32)
            .collect();
        set.insert((Params::default().hold * 1000.0) as u32);
        set.into_iter().map(|h| h as f32 / 1000.0).collect()
    };

    // `settle` catches what `hold` cannot: a mouth that crosses and comes
    // straight back.
    let settle: f32 = match std::env::args().nth(2) {
        Some(s) => s.parse()?,
        None => Params::default().settle,
    };

    let takes = store.list()?;
    for hold in holds {
        let params = Params {
            hold,
            settle,
            ..Params::default()
        };
        println!("hold = {hold:.2}, settle = {settle:.2}s");
        println!(
            "  {:<18} {:>7} {:>9} {:>9} {:>9} {:>10}",
            "take", "chords", "median", "p90", "longest", "ring>=1s"
        );

        let mut all = Vec::new();
        let mut sounding_total = 0.0;
        for meta in &takes {
            let Ok(vp) = store.voiceprint(&meta.id) else {
                continue;
            };
            let Some(path) = tonnetz::harmonic_path(&vp, &calibrated.voice, params) else {
                println!("  {:<18} {:>7}", meta.id, "—");
                continue;
            };
            let peak = vp.rms_db.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            let sounding: Vec<bool> = vp
                .rms_db
                .iter()
                .map(|db| *db > peak - PEAK_DROP_DB)
                .collect();
            let d = dwells(&path, &sounding, vp.frame.hop_s);
            println!(
                "  {:<18} {:>7} {:>8.2}s {:>8.2}s {:>8.2}s {:>9.0}%",
                meta.label,
                d.durations.len(),
                d.quantile(0.5),
                d.quantile(0.9),
                d.durations.last().copied().unwrap_or(0.0),
                d.ring_share() * 100.0,
            );
            sounding_total += d.sounding_s;
            all.extend(d.durations);
        }

        all.sort_by(f32::total_cmp);
        let pooled = Dwells {
            durations: all,
            sounding_s: sounding_total,
        };
        println!(
            "  {:<18} {:>7} {:>8.2}s {:>8.2}s {:>8.2}s {:>9.0}%\n",
            "ALL",
            pooled.durations.len(),
            pooled.quantile(0.5),
            pooled.quantile(0.9),
            pooled.durations.last().copied().unwrap_or(0.0),
            pooled.ring_share() * 100.0,
        );
    }

    Ok(())
}
