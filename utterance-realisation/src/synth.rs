//! Summing sinusoids, and the several things that stop that sounding dead.
//!
//! A bare sum of steady sinusoids is the sound every naive additive synthesiser
//! makes: correct in every partial and lifeless in every other respect. Four
//! things here are what a real tone has and that does not, and each is a
//! *capability* rather than a choice — the score says how much of each, so
//! nothing below decides anything musical.
//!
//! - **The spectrum moves.** Each note interpolates across the score's palette
//!   from its start colour to its end colour. A spectrum that holds still is
//!   most of what makes additive synthesis sound like an organ.
//! - **High partials die first.** Every real resonator damps high frequencies
//!   faster than low, so a tone darkens as it decays.
//! - **Partials are not exactly locked.** A trace of detune, from the speaker's
//!   own pitch instability, is the difference between alive and machine-made.
//! - **There is noise in it.** Breath, bow, wind: no acoustic sound is purely
//!   periodic, and the absence of noise is heard as sterility. The noise is
//!   filtered to sit where the tone's own energy sits — unfiltered white noise
//!   is heard as tape hiss laid over the music rather than as part of it.

use utterance_mapping::score::{Event, Field, NoiseEvent, Score};

/// Rate everything is rendered at.
///
/// 44.1 kHz because the output is for listening rather than for analysis, and
/// this is what every browser and audio player expects without resampling.
pub const RENDER_RATE: u32 = 44_100;

/// Attack and release of a note, in seconds.
///
/// Short, but never zero: a sinusoid switched on mid-cycle is a step
/// discontinuity, and a step is a click. Long enough to remove that, short
/// enough that the onset still reads as an onset.
const ATTACK_S: f32 = 0.012;
const RELEASE_S: f32 = 0.09;

/// How much faster the top of the spectrum decays than the bottom.
///
/// By the end of a note the highest partial retains this fraction of the level
/// the fundamental keeps. Damping rises with frequency in every real resonator,
/// and without it a decaying note keeps its attack brightness the whole way down
/// — which reads as synthetic long before anyone can say why.
const HIGH_PARTIAL_SURVIVAL: f32 = 0.35;

/// Peak level the finished render is scaled to.
///
/// Under full scale on purpose: notes overlap, and a render normalised to
/// exactly 1.0 leaves no room for the intersample peaks that appear when it is
/// converted for playback.
const HEADROOM: f32 = 0.89;

/// Samples between recalculations of a note's moving spectrum.
///
/// About a millisecond and a half. Interpolating per sample would be exact and
/// pointless — a spectrum crossing the palette over a whole note moves far more
/// slowly than this, and the saving is what keeps rendering a long take quick.
const SPECTRUM_HOP: usize = 64;

/// Fractional part of the golden ratio, to the precision an `f32` holds.
///
/// Used to space partial phases: successive multiples of an irrational number
/// fill the interval about as evenly as anything can, so no two partials start
/// near the same phase and none of them line up periodically.
const GOLDEN_FRACTION: f32 = 0.618_034;

/// Width of the band a note's breath is shaped into, as a fraction of its
/// centre.
///
/// Breath in a voice is noise driven through the same resonances that shape the
/// tone, so it carries the vowel's colour rather than being white. Wide enough
/// to still read as air, narrow enough that it belongs to the note instead of
/// sitting on top of it.
const BREATH_BANDWIDTH_RATIO: f32 = 0.9;

/// Render a score to mono samples at [`RENDER_RATE`].
///
/// Deterministic, as everything in this project is: no clock, no system
/// randomness, and the noise below comes from a counter-seeded generator, so the
/// same score renders to the same bytes on every run.
pub fn render(score: &Score) -> Vec<f32> {
    let length = (score.duration_s.max(0.0) * RENDER_RATE as f32).ceil() as usize;
    let mut out = vec![0.0f32; length];
    if length == 0 {
        return out;
    }

    if let Some(field) = &score.field {
        sum_field(&mut out, field, score);
    }

    for (index, event) in score.events.iter().enumerate() {
        sum_note(&mut out, event, score, index);
    }

    // Seeded past the notes so a consonant never draws the same noise as the
    // breath of the note beside it, which would correlate the two and read as
    // one sound rather than two.
    for (index, event) in score.noise.iter().enumerate() {
        sum_noise(&mut out, event, score.events.len() + index);
    }

    normalise(&mut out);
    out
}

/// Add one note to the buffer.
fn sum_note(out: &mut [f32], event: &Event, score: &Score, index: usize) {
    let start = (event.start_s * RENDER_RATE as f32).max(0.0) as usize;
    if start >= out.len() || event.hz <= 0.0 || event.duration_s <= 0.0 {
        return;
    }
    let samples = (event.duration_s * RENDER_RATE as f32) as usize;
    let end = (start + samples).min(out.len());

    let width = spectrum_width(score);
    if width == 0 {
        return;
    }

    // Detune is fixed per partial for the whole note, not wandering: a partial
    // that drifts is vibrato, and vibrato is a musical decision that belongs
    // upstream. This is the static mistuning a real resonator has.
    let mut noise = Noise::seeded(index as u32);
    let detune: Vec<f32> = (0..width)
        .map(|_| {
            let spread = noise.next_bipolar() * score.detune_cents;
            2f32.powf(spread / 1200.0)
        })
        .collect();

    // Deterministic per-partial phase. All-zero phases make every partial peak
    // at once, which concentrates the waveform into a spike: the same energy
    // arrives as a click rather than as a tone, and it wastes headroom the rest
    // of the render then has to be scaled down to accommodate.
    let phase: Vec<f32> = (0..width)
        .map(|k| (k * k) as f32 * GOLDEN_FRACTION * std::f32::consts::TAU)
        .collect();

    let nyquist = RENDER_RATE as f32 / 2.0;
    let pitched = 1.0 - event.breath.clamp(0.0, 1.0);
    let breath = event.breath.clamp(0.0, 1.0);

    let mut spectrum = vec![0.0f32; width];
    let mut gain = 0.0f32;

    // Breath is shaped by a resonator centred on where the note's own energy
    // sits, recomputed as the spectrum moves. White noise here was the first
    // thing a listener noticed: it reads as hiss over the piece rather than as
    // a quality of the tone.
    let mut breath_state = (0.0f32, 0.0f32);
    let mut breath_filter = Resonator::silent();

    for (i, sample) in out[start..end].iter_mut().enumerate() {
        let t = i as f32 / RENDER_RATE as f32;
        let progress = (t / event.duration_s).clamp(0.0, 1.0);

        // Recomputed on a coarse grid; the spectrum moves far slower than audio.
        if i % SPECTRUM_HOP == 0 {
            let colour = event.colour_from + (event.colour_to - event.colour_from) * progress;
            spectrum = score.spectrum_at(colour);
            spectrum.resize(width, 0.0);
            damp(&mut spectrum, progress);
            // Constant power however many partials survived, so a low note
            // keeping twenty-four is not louder than a high one keeping six.
            gain = 1.0 / spectrum.iter().sum::<f32>().max(f32::EPSILON);

            if breath > 0.0 {
                let centre = event.hz * spectral_centroid(&spectrum);
                breath_filter = Resonator::at(centre, centre * BREATH_BANDWIDTH_RATIO);
            }
        }

        let envelope = envelope(t, event.duration_s);
        let mut value = 0.0f32;
        for (k, &amplitude) in spectrum.iter().enumerate() {
            if amplitude <= 0.0 {
                continue;
            }
            let hz = event.hz * (k + 1) as f32 * detune[k];
            // Partials past Nyquist alias down into the audible range as
            // inharmonic rubbish, which is heard as the tuning being wrong
            // rather than as the synthesiser being wrong.
            if hz >= nyquist {
                break;
            }
            value += amplitude * (std::f32::consts::TAU * hz * t + phase[k]).sin();
        }

        let breath_sample = breath_filter.step(noise.next_bipolar(), &mut breath_state);
        *sample += (value * gain * pitched + breath_sample * breath) * envelope * event.amplitude;
    }
}

/// Partials each field voice is rendered with.
///
/// Fewer than a note gets. Five voices at the full twenty-four would be a
/// hundred and twenty oscillators per sample, and the partials past this
/// contribute less to the sound than the fifth voice does.
const FIELD_PARTIALS: usize = 12;

/// Render the continuously sounding field.
///
/// **Phase is accumulated, never recomputed from the elapsed time.** A voice
/// here changes frequency every frame, and `sin(2πft)` with a moving `f` jumps
/// discontinuously at each change — a click a hundred times a second, which is
/// itself a tone at the frame rate. Advancing a phase by the current frequency
/// each sample is the only way a glide sounds like a glide.
fn sum_field(out: &mut [f32], field: &Field, score: &Score) {
    let frames = field.frames();
    let voice_count = field.voice_count();
    if frames == 0 || voice_count == 0 {
        return;
    }

    let nyquist = RENDER_RATE as f32 / 2.0;
    let samples_per_frame = field.hop_s * RENDER_RATE as f32;

    // One accumulator per partial per voice, carried for the whole piece.
    let mut phase = vec![vec![0.0f32; FIELD_PARTIALS]; voice_count];
    let mut breath_phase = (0.0f32, 0.0f32);
    let mut noise = Noise::seeded(0x5EED);

    let mut spectrum = Vec::new();
    let mut spectrum_gain = 0.0f32;
    let mut breath_filter = Resonator::silent();

    for (i, sample) in out.iter_mut().enumerate() {
        // Where this sample sits on the frame grid, and how far between frames.
        let position = i as f32 / samples_per_frame;
        let frame = (position as usize).min(frames - 1);
        let next = (frame + 1).min(frames - 1);
        let blend = position - frame as f32;

        if i % SPECTRUM_HOP == 0 {
            spectrum = score.spectrum_at(field.colour[frame]);
            spectrum.truncate(FIELD_PARTIALS);
            spectrum_gain = 1.0 / spectrum.iter().sum::<f32>().max(f32::EPSILON);
        }

        let breath = field.breath[frame].clamp(0.0, 1.0);
        let mut value = 0.0f32;

        for (v, phases) in phase.iter_mut().enumerate() {
            // Interpolated across the frame boundary, so a voice moving between
            // degrees glides rather than stepping.
            let hz = lerp(field.voices[v][frame], field.voices[v][next], blend);
            let gain = lerp(field.gains[v][frame], field.gains[v][next], blend);
            if gain <= 0.0 || hz <= 0.0 {
                continue;
            }

            let mut voiced = 0.0f32;
            for (k, &amplitude) in spectrum.iter().enumerate() {
                let partial_hz = hz * (k + 1) as f32;
                if partial_hz >= nyquist {
                    break;
                }
                phases[k] += std::f32::consts::TAU * partial_hz / RENDER_RATE as f32;
                if amplitude > 0.0 {
                    voiced += amplitude * phases[k].sin();
                }
            }
            value += voiced * spectrum_gain * gain;
        }

        if breath > 0.0 {
            if i % SPECTRUM_HOP == 0 {
                let centre = field.voices[0][frame] * spectral_centroid(&spectrum);
                breath_filter = Resonator::at(centre, centre * BREATH_BANDWIDTH_RATIO);
            }
            let air = breath_filter.step(noise.next_bipolar(), &mut breath_phase);
            value = value * (1.0 - breath) + air * breath * field.gains[0][frame];
        }

        *sample += value;
    }
}

/// Linear interpolation between two values.
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Add one consonant to the buffer.
///
/// A two-pole resonator driven by white noise. The same arithmetic the vocal
/// tract does to the glottal source, which is why it is the right shape here:
/// a fricative *is* noise through a resonance, so reproducing the measured
/// centre and width reproduces the sound rather than approximating it.
fn sum_noise(out: &mut [f32], event: &NoiseEvent, seed: usize) {
    let start = (event.start_s * RENDER_RATE as f32).max(0.0) as usize;
    if start >= out.len() || event.duration_s <= 0.0 {
        return;
    }
    let end = (start + (event.duration_s * RENDER_RATE as f32) as usize).min(out.len());

    let filter = Resonator::at(event.centre_hz, event.bandwidth_hz);
    let mut noise = Noise::seeded(seed as u32);
    let mut state = (0.0f32, 0.0f32);

    for (i, sample) in out[start..end].iter_mut().enumerate() {
        let y = filter.step(noise.next_bipolar(), &mut state);
        let t = i as f32 / RENDER_RATE as f32;
        *sample += y * envelope(t, event.duration_s) * event.amplitude;
    }
}

/// A two-pole resonator, as both the vocal tract and a fricative are.
#[derive(Clone, Copy)]
struct Resonator {
    a1: f32,
    a2: f32,
    /// Compensation for the gain a resonator picks up as its band narrows.
    gain: f32,
}

impl Resonator {
    fn at(centre_hz: f32, bandwidth_hz: f32) -> Self {
        let nyquist = RENDER_RATE as f32 / 2.0;
        let centre = centre_hz.clamp(20.0, nyquist * 0.95);
        let bandwidth = bandwidth_hz.max(1.0);

        let theta = std::f32::consts::TAU * centre / RENDER_RATE as f32;
        let radius = (-std::f32::consts::PI * bandwidth / RENDER_RATE as f32).exp();
        Resonator {
            a1: 2.0 * radius * theta.cos(),
            a2: -radius * radius,
            gain: (1.0 - radius).max(1e-4),
        }
    }

    /// Passes its input through unchanged, for a note with no breath in it.
    fn silent() -> Self {
        Resonator {
            a1: 0.0,
            a2: 0.0,
            gain: 1.0,
        }
    }

    fn step(&self, input: f32, state: &mut (f32, f32)) -> f32 {
        let y = input + self.a1 * state.0 + self.a2 * state.1;
        state.1 = state.0;
        state.0 = y;
        y * self.gain
    }
}

/// Where a spectrum's energy sits, as a multiple of the fundamental.
fn spectral_centroid(spectrum: &[f32]) -> f32 {
    let total: f32 = spectrum.iter().sum();
    if total <= 0.0 {
        return 1.0;
    }
    spectrum
        .iter()
        .enumerate()
        .map(|(k, a)| (k + 1) as f32 * a)
        .sum::<f32>()
        / total
}

/// Longest spectrum the palette holds.
fn spectrum_width(score: &Score) -> usize {
    score.palette.iter().map(Vec::len).max().unwrap_or(0)
}

/// Damp the spectrum according to how far through the note it is.
///
/// Partial *k* keeps a fraction that falls from 1 at the fundamental toward
/// [`HIGH_PARTIAL_SURVIVAL`] at the top, interpolated by how far the note has
/// run. At the attack the spectrum is untouched, which is what makes the attack
/// the brightest moment — as it is in anything struck, plucked or bowed.
fn damp(spectrum: &mut [f32], progress: f32) {
    let width = spectrum.len().max(1) as f32;
    for (k, amplitude) in spectrum.iter_mut().enumerate() {
        let height = k as f32 / width;
        let survival = 1.0 - (1.0 - HIGH_PARTIAL_SURVIVAL) * height;
        *amplitude *= 1.0 + (survival - 1.0) * progress;
    }
}

/// Amplitude envelope: a short fade in, a longer fade out, flat between.
fn envelope(t: f32, duration_s: f32) -> f32 {
    let attack = ATTACK_S.min(duration_s / 2.0);
    let release = RELEASE_S.min(duration_s / 2.0);
    if t < attack {
        t / attack
    } else if t > duration_s - release {
        ((duration_s - t) / release).max(0.0)
    } else {
        1.0
    }
}

/// Scale the whole render so its loudest moment sits at [`HEADROOM`].
///
/// Whole-render rather than per-note, because the dynamics between notes are
/// carried from the speaker's energy envelope and levelling them would throw
/// away a measurement.
fn normalise(out: &mut [f32]) {
    let peak = out.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    if peak <= 0.0 {
        return;
    }
    let gain = HEADROOM / peak;
    for sample in out {
        *sample *= gain;
    }
}

/// A deterministic noise source.
///
/// Seeded from the note's index rather than from a clock, so a render is
/// reproducible — which the whole project depends on, since it is how "the
/// mapping changed" is told apart from "the renderer wandered". An xorshift is
/// ample: this is breath, not cryptography.
struct Noise(u32);

impl Noise {
    fn seeded(index: u32) -> Self {
        // Any non-zero state will do; xorshift is stuck at zero.
        Noise(index.wrapping_mul(2_654_435_761).max(1))
    }

    /// The next sample, in -1..1.
    fn next_bipolar(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}
