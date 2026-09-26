//! How rough two spectra sound together: Plomp and Levelt's curve, in Sethares'
//! parameterisation.
//!
//! Two nearby sinusoids beat; roughness peaks about a quarter of a critical band
//! apart and vanishes at unison and at a distance. Complex tones are rough where
//! their partials collide, so the curve depends on which partials a voice has —
//! the reason this project measures them. The curve is fitted psychoacoustics,
//! and calling its minima notes is an interpretation, which is why it is mapping.

/// A single sinusoid: where it is and how loud.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Component {
    pub hz: f32,
    /// Relative amplitude. Only ratios between components matter, since the
    /// curve scales linearly with the product of the two amplitudes.
    pub amplitude: f32,
}

/// Sethares' fit to the Plomp–Levelt data: published constants, not knobs.
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

/// Roughness between two sinusoids: zero at unison and at a distance, peaking
/// between.
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

    // Critical bandwidth at the lower frequency.
    let scale = fit::PEAK_FRACTION / (fit::BANDWIDTH_SLOPE * low + fit::BANDWIDTH_OFFSET);
    let x = scale * separation;
    a.amplitude * b.amplitude * ((-fit::RISE * x).exp() - (-fit::FALL * x).exp())
}

/// Roughness of one spectrum against another: cross terms only, since each
/// spectrum's roughness with itself is constant and moves no minimum.
pub fn between_spectra(a: &[Component], b: &[Component]) -> f32 {
    a.iter()
        .flat_map(|&x| b.iter().map(move |&y| between(x, y)))
        .sum()
}

/// A spectrum against a copy of itself shifted by `ratio` — swept, the curve a
/// scale is read from.
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
