//! The mappings, as a closed set.
//!
//! Every other taxonomy in this project is an enum — a take's role, a vowel
//! corner, a calibration step — and this one was a table of `&str` for longer
//! than the rest, because it started as one row and a match arm and never
//! stopped being spelled out by hand. The cost was not hypothetical: the route
//! chose a mapping with `names.contains(&"tonnetz")`, so a misspelled literal
//! compiled and quietly fell through to the branch below it, and the table that
//! promised to be the single source said so itself — *the compiler will not
//! remind you about the second*.
//!
//! It does now. [`Mapping::score_with`] is the dispatch, so a variant added here
//! fails to compile until it says what it makes and how it sounds, and the route
//! that combines them never names one.

use serde::{Deserialize, Serialize};
use utterance_analysis::voiceprint::Voiceprint;

use crate::params::Params;
use crate::score::Score;
use crate::voice::Voice;

/// What a mapping produces, and therefore what it competes for.
///
/// A score carries one continuous field and one list of events, so two mappings
/// making the same material cannot both be heard. Naming the material rather
/// than writing the clash out as a rule between named pairs means a fourth
/// mapping inherits the answer instead of needing a new line.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "lowercase")]
pub enum Material {
    /// The continuously sounding layer. Two mappings make it.
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

/// One mapping a render may ask for.
///
/// Serde decides the wire spelling. [`Mapping::name`] restates it, because a
/// `&'static str` is what an error message and a URL both want and serialising
/// a value to get one needs a `Serializer` this crate has no other use for. The
/// restatement is held honest by `name_round_trips_through_serde`: `from_name`
/// goes through the derive, so a `name` that drifted from the attribute stops
/// parsing and fails the test.
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
    /// Every mapping, in the order a UI should offer them.
    ///
    /// An array rather than an iterator so its length is the variant count at
    /// compile time: `[Mapping; 3]` stops compiling when a fourth is added, and
    /// a mapping missing from a list a person chooses from is a mapping that
    /// exists and cannot be heard.
    pub const ALL: [Mapping; 3] = [Mapping::Field, Mapping::Tonnetz, Mapping::Notes];

    /// The wire spelling, from the same serde attribute that writes it.
    pub fn name(self) -> &'static str {
        match self {
            Mapping::Field => "field",
            Mapping::Tonnetz => "tonnetz",
            Mapping::Notes => "notes",
        }
    }

    /// The wire spelling read back, or `None` for a name no mapping has.
    ///
    /// Through `serde` rather than a hand-written match so there is one table of
    /// spellings and not two — the same trick `CalibrationStep::from_label`
    /// uses, for the same reason.
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
                "Discrete events at onsets. Closer to a melody, and the weaker of the \
                 two — kept because comparing them is how either gets judged."
            }
        }
    }

    /// Sound this mapping.
    ///
    /// **The dispatch lives here and not in the route.** It was three `if`s on
    /// string equality in `routes::api`, which is the composition root and so
    /// the one place with no business knowing that a lattice is a kind of
    /// texture. Here the match is exhaustive, so adding a variant is a compile
    /// error until it has a score to produce — which is exactly the reminder the
    /// old table admitted it could not give.
    pub fn score_with(self, vp: &Voiceprint, voice: &Voice, params: Params) -> Score {
        match self {
            Mapping::Field => crate::field::score_with(vp, voice, params),
            Mapping::Tonnetz => crate::tonnetz::score_with(vp, voice, params),
            Mapping::Notes => crate::compose::compose_with(vp, voice, params),
        }
    }
}

/// Mappings that sound a continuous field, and so read the field knobs.
///
/// A `const` rather than a filter over [`Mapping::ALL`], because `Knob::mappings`
/// is a `const` too and a const context cannot filter an array. That makes it
/// the one restatement left in this module, so `continuous_is_every_texture_mapping`
/// in `tests/mapping.rs` holds it to [`Mapping::makes`].
pub const CONTINUOUS: &[Mapping] = &[Mapping::Field, Mapping::Tonnetz];
