//! Linear prediction: fitting an all-pole filter to one frame of speech. A vowel
//! is a glottal buzz shaped by the tract's resonances, and linear prediction
//! recovers the resonances apart from the pitch. Turning poles into formants is
//! [`crate::formant`]'s job.

use rustfft::num_complex::Complex64;

/// Pre-emphasis coefficient (+6 dB/octave), flattening the source's net tilt so
/// the poles go on the resonances, not the slope.
const PRE_EMPHASIS: f32 = 0.97;

/// Prediction order: the rule of thumb of one pole per kHz plus two — a pair
/// per formant below Nyquist and a spare for residual tilt. Too low merges
/// formants; too high fits individual harmonics.
pub const ORDER: usize = 18;

/// Iterations of the root solver before giving up.
const MAX_ROOT_ITERATIONS: usize = 200;

/// Convergence tolerance for the root solver, in the complex plane.
const ROOT_TOLERANCE: f64 = 1e-10;

/// Pre-emphasised copy of `x`.
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

/// Linear-prediction coefficients for one frame, as `1 + a₁z⁻¹ + … + a_p z⁻ᵖ`,
/// or `None` for a silent frame.
pub fn coefficients(frame: &[f32], order: usize) -> Option<Vec<f64>> {
    let r = autocorrelate(frame, order);
    if r[0] <= f64::EPSILON {
        return None;
    }

    // Levinson-Durbin: O(p²), building each order from the last.
    let mut a = vec![0.0f64; order + 1];
    a[0] = 1.0;
    let mut error = r[0];

    for i in 1..=order {
        let acc: f64 = r[i] + (1..i).map(|j| a[j] * r[i - j]).sum::<f64>();
        let k = -acc / error;

        // A reflection coefficient outside the unit circle means the recursion
        // went unstable; the fit so far is usable.
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

/// Roots of the prediction polynomial — the poles of the fitted filter.
///
/// Durand-Kerner refines all roots together, with no deflation to push error
/// into the later roots (the high formants). It starts from a fixed spiral,
/// so analysis stays a pure function of the audio.
pub fn roots(coefficients: &[f64]) -> Vec<Complex64> {
    // Descending powers of z: A(z)·zᵖ = zᵖ + a₁zᵖ⁻¹ + … + a_p.
    let degree = coefficients.len() - 1;
    if degree == 0 {
        return Vec::new();
    }

    // Off the real axis, so conjugate pairs do not start together and stall.
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
