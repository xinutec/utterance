//! The knobs: every arguable number in the mappings, in one type, so it can be
//! swept by ear instead of edited and rebuilt.
//!
//! They live in mapping, never analysis: a knob in analysis would invalidate
//! every stored voiceprint each time it moved. A knob's default reproduces the
//! behaviour from before it existed, so renders stay comparable.

use serde::{Deserialize, Serialize};

use crate::mapping::{CONTINUOUS, Mapping};
use crate::tuning::{Degree, Tuning};

/// One knob, described well enough that a UI can offer it without being told.
///
/// The range lives here, not in the UI, so a slider can never offer a value the
/// mapping clamps away. It is also the wire type: the API forwards it unchanged.
#[derive(Clone, Copy, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Knob {
    /// Which knob: its name is both the query parameter and the `Params` field.
    pub name: KnobName,
    /// What to call it in front of a person.
    pub label: &'static str,
    pub min: f32,
    pub max: f32,
    /// Smallest move worth offering. 1.0 where the value counts things.
    pub step: f32,
    pub default: f32,
    /// What moving it does, and what each end sounds like.
    pub about: &'static str,
    /// Mappings this knob reaches; empty means all. A slider shown beside a
    /// mapping that ignores it moves and changes nothing, so the knob says, and
    /// `tests/api.rs` checks the claim.
    pub mappings: &'static [Mapping],
    /// Whether to offer this one before anybody asks for it.
    ///
    /// Primary knobs decide what kind of piece this is; the rest adjust a piece
    /// you have. Not a ranking by audible authority: `bind` is primary because
    /// the project argues about it, `spacing` for how much it changes.
    pub primary: bool,
}

impl Knob {
    /// The nearest value this knob actually accepts.
    pub fn clamped(&self, value: f32) -> f32 {
        value.clamp(self.min, self.max)
    }

    /// Whether this knob does anything to the given mapping (empty means all).
    pub fn reaches(&self, mapping: Mapping) -> bool {
        self.mappings.is_empty() || self.mappings.contains(&mapping)
    }
}

/// A value a knob can hold. The table speaks `f32` and some fields count things,
/// so one conversion rule is applied everywhere the macro generates.
pub trait KnobValue: Copy {
    fn from_knob(value: f32) -> Self;
    fn to_knob(self) -> f32;
}

impl KnobValue for f32 {
    fn from_knob(value: f32) -> Self {
        value
    }
    fn to_knob(self) -> f32 {
        self
    }
}

impl KnobValue for usize {
    /// Rounded, not truncated: a slider stopped just under 5 means 5.
    fn from_knob(value: f32) -> Self {
        value.round().max(0.0) as usize
    }
    fn to_knob(self) -> f32 {
        self as f32
    }
}

/// Declare the knobs once.
///
/// A knob appears in the `Knob` const, the `Params` field, `Default`, `sane`,
/// `with`, `KnobName` and `KnobQuery`. Written by hand, one missing would not
/// always fail to compile — and a knob missing from the query is a slider
/// connected to nothing. The cost: `Params::bind` jumps to this macro.
macro_rules! knobs {
    ($(
        $(#[$field_doc:meta])*
        $variant:ident $name:ident: $ty:ty = {
            label: $label:expr,
            min: $min:expr,
            max: $max:expr,
            step: $step:expr,
            default: $default:expr,
            about: $about:expr,
            mappings: $mappings:expr,
            primary: $primary:expr,
        }
    )*) => {
        /// Which knob, as a value, so a name that is not a knob cannot be written.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
        #[cfg_attr(feature = "ts", ts(export))]
        #[serde(rename_all = "lowercase")]
        pub enum KnobName {
            $( #[doc = $label] $variant, )*
        }

        impl KnobName {
            /// The wire spelling, which is also the `Params` field name — from
            /// `stringify!`, and held to serde's by `name_round_trips_through_serde`.
            pub fn name(self) -> &'static str {
                match self {
                    $( KnobName::$variant => stringify!($name), )*
                }
            }

            /// The wire spelling read back, or `None` for a name no knob has.
            pub fn from_name(name: &str) -> Option<Self> {
                use serde::de::value::StrDeserializer;
                use serde::Deserialize;
                Self::deserialize(StrDeserializer::<serde::de::value::Error>::new(name)).ok()
            }
        }

        impl std::fmt::Display for KnobName {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.name())
            }
        }

        $(
            #[doc = $about]
            pub const $variant: Knob = Knob {
                name: KnobName::$variant,
                label: $label,
                min: $min,
                max: $max,
                step: $step,
                default: $default,
                about: $about,
                mappings: $mappings,
                primary: $primary,
            };
        )*

        /// Every knob, in the order a person should meet them.
        pub const KNOBS: &[Knob] = &[ $( $variant, )* ];

        /// How the voice binds, and what it drives.
        #[derive(Clone, Copy, Debug, PartialEq)]
        pub struct Params {
            $( $(#[$field_doc])* pub $name: $ty, )*
        }

        impl Default for Params {
            fn default() -> Self {
                Params { $( $name: <$ty as KnobValue>::from_knob($variant.default), )* }
            }
        }

        impl Params {
            /// Clamp everything into a range that produces sound: an out-of-range
            /// knob is someone exploring, and the answer is the nearest thing
            /// that works.
            pub fn sane(self) -> Self {
                Params {
                    $( $name: <$ty as KnobValue>::from_knob(
                        $variant.clamped(self.$name.to_knob())
                    ), )*
                }
            }

            /// The same parameters with one knob moved, chosen by name. Total,
            /// since [`KnobName`] cannot name a knob that does not exist.
            pub fn with(self, knob: KnobName, value: f32) -> Self {
                match knob {
                    $( KnobName::$variant => Params {
                        $name: <$ty as KnobValue>::from_knob($variant.clamped(value)),
                        ..self
                    }, )*
                }
            }

            /// What this knob is currently set to, on the table's `f32` scale.
            pub fn get(&self, knob: KnobName) -> f32 {
                match knob {
                    $( KnobName::$variant => self.$name.to_knob(), )*
                }
            }
        }

        /// The knobs as a query string gives them: each absent or given.
        ///
        /// Its own extractor rather than part of `VoiceParams`: `serde_urlencoded`
        /// cannot flatten a nested struct without losing the numbers.
        #[derive(Debug, Default, Deserialize)]
        pub struct KnobQuery {
            $( #[serde(default)] pub $name: Option<f32>, )*
        }

        impl KnobQuery {
            /// The knobs, defaulted where the caller said nothing, then clamped.
            pub fn params(&self) -> Params {
                let mut params = Params::default();
                $( if let Some(value) = self.$name {
                    params = params.with(KnobName::$variant, value);
                } )*
                params.sane()
            }
        }
    };
}

knobs! {
    /// How far the speaker's own scale is used, 0..1 — the convention-to-speaker
    /// axis. At 1 the degrees are where the spectrum puts them, at 0 they snap to
    /// equal temperament, between they interpolate in cents. A knob because
    /// where on this axis the music is can only be settled by listening.
    BIND bind: f32 = {
        label: "Bind to the voice",
        min: 0.0,
        max: 1.0,
        step: 0.05,
        default: 1.0,
        about: "At 1 the notes are exactly where this voice's spectrum puts them. \
                At 0 they snap to the twelve everyone else uses.",
        mappings: &[],
        primary: true,
    }

    /// How deep a dip in the roughness curve must be to count as a note: high
    /// gives a few very stable intervals, low a dense microtonal set.
    DENSITY density: f32 = {
        label: "Scale density",
        min: 0.0005,
        max: 0.5,
        step: 0.002,
        default: crate::tuning::MIN_DEPTH,
        about: "How firm a note has to be to count. Low gives a crowded microtonal \
                set, high gives a handful of very stable intervals.",
        mappings: &[],
        primary: true,
    }

    /// How many voices sound at once in the continuous mappings.
    VOICES voices: usize = {
        label: "Voices",
        min: 1.0,
        max: 12.0,
        step: 1.0,
        default: crate::field::VOICES as f32,
        about: "How many tones sound at once.",
        mappings: CONTINUOUS,
        primary: true,
    }

    /// How far apart the voices of the continuous mappings sit.
    SPACING spacing: usize = {
        label: "Spacing",
        min: 1.0,
        max: 6.0,
        step: 1.0,
        default: 2.0,
        about: "How far apart the voices sit. Scale degrees between one and the \
                next in the field mapping, least air between them in the Tonnetz. \
                1 is a cluster, higher is an open chord.",
        mappings: CONTINUOUS,
        primary: true,
    }

    /// Octaves the whole field transposes across the speaker's pitch range. Near
    /// 1 it reads as a parallel melody — the naive mapping this project avoids —
    /// so the default sits low.
    DRIFT drift: f32 = {
        label: "Follow the pitch",
        min: 0.0,
        max: 2.0,
        step: 0.05,
        default: 0.25,
        about: "How far the music transposes with the speaker's pitch. At 0 it sits \
                still; near 1 it reads as a parallel melody.",
        mappings: CONTINUOUS,
        primary: false,
    }

    /// How far the vowel moves the harmony: octaves the root travels in the
    /// field, cells crossed in the Tonnetz. One quantity on two geometries.
    REACH reach: f32 = {
        label: "Follow the vowel",
        min: 0.0,
        max: 3.0,
        step: 0.05,
        default: 1.0,
        about: "How far the vowel moves the harmony: octaves the root travels in \
                the field mapping, cells of lattice crossed in the Tonnetz. This is \
                the articulation showing up as harmony.",
        mappings: CONTINUOUS,
        primary: false,
    }

    /// How far past a boundary the mouth must go before the harmony follows —
    /// whether a chord rings long enough for its tuning to be heard.
    HOLD hold: f32 = {
        label: "Hold the harmony",
        min: 0.0,
        max: 1.0,
        step: 0.05,
        default: 0.35,
        about: "How far the mouth must move past a boundary before the chord \
                changes. At 0 the harmony follows every wobble; higher makes it \
                commit, so a chord rings long enough to hear what it is tuned to.",
        mappings: &[Mapping::Tonnetz],
        primary: true,
    }

    /// How long the mouth must stay away before the harmony follows, in seconds.
    /// [`Self::hold`] asks how far; this asks how long, so a mouth that crosses
    /// a line and comes straight back leaves the chord alone. Seconds, not
    /// frames, so the meaning does not move with the hop size.
    SETTLE settle: f32 = {
        label: "Settle",
        min: 0.0,
        max: 0.5,
        step: 0.01,
        default: 0.0,
        about: "How long the mouth must stay in its new place before the chord \
                follows, in seconds. At 0 it follows the moment it is allowed to; \
                higher ignores a mouth that crosses a boundary and comes straight \
                back.",
        mappings: &[Mapping::Tonnetz],
        primary: false,
    }

    /// How far the third formant opens or clusters the chord. F3 separates mouth
    /// shapes the vowel chart cannot — rounded from spread — and moves while F1
    /// and F2 hold still. At 0 the voices stack evenly.
    VOICING voicing: f32 = {
        label: "Voicing",
        min: 0.0,
        max: 1.0,
        step: 0.05,
        default: 0.5,
        about: "How much the shape of the mouth shows up in the chord. Lip rounding \
                and tongue position move the third formant while leaving the vowel \
                where it is: that opens or clusters the stack in the field mapping, \
                and tips the weight between the chord's top and bottom in the \
                Tonnetz.",
        mappings: CONTINUOUS,
        primary: false,
    }

    /// How much the rate of spectral change stirs the texture: rhythm without
    /// cutting anything into notes. Flux says *the sound is changing now*, which
    /// makes it a poor onset detector and a good continuous stream.
    ARTICULATION articulation: f32 = {
        label: "Articulation",
        min: 0.0,
        max: 1.5,
        step: 0.05,
        default: 0.4,
        about: "How much a moving mouth stirs the texture. A held vowel settles, a \
                busy passage opens the upper voices — rhythm from how fast the \
                spectrum is changing, without cutting anything into notes.",
        mappings: CONTINUOUS,
        primary: false,
    }

    /// How loud the consonants are against the pitched material. At 0 they are
    /// silent.
    CONSONANTS consonants: f32 = {
        label: "Consonants",
        min: 0.0,
        max: 2.0,
        step: 0.05,
        default: 1.0,
        about: "How loud the unpitched material is against the tones. At 0 they are \
                silent.",
        mappings: &[],
        primary: false,
    }
}

/// Cents in an equal-tempered semitone.
const SEMITONE_CENTS: f32 = 100.0;

/// Pull one pitch toward the nearest equal-tempered one by `1 - bind`,
/// interpolating in cents, where the perceptual midpoint is.
///
/// Separate from [`bind_toward_equal`] because the Tonnetz binds each sounding
/// pitch rather than its axes: a lattice point is `x·a + y·b`, so binding the
/// axes would move far-out chords by more than a quarter tone and eventually
/// into different notes — a structural change wearing a tuning knob's name.
pub fn bind_cents_toward_equal(cents: f32, bind: f32) -> f32 {
    if bind >= 1.0 {
        return cents;
    }
    let tempered = (cents / SEMITONE_CENTS).round() * SEMITONE_CENTS;
    tempered + (cents - tempered) * bind.clamp(0.0, 1.0)
}

/// Pull a tuning toward equal temperament by `1 - bind`. At 0 neighbouring
/// degrees can snap to one note, so the scale may shrink — which is what
/// conventional tuning does to a spectrum that did not ask for it.
pub fn bind_toward_equal(tuning: &Tuning, bind: f32) -> Tuning {
    if bind >= 1.0 {
        return tuning.clone();
    }

    let mut degrees: Vec<Degree> = tuning
        .degrees
        .iter()
        .map(|d| {
            let cents = bind_cents_toward_equal(d.cents, bind);
            Degree {
                cents,
                ratio: crate::tuning::cents_to_ratio(cents),
                ..*d
            }
        })
        .collect();

    // Degrees that snapped to one note are one note: keeping both would double
    // a voice in the field.
    degrees.dedup_by(|a, b| (a.cents - b.cents).abs() < 1.0);

    Tuning {
        degrees,
        curve: tuning.curve.clone(),
    }
}
