//! Whether there is anything to hear when the tuning changes.
//!
//! `bind` should make partials of different voices *lock* rather than *beat*,
//! and beating is an amplitude modulation at the difference frequency — so it
//! can be measured in the rendered **audio**, which the score cannot show. This
//! renders one take twice and compares the slow modulation in each.
//!
//! It can falsify audibility but not establish it: no difference means nothing
//! to hear; a difference still has to clear someone's threshold. The analysis
//! bands are ERB-spaced and wide on purpose — beating shows only when both
//! partials share a band, as they share a place on the cochlea.
//!
//! ```text
//! cargo run --bin beating                          # the sung take, default hold
//! cargo run --bin beating -- 0356e27885ef254c 0.9  # a take, and a hold
//! ```
//! ```text
//! cargo run --bin beating                          # the sung take, default hold
//! cargo run --bin beating -- 0356e27885ef254c 0.9  # a take, and a hold
//! ```

use rustfft::FftPlanner;
use rustfft::num_complex::Complex32;

use utterance::store::Store;
use utterance::voice;
use utterance_mapping::mapping::{CONTINUOUS, Mapping};
use utterance_mapping::params::Params;
use utterance_realisation::synth::{self, RENDER_RATE};

/// Samples per analysis window: 12 ms, bins about 86 Hz wide. Coarse on purpose,
/// so partials a few hertz apart share a bin and their sum pulses; a longer
/// window would resolve them into steady tones.
const WINDOW: usize = 512;

/// Samples between windows: the envelope is sampled at about 344 Hz.
const HOP: usize = 128;

/// Slowest and fastest modulation counted as beating, in hertz. Measured: at
/// `bind = 1` the strongest coincidences beat at 0.01–0.26 Hz, at 0 at 4.8–14.3.
/// Below 2 Hz a chord sounds steady; above 20 a beat becomes roughness.
const BEAT_LO_HZ: f32 = 2.0;
const BEAT_HI_HZ: f32 = 20.0;

/// Lowest and highest band edges, in hertz.
const BAND_LO_HZ: f32 = 60.0;
const BAND_HI_HZ: f32 = 8000.0;

/// Equivalent rectangular bandwidth at a centre frequency, in hertz (Glasberg
/// and Moore).
fn erb(hz: f32) -> f32 {
    24.7 * (0.00437 * hz + 1.0)
}

/// Band edges from [`BAND_LO_HZ`] up, each one ERB wide.
fn bands() -> Vec<(f32, f32)> {
    let mut edges = Vec::new();
    let mut lo = BAND_LO_HZ;
    while lo < BAND_HI_HZ {
        let hi = lo + erb(lo);
        edges.push((lo, hi.min(BAND_HI_HZ)));
        lo = hi;
    }
    edges
}

/// Energy per band per frame: the envelope each band's contents ride on.
fn envelopes(samples: &[f32], bands: &[(f32, f32)]) -> Vec<Vec<f32>> {
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(WINDOW);

    // Hann, so a partial between bins does not leak into distant bands.
    let window: Vec<f32> = (0..WINDOW)
        .map(|i| {
            let x = std::f32::consts::PI * i as f32 / WINDOW as f32;
            x.sin() * x.sin()
        })
        .collect();

    let bin_hz = RENDER_RATE as f32 / WINDOW as f32;
    let frames = samples.len().saturating_sub(WINDOW) / HOP;
    let mut out = vec![Vec::with_capacity(frames); bands.len()];

    let mut buffer = vec![Complex32::new(0.0, 0.0); WINDOW];
    for f in 0..frames {
        let start = f * HOP;
        for (i, slot) in buffer.iter_mut().enumerate() {
            *slot = Complex32::new(samples[start + i] * window[i], 0.0);
        }
        fft.process(&mut buffer);

        for (b, &(lo, hi)) in bands.iter().enumerate() {
            let first = (lo / bin_hz).floor() as usize;
            let last = ((hi / bin_hz).ceil() as usize).min(WINDOW / 2);
            let energy: f32 = (first..last).map(|k| buffer[k].norm_sqr()).sum();
            // Amplitude, not power: amplitude is what a beat modulates linearly.
            out[b].push(energy.sqrt());
        }
    }
    out
}

/// How deeply one band's envelope pulses in the beating range, 0..1-ish —
/// normalised by the band's mean, so a quiet band that pulses fully counts.
fn modulation_depth(envelope: &[f32]) -> f32 {
    if envelope.len() < 8 {
        return 0.0;
    }
    let mean = envelope.iter().sum::<f32>() / envelope.len() as f32;
    if mean <= f32::EPSILON {
        return 0.0;
    }

    let n = envelope.len();
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(n);
    let mut buffer: Vec<Complex32> = envelope
        .iter()
        .map(|v| Complex32::new(v - mean, 0.0))
        .collect();
    fft.process(&mut buffer);

    let envelope_rate = RENDER_RATE as f32 / HOP as f32;
    let bin_hz = envelope_rate / n as f32;
    let first = (BEAT_LO_HZ / bin_hz).ceil() as usize;
    let last = ((BEAT_HI_HZ / bin_hz).floor() as usize).min(n / 2);
    if first >= last {
        return 0.0;
    }

    let energy: f32 = (first..last).map(|k| buffer[k].norm_sqr()).sum();
    // Root-mean-square of the modulation, against the mean it rides on.
    (energy.sqrt() / n as f32) / mean
}

/// Modulation depth across the render, weighted by band loudness: an empty
/// band's envelope is noise, which modulates at every frequency.
fn beating(samples: &[f32]) -> f32 {
    let bands = bands();
    let envelopes = envelopes(samples, &bands);
    let mut total = 0.0;
    let mut weight = 0.0;
    for envelope in &envelopes {
        let level = envelope.iter().sum::<f32>() / envelope.len().max(1) as f32;
        total += modulation_depth(envelope) * level;
        weight += level;
    }
    if weight > 0.0 { total / weight } else { 0.0 }
}

/// Whether changing `bind` on this mapping leaves the chords alone — the whole
/// validity of the comparison. Matched exhaustively, so a new mapping must
/// answer before it can be measured.
#[expect(
    clippy::match_same_arms,
    reason = "Field and Tonnetz both answer true for unrelated reasons, and each \
              arm's comment is the reason. Merging them into one `|` arm would \
              leave a single comment covering two different arguments."
)]
fn holds_the_chord_still(mapping: Mapping) -> bool {
    match mapping {
        // Voices at a fixed spacing in degrees: retuning moves the same chord.
        Mapping::Field => true,
        // `bind` applies per sounding pitch, so the chord sequence is unchanged.
        Mapping::Tonnetz => true,
        // Onsets: nothing held long enough to beat.
        Mapping::Notes => false,
    }
}

/// The rendered audio of one mapping at these settings.
fn samples(
    mapping: Mapping,
    vp: &utterance_analysis::voiceprint::Voiceprint,
    voice: &utterance_mapping::voice::Voice,
    params: Params,
) -> Vec<f32> {
    synth::render(&mapping.score_with(vp, voice, params))
}

fn main() -> anyhow::Result<()> {
    let data_dir = std::env::var("DATA_DIR").unwrap_or_else(|_| "data".into());
    let store = Store::open(&data_dir)?;
    let calibrated = voice::calibrate(&store, None).map_err(|e| anyhow::anyhow!("{e}"))?;

    let args: Vec<String> = std::env::args().skip(1).collect();
    // The sung take from the listening test, which is where the chords hold.
    let take = args
        .first()
        .cloned()
        .unwrap_or_else(|| "0356e27885ef254c".into());
    let hold: f32 = match args.get(1) {
        Some(h) => h.parse()?,
        None => Params::default().hold,
    };

    let meta = store
        .list()?
        .into_iter()
        .find(|m| m.id == take || m.label == take)
        .ok_or_else(|| anyhow::anyhow!("no take called {take}"))?;
    let vp = store.voiceprint(&meta.id)?;
    let voice = &calibrated.voice;

    println!(
        "take: {} ({:.1}s)   hold = {hold:.2}",
        meta.label, meta.duration_s
    );

    // The scale and the chord the lattice builds from it: the curve only
    // measures dyads against the tonic, so the triangle's third is worth seeing.
    let lattice = utterance_mapping::lattice::Lattice::from_tuning(&voice.tuning)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let degrees: Vec<String> = voice
        .tuning
        .degrees
        .iter()
        .map(|d| format!("{:.0}", d.cents))
        .collect();
    println!("scale: {}", degrees.join(", "));
    println!(
        "lattice axes: {:.0} and {:.0}; the up-triangle is 0, {:.0}, {:.0}, \
         whose own internal interval is {:.0}\n",
        lattice.a_cents,
        lattice.b_cents,
        lattice.b_cents.min(lattice.a_cents),
        lattice.a_cents.max(lattice.b_cents),
        (lattice.a_cents - lattice.b_cents).abs(),
    );

    let at = |mapping: Mapping, params: Params| beating(&samples(mapping, &vp, voice, params));
    let with = |bind: f32| Params {
        bind,
        hold,
        ..Params::default()
    };

    // A ruler that cannot say "different" cannot be believed saying "same": a
    // cluster against an open chord must move this number.
    let cluster = at(
        Mapping::Field,
        Params {
            spacing: 1,
            ..with(1.0)
        },
    );
    let open = at(
        Mapping::Field,
        Params {
            spacing: 6,
            ..with(1.0)
        },
    );
    let sensitivity = if cluster > 0.0 { open / cluster } else { 0.0 };
    println!("  control: a cluster against an open chord reads {sensitivity:.2}×");
    if (sensitivity - 1.0).abs() < 0.10 {
        println!("  => the measure does not move for a change nobody could miss.");
        println!("     Nothing below is worth reading.\n");
        return Ok(());
    }
    println!();

    // Continuous mappings only: onsets hold nothing long enough to beat.
    for mapping in CONTINUOUS.iter().copied() {
        let (locked, tempered) = (at(mapping, with(1.0)), at(mapping, with(0.0)));
        let ratio = if locked > 0.0 { tempered / locked } else { 0.0 };
        println!(
            "  {:<8} bind=1 {locked:.4}   bind=0 {tempered:.4}   {ratio:.2}×",
            mapping.name()
        );
        if !holds_the_chord_still(mapping) {
            println!(
                "           (retuning also changes what this mapping plays, so the\n                 \x20           two renders are different chords — not a test of tuning)"
            );
        }
    }

    Ok(())
}
