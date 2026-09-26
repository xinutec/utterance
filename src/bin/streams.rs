//! How independent the voice's streams actually are, across the store.
//!
//! *One stream drives one parameter* can only be checked by measurement: two
//! streams at |r| near 1 are one stream counted twice. It is also the gate on
//! adding a stream — one that restates an existing one adds a knob and nothing
//! anyone can hear. Reads the mapping's own stream readers, so what is measured
//! is what is mapped.
//!
//! ```text
//! cargo run --bin streams
//! ```

use utterance::store::Store;
use utterance::voice;
use utterance_analysis::voiceprint::Voiceprint;
use utterance_mapping::streams;
use utterance_mapping::voice::Voice;

/// How far below a take's loudest frame still counts as sounding. Every stream
/// is constant in silence, so without the gate unrelated streams would agree
/// about the pauses and read as correlated.
const PEAK_DROP_DB: f32 = 40.0;

/// Above this, two streams are reported as one counted twice — room for streams
/// sharing a cause (loudness and aperiodicity at a phrase boundary).
const WELDED: f32 = 0.9;

/// One named per-frame series, as the mapping reads it.
struct Stream {
    name: &'static str,
    values: Vec<f32>,
}

/// Pearson's r where both series are defined, or `None` where one never moves —
/// a constant says nothing, which 0 would misreport as independence.
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

/// Every stream a continuous mapping reads, plus the candidates for admission,
/// smoothed as the mapping sees them: two series can correlate weakly per frame
/// and strongly over a syllable.
fn collect(vp: &Voiceprint, voice: &Voice) -> Vec<Stream> {
    let (open, front) = streams::vowel(vp, voice);
    let peak = vp.rms_db.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let sounding: Vec<bool> = vp
        .rms_db
        .iter()
        .map(|db| *db > peak - PEAK_DROP_DB)
        .collect();
    // Smoothed first and gated second: gating first would average across pauses.
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
        // Measured but not yet read by any mapping.
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

    // Pooled over every take: one take can make two streams agree by accident.
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
                // Lower triangle blank: correlation is symmetric.
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

    // The verdict, so nobody has to read the triangle.
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
