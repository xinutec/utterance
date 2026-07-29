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

use crate::tuning::{Degree, Tuning};

/// Mappings that sound a continuous field, and so read the field knobs.
///
/// Named here rather than in the route because the knob table is what has to
/// agree with them: a knob claiming a mapping that does not exist, or a mapping
/// gaining a knob nobody told the UI about, are both caught by `tests/api.rs`
/// only because the claim is written down somewhere.
const CONTINUOUS: &[&str] = &["field", "tonnetz"];

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
#[derive(Clone, Copy, Debug)]
pub struct Knob {
    /// Query-parameter name, which is also the field name on `Params`.
    pub name: &'static str,
    /// What to call it in front of a person.
    pub label: &'static str,
    pub min: f32,
    pub max: f32,
    /// Smallest move worth offering. 1.0 where the value counts things.
    pub step: f32,
    pub default: f32,
    /// What moving it does, and what each end sounds like.
    pub about: &'static str,
    /// Mappings this knob reaches, by name. Empty means every one of them.
    ///
    /// **Why a knob has to say.** The table exists so that a control cannot be
    /// offered at a value the mapping clamps away — a slider that moves and
    /// changes nothing. A knob belonging to one mapping and shown while another
    /// is playing is the same failure by another route, and the only thing that
    /// can be trusted to know which is the knob itself. `tests/api.rs` renders
    /// every knob against every mapping it claims and fails if the audio is
    /// unchanged, so a claim made here is a claim that is checked.
    pub mappings: &'static [&'static str],
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
    /// Note this is not simply a ranking by audible authority. `bind` moves the
    /// field by 18 cents where `spacing` moves it by 1200, and `bind` is still
    /// primary: it is the axis the whole project is an argument about, and the
    /// question a listener is being asked to answer.
    pub primary: bool,
}

impl Knob {
    /// The nearest value this knob actually accepts.
    pub fn clamped(&self, value: f32) -> f32 {
        value.clamp(self.min, self.max)
    }
}

pub const BIND: Knob = Knob {
    name: "bind",
    label: "Bind to the voice",
    min: 0.0,
    max: 1.0,
    step: 0.05,
    default: 1.0,
    about: "At 1 the notes are exactly where this voice's spectrum puts them. \
            At 0 they snap to the twelve everyone else uses.",
    mappings: &[],
    primary: true,
};

pub const DENSITY: Knob = Knob {
    name: "density",
    label: "Scale density",
    min: 0.0005,
    max: 0.5,
    step: 0.002,
    default: crate::tuning::MIN_DEPTH,
    about: "How firm a note has to be to count. Low gives a crowded microtonal \
            set, high gives a handful of very stable intervals.",
    mappings: &[],
    primary: true,
};

pub const VOICES: Knob = Knob {
    name: "voices",
    label: "Voices",
    min: 1.0,
    max: 12.0,
    step: 1.0,
    default: 5.0,
    about: "How many tones sound at once.",
    mappings: CONTINUOUS,
    primary: true,
};

pub const SPACING: Knob = Knob {
    name: "spacing",
    label: "Spacing",
    min: 1.0,
    max: 6.0,
    step: 1.0,
    default: 2.0,
    about: "How far apart the voices sit. Scale degrees between one and the \
            next in the field mapping, least air between them in the Tonnetz. \
            1 is a cluster, higher is an open chord.",
    mappings: CONTINUOUS,
    primary: false,
};

pub const DRIFT: Knob = Knob {
    name: "drift",
    label: "Follow the pitch",
    min: 0.0,
    max: 2.0,
    step: 0.05,
    default: 0.25,
    about: "How far the music transposes with the speaker's pitch. At 0 it sits \
            still; near 1 it reads as a parallel melody.",
    mappings: CONTINUOUS,
    primary: false,
};

pub const REACH: Knob = Knob {
    name: "reach",
    label: "Follow the vowel",
    min: 0.0,
    max: 3.0,
    step: 0.05,
    default: 1.0,
    about: "How far the vowel moves the harmony: octaves the root travels in \
            the field mapping, cells of lattice crossed in the Tonnetz. This is \
            the articulation showing up as harmony.",
    mappings: CONTINUOUS,
    primary: true,
};

pub const VOICING: Knob = Knob {
    name: "voicing",
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
};

pub const ARTICULATION: Knob = Knob {
    name: "articulation",
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
};

pub const CONSONANTS: Knob = Knob {
    name: "consonants",
    label: "Consonants",
    min: 0.0,
    max: 2.0,
    step: 0.05,
    default: 1.0,
    about: "How loud the unpitched material is against the tones. At 0 they are \
            silent.",
    mappings: &[],
    primary: false,
};

pub const HOLD: Knob = Knob {
    name: "hold",
    label: "Hold the harmony",
    min: 0.0,
    max: 1.0,
    step: 0.05,
    default: 0.35,
    about: "How far the mouth must move past a boundary before the chord \
            changes. At 0 the harmony follows every wobble; higher makes it \
            commit, so a chord rings long enough to hear what it is tuned to.",
    mappings: &["tonnetz"],
    primary: true,
};

/// Every knob, in the order a person should meet them.
///
/// Ordered by how much each one changes what you hear, so someone exploring
/// from the top down hears something different at each step.
pub const KNOBS: [Knob; 10] = [
    BIND,
    DENSITY,
    VOICES,
    SPACING,
    DRIFT,
    REACH,
    HOLD,
    VOICING,
    ARTICULATION,
    CONSONANTS,
];

/// How the voice binds, and what it drives.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Params {
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
    pub bind: f32,
    /// How deep a dip in the roughness curve must be to count as a note.
    ///
    /// Raise it for a handful of very stable intervals, lower it for a dense
    /// microtonal set. The same speaker's *ah* gave eight degrees and their *ee*
    /// gave three, and part of that spread is this number rather than the voice.
    pub density: f32,
    /// How many voices sound at once in the field mapping.
    pub voices: usize,
    /// Scale degrees between one field voice and the next.
    pub spacing: usize,
    /// Octaves the whole field transposes across the speaker's pitch range.
    ///
    /// At 0 the prosody is discarded and the field sits still; at 1 it follows
    /// the speaker's pitch closely enough to read as a parallel melody, which is
    /// the naive mapping this project exists to avoid. The default is deliberately
    /// nearer the first.
    pub drift: f32,
    /// How far the vowel moves the harmony.
    ///
    /// Octaves the root travels front to back in the field mapping; cells of
    /// lattice crossed in the Tonnetz one. The same quantity read onto two
    /// geometries, which is why it is one knob rather than two.
    pub reach: f32,
    /// How far past a boundary the mouth must go before the harmony follows.
    ///
    /// Read only by the mappings that quantise their harmony, which today means
    /// the Tonnetz. It is the knob that decides whether a chord rings — and so
    /// the one that decides whether the derived tuning can be heard at all, the
    /// oldest open question in `docs/roadmap.md`.
    pub hold: f32,
    /// How far the third formant opens or clusters the chord.
    ///
    /// The dimension of articulation the vowel chart cannot see. F1 and F2 place
    /// a vowel; F3 separates mouth shapes that share a place — rounded from
    /// spread, retroflex from not — and it moves while the other two hold still.
    /// At 0 the voices are evenly stacked whatever the mouth is doing.
    pub voicing: f32,
    /// How much the rate of spectral change stirs the texture.
    ///
    /// The field's only answer to rhythm that does not involve cutting anything
    /// into notes. Spectral flux says *the sound is changing now* without
    /// claiming a syllable began — which is exactly the weakness that makes it
    /// a bad onset detector and a good continuous stream.
    pub articulation: f32,
    /// How loud the consonants are against the pitched material, 0..1.
    ///
    /// At 0 they are silent, which is what every version of this project did
    /// before they were measured at all.
    pub consonants: f32,
}

impl Default for Params {
    fn default() -> Self {
        Params {
            bind: BIND.default,
            density: DENSITY.default,
            voices: VOICES.default as usize,
            spacing: SPACING.default as usize,
            drift: DRIFT.default,
            reach: REACH.default,
            hold: HOLD.default,
            voicing: VOICING.default,
            articulation: ARTICULATION.default,
            consonants: CONSONANTS.default,
        }
    }
}

impl Params {
    /// Clamp everything into a range that produces sound rather than an error.
    ///
    /// Called once where the values arrive rather than checked at each use: a
    /// knob that arrives out of range is someone exploring, not a bug, and the
    /// useful response is the nearest thing that works.
    pub fn sane(self) -> Self {
        Params {
            bind: BIND.clamped(self.bind),
            density: DENSITY.clamped(self.density),
            voices: VOICES.clamped(self.voices as f32) as usize,
            spacing: SPACING.clamped(self.spacing as f32) as usize,
            drift: DRIFT.clamped(self.drift),
            reach: REACH.clamped(self.reach),
            hold: HOLD.clamped(self.hold),
            voicing: VOICING.clamped(self.voicing),
            articulation: ARTICULATION.clamped(self.articulation),
            consonants: CONSONANTS.clamped(self.consonants),
        }
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
pub fn bind_toward_equal(tuning: &Tuning, bind: f32) -> Tuning {
    if bind >= 1.0 {
        return tuning.clone();
    }

    let mut degrees: Vec<Degree> = tuning
        .degrees
        .iter()
        .map(|d| {
            let tempered = (d.cents / SEMITONE_CENTS).round() * SEMITONE_CENTS;
            let cents = tempered + (d.cents - tempered) * bind;
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
