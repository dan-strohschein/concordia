//! "Did you mean" suggestions (M0 phase 0.5; shared by scene errors and the Engine API).

use concordia_core::suggest::did_you_mean;

const NAMES: [&str; 5] = ["Transform", "Velocity", "Name", "Parent", "Translation"];

#[test]
fn close_typo_suggests_the_intended_name_first() {
    assert_eq!(did_you_mean("Transfrom", NAMES)[0], "Transform");
}

#[test]
fn at_most_three_suggestions_best_first() {
    let found = did_you_mean("Tran", NAMES);
    assert!(found.len() <= 3);
    assert!(found.iter().all(|n| n.starts_with("Tran")), "{found:?}");
}

#[test]
fn unrelated_input_suggests_nothing() {
    assert!(did_you_mean("zzzz", NAMES).is_empty());
    assert!(did_you_mean("", NAMES).is_empty());
}

#[test]
fn exact_match_is_not_suggested_back() {
    assert!(!did_you_mean("Name", NAMES).contains(&"Name"));
}
