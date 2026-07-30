//! The guided steps as a taxonomy: labels in, steps out, corners where there are any.
//!
//! Small, but this is the join between a prompt on a screen and a measurement,
//! and it is the only thing standing between "the person was asked for *ee*" and
//! "this point is their *ee*".

use utterance::calibration::CalibrationStep;
use utterance_analysis::speaker::Corner;

#[test]
fn a_take_label_names_its_step() {
    // The ids the guided flow writes, verbatim. Spelled out here rather than
    // round-tripped through serde: a round trip would agree with itself after a
    // rename, and what needs pinning is the value on disk in takes already
    // recorded.
    assert_eq!(
        CalibrationStep::from_label("steady-ah"),
        Some(CalibrationStep::SteadyAh)
    );
    assert_eq!(
        CalibrationStep::from_label("vowel-ee"),
        Some(CalibrationStep::VowelEe)
    );
    assert_eq!(
        CalibrationStep::from_label("vowel-ah"),
        Some(CalibrationStep::VowelAh)
    );
    assert_eq!(
        CalibrationStep::from_label("vowel-oo"),
        Some(CalibrationStep::VowelOo)
    );
    assert_eq!(
        CalibrationStep::from_label("pitch-low"),
        Some(CalibrationStep::PitchLow)
    );
    assert_eq!(
        CalibrationStep::from_label("pitch-high"),
        Some(CalibrationStep::PitchHigh)
    );
    assert_eq!(
        CalibrationStep::from_label("speech"),
        Some(CalibrationStep::Speech)
    );
}

#[test]
fn a_label_that_names_no_step_is_not_one() {
    // The ordinary case, not an error: an uploaded file carries a filename, and
    // a take can be marked as calibration by hand without passing through the
    // guided flow. It still pools into the profile; it just cannot claim to be
    // a particular vowel.
    assert_eq!(CalibrationStep::from_label("my-song.wav"), None);
    assert_eq!(CalibrationStep::from_label(""), None);
    // Not the serde spelling of the variant, and deliberately not accepted —
    // the label on disk is the id, and admitting a second spelling would make
    // two takes of one step look like takes of two.
    assert_eq!(CalibrationStep::from_label("vowelEe"), None);
    assert_eq!(CalibrationStep::from_label("VowelEe"), None);
}

#[test]
fn only_the_three_corner_steps_reach_a_corner() {
    assert_eq!(
        CalibrationStep::VowelEe.corner(),
        Some(Corner::CloseFront),
        "ee is the close front corner"
    );
    assert_eq!(CalibrationStep::VowelAh.corner(), Some(Corner::Open));
    assert_eq!(CalibrationStep::VowelOo.corner(), Some(Corner::CloseBack));

    // steady-ah is held on *ah* and is still not the open corner: it is recorded
    // for its spectrum, at whatever shape holds a note steadiest, and it is the
    // longest take in the set. Counting it would let the more casual *ah* place
    // the corner by weight of evidence.
    assert_eq!(CalibrationStep::SteadyAh.corner(), None);
    assert_eq!(CalibrationStep::PitchLow.corner(), None);
    assert_eq!(CalibrationStep::PitchHigh.corner(), None);
    assert_eq!(CalibrationStep::Speech.corner(), None);
}
