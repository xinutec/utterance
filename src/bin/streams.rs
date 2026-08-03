//! How independent the voice's streams actually are, measured across the store.
//!
//! **Why this exists.** The field mapping's own doc once claimed six streams
//! while `colour` was set from the same normalised F2 that walked the root: two
//! streams welded into one, and the mapping was therefore simpler than the count
//! said. That was found by reading the code rather than by listening, which is
//! luck. The decision it produced — *one stream drives one parameter* — has had
//! nothing to check it since.
//!
//! So this reports the correlation between every stream a mapping reads. What a
//! listener hears as variety is how many things can move **independently**, and
//! two streams at |r| near 1 are one stream counted twice however separately
//! they are computed.
//!
//! **It is also the gate on adding a stream.** A new measurement earns its place
//! by moving where the existing ones do not; one that merely restates the
//! centroid would add a parameter, a knob and a line of documentation while
//! adding nothing anybody can hear.
//!
//! Reads `utterance_mapping::streams` — the mapping's own readers, gaps carried
//! and all — rather than the raw voiceprint fields, so what is measured is what
//! is actually mapped.
//!
//! ```text
//! cargo run --bin streams
//! ```

use utterance::store::Store;
use utterance::voice;
use utterance_analysis::voiceprint::Voiceprint;
use utterance_mapping::streams;
use utterance_mapping::voice::Voice;

/// How far below a take's loudest frame still counts as sounding.
///
/// The same gate `dwell` uses, and here for a sharper reason than tidiness.
/// Every stream reports something constant in digital silence — tilt fits a flat
/// line through the bin floor and returns exactly 0, energy is 0, flatness is 0 —
/// so the silent frames of every take pile up at one point of the scatter. Two
/// streams that are unrelated while the voice sounds are then reported as
/// correlated, because they agree about the silence. This tool exists to decide
/// whether a stream is worth reading, and silence is not what any of them will be
/// read for.
const PEAK_DROP_DB: f32 = 40.0;

/// Above this, two streams are reported as one stream counted twice.
///
/// 0.9 leaves room for streams that genuinely share a cause — loudness and
/// aperiodicity both move at a phrase boundary — while catching the case the
/// field mapping actually had, where one series *was* the other after a linear
/// rescaling and the correlation was exactly 1.
const WELDED: f32 = 0.9;

/// One named per-frame series, as the mapping reads it.
struct Stream {
    name: &'static str,
    values: Vec<f32>,
}

/// Pearson's r, on the frames where both series are defined.
///
/// Returns `None` where either series never moves: a constant has no
/// correlation with anything, and reporting 0 would read as *independent* when
/// the truth is *this take says nothing about it*.
fn correlation(a: &[f32], b: &[f32]) -> Option<f32> {
    let n = a.len().min(b.len());
    if n < 2 {
        return None;
    }
    let mean = |v: &[f32]| v[..n].iter().sum::<f32>() / n as f32;
    let (ma, mb) = (mean(a), mean(b));

    let mut cov = 0.0f32;
    let mut va = 0.0f32;
    let mut vb = 0.0f32;
    for i in 0..n {
        let (da, db) = (a[i] - ma, b[i] - mb);
        cov += da * db;
        va += da * da;
        vb += db * db;
    }
    if va <= f32::EPSILON || vb <= f32::EPSILON {
        return None;
    }
    Some(cov / (va * vb).sqrt())
}

/// Every stream a continuous mapping reads, plus the candidates for admission.
///
/// Smoothed at the timescale each one belongs to, because that is the form the
/// mapping sees. Two series can correlate weakly frame by frame and strongly
/// once both are averaged over a syllable, and the smoothed pair is the one that
/// decides whether the music has two things moving in it or one.
fn collect(vp: &Voiceprint, voice: &Voice) -> Vec<Stream> {
    let (open, front) = streams::vowel(vp, voice);
    let peak = vp.rms_db.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let sounding: Vec<bool> = vp
        .rms_db
        .iter()
        .map(|db| *db > peak - PEAK_DROP_DB)
        .collect();
    // Smoothed first and gated second. Gating first would splice the sounding
    // frames together and let a moving average run across a pause as though the
    // voice had carried straight on through it.
    let heard = |values: Vec<f32>, window: usize| {
        streams::smooth(&values, window)
            .into_iter()
            .zip(&sounding)
            .filter_map(|(v, keep)| keep.then_some(v))
            .collect::<Vec<f32>>()
    };
    vec![
        Stream {
            name: "f0",
            values: heard(streams::filled(&vp.pitch.hz), streams::DRIFT_FRAMES),
        },
        Stream {
            name: "openness",
            values: heard(open, streams::ROOT_FRAMES),
        },
        Stream {
            name: "frontness",
            values: heard(front, streams::ROOT_FRAMES),
        },
        Stream {
            name: "f3",
            values: heard(streams::depth(vp, voice), streams::ROOT_FRAMES),
        },
        Stream {
            name: "flux",
            values: heard(vp.events.flux.clone(), streams::LEVEL_FRAMES),
        },
        Stream {
            name: "energy",
            values: heard(streams::level(vp), streams::LEVEL_FRAMES),
        },
        Stream {
            name: "brightness",
            values: heard(streams::brightness(vp, voice), streams::ROOT_FRAMES),
        },
        Stream {
            name: "aperiodicity",
            values: heard(vp.pitch.aperiodicity.clone(), streams::LEVEL_FRAMES),
        },
        // Measured but not yet read by any mapping. The question this tool is
        // being run to answer.
        Stream {
            name: "*tilt",
            values: heard(vp.texture.tilt_db_per_octave.clone(), streams::ROOT_FRAMES),
        },
        Stream {
            name: "*flatness",
            values: heard(vp.texture.flatness.clone(), streams::ROOT_FRAMES),
        },
    ]
}

fn main() -> anyhow::Result<()> {
    let data_dir = std::env::var("DATA_DIR").unwrap_or_else(|_| "data".into());
    let store = Store::open(&data_dir)?;
    let calibrated = voice::calibrate(&store, None).map_err(|e| anyhow::anyhow!("{e}"))?;
    println!("calibration: {}\n", calibrated.source.id);
    println!("* marks a stream nothing reads yet.\n");

    // Pooled over every take rather than reported per take. A correlation is a
    // claim about the voice, and one take can make two streams agree by
    // accident — a take that is all one vowel has no vowel motion to disagree
    // with anything.
    let takes = store.list()?;
    let mut pooled: Vec<Stream> = Vec::new();
    let mut used = 0;

    for meta in &takes {
        let Ok(vp) = store.voiceprint(&meta.id) else {
            continue;
        };
        let here = collect(&vp, &calibrated.voice);
        if pooled.is_empty() {
            pooled = here;
        } else {
            for (into, from) in pooled.iter_mut().zip(here) {
                into.values.extend(from.values);
            }
        }
        used += 1;
    }
    if pooled.is_empty() {
        anyhow::bail!("no analysable takes");
    }
    println!("{used} takes, {} frames\n", pooled[0].values.len());

    print!("{:<14}", "");
    for s in &pooled {
        print!("{:>13}", s.name);
    }
    println!();
    for (i, a) in pooled.iter().enumerate() {
        print!("{:<14}", a.name);
        for (j, b) in pooled.iter().enumerate() {
            match j.cmp(&i) {
                // Lower triangle left blank: correlation is symmetric, so
                // printing it twice only makes the table harder to read across.
                std::cmp::Ordering::Less => print!("{:>13}", ""),
                std::cmp::Ordering::Equal => print!("{:>13}", "—"),
                std::cmp::Ordering::Greater => match correlation(&a.values, &b.values) {
                    Some(r) => print!("{r:>13.2}"),
                    None => print!("{:>13}", "flat"),
                },
            }
        }
        println!();
    }

    // The verdict, so nobody has to read a triangle of numbers to find it.
    println!("\nwelded pairs (|r| >= {WELDED:.1}):");
    let mut any = false;
    for (i, a) in pooled.iter().enumerate() {
        for b in pooled.iter().skip(i + 1) {
            if let Some(r) = correlation(&a.values, &b.values)
                && r.abs() >= WELDED
            {
                println!("  {:<14} {:<14} r = {r:+.2}", a.name, b.name);
                any = true;
            }
        }
    }
    if !any {
        println!("  none");
    }

    Ok(())
}
