//! Whether an utterance has structure above the two-second ceiling — and whether
//! that structure is in the voice or in the method.
//!
//! Nothing in the field operates above `streams::DRIFT_FRAMES` (two seconds),
//! and the routes to a slower harmonic rhythm — meter, the symbol stream — are
//! blocked on labels. Recurrence needs none: if the mouth returns to where it
//! has been, a harmony could return with it. This asks whether real takes
//! recur above two seconds more than the same material with its long-range
//! order destroyed. Yes would license a form layer; no costs one tool.
//!
//! **The control is the whole argument.** Smooth streams look structured
//! whatever the material, so every figure is reported beside a surrogate: each
//! stream's Fourier magnitudes kept and its phases replaced, one phase sequence
//! across all eight so their correlations survive. It keeps smoothness and slow
//! drift and destroys only the arrangement in time. A block shuffle would not
//! do: it manufactures a boundary at every block edge, a bias along the very
//! axis measured.
//!
//! Silence is gated out (every stream agrees in silence), so pauses — real
//! phrase structure — are not counted: the result is a floor, not an estimate.
//!
//! ```text
//! cargo run --bin form
//! ```

use utterance::store::Store;
use utterance::voice;
use utterance_analysis::voiceprint::Voiceprint;
use utterance_mapping::streams;
use utterance_mapping::voice::Voice;

/// How far below a take's loudest frame still counts as sounding — the gate
/// `dwell` and `streams` use.
const PEAK_DROP_DB: f32 = 40.0;

/// Frames per second the similarity matrix is built at. The streams are already
/// smoothed coarser than this, so decimating to it loses nothing.
const COARSE_HZ: f32 = 10.0;

/// Kernel widths, in seconds, that novelty is measured at: 1 and 2 are inside
/// what the field reaches, 4 to 16 above it.
const SCALES_S: [f32; 5] = [1.0, 2.0, 4.0, 8.0, 16.0];

/// Nearest a frame may be and still count as a return rather than as itself —
/// past every timescale the field reads, so a held vowel is not a return.
const RETURN_LAG_S: f32 = 5.0;

/// How many surrogates each figure is compared against, averaged: one is a
/// single draw.
const SURROGATES: usize = 5;

/// Fraction of surrogate frame pairs that define "resembling". The threshold is
/// read off the surrogate's own distribution, so the surrogate scores this by
/// construction and the take is a multiple of it.
const RECURRENCE_QUANTILE: f32 = 0.95;

/// The increment the surrogate's phases advance by, as a turn of the circle: the
/// golden ratio's fractional part, equidistributed for any length.
const GOLDEN: f64 = 0.618_033_988_749_895;

/// One take's per-frame feature vectors, one vector per coarse frame.
struct Trajectory {
    frames: Vec<Vec<f32>>,
    rate_hz: f32,
}

/// Every stream a continuous mapping reads, gated to sounding frames, decimated
/// and standardised — so cosine similarity is not decided by the stream with the
/// largest units.
fn trajectory(vp: &Voiceprint, voice: &Voice, gate_silence: bool) -> Option<Trajectory> {
    let (open, front) = streams::vowel(vp, voice);
    let peak = vp.rms_db.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let sounding: Vec<bool> = vp
        .rms_db
        .iter()
        .map(|db| !gate_silence || *db > peak - PEAK_DROP_DB)
        .collect();

    // Smoothed first and gated second: gating first would average across pauses.
    let heard = |values: Vec<f32>, window: usize| {
        streams::smooth(&values, window)
            .into_iter()
            .zip(&sounding)
            .filter_map(|(v, keep)| keep.then_some(v))
            .collect::<Vec<f32>>()
    };

    let columns: Vec<Vec<f32>> = vec![
        heard(streams::filled(&vp.pitch.hz), streams::DRIFT_FRAMES),
        heard(open, streams::ROOT_FRAMES),
        heard(front, streams::ROOT_FRAMES),
        heard(streams::depth(vp, voice), streams::ROOT_FRAMES),
        heard(streams::level(vp), streams::LEVEL_FRAMES),
        heard(streams::brightness(vp, voice), streams::ROOT_FRAMES),
        heard(vp.pitch.aperiodicity.clone(), streams::LEVEL_FRAMES),
        heard(vp.texture.tilt_db_per_octave.clone(), streams::ROOT_FRAMES),
    ];

    // `hop_s`, not `analysis_rate_hz` (the audio rate): this grid is in frames.
    let frame_hz = 1.0 / vp.frame.hop_s;
    let step = ((frame_hz / COARSE_HZ).round() as usize).max(1);
    let n = columns.iter().map(Vec::len).min()?;
    let coarse: Vec<Vec<f32>> = columns
        .iter()
        .map(|c| c[..n].iter().step_by(step).copied().collect())
        .collect();

    let standardised: Vec<Vec<f32>> = coarse.into_iter().map(standardise).collect();
    let count = standardised.iter().map(Vec::len).min()?;
    if count < 2 {
        return None;
    }
    let frames = (0..count)
        .map(|i| standardised.iter().map(|c| c[i]).collect())
        .collect();
    Some(Trajectory {
        frames,
        rate_hz: frame_hz / step as f32,
    })
}

/// Zero mean, unit variance. A stream that never moves becomes zeros rather than
/// being divided by nothing.
fn standardise(values: Vec<f32>) -> Vec<f32> {
    let n = values.len();
    if n == 0 {
        return values;
    }
    let mean = values.iter().sum::<f32>() / n as f32;
    let var = values.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / n as f32;
    if var <= f32::EPSILON {
        return vec![0.0; n];
    }
    let sd = var.sqrt();
    values.into_iter().map(|v| (v - mean) / sd).collect()
}

/// Cosine similarity between two standardised frames, in -1..1.
fn similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|y| y * y).sum::<f32>().sqrt();
    if na <= f32::EPSILON || nb <= f32::EPSILON {
        0.0
    } else {
        (dot / (na * nb)).clamp(-1.0, 1.0)
    }
}

/// The full self-similarity matrix, row-major.
fn matrix(t: &Trajectory) -> Vec<Vec<f32>> {
    let n = t.frames.len();
    let mut s = vec![vec![0.0f32; n]; n];
    for (i, a) in t.frames.iter().enumerate() {
        for (j, b) in t.frames.iter().enumerate().skip(i) {
            let v = similarity(a, b);
            s[i][j] = v;
            s[j][i] = v;
        }
    }
    s
}

/// Foote novelty: a Gaussian-tapered checkerboard kernel dragged down the
/// diagonal, peaking where the take stops resembling what it was. `None` where
/// the take is shorter than the kernel.
fn novelty(s: &[Vec<f32>], half: usize) -> Option<Vec<f32>> {
    let n = s.len();
    if half == 0 || n < 2 * half + 1 {
        return None;
    }
    let width = 2 * half;
    let taper: Vec<f32> = (0..width)
        .map(|u| {
            let x = (u as f32 - half as f32 + 0.5) / half as f32;
            (-4.0 * x * x).exp()
        })
        .collect();

    let mut out = vec![0.0f32; n];
    for c in half..n - half {
        let mut acc = 0.0f32;
        let mut weight = 0.0f32;
        for (u, &tu) in taper.iter().enumerate() {
            let row = &s[c - half + u];
            for (v, &tv) in taper.iter().enumerate() {
                let sign = if (u < half) == (v < half) { 1.0 } else { -1.0 };
                let w = tu * tv;
                acc += sign * w * row[c - half + v];
                weight += w;
            }
        }
        out[c] = if weight > 0.0 { acc / weight } else { 0.0 };
    }
    Some(out)
}

/// How far the strongest boundary stands above an ordinary moment, in units of
/// the curve's own spread. Not the raw maximum: a maximum is a contest the
/// noisier curve wins, and a surrogate's curve is noisier.
fn contrast(curve: &[f32]) -> Option<f32> {
    // The kernel cannot reach the first and last half-width; leave them out.
    let measured: Vec<f32> = curve.iter().copied().filter(|v| *v != 0.0).collect();
    if measured.len() < 4 {
        return None;
    }
    let n = measured.len() as f32;
    let mean = measured.iter().sum::<f32>() / n;
    let sd = (measured.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / n).sqrt();
    if sd <= f32::EPSILON {
        return None;
    }
    let max = measured.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    Some((max - mean) / sd)
}

/// Every similarity between frames at least `RETURN_LAG_S` apart — closer is the
/// same sound held, not a return.
fn distant(s: &[Vec<f32>], lag: usize) -> Vec<f32> {
    let mut out = Vec::new();
    for (i, row) in s.iter().enumerate() {
        out.extend(row.iter().skip(i + lag).copied());
    }
    out
}

/// The value at a quantile of a sample, by nearest rank.
fn quantile(values: &mut [f32], q: f32) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f32::total_cmp);
    let k = ((values.len() - 1) as f32 * q).round() as usize;
    Some(values[k])
}

/// How much more often than chance the take returns to where it has been, as a
/// multiple of the surrogate's rate; one means no more than arrangement alone
/// would give. A rate above a threshold rather than a per-frame maximum, which
/// saturates: with hundreds of candidates something matches everywhere.
fn recurrence_ratio(real: &[Vec<f32>], surrogate: &[f32], lag: usize) -> Option<f32> {
    let mut sample = surrogate.to_vec();
    let threshold = quantile(&mut sample, RECURRENCE_QUANTILE)?;
    let d = distant(real, lag);
    if d.is_empty() {
        return None;
    }
    let rate = d.iter().filter(|v| **v > threshold).count() as f32 / d.len() as f32;
    Some(rate / (1.0 - RECURRENCE_QUANTILE))
}

/// The same streams with their spectra kept and their arrangement destroyed.
///
/// One phase sequence serves every stream — a linear filter applied to all at
/// once — so their correlations survive; a control that decorrelated them would
/// be beaten by any take whose streams move together. The phases are a fixed
/// sequence, not random, so the answer does not move between runs.
fn surrogate(t: &Trajectory, variant: usize) -> Trajectory {
    let n = t.frames.len();
    let dims = t.frames.first().map_or(0, Vec::len);
    // Offset per variant, so the surrogates are different arrangements.
    let phases: Vec<f64> = (0..n)
        .map(|k| {
            let x = ((k + 1) as f64 * GOLDEN + variant as f64 * 0.37).fract();
            x * std::f64::consts::TAU
        })
        .collect();

    let mut frames = vec![vec![0.0f32; dims]; n];
    for d in 0..dims {
        let column: Vec<f64> = t.frames.iter().map(|f| f[d] as f64).collect();
        for (i, v) in phase_scramble(&column, &phases).into_iter().enumerate() {
            frames[i][d] = v as f32;
        }
    }
    Trajectory {
        frames,
        rate_hz: t.rate_hz,
    }
}

/// One real series, its magnitude spectrum kept and its phases replaced.
///
/// A direct transform: the series are a few hundred frames and this runs a few
/// times per take. Conjugate symmetry keeps the result real; DC (the mean) is
/// kept, and at even lengths the unpaired Nyquist bin keeps its sign.
fn phase_scramble(x: &[f64], phases: &[f64]) -> Vec<f64> {
    let n = x.len();
    if n < 4 {
        return x.to_vec();
    }
    let mut out = vec![0.0f64; n];
    let half = n / 2;

    // Forward transform, positive frequencies only.
    let mut spectrum = Vec::with_capacity(half + 1);
    for k in 0..=half {
        let (mut re, mut im) = (0.0f64, 0.0f64);
        for (j, v) in x.iter().enumerate() {
            let a = -std::f64::consts::TAU * (j * k) as f64 / n as f64;
            re += v * a.cos();
            im += v * a.sin();
        }
        spectrum.push((re, im));
    }

    for (j, slot) in out.iter_mut().enumerate() {
        let mut acc = spectrum[0].0; // DC, kept as measured.
        for k in 1..=half {
            let (re, im) = spectrum[k];
            let magnitude = (re * re + im * im).sqrt();
            let nyquist = n.is_multiple_of(2) && k == half;
            let phase = if nyquist { 0.0 } else { phases[k] };
            let a = std::f64::consts::TAU * (j * k) as f64 / n as f64 + phase;
            // Bins below Nyquist stand in for their conjugate partner too.
            let weight = if nyquist { 1.0 } else { 2.0 };
            acc += weight * magnitude * a.cos() * if nyquist { re.signum() } else { 1.0 };
        }
        *slot = acc / n as f64;
    }
    out
}

/// Everything measured about one take, real beside its surrogates.
struct Measured {
    label: String,
    seconds: f32,
    /// Boundary contrast at each of `SCALES_S`, real and mean-surrogate, or `None`
    /// where the take is shorter than the kernel.
    novelty: Vec<Option<(f32, f32)>>,
    /// How many times chance the take returns to itself.
    recurrence: Option<f32>,
}

fn measure(label: String, t: &Trajectory) -> Measured {
    let real = matrix(t);
    let surrogates: Vec<Vec<Vec<f32>>> =
        (0..SURROGATES).map(|v| matrix(&surrogate(t, v))).collect();
    let lag = (RETURN_LAG_S * t.rate_hz).round() as usize;

    let novelty = SCALES_S
        .iter()
        .map(|scale| {
            let half = ((scale * t.rate_hz / 2.0).round() as usize).max(1);
            let r = contrast(&novelty(&real, half)?)?;
            // Reported only where every surrogate could be measured too.
            let mut total = 0.0f32;
            for s in &surrogates {
                total += contrast(&novelty(s, half)?)?;
            }
            Some((r, total / SURROGATES as f32))
        })
        .collect();

    // Pooled across surrogates: one alone is too small a sample on a short take.
    let pooled: Vec<f32> = surrogates.iter().flat_map(|s| distant(s, lag)).collect();

    Measured {
        label,
        seconds: t.frames.len() as f32 / t.rate_hz,
        novelty,
        recurrence: recurrence_ratio(&real, &pooled, lag),
    }
}

fn main() -> anyhow::Result<()> {
    let data_dir = std::env::var("DATA_DIR").unwrap_or_else(|_| "data".into());
    let store = Store::open(&data_dir)?;
    let calibrated = voice::calibrate(&store, None).map_err(|e| anyhow::anyhow!("{e}"))?;
    println!("calibration: {}\n", calibrated.source.id);
    println!(
        "Novelty cells are real / surrogate: the surrogate keeps every stream's\n\
         spectrum and correlations and destroys only the arrangement in time, so\n\
         beating it means arrangement rather than slowness. Contrast is how far the\n\
         clearest boundary stands above an ordinary moment, in the curve's own spread. Return is a multiple of\n\
         chance; 1.0 is a take that revisits no more than its own spectrum implies.\n"
    );

    for gate_silence in [true, false] {
        println!(
            "\n=== {} ===",
            if gate_silence {
                "sounding frames only — what the voice does"
            } else {
                "every frame — pauses included, so a breath can be a boundary"
            }
        );
        let mut rows = Vec::new();
        for meta in &store.list()? {
            let Ok(vp) = store.voiceprint(&meta.id) else {
                continue;
            };
            let Some(t) = trajectory(&vp, &calibrated.voice, gate_silence) else {
                continue;
            };
            // A take with nothing above the ceiling cannot answer the question.
            if t.frames.len() as f32 / t.rate_hz < SCALES_S[1] * 2.0 {
                continue;
            }
            rows.push(measure(meta.label.clone(), &t));
        }
        if rows.is_empty() {
            anyhow::bail!("no take long enough to have structure above the ceiling");
        }
        report(&rows);
    }
    Ok(())
}

/// One table, and the tally underneath it that is the actual answer.
fn report(rows: &[Measured]) {
    print!("{:<26}{:>7}", "take", "sound");
    for scale in SCALES_S {
        print!("{:>16}", format!("novelty {scale}s"));
    }
    println!("{:>14}", "return >5s");

    for r in rows {
        print!("{:<26}{:>6.1}s", elide(&r.label, 25), r.seconds);
        for cell in &r.novelty {
            match cell {
                Some((real, surrogate)) => print!("{:>16}", format!("{real:.2} / {surrogate:.2}")),
                None => print!("{:>16}", "—"),
            }
        }
        match r.recurrence {
            Some(ratio) => println!("{:>14}", format!("{ratio:.2}x")),
            None => println!("{:>14}", "—"),
        }
    }

    println!("\nTakes where real beats its surrogate, per scale:");
    for (k, s) in SCALES_S.iter().enumerate() {
        let judged: Vec<&Measured> = rows.iter().filter(|r| r.novelty[k].is_some()).collect();
        let won = judged
            .iter()
            .filter(|r| {
                let (real, surrogate) = r.novelty[k].expect("filtered to the measured ones");
                real > surrogate
            })
            .count();
        println!("  novelty {s:>4}s   {won}/{}", judged.len());
    }
    let judged: Vec<&Measured> = rows.iter().filter(|r| r.recurrence.is_some()).collect();
    let won = judged
        .iter()
        .filter(|r| r.recurrence.expect("filtered to the measured ones") > 1.0)
        .count();
    println!("  return >5s     {won}/{}", judged.len());
}

/// Trim a label to fit the column, marking that it was trimmed.
fn elide(label: &str, width: usize) -> String {
    if label.chars().count() <= width {
        return label.to_string();
    }
    let kept: String = label.chars().take(width.saturating_sub(1)).collect();
    format!("{kept}…")
}
