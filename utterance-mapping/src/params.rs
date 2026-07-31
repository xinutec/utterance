//! The knobs.
//!
//! Every number here was a constant somewhere in this crate, chosen by whoever
//! wrote the mapping and documented as arguable. They are gathered into one type
//! because the constants were never the point: the mapping layer exists to be
//! swept and compared by ear, and a value buried in a `const` can only be
//! changed by editing, rebuilding and re-rendering.
//!
//! **Why these live in mapping.** A control over how the music sounds is
//! aesthetic, so it belongs here and never in analysis — a knob in analysis
//! would invalidate every stored voiceprint each time it moved, where one here
//! is swept against a fixed voiceprint and heard immediately. That is recorded
//! as a decision in `docs/roadmap.md`.
//!
//! Defaults reproduce what the mapping did before it was parameterised, so
//! taking none of them changes nothing.

use serde::{Deserialize, Serialize};

use crate::mapping::{CONTINUOUS, Mapping};
use crate::tuning::{Degree, Tuning};

/// One knob, described well enough that a UI can offer it without being told.
///
/// **Why the range lives here and not in the UI.** A slider needs a minimum, a
/// maximum, a step and a starting position, and every one of those is a fact
/// about the mapping rather than about the browser. Written twice they drift,
/// and the way that failure shows up is a slider that cheerfully offers a value
/// the mapping quietly clamps away — the person moves it and hears nothing
/// change. Declared once here, `Params::default`, `Params::sane` and the
/// controls in the UI cannot disagree, and a knob added to this table appears
/// in the UI without anyone editing the UI.
///
/// **This is the wire type as well.** `routes::api` used to hold a second
/// `Knob`, field for field, differing only in `String` where this has
/// `&'static str`, plus the loop in `controls` that copied one into the other.
/// The stated reason was that the mapping crate carries no serialisation for a
/// UI — true of a `Score`, which the API projects on the way out, and not true
/// of this, which the API forwards unchanged. A copy that is required to be
/// identical is not an abstraction boundary; it is a second place to forget.
#[derive(Clone, Copy, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Knob {
    /// Which knob this is. Its `name` is the query parameter and the
    /// `Params` field alike.
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
    /// Mappings this knob reaches. Empty means every one of them.
    ///
    /// **Why a knob has to say.** The table exists so that a control cannot be
    /// offered at a value the mapping clamps away — a slider that moves and
    /// changes nothing. A knob belonging to one mapping and shown while another
    /// is playing is the same failure by another route, and the only thing that
    /// can be trusted to know which is the knob itself. `tests/api.rs` renders
    /// every knob against every mapping it claims and fails if the audio is
    /// unchanged, so a claim made here is a claim that is checked.
    ///
    /// [`Mapping`] rather than a name, so a knob cannot claim a mapping that
    /// does not exist. It used to be able to: the claim was a `&'static str`
    /// compared against another `&'static str`, and a typo here made a knob that
    /// reached nothing and was therefore never shown.
    pub mappings: &'static [Mapping],
    /// Whether to offer this one before anybody asks for it.
    ///
    /// **The rule: primary knobs decide what kind of piece this is, advanced
    /// ones adjust a piece you already have.** Ten sliders at equal weight is
    /// an instrument panel for someone who already knows what each does; to
    /// anyone else it reads as ten things they might be getting wrong. So the
    /// UI shows the primary ones and puts the rest behind a disclosure.
    ///
    /// Declared here for the same reason the range is. The alternative — a list
    /// of important names kept in the frontend — is a second opinion about the
    /// knob table that drifts the first time somebody adds a knob in Rust, and
    /// the way *that* failure shows up is a new control nobody can find.
    ///
    /// Note this is not simply a ranking by audible authority. `bind` was kept
    /// primary while the only figure for it said 18 cents — the smallest in the
    /// table — because it is the axis the whole project argues about. Nor is it
    /// the reverse: `spacing` earns its place on authority alone, having no
    /// thesis behind it whatever. Both arguments are admissible and a knob needs
    /// only one of them.
    ///
    /// That 18 cents turned out to be a measurement artefact — it was the field
    /// mapping's pitch travel, and on the Tonnetz `bind` moves 1168 cents, since
    /// the lattice's axes are derived from the scale being retuned. Left written
    /// down because the decision was right *before* anyone knew that, and a rule
    /// that only ever agrees with the latest measurement is not a rule.
    pub primary: bool,
}

impl Knob {
    /// The nearest value this knob actually accepts.
    pub fn clamped(&self, value: f32) -> f32 {
        value.clamp(self.min, self.max)
    }

    /// Whether this knob does anything to the given mapping.
    ///
    /// Empty means every one of them, and reading that convention is the whole
    /// of this function — which is why it is here and not written out again by
    /// each caller. It had been: the measurement bin and the browser both
    /// spelled out `is_empty() || contains(...)`, and a control shown beside a
    /// mapping it cannot reach is a slider that moves and changes nothing.
    pub fn reaches(&self, mapping: Mapping) -> bool {
        self.mappings.is_empty() || self.mappings.contains(&mapping)
    }
}

/// A value a knob can hold.
///
/// The knob table speaks `f32` — a slider's range is a range of numbers — while
/// two of the fields it sets count things and are `usize`. Without this the
/// conversion was written out at each site and written *differently*: `with`
/// rounded, on the stated grounds that a slider stopping just under an integer
/// should not silently mean the integer below, while `default` and `sane` cast,
/// which truncates. One rule here, applied everywhere the macro generates.
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
    /// Rounded, not truncated: these count things, and a slider stopping just
    /// under an integer would otherwise mean the one below it.
    fn from_knob(value: f32) -> Self {
        value.round().max(0.0) as usize
    }
    fn to_knob(self) -> f32 {
        self as f32
    }
}

/// Declare the knobs once.
///
/// **Why this is a macro and the alternative was not sustainable.** Every knob
/// used to be written seven times: the `Knob` const, the `Params` field, the
/// entry in `Default`, the clamp in `sane`, the arm in `with`, the query field
/// on the route's `VoiceParams`, and the line in `VoiceParams::params`. Three of
/// those seven the compiler did not check — the const, the `with` arm and the
/// query field — because none of them is an exhaustive struct literal. What that
/// bought was a knob published to the browser, drawn as a slider, and connected
/// to nothing: the query field was missing, so the value never reached `Params`.
/// A test caught that class, which is to say it was caught after being written
/// rather than instead.
///
/// So one declaration generates all seven, and adding a knob is adding a line.
/// The cost is honest and worth naming: fields declared here do not answer to
/// grep, and jumping to `Params::bind` lands on this macro rather than on a
/// field. At eleven knobs across seven sites that trade is clearly right; at
/// three knobs it would not have been.
///
/// [`KnobName`] is generated too, which is what makes `with` total — it had a
/// `panic!` for a name no knob has, the only one left in library code.
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
        /// Which knob, as a value.
        ///
        /// Generated beside the table, so a name that is not a knob cannot be
        /// spoken. It reaches the browser through `Knob::name`, so the settings
        /// a listener shares are keyed by a union rather than by `string`.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
        #[cfg_attr(feature = "ts", ts(export))]
        #[serde(rename_all = "lowercase")]
        pub enum KnobName {
            $( #[doc = $label] $variant, )*
        }

        impl KnobName {
            /// The wire spelling, which is also the `Params` field name.
            ///
            /// From `stringify!` on the field itself rather than restated, so
            /// this cannot drift from what it names. Serde lowercases the
            /// variant to the same string, and `name_round_trips_through_serde`
            /// in `tests/params.rs` holds the two together.
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
        ///
        /// Ordered by how much each one changes what you hear, so someone
        /// exploring from the top down hears something different at each step.
        /// A slice rather than a sized array: the length used to be written by
        /// hand beside the list, and it is not a fact anybody should have to
        /// maintain.
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
            /// Clamp everything into a range that produces sound rather than an
            /// error.
            ///
            /// Called once where the values arrive rather than checked at each
            /// use: a knob that arrives out of range is someone exploring, not a
            /// bug, and the useful response is the nearest thing that works.
            pub fn sane(self) -> Self {
                Params {
                    $( $name: <$ty as KnobValue>::from_knob(
                        $variant.clamped(self.$name.to_knob())
                    ), )*
                }
            }

            /// The same parameters with one knob moved, chosen by name.
            ///
            /// The table promises that a knob's name is the field it sets, and
            /// this is what acts on the promise — so a sweep, a test or the
            /// route can set a knob generically instead of keeping its own copy
            /// of the mapping from name to field.
            ///
            /// Total, since [`KnobName`] cannot name a knob that does not exist.
            /// It used to take a `&str` and `panic!` on anything unrecognised,
            /// because the alternative was returning the parameters unchanged —
            /// a silent no-op that reports a knob as doing nothing when what
            /// happened is that nobody set it.
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

        /// The knobs as a query string accepts them: each one absent or given.
        ///
        /// Generated from the same list as `Params`, which is the whole point.
        /// This was eleven hand-written `Option` fields on the route's
        /// `VoiceParams` plus eleven `unwrap_or` lines beside them, and nothing
        /// but a test tied either list to the knob table.
        ///
        /// Its own extractor rather than part of `VoiceParams`, because
        /// `serde_urlencoded` cannot flatten: a nested struct would have to be
        /// deserialised through `deserialize_any`, and every value in a query
        /// string is a string, so the numbers would not parse.
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
    /// How far the speaker's own scale is used, 0..1.
    ///
    /// **The convention-to-speaker axis**, and the longest-standing open
    /// question in the project. At 1 the degrees are exactly where this voice's
    /// spectrum puts them; at 0 they snap to twelve-tone equal temperament; in
    /// between they are interpolated in cents.
    ///
    /// The reason it exists rather than being decided: nobody knows where on
    /// this axis the music is, and it is not a thing anyone can settle by
    /// argument. It converts a question into something you listen to.
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

    /// How deep a dip in the roughness curve must be to count as a note.
    ///
    /// Raise it for a handful of very stable intervals, lower it for a dense
    /// microtonal set. The same speaker's *ah* gave eight degrees and their *ee*
    /// gave three, and part of that spread is this number rather than the voice.
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

    /// How many voices sound at once in the field mapping.
    VOICES voices: usize = {
        label: "Voices",
        min: 1.0,
        max: 12.0,
        step: 1.0,
        default: 5.0,
        about: "How many tones sound at once.",
        mappings: CONTINUOUS,
        primary: true,
    }

    /// Scale degrees between one field voice and the next.
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

    /// Octaves the whole field transposes across the speaker's pitch range.
    ///
    /// At 0 the prosody is discarded and the field sits still; at 1 it follows
    /// the speaker's pitch closely enough to read as a parallel melody, which is
    /// the naive mapping this project exists to avoid. The default is deliberately
    /// nearer the first.
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

    /// How far the vowel moves the harmony.
    ///
    /// Octaves the root travels front to back in the field mapping; cells of
    /// lattice crossed in the Tonnetz one. The same quantity read onto two
    /// geometries, which is why it is one knob rather than two.
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

    /// How far past a boundary the mouth must go before the harmony follows.
    ///
    /// Read only by the mappings that quantise their harmony, which today means
    /// the Tonnetz. It is the knob that decides whether a chord rings — and so
    /// the one that decides whether the derived tuning can be heard at all, the
    /// oldest open question in `docs/roadmap.md`.
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
    ///
    /// The other half of [`Self::hold`], and the half that reaches an artifact
    /// hysteresis in space cannot. Spatial hold asks *how far* past the boundary;
    /// this asks *for how long*, so a mouth that crosses a line and comes
    /// straight back leaves the chord alone. Measured need: at `hold = 1.0` a
    /// sung take spends 99% of its time in rings of a second or more and still
    /// has a median ring of 0.04 s.
    ///
    /// In seconds rather than frames because the frame rate is an analysis
    /// detail, and a knob whose meaning moved with the hop size would be a
    /// different knob on a different recording.
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

    /// How far the third formant opens or clusters the chord.
    ///
    /// The dimension of articulation the vowel chart cannot see. F1 and F2 place
    /// a vowel; F3 separates mouth shapes that share a place — rounded from
    /// spread, retroflex from not — and it moves while the other two hold still.
    /// At 0 the voices are evenly stacked whatever the mouth is doing.
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

    /// How much the rate of spectral change stirs the texture.
    ///
    /// The field's only answer to rhythm that does not involve cutting anything
    /// into notes. Spectral flux says *the sound is changing now* without
    /// claiming a syllable began — which is exactly the weakness that makes it
    /// a bad onset detector and a good continuous stream.
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

    /// How loud the consonants are against the pitched material, 0..1.
    ///
    /// At 0 they are silent, which is what every version of this project did
    /// before they were measured at all.
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

/// Pull a tuning toward equal temperament by `1 - bind`.
///
/// Interpolating in cents rather than in frequency ratio, because cents are
/// where the perceptual midpoint is: halfway between a just third at 386 and a
/// tempered one at 400 is 393, which is what a listener hears as halfway.
///
/// At `bind = 1` this returns the scale untouched. At 0 every degree lands on a
/// tempered note — which usually means the scale collapses to fewer degrees than
/// it had, since two neighbours can snap to the same place. That is honest
/// rather than a defect: it is what conventional tuning does to a spectrum that
/// did not ask for it.
/// Pull one pitch toward the nearest equal-tempered one by `1 - bind`.
///
/// The whole convention-to-speaker axis, on a single number. Separate from
/// [`bind_toward_equal`] because the two callers apply it at different places
/// and only one of them has a scale to hand: the field mapping binds the
/// *degrees* and stacks voices on them, while the Tonnetz binds each **sounding
/// pitch**, having built its chord from the speaker's own lattice first.
///
/// **Why the Tonnetz cannot bind its axes instead.** A lattice point's pitch is
/// `x·a + y·b`, so an error in an axis multiplies by how far out the point sits.
/// Binding the axes moves a chord near the tonic by a few cents and one three
/// cells away by fifty or more — and since the pitch folds into an octave, far
/// enough out it becomes a different note altogether. That is a structural
/// change wearing a tuning knob's name, and it made `bind` untestable on the one
/// mapping whose chords hold still long enough to test it.
pub fn bind_cents_toward_equal(cents: f32, bind: f32) -> f32 {
    if bind >= 1.0 {
        return cents;
    }
    let tempered = (cents / SEMITONE_CENTS).round() * SEMITONE_CENTS;
    tempered + (cents - tempered) * bind.clamp(0.0, 1.0)
}

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

    // Two degrees that snapped to the same tempered note are now one note played
    // twice. Keeping both would silently double a voice in the field and change
    // the balance for a reason nothing reports.
    degrees.dedup_by(|a, b| (a.cents - b.cents).abs() < 1.0);

    Tuning {
        degrees,
        curve: tuning.curve.clone(),
    }
}
