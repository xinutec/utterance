//! How long a chord actually rings, measured across every take in the store.
//!
//! **Why this exists, and why the number it replaces was misleading.** The
//! Tonnetz mapping was built to make the derived tuning audible, and the figure
//! reported for it was the *fraction* of a take spent holding one chord — 55%,
//! against the field mapping's 26%. That is the wrong statistic. The perceptual
//! threshold recorded in `docs/roadmap.md` is about a single chord ringing for
//! roughly a second, because that is how long a 5–14 Hz beat between two voices'
//! partials needs to establish. A fraction cannot see the difference between
//! eight seconds of held harmony and eighty chords of a hundred milliseconds,
//! and only the first of those is audible as a tuning.
//!
//! So this measures *durations*, one per ring, and reports the distribution.
//! It reads [`utterance_mapping::tonnetz::harmonic_path`] — the mapping's own
//! walk — rather than reimplementing it, because a tool that drifted from the
//! code would report on a mapping nobody is listening to.
//!
//! **Silence splits a ring.** A run of frames is only counted while the take is
//! actually sounding: if the voice stops and resumes on the same triangle, the
//! ear heard two chords, not one held through the gap. Without that, a pause
//! would be scored as the longest chord in the piece.
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

/// How far below a take's loudest frame still counts as sounding.
///
/// 40 dB down is inaudible in any room this will be played in, and the field
/// keeps its lowest voice at a floor of 0.02 forever — so without a gate every
/// take would score one enormous chord held through its own silence.
const PEAK_DROP_DB: f32 = 40.0;

/// How long one chord must ring before its tuning is perceptible, in seconds.
///
/// From the `bind` post-mortem in `docs/roadmap.md`: the five strongest partial
/// coincidences beat at 4.8–14.3 Hz when the tuning is equal-tempered and at
/// 0.01–0.26 Hz when it is the speaker's own, and telling those apart takes
/// about a second of stable chord. This is the number the whole question turns
/// on, which is why it is named here rather than written into a comparison.
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

    /// Share of sounding time inside rings long enough to have a tuning.
    ///
    /// Weighted by duration rather than counted, because the question is how
    /// much of what someone hears is a held chord — and a hundred flickers and
    /// one long ring are not half-and-half to a listener.
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

    // The published range's ends and its middle, plus the default — the same
    // discipline the knob tests use, because a knob is judged by what it does
    // across its travel and not at the setting someone happened to leave it on.
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

    // The second knob that decides how long a chord rings, and the one the
    // sweep above cannot reach: `hold` is hysteresis in space and `settle` is
    // hysteresis in time, so a mouth that crosses a boundary and comes straight
    // back is invisible to the first and caught by the second.
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
