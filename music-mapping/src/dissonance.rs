//! How rough two spectra sound together.
//!
//! The model is Plomp and Levelt's (1965), in the parameterisation Sethares
//! fitted in 1993. Two sinusoids close in frequency beat against each other; the
//! roughness that produces peaks when they are about a quarter of a critical
//! band apart and falls away both as they converge on unison and as they
//! separate. Two *complex* tones are rough to the extent that their partials
//! collide, so the shape of the curve for a given pair of spectra depends
//! entirely on which partials those spectra have and how loud they are.
//!
//! That is the whole reason this project measures a harmonic series. A voice
//! emphasising partials 2 and 6 has a different set of intervals that sit still
//! from one emphasising 2 and 3, and the difference is not a matter of opinion —
//! it follows from where the collisions land.
//!
//! **What is a model here and what is not.** The roughness curve is empirical
//! psychoacoustics fitted to listening tests, not arithmetic: it describes what
//! people reported, averaged. Calling its minima *consonant* is already an
//! interpretation, and calling them *notes* is a further one. Both belong to
//! this crate rather than to analysis for exactly that reason.

/// A single sinusoid: where it is and how loud.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Component {
    pub hz: f32,
    /// Relative amplitude. Only ratios between components matter, since the
    /// curve scales linearly with the product of the two amplitudes.
    pub amplitude: f32,
}

/// Sethares' fit to the Plomp–Levelt data.
///
/// Named rather than inlined so the source of each is traceable: these are
/// fitted constants from published listening experiments, not tunable knobs, and
/// changing one is changing the psychoacoustic claim rather than adjusting a
/// parameter.
mod fit {
    /// Frequency separation, as a fraction of critical bandwidth, at which
    /// roughness peaks.
    pub const PEAK_FRACTION: f32 = 0.24;
    /// Critical bandwidth grows with frequency; these place that growth.
    pub const BANDWIDTH_SLOPE: f32 = 0.0207;
    pub const BANDWIDTH_OFFSET: f32 = 18.96;
    /// Decay rates of the two exponentials whose difference makes the curve.
    pub const RISE: f32 = 3.51;
    pub const FALL: f32 = 5.75;
}

/// Roughness between two sinusoids.
///
/// Zero at unison and zero as the two separate, with a maximum between — which
/// is the entire content of the model. Both limits matter: without the first,
/// nothing would make a unison consonant; without the second, every wide
/// interval would be rough.
pub fn between(a: Component, b: Component) -> f32 {
    let (low, high) = if a.hz <= b.hz {
        (a.hz, b.hz)
    } else {
        (b.hz, a.hz)
    };
    let separation = high - low;
    if separation <= 0.0 {
        return 0.0;
    }

    // Critical bandwidth at the lower frequency, scaled so the curve peaks where
    // the listening data said it does.
    let scale = fit::PEAK_FRACTION / (fit::BANDWIDTH_SLOPE * low + fit::BANDWIDTH_OFFSET);
    let x = scale * separation;
    a.amplitude * b.amplitude * ((-fit::RISE * x).exp() - (-fit::FALL * x).exp())
}

/// Roughness of one spectrum sounded against another.
///
/// Every partial of one against every partial of the other. Only the cross terms
/// are counted: a spectrum's roughness against *itself* is real but constant
/// however the two are tuned apart, so including it would raise the whole curve
/// by a fixed amount and move no minimum.
pub fn between_spectra(a: &[Component], b: &[Component]) -> f32 {
    a.iter()
        .flat_map(|&x| b.iter().map(move |&y| between(x, y)))
        .sum()
}

/// A spectrum sounded against a copy of itself shifted by `ratio`.
///
/// The curve this traces as `ratio` sweeps upward is the thing a scale gets read
/// out of.
pub fn at_interval(spectrum: &[Component], ratio: f32) -> f32 {
    let shifted: Vec<Component> = spectrum
        .iter()
        .map(|c| Component {
            hz: c.hz * ratio,
            amplitude: c.amplitude,
        })
        .collect();
    between_spectra(spectrum, &shifted)
}
