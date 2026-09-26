//! Summing sinusoids, and the things that stop that sounding dead.
//!
//! A bare sum of steady sinusoids is correct and lifeless. Four capabilities
//! fix that, each in the amount the score asks for, so nothing here decides
//! anything musical:
//!
//! - **The spectrum moves** across the score's palette through each note.
//! - **High partials die first**, so a tone darkens as it decays.
//! - **Partials are not exactly locked**: the speaker's own detune.
//! - **There is breath in it**, filtered to sit where the tone's energy sits —
//!   white noise reads as tape hiss over the music.

use utterance_mapping::score::{Event, Field, NoiseEvent, Score};

/// Rate everything is rendered at: what every browser and player expects.
pub const RENDER_RATE: u32 = 44_100;

/// Attack and release of a note, in seconds: never zero, since a sinusoid
/// switched on mid-cycle clicks.
const ATTACK_S: f32 = 0.012;
const RELEASE_S: f32 = 0.09;

/// Fraction of the fundamental's level the highest partial keeps by the end of a
/// note. Without it a decaying note keeps its attack brightness and sounds
/// synthetic.
const HIGH_PARTIAL_SURVIVAL: f32 = 0.35;

/// Peak level the finished render is scaled to, under full scale to leave room
/// for intersample peaks.
const HEADROOM: f32 = 0.89;

/// Samples between recalculations of a note's moving spectrum (about 1.5 ms):
/// far faster than any spectrum moves, far cheaper than per sample.
const SPECTRUM_HOP: usize = 64;

/// Fractional part of the golden ratio, spacing partial phases so none start
/// together.
const GOLDEN_FRACTION: f32 = 0.618_034;

/// Width of the band a note's breath is shaped into, as a fraction of its
/// centre: wide enough to read as air, narrow enough to belong to the note.
const BREATH_BANDWIDTH_RATIO: f32 = 0.9;

/// Render a score to mono samples at [`RENDER_RATE`]. Deterministic: the noise
/// is counter-seeded, so the same score renders to the same bytes.
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

    // Seeded past the notes, so a consonant never shares noise with a note's
    // breath and fuses with it.
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

    // Detune fixed per partial for the whole note: drifting would be vibrato, a
    // musical decision that belongs upstream.
    let mut noise = Noise::seeded(index as u32);
    let detune: Vec<f32> = (0..width)
        .map(|_| {
            let spread = noise.next_bipolar() * score.detune_cents;
            2f32.powf(spread / 1200.0)
        })
        .collect();

    // Spread phases: all-zero phases peak together, turning energy into a click
    // and wasting headroom.
    let phase: Vec<f32> = (0..width)
        .map(|k| (k * k) as f32 * GOLDEN_FRACTION * std::f32::consts::TAU)
        .collect();

    let nyquist = RENDER_RATE as f32 / 2.0;
    let pitched = 1.0 - event.breath.clamp(0.0, 1.0);
    let breath = event.breath.clamp(0.0, 1.0);

    let mut spectrum = vec![0.0f32; width];
    let mut gain = 0.0f32;

    // Breath through a resonator centred where the note's energy sits.
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
            // Constant power however many partials survive.
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
            // Past Nyquist a partial aliases, and sounds like a wrong tuning.
            if hz >= nyquist {
                break;
            }
            value += amplitude * (std::f32::consts::TAU * hz * t + phase[k]).sin();
        }

        let breath_sample = breath_filter.step(noise.next_bipolar(), &mut breath_state);
        *sample += (value * gain * pitched + breath_sample * breath) * envelope * event.amplitude;
    }
}

/// Partials each field voice is rendered with: fewer than a note gets, since
/// several voices share the load.
const FIELD_PARTIALS: usize = 12;

/// Render the continuously sounding field.
///
/// Phase is accumulated, never recomputed from elapsed time: with a frequency
/// that changes every frame, `sin(2πft)` jumps at each change — a click a
/// hundred times a second.
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
            // Interpolated across the frame boundary, so a voice glides.
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

/// Add one consonant to the buffer: white noise through a two-pole resonator at
/// the measured centre and width, which is what a fricative is.
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

/// Damp the spectrum by how far through the note it is: partial *k* falls from
/// full at the fundamental toward [`HIGH_PARTIAL_SURVIVAL`] at the top, so the
/// attack is the brightest moment.
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

/// Scale the whole render so its loudest moment sits at [`HEADROOM`] — the whole
/// render, since the dynamics between notes are the speaker's.
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

/// A deterministic noise source: an xorshift seeded from an index, so renders
/// are reproducible.
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
