//! Band-limited sample-rate conversion.
//!
//! Everything downstream analyses at [`ANALYSIS_RATE`], so a voiceprint means the
//! same thing whatever the recording device produced. Frame indices are only
//! comparable across recordings if the rate underneath them is fixed.

/// The rate all analysis runs at.
///
/// 16 kHz is the speech-analysis convention: it carries the whole f0 range and
/// the first three formants with room to spare, and the 8 kHz ceiling only
/// clips fricative energy we measure in aggregate rather than resolve.
pub const ANALYSIS_RATE: u32 = 16_000;

/// Zero crossings of the sinc kernel retained on each side. Higher is a longer
/// filter and a sharper transition; 16 puts the stopband well below the noise
/// floor of any real microphone.
const ZERO_CROSSINGS: usize = 16;

/// Resample `input` from `from_rate` to `to_rate` by windowed-sinc interpolation.
///
/// The kernel is a Blackman-windowed sinc, widened by the conversion ratio when
/// downsampling so it doubles as the anti-alias filter. Equal rates return the
/// input untouched — not merely approximately, exactly, so a 16 kHz recording is
/// never degraded by a no-op conversion.
pub fn resample(input: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate || input.is_empty() {
        return input.to_vec();
    }

    let ratio = f64::from(to_rate) / f64::from(from_rate);
    // Relative to the *input* Nyquist. Upsampling needs no filtering (the band
    // is already narrower than the new Nyquist); downsampling must cut to the
    // new Nyquist or the discarded band folds back as alias.
    let cutoff = ratio.min(1.0);
    let half = ((ZERO_CROSSINGS as f64) / cutoff).ceil() as isize;

    let out_len = ((input.len() as f64) * ratio).round() as usize;
    let mut out = Vec::with_capacity(out_len);

    for n in 0..out_len {
        // Where this output sample sits on the input timeline.
        let center = (n as f64) / ratio;
        let base = center.floor() as isize;

        let mut acc = 0.0f64;
        for j in (base - half + 1)..=(base + half) {
            if j < 0 || j as usize >= input.len() {
                continue;
            }
            let t = center - (j as f64);
            acc += f64::from(input[j as usize])
                * blackman(t / (half as f64))
                * sinc(cutoff * t)
                * cutoff;
        }
        out.push(acc as f32);
    }
    out
}

/// Normalised sinc, sin(pi x) / (pi x), with the removable singularity filled in.
fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-12 {
        1.0
    } else {
        let pix = std::f64::consts::PI * x;
        pix.sin() / pix
    }
}

/// Blackman window over `u` in [-1, 1]; zero outside.
fn blackman(u: f64) -> f64 {
    if u.abs() > 1.0 {
        return 0.0;
    }
    // Shift to [0, 1] for the standard form.
    let x = (u + 1.0) * 0.5;
    let two_pi_x = 2.0 * std::f64::consts::PI * x;
    0.42 - 0.5 * two_pi_x.cos() + 0.08 * (2.0 * two_pi_x).cos()
}

/// Mix interleaved multi-channel samples down to mono by averaging.
///
/// Averaging, not picking channel 0: a stereo recording of one voice usually has
/// the signal in both channels, and dropping one throws away 3 dB of SNR.
pub fn to_mono(interleaved: &[f32], channels: u16) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    let ch = usize::from(channels);
    interleaved
        .chunks_exact(ch)
        .map(|frame| frame.iter().sum::<f32>() / (ch as f32))
        .collect()
}
