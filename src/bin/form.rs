//! Whether an utterance has structure above the two-second ceiling — and whether
//! that structure is in the voice or in the method.
//!
//! **Why this exists.** Two gaps in the roadmap are one gap: the derived tuning
//! is inaudible because no chord rings for the second a beat needs, and nothing
//! in the field operates above `streams::DRIFT_FRAMES` — two seconds. Both want
//! a harmonic rhythm slow enough to have sections, and the only routes to one
//! that have been tried, meter and the symbol stream, are blocked behind
//! syllables somebody marks by ear.
//!
//! Recurrence is a third route and it needs no labels. If the mouth returns to
//! where it has been, that return is measurable directly from the streams a
//! mapping already reads, and a harmony that returns with it would hold at the
//! timescale of the returning rather than of the syllable.
//!
//! **This tool does not build that.** It asks the one question that decides
//! whether it is worth building: is there recurrent structure above two seconds
//! in real takes, above what the same material produces with its long-range
//! order destroyed? A yes is a licence to build a form layer. A no costs one
//! tool instead of a layer, which is the point of asking here first.
//!
//! **The control is the whole argument.** A self-similarity matrix over smooth
//! streams looks blocky whatever the material, because neighbouring frames
//! resemble each other by construction, and a novelty curve over any smooth
//! signal has peaks. So every figure is reported beside the same figure computed
//! on a surrogate: the take's streams with their Fourier magnitudes kept exactly
//! and their phases replaced. That preserves every stream's power spectrum, and
//! therefore all of its smoothness and all of its slow drift, and — because one
//! phase sequence is applied to all eight streams together — every correlation
//! between them. What it destroys is the arrangement in time and nothing else.
//!
//! **Why not a block shuffle.** That was the first control here and it was
//! wrong in a way worth leaving on the record. Permuting one-second blocks
//! manufactures a discontinuity at every block edge, so a one-second kernel
//! lands on a fabricated boundary once a second and a sixteen-second kernel
//! averages them all away. Its strength therefore varied with the very axis the
//! tool measures along, and real material duly "lost" at 1 s and "won" at 16 s —
//! a result that was a property of the control. A control whose bias runs along
//! the axis under test cannot settle anything on that axis.
//!
//! **A surrogate that keeps the drift is what makes the answer mean something.**
//! Slow structure and slow *trend* are different claims: a take that simply gets
//! quieter throughout has structure at sixteen seconds in no sense worth
//! building a harmony on. The surrogate has that take's trend too, so beating it
//! requires arrangement rather than slowness.
//!
//! **What it cannot answer.** Silence is gated out, for the reason `streams`
//! gates it: every stream reports something constant in digital silence, so two
//! pauses resemble each other perfectly and recurrence would mostly be counting
//! them. That makes this a measure of what the voice *does*, not of where it
//! stops — and pauses are real phrase structure, so this measurement is a floor
//! on the structure present rather than an estimate of it.
//!
//! ```text
//! cargo run --bin form
//! ```

use utterance::store::Store;
use utterance::voice;
use utterance_analysis::voiceprint::Voiceprint;
use utterance_mapping::streams;
use utterance_mapping::voice::Voice;

/// How far below a take's loudest frame still counts as sounding.
///
/// The same gate `dwell` and `streams` use, and here for their reason: silence
/// makes every stream agree with every other, and a recurrence measure over
/// silent frames would report the pauses as the structure.
const PEAK_DROP_DB: f32 = 40.0;

/// Frames per second the similarity matrix is built at.
///
/// Ten. The question is about structure measured in seconds, and at the
/// analyser's own 100 Hz a minute-long take is a 6000-square matrix answering it
/// no better. Every stream is smoothed at its own timescale before this
/// decimation, so what is dropped is detail the smoothing already removed rather
/// than detail the matrix would have used.
const COARSE_HZ: f32 = 10.0;

/// Kernel widths, in seconds, that novelty is measured at.
///
/// Deliberately straddling the ceiling: 1 and 2 seconds are inside what the
/// field already reaches, 4 through 16 are the region nothing in the project
/// operates at. Structure that shows only at 1 s would be the syllable rate
/// found again under a new name.
const SCALES_S: [f32; 5] = [1.0, 2.0, 4.0, 8.0, 16.0];

/// Nearest a frame may be and still count as a return rather than as itself.
///
/// Five seconds, which is past every timescale the field reads, so nothing here
/// can be satisfied by a stream that is merely slow. A vowel held for four
/// seconds resembles itself throughout and that is not a return; the mouth
/// leaving and coming back is.
const RETURN_LAG_S: f32 = 5.0;

/// How many surrogates each figure is compared against.
///
/// Five, averaged. One surrogate is one draw and its peak novelty moves with the
/// phases it happened to get; the question is whether real material beats what
/// its own spectrum typically produces, not whether it beats one instance.
const SURROGATES: usize = 5;

/// Fraction of surrogate frame pairs that define "resembling".
///
/// The recurrence threshold is read off the surrogate's own distribution at this
/// quantile, so the surrogate scores 5% by construction and the real take's
/// score is a multiple of it. An absolute threshold could not be chosen without
/// deciding in advance how similar counts as similar, which is the thing being
/// measured.
const RECURRENCE_QUANTILE: f32 = 0.95;

/// The increment the surrogate's phases advance by, as a turn of the circle.
///
/// The golden ratio's fractional part, which is equidistributed for any length,
/// so no take gets a phase sequence that happens to repeat inside itself.
const GOLDEN: f64 = 0.618_033_988_749_895;

/// One take's per-frame feature vectors, one vector per coarse frame.
struct Trajectory {
    frames: Vec<Vec<f32>>,
    rate_hz: f32,
}

/// Every stream a continuous mapping reads, gated to sounding frames, decimated
/// and standardised.
///
/// Standardised per stream over the take, because cosine similarity between
/// these vectors is otherwise a statement about which stream has the largest
/// units. Hertz would decide every comparison and openness would never be heard
/// from.
fn trajectory(vp: &Voiceprint, voice: &Voice, gate_silence: bool) -> Option<Trajectory> {
    let (open, front) = streams::vowel(vp, voice);
    let peak = vp.rms_db.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let sounding: Vec<bool> = vp
        .rms_db
        .iter()
        .map(|db| !gate_silence || *db > peak - PEAK_DROP_DB)
        .collect();

    // Smoothed first and gated second, as `streams` does it: gating first
    // splices the sounding frames together and runs a moving average across a
    // pause as though the voice had carried on through it.
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

    // `frame.analysis_rate_hz` is the audio rate after resampling, not the frame
    // rate: `hop_s` is the seconds between frames, and this grid is indexed in
    // frames. Reading the wrong one decimates by 1600 instead of 10 and leaves a
    // minute-long take three frames long.
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

/// Zero mean, unit variance. A stream that never moves is returned as zeros
/// rather than divided by nothing, so it contributes to no comparison instead of
/// dominating every one.
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

/// Foote novelty: a checkerboard kernel dragged down the diagonal.
///
/// Two quadrants reward self-similarity either side of the point and two punish
/// similarity across it, so the curve peaks where the take stops resembling what
/// it was and starts resembling what it becomes. The kernel is Gaussian-tapered,
/// which is what keeps a peak from being decided by the two frames at the
/// kernel's corners.
///
/// Returns `None` where the take is shorter than the kernel: a novelty curve
/// measured over less than one kernel width is measuring the edge of the take.
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

/// How far the strongest boundary stands above an ordinary moment, in the
/// curve's own units.
///
/// **Not the raw maximum.** A maximum is a competition the noisier curve wins,
/// and a phase surrogate's novelty curve is noisier than a real take's at every
/// scale — so comparing raw peaks reported real material as having *less*
/// structure than its surrogate at 1 s, where nobody claims it has any. That is
/// the third statistic in this tool to be biased by taking an extreme over many
/// draws, after the recurrence maximum above, and the second to be caught by the
/// control rather than by reading it.
///
/// Standardising by the curve's own spread asks the question that was meant: a
/// boundary is a moment that stands out from this take's other moments, and how
/// jumpy the curve is overall is exactly what has to be divided out.
fn contrast(curve: &[f32]) -> Option<f32> {
    // The kernel cannot reach the first and last half-width, which are left at
    // zero; including them would put a spike of zeros into the spread.
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

/// Every similarity between frames far enough apart to be a return.
///
/// Anything closer than `RETURN_LAG_S` is dropped, not because it is
/// uninteresting but because it is not a return: a vowel held for four seconds
/// resembles itself throughout, and counting that would report sustain as form.
fn distant(s: &[Vec<f32>], lag: usize) -> Vec<f32> {
    let mut out = Vec::new();
    for (i, row) in s.iter().enumerate() {
        out.extend(row.iter().skip(i + lag).copied());
    }
    out
}

/// The value at a quantile of a sample, by nearest rank.
///
/// Sorted by `total_cmp` rather than `partial_cmp`: a similarity here cannot be
/// NaN, because `similarity` returns zero rather than dividing by a zero norm —
/// but a total order costs nothing and does not depend on that guarantee holding
/// after somebody edits the function it comes from.
fn quantile(values: &mut [f32], q: f32) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f32::total_cmp);
    let k = ((values.len() - 1) as f32 * q).round() as usize;
    Some(values[k])
}

/// How much more often than chance the take returns to where it has been.
///
/// The threshold is the surrogate's own `RECURRENCE_QUANTILE`, so the surrogate
/// scores that quantile's complement by construction and the real take is
/// reported as a multiple of it. One means the mouth revisits no more than its
/// own spectrum would produce by arrangement alone.
///
/// **A maximum was tried here first and saturated.** Reporting, per frame, the
/// best resemblance to any distant frame gave 0.91 against a control's 0.94: with
/// hundreds of candidates and eight dimensions, something matches well by chance
/// almost everywhere, and the statistic had no room left to show a difference.
/// A rate above a threshold has that room; a maximum over many draws does not.
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
/// Each stream's Fourier magnitudes are left exactly as measured and every phase
/// is replaced, so the surrogate has the take's smoothness, its variance and its
/// slow drift and none of its order. **One phase sequence serves all streams**,
/// which is what keeps the correlations between them intact: replacing phases
/// identically is a linear filter applied to every stream at once, and a control
/// that also decorrelated the streams would be beaten by a take merely for
/// having streams that move together.
///
/// The phases are a fixed equidistributed sequence rather than drawn from a
/// generator. Analysis in this project is a pure function of the audio, and a
/// control that moved between runs would let this tool's answer move with it.
fn surrogate(t: &Trajectory, variant: usize) -> Trajectory {
    let n = t.frames.len();
    let dims = t.frames.first().map_or(0, Vec::len);
    // Golden-ratio increments: equidistributed for any length, and offset per
    // variant so five surrogates are five different arrangements rather than one
    // repeated.
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
/// A direct transform rather than an FFT: the series here are a few hundred
/// frames long, this runs a handful of times per take, and a dependency bought
/// for it would have to be justified to everyone who builds the workspace.
///
/// The conjugate symmetry is what keeps the result real. DC is left alone
/// because it is the stream's mean, which the surrogate is meant to preserve,
/// and at even lengths the Nyquist bin has no partner to be symmetric with, so
/// its sign is kept rather than given a phase it cannot carry.
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
            // A scale is reported only where every surrogate could be measured
            // at it too, so the pair being compared is always like for like.
            let mut total = 0.0f32;
            for s in &surrogates {
                total += contrast(&novelty(s, half)?)?;
            }
            Some((r, total / SURROGATES as f32))
        })
        .collect();

    // Pooled across surrogates: one surrogate's distant similarities are a small
    // sample on a short take, and the threshold read off it would move with the
    // sample rather than with the material.
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
            // A take with nothing above the ceiling cannot answer a question about
            // what is above the ceiling, and averaging it in would answer it wrongly.
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
