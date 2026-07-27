//! Linear prediction: fitting an all-pole filter to a frame of speech.
//!
//! The source-filter model of the voice says a vowel is a buzz from the glottis
//! shaped by the resonances of the throat and mouth. Linear prediction recovers
//! the *filter* — the resonances — while ignoring what drove it. That separation
//! is the whole reason to use it here: the resonances are a property of the
//! speaker's anatomy and what they are doing with it, independent of the pitch
//! they happen to be saying it at.
//!
//! Everything in this module is arithmetic on one frame. Turning poles into
//! formants is [`crate::formant`]'s job.

use rustfft::num_complex::Complex64;

/// Pre-emphasis coefficient, a one-zero high-pass at roughly +6 dB/octave.
///
/// The glottal source rolls off at about -12 dB/octave and lip radiation adds
/// +6, leaving a net tilt that linear prediction would otherwise spend its poles
/// modelling. Flattening it first means the poles go where they are wanted — on
/// the vocal-tract resonances rather than on the spectral slope.
const PRE_EMPHASIS: f32 = 0.97;

/// Prediction order.
///
/// The usual rule is two poles per expected resonance plus a few spare: at a
/// 16 kHz analysis rate there are four or five formants below Nyquist, so 18
/// leaves room for them and for whatever tilt survives pre-emphasis. Too low
/// merges neighbouring formants into one pole; too high spends poles on
/// individual harmonics of the source, which is exactly what this is supposed to
/// see past.
pub const ORDER: usize = 18;

/// Iterations of the root solver before giving up.
const MAX_ROOT_ITERATIONS: usize = 200;

/// Convergence tolerance for the root solver, in the complex plane.
const ROOT_TOLERANCE: f64 = 1e-10;

/// Apply pre-emphasis in place of the caller's buffer.
pub fn pre_emphasise(x: &[f32]) -> Vec<f32> {
    let mut out = Vec::with_capacity(x.len());
    out.push(x.first().copied().unwrap_or(0.0));
    for i in 1..x.len() {
        out.push(x[i] - PRE_EMPHASIS * x[i - 1]);
    }
    out
}

/// Autocorrelation of `x` at lags `0..=max_lag`.
fn autocorrelate(x: &[f32], max_lag: usize) -> Vec<f64> {
    (0..=max_lag)
        .map(|lag| {
            (0..x.len().saturating_sub(lag))
                .map(|n| f64::from(x[n]) * f64::from(x[n + lag]))
                .sum()
        })
        .collect()
}

/// Linear-prediction coefficients for one frame, as the polynomial
/// `1 + a₁z⁻¹ + … + a_p z⁻ᵖ`.
///
/// Returns `None` for a frame with no energy, where the fit is meaningless
/// rather than merely poor — silence has no resonances to find.
pub fn coefficients(frame: &[f32], order: usize) -> Option<Vec<f64>> {
    let r = autocorrelate(frame, order);
    if r[0] <= f64::EPSILON {
        return None;
    }

    // Levinson-Durbin. Solves the Toeplitz normal equations in O(p²) by building
    // the order-i solution from the order-(i-1) one, rather than inverting.
    let mut a = vec![0.0f64; order + 1];
    a[0] = 1.0;
    let mut error = r[0];

    for i in 1..=order {
        let acc: f64 = r[i] + (1..i).map(|j| a[j] * r[i - j]).sum::<f64>();
        let k = -acc / error;

        // The reflection coefficient leaving the unit circle means the recursion
        // has gone numerically unstable; the fit so far is still usable.
        if !k.is_finite() || k.abs() >= 1.0 {
            break;
        }

        let previous = a.clone();
        a[i] = k;
        for j in 1..i {
            a[j] = previous[j] + k * previous[i - j];
        }
        error *= 1.0 - k * k;
        if error <= f64::EPSILON {
            break;
        }
    }
    Some(a)
}

/// Roots of the prediction polynomial, i.e. the poles of the fitted filter.
///
/// Solved by Durand-Kerner: all roots are refined simultaneously from a fixed
/// starting spiral, which needs no derivative and no deflation. Deflation is the
/// thing worth avoiding — dividing out each root as it is found accumulates
/// error into the later ones, and the later ones here are the high formants.
///
/// The fixed initialisation matters for more than convergence: analysis has to
/// be a pure function of the audio, so the solver may not start anywhere that
/// varies between runs.
pub fn roots(coefficients: &[f64]) -> Vec<Complex64> {
    // Descending powers of z: A(z)·zᵖ = zᵖ + a₁zᵖ⁻¹ + … + a_p.
    let degree = coefficients.len() - 1;
    if degree == 0 {
        return Vec::new();
    }

    // The conventional off-axis spiral. Off the real axis so that conjugate
    // pairs — which is what every resonance is — do not start on top of each
    // other and stall.
    let seed = Complex64::new(0.4, 0.9);
    let mut z: Vec<Complex64> = (0..degree).map(|k| seed.powu(k as u32)).collect();

    for _ in 0..MAX_ROOT_ITERATIONS {
        let mut moved: f64 = 0.0;
        for k in 0..degree {
            let numerator = evaluate(coefficients, z[k]);
            let denominator = (0..degree)
                .filter(|&j| j != k)
                .fold(Complex64::new(1.0, 0.0), |acc, j| acc * (z[k] - z[j]));
            if denominator.norm() < f64::EPSILON {
                continue;
            }
            let step = numerator / denominator;
            z[k] -= step;
            moved = moved.max(step.norm());
        }
        if moved < ROOT_TOLERANCE {
            break;
        }
    }
    z
}

/// Evaluate the polynomial (descending powers) at `z` by Horner's method.
fn evaluate(coefficients: &[f64], z: Complex64) -> Complex64 {
    coefficients
        .iter()
        .fold(Complex64::new(0.0, 0.0), |acc, &c| {
            acc * z + Complex64::new(c, 0.0)
        })
}
