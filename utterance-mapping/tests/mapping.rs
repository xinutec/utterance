//! The mapping taxonomy, held to the promises the enum makes for it.
//!
//! Most of what used to need a test here is now a type: a knob cannot claim a
//! mapping that does not exist, the render cannot dispatch on a misspelled
//! name, and a variant added without a score does not compile. What remains is
//! the handful of places where a `&'static str` or a `const` still restates
//! something the compiler cannot check — and each of those is checked here
//! rather than left to hold by inspection.

use utterance_mapping::mapping::{CONTINUOUS, Mapping, Material};

/// [`Mapping::name`] restates the serde attribute, so it can drift from it.
///
/// `from_name` goes through the derive, so this fails the moment the two
/// disagree — which is the only thing standing between a renamed variant and a
/// URL nobody can parse.
#[test]
fn name_round_trips_through_serde() {
    for mapping in Mapping::ALL {
        assert_eq!(
            Mapping::from_name(mapping.name()),
            Some(mapping),
            "{:?} spells itself {:?}, which serde does not read back",
            mapping,
            mapping.name()
        );
    }
}

/// A name no mapping has is refused rather than resolved to something near it.
///
/// The render route turns this `None` into a 400 naming the alternatives.
/// Silently rendering the default instead is how someone ends up describing a
/// mapping they never heard.
#[test]
fn an_unknown_name_is_not_a_mapping() {
    assert_eq!(Mapping::from_name("feild"), None);
    assert_eq!(Mapping::from_name(""), None);
    assert_eq!(Mapping::from_name("Field"), None, "the wire is lowercase");
}

/// [`CONTINUOUS`] is the one restatement left in the module.
///
/// It exists because `Knob::mappings` is a `const` and a const context cannot
/// filter an array — so the list is written out, and a mapping that started
/// making a texture without being added to it would quietly stop being reached
/// by the field knobs. That failure looks like five sliders that do nothing.
#[test]
fn continuous_is_every_texture_mapping() {
    let derived: Vec<Mapping> = Mapping::ALL
        .into_iter()
        .filter(|m| m.makes() == Material::Texture)
        .collect();
    assert_eq!(CONTINUOUS, derived.as_slice());
}

/// Every mapping is offerable: a label and a blurb, not an empty string.
///
/// `Mapping::ALL` is what the API publishes, so a variant with nothing to say
/// reaches a person as a blank row in the list they choose from.
#[test]
fn every_mapping_says_what_it_is() {
    for mapping in Mapping::ALL {
        assert!(!mapping.label().is_empty(), "{mapping:?} has no label");
        assert!(!mapping.about().is_empty(), "{mapping:?} has no blurb");
    }
}
