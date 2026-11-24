//! Budget files are canonical text, validated on read (M4 phase 4.1a).

mod common;

use common::{budget, limit};
use concordia_perf::BudgetFile;

#[test]
fn budget_file_roundtrips_canonically() {
    let file = budget(&[
        ("tonemap", limit(Some(0.3), None)),
        ("forward", limit(Some(2.0), Some(2.5))),
        ("shadow 0", limit(None, Some(0.8))),
    ]);
    let text = file.to_text().expect("writes");
    insta::assert_snapshot!("m1_gate_budget", text);
    let back = BudgetFile::read(&text).expect("reads its own text");
    assert_eq!(back, file);
    assert_eq!(
        back.to_text().unwrap(),
        text,
        "canonical: a second write is identical"
    );
}

#[test]
fn budget_file_rejects_unknown_fields_negative_and_empty_limits() {
    let good = budget(&[("forward", limit(Some(2.0), None))])
        .to_text()
        .unwrap();
    let unknown = good.replace("median_ms", "mean_ms");
    assert!(
        BudgetFile::read(&unknown).is_err(),
        "unknown field: {unknown}"
    );
    let negative = good.replace("Some(2.0)", "Some(-2.0)");
    let err = BudgetFile::read(&negative).expect_err("negative budget");
    assert!(err.to_string().contains("forward"), "{err}");
    let empty = good.replace("median_ms: Some(2.0)", "median_ms: None");
    let err = BudgetFile::read(&empty).expect_err("a limit with no bound");
    assert!(err.to_string().contains("forward"), "{err}");
}
