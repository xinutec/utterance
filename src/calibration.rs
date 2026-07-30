//! What the guided calibration asks for, named once.
//!
//! A calibration take carries its step's id verbatim as its label, which is what
//! lets the backend tell one held vowel from another without anybody marking
//! audio by ear: the vowel's identity comes from the prompt that was on screen,
//! not from the sound. `frontend/src/app/features/calibration/steps.ts` puts it
//! plainly — a take per step makes the label free and exact.
//!
//! **The ids therefore have to agree across two languages, and the agreement was
//! by convention.** The frontend wrote `"vowel-ee"` and the backend would have
//! matched `"vowel-ee"`, with nothing to notice a rename on either side; the
//! failure would be silent and total — an unrecognised step is simply a take
//! that stops counting, and the plot would go back to generic landmarks without
//! saying why. The enum is exported to TypeScript by ts-rs, so `steps.ts` types
//! its ids against this list and a disagreement is a build error.

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
    /// The step a take's label names, or `None` for a label that names none.
    ///
    /// `None` is ordinary rather than exceptional: audio that arrives as a file
    /// carries a filename, and a take can be marked as calibration by hand
    /// without ever having passed through the guided flow. Such a take still
    /// contributes to the pooled profile — it just cannot claim to *be* a
    /// particular vowel, which is the one thing a label would have to earn.
    /// Deserialised rather than matched by hand, so the ids exist once. A
    /// `match` on string literals here would be a second copy of the rename
    /// attribute above, free to disagree with it — and the disagreement would
    /// read as a take that simply stopped being a vowel.
    pub fn from_label(label: &str) -> Option<Self> {
        Self::deserialize(StrDeserializer::<serde::de::value::Error>::new(label)).ok()
    }

    /// Which corner of the vowel space this step reaches for, if it reaches one.
    ///
    /// Only the three corner steps map: `steady-ah` is held on *ah* too, but it
    /// is recorded for its spectrum and at whatever mouth shape holds a note
    /// steadiest, so counting it as the open corner would put a second, differently
    /// produced *ah* in the same place and let the more casual one win by being
    /// longer. The pitch steps are sung at an extreme of range, where vowels move.
    pub fn corner(self) -> Option<Corner> {
        match self {
            Self::VowelEe => Some(Corner::CloseFront),
            Self::VowelAh => Some(Corner::Open),
            Self::VowelOo => Some(Corner::CloseBack),
            Self::SteadyAh | Self::PitchLow | Self::PitchHigh | Self::Speech => None,
        }
    }
}
