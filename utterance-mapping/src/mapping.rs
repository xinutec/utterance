//! The mappings, as a closed set.
//!
//! An enum rather than names, so a misspelling cannot compile.
//! [`Mapping::score_with`] is the dispatch: a variant added here fails to compile
//! until it says what it makes and how it sounds, and the route that combines
//! them never names one.

use serde::{Deserialize, Serialize};
use utterance_analysis::voiceprint::Voiceprint;

use crate::params::Params;
use crate::score::Score;
use crate::voice::Voice;

/// What a mapping produces, and so what it competes for: a score holds one field
/// and one list of events. Clashes follow from the material, so a new mapping
/// needs no new rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "lowercase")]
pub enum Material {
    /// The continuously sounding layer.
    Texture,
    /// Discrete events at onsets.
    Events,
}

impl Material {
    /// The wire spelling, for a refusal that has to name what clashed.
    pub fn name(self) -> &'static str {
        match self {
            Material::Texture => "texture",
            Material::Events => "events",
        }
    }
}

/// One mapping a render may ask for. [`Mapping::name`] restates serde's spelling
/// for error messages and URLs; `name_round_trips_through_serde` holds them
/// together.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "lowercase")]
pub enum Mapping {
    Field,
    Tonnetz,
    Notes,
}

impl Mapping {
    /// Every mapping, in the order a UI offers them. A sized array, so adding a
    /// variant fails to compile until it is listed.
    pub const ALL: [Mapping; 3] = [Mapping::Field, Mapping::Tonnetz, Mapping::Notes];

    /// The wire spelling, from the same serde attribute that writes it.
    pub fn name(self) -> &'static str {
        match self {
            Mapping::Field => "field",
            Mapping::Tonnetz => "tonnetz",
            Mapping::Notes => "notes",
        }
    }

    /// The wire spelling read back through serde, or `None` for a name no
    /// mapping has.
    pub fn from_name(name: &str) -> Option<Self> {
        use serde::de::value::StrDeserializer;
        Self::deserialize(StrDeserializer::<serde::de::value::Error>::new(name)).ok()
    }

    /// What to call it in front of a person.
    pub fn label(self) -> &'static str {
        match self {
            Mapping::Field => "Field",
            Mapping::Tonnetz => "Lattice",
            Mapping::Notes => "Notes",
        }
    }

    /// The material it makes, and so what it cannot be heard beside.
    pub fn makes(self) -> Material {
        match self {
            Mapping::Field | Mapping::Tonnetz => Material::Texture,
            Mapping::Notes => Material::Events,
        }
    }

    /// What it does, for someone deciding whether to pick it.
    pub fn about(self) -> &'static str {
        match self {
            Mapping::Field => {
                "Every frame sounds. A continuous texture that moves with the voice \
                 rather than a sequence of notes."
            }
            Mapping::Tonnetz => {
                "The same texture, with the vowel walking a harmonic lattice built from \
                 the speaker's own consonances. Chords hold while the mouth holds, and \
                 change by keeping two voices and stepping one."
            }
            Mapping::Notes => {
                "Discrete events at onsets. Closer to a melody, though its rhythm \
                 follows spectral change rather than syllables."
            }
        }
    }

    /// Sound this mapping. The dispatch lives here, exhaustively, so the route
    /// never needs to know what kind of thing a mapping is.
    pub fn score_with(self, vp: &Voiceprint, voice: &Voice, params: Params) -> Score {
        match self {
            Mapping::Field => crate::field::score_with(vp, voice, params),
            Mapping::Tonnetz => crate::tonnetz::score_with(vp, voice, params),
            Mapping::Notes => crate::compose::compose_with(vp, voice, params),
        }
    }
}

/// Mappings that sound a continuous field, and so read the field knobs. A
/// `const` because `Knob::mappings` is one; `continuous_is_every_texture_mapping`
/// holds it to [`Mapping::makes`].
pub const CONTINUOUS: &[Mapping] = &[Mapping::Field, Mapping::Tonnetz];
