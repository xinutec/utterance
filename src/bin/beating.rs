//! Whether there is anything to hear when the tuning changes.
//!
//! **The claim under test.** `bind` is supposed to work by making partials of
//! different voices *lock* rather than *beat*. Two partials a few hertz apart do
//! not sound like two tones: they sound like one tone whose loudness pulses at
//! their difference frequency. So the whole effect of `bind` — if it has one —
//! is an amplitude modulation, and an amplitude modulation is something a
//! machine can measure in the rendered audio.
//!
//! This renders the same take twice and compares how much slow modulation each
//! carries. It measures the **audio**, not the score: everything before this
//! measured frequencies the synthesiser was *asked* for, which cannot show
//! beating at all, because beating is what happens when two of those frequencies
//! are added together.
//!
//! **What it can and cannot settle, which is the point of running it.** The two
//! directions are not symmetric:
//!
//! - *No modulation difference* → there is nothing to hear. Decisive, and no
//!   ears required: the mechanism does not work, rather than the listener
//!   failing to notice.
//! - *A modulation difference* → something is physically there, and whether it
//!   is above anyone's threshold is still a listening question.
//!
//! So this can falsify audibility but not establish it, and after a listening
//! session that reported "very little difference" the falsifying direction is
//! the one worth having.
//!
//! **Why the analysis bands are wide.** Beating appears as modulation only when
//! the two partials fall inside *one* band — resolve them into separate bins and
//! each looks like a steady tone and the pulsing vanishes. That is also how the
//! ear works, which is why the bands here are ERB-spaced rather than uniform:
//! the measurement has to be as blunt as a cochlea or it will not see what a
//! cochlea sees.
//!
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

/// Samples per analysis window.
///
/// 512 at 44.1 kHz is 12 ms, giving bins about 86 Hz wide. Chosen to be
/// *coarse*: two partials 16 cents apart near 500 Hz sit around 5 Hz apart, and
/// they have to land in the same bin for their sum to pulse. A longer window
/// resolves them into two steady tones and reports no beating at all — which
/// would be an artefact of the ruler, not a fact about the sound.
const WINDOW: usize = 512;

/// Samples between one window and the next.
///
/// A quarter of the window, so the envelope is sampled at about 344 Hz — far
/// above the 20 Hz ceiling below, with room to spare for the modulation FFT.
const HOP: usize = 128;

/// Slowest and fastest modulation counted as beating, in hertz.
///
/// From the measurement this exists to check: at `bind = 1` the five strongest
/// partial coincidences beat at 0.01–0.26 Hz, and at `bind = 0` the same ones
/// beat at 4.8–14.3 Hz. The floor is above the first of those on purpose — a
/// beat slower than 2 Hz is heard as the chord being steady — and the ceiling is
/// where beating stops being a pulse and starts being roughness.
const BEAT_LO_HZ: f32 = 2.0;
const BEAT_HI_HZ: f32 = 20.0;

/// Lowest and highest band edges, in hertz.
const BAND_LO_HZ: f32 = 60.0;
const BAND_HI_HZ: f32 = 8000.0;

/// Equivalent rectangular bandwidth at a centre frequency, in hertz.
///
/// Glasberg and Moore's fit. Bands this wide are what makes two nearby partials
/// share one channel and so beat, rather than being resolved into two tones.
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

    // Hann, so a partial sitting between two bins does not smear across the
    // spectrum and put energy in bands it is nowhere near.
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
            // Amplitude rather than power, because that is what modulates
            // linearly with a beat: two equal partials in phase sum to twice the
            // amplitude and out of phase to nothing.
            out[b].push(energy.sqrt());
        }
    }
    out
}

/// How deeply one band's envelope pulses in the beating range, 0..1-ish.
///
/// Normalised by the band's own mean, so this is a modulation *depth* rather
/// than a loudness: a quiet band that pulses fully counts as much as a loud one
/// that does, which is roughly how hearing treats it and is certainly how the
/// question is posed.
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

/// Modulation depth across the whole render, weighted by how loud each band is.
///
/// Weighted because an empty band's envelope is noise, and noise has plenty of
/// modulation at every frequency — unweighted, thirty silent high bands would
/// drown the handful carrying the chord.
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

/// Whether changing `bind` on this mapping leaves the chord's structure alone.
///
/// **This is the whole validity of the comparison.** The claim is that a derived
/// scale makes partials lock where a tempered one makes them beat, which is a
/// statement about one chord under two tunings. On the Tonnetz retuning also
/// moves the lattice axes, so the two renders would be different chords and any
/// difference in their beating confounded by that.
///
/// Matched exhaustively rather than defaulted to true: a mapping added to the
/// crate has to answer this before it can be measured here, and answering wrong
/// by omission is how a confounded comparison gets published as a result.
#[expect(
    clippy::match_same_arms,
    reason = "Field and Tonnetz both answer true for unrelated reasons, and each \
              arm's comment is the reason. Merging them into one `|` arm would \
              leave a single comment covering two different arguments — and the \
              Tonnetz one is a fact about a change that has already caught this \
              measurement out once."
)]
fn holds_the_chord_still(mapping: Mapping) -> bool {
    match mapping {
        // Voices stacked at a fixed spacing in scale degrees, so retuning the
        // scale moves the *same* chord. The controlled experiment.
        Mapping::Field => true,
        // True since `bind` moved from the lattice axes to the sounding pitch.
        // Before that, retuning rebuilt the geometry and the two renders were
        // different chord sequences.
        Mapping::Tonnetz => true,
        // Onsets, not a sustained chord. There are no partials held together
        // long enough to beat, so the measurement has nothing to look at.
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

    // The scale, and the chord the lattice actually builds from it. Printed
    // because the curve the scale comes from measures *dyads against the
    // tonic*, and a triangle's third interval is one nobody ever measured.
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

    // **A measurement that cannot say "different" cannot be believed when it
    // says "same".** Two settings nobody would confuse — a cluster against an
    // open chord — have to move this number, or a null result below is a
    // property of the ruler.
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

    // The continuous mappings only: this measures partials of a sustained chord
    // beating against each other, and discrete onsets hold nothing long enough
    // to have any.
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
