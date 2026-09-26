//! What the guided calibration asks for, named once.
//!
//! A calibration take carries its step's id as its label, which is how the
//! backend knows which vowel it is without anyone marking audio. The ids must
//! agree across two languages, so this enum is exported to TypeScript and a
//! rename is a build error rather than a take that silently stops counting.

use serde::de::value::StrDeserializer;
use serde::{Deserialize, Serialize};
use utterance_analysis::speaker::Corner;

/// One thing the guided calibration asks a person to record.
///
/// Serialised as the kebab-case id used as the take's label.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "kebab-case")]
pub enum CalibrationStep {
    /// A long steady note. The scale is usually derived from this one.
    SteadyAh,
    VowelEe,
    VowelAh,
    VowelOo,
    PitchLow,
    PitchHigh,
    /// A minute of ordinary talking.
    Speech,
}

impl CalibrationStep {
    /// The step a take's label names, or `None` — ordinary for an uploaded file,
    /// which still pools into the profile but cannot claim to be a vowel.
    /// Deserialised, so the ids are spelled once.
    pub fn from_label(label: &str) -> Option<Self> {
        Self::deserialize(StrDeserializer::<serde::de::value::Error>::new(label)).ok()
    }

    /// Which corner of the vowel space this step reaches for, if any. Not
    /// `steady-ah`: it is held for its spectrum, and being longest would let a
    /// casual *ah* win the corner. Not the pitch steps: vowels move at the
    /// extremes of range.
    pub fn corner(self) -> Option<Corner> {
        match self {
            Self::VowelEe => Some(Corner::CloseFront),
            Self::VowelAh => Some(Corner::Open),
            Self::VowelOo => Some(Corner::CloseBack),
            Self::SteadyAh | Self::PitchLow | Self::PitchHigh | Self::Speech => None,
        }
    }
}
