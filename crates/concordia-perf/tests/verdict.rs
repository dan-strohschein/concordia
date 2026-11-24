//! Verdicts: budgets name the pass and the excess, and only the reference machine can pass
//! (M4 phase 4.1a, ADR-0089).

mod common;

use common::{budget, limit, reference, reference_host, report};
use concordia_perf::{Stat, Verdict, Violation, check, judge};

#[test]
fn budget_violation_names_pass_and_excess() {
    let r = report(
        reference_host(),
        &[("forward", 1.9, 2.70), ("tonemap", 0.2, 0.25)],
    );
    let b = budget(&[
        ("forward", limit(Some(2.0), Some(2.00))),
        ("tonemap", limit(Some(0.3), Some(0.4))),
    ]);
    let violations = check(&r, &b);
    assert_eq!(
        violations,
        [Violation {
            pass: "forward".into(),
            stat: Stat::P95,
            measured_ms: Some(2.70),
            budget_ms: 2.00,
        }]
    );
    assert_eq!(
        violations[0].to_string(),
        "forward: p95 2.70 ms > budget 2.00 ms (+35%)"
    );
    assert_eq!(judge(&r, Some(&b), &reference()), Verdict::Fail(violations));
}

#[test]
fn the_frame_and_the_median_are_checked_too() {
    let mut r = report(reference_host(), &[("forward", 2.5, 2.6)]);
    r.frame = common::summary(5.5, 5.9);
    let b = budget(&[("forward", limit(Some(2.0), None))]);
    let shown: Vec<String> = check(&r, &b).iter().map(ToString::to_string).collect();
    assert_eq!(
        shown,
        [
            "frame: median 5.50 ms > budget 5.00 ms (+10%)",
            "forward: median 2.50 ms > budget 2.00 ms (+25%)",
        ]
    );
}

#[test]
fn a_budgeted_pass_that_never_ran_is_a_violation() {
    let r = report(reference_host(), &[("forward", 1.0, 1.1)]);
    let b = budget(&[("meshlet.draw", limit(Some(2.0), None))]);
    let v = check(&r, &b);
    assert_eq!(v.len(), 1);
    assert_eq!(
        v[0].to_string(),
        "meshlet.draw: not measured (budget median 2.00 ms)"
    );
}

#[test]
fn within_budget_on_the_reference_machine_passes() {
    let r = report(reference_host(), &[("forward", 1.0, 1.1)]);
    let b = budget(&[("forward", limit(Some(2.0), Some(2.5)))]);
    assert_eq!(judge(&r, Some(&b), &reference()), Verdict::Pass);
    assert_eq!(judge(&r, None, &reference()), Verdict::NoBudget);
}

#[test]
fn perf_test_on_non_reference_hardware_reports_but_does_not_claim_pass() {
    let b = budget(&[("forward", limit(Some(2.0), Some(2.5)))]);
    let mut host = reference_host();
    host.gpu_cores = Some(14);
    let fast = report(host.clone(), &[("forward", 1.0, 1.1)]);
    match judge(&fast, Some(&b), &reference()) {
        Verdict::NotReference { reason, violations } => {
            assert!(reason.contains("14") && reason.contains("18"), "{reason}");
            assert!(violations.is_empty());
        }
        other => panic!("within budget elsewhere is not a pass: {other:?}"),
    }
    let slow = report(host, &[("forward", 3.0, 3.5)]);
    let Verdict::NotReference { violations, .. } = judge(&slow, Some(&b), &reference()) else {
        panic!("over budget elsewhere is not a fail either");
    };
    assert_eq!(violations.len(), 2, "still reported: {violations:?}");
}

#[test]
fn every_difference_from_the_reference_is_named() {
    let r = reference();
    assert_eq!(r.mismatch(&reference_host()), None);
    type Change = fn(&mut concordia_perf::HostInfo);
    let cases: [(&str, Change); 6] = [
        ("Apple M2", |h| h.device_name = "Apple M2".into()),
        ("unknown", |h| h.gpu_cores = None),
        ("16 GB", |h| h.memory_gb = Some(16)),
        ("macOS 27", |h| h.os_version = "27.0".into()),
        ("Linux", |h| h.os = "Linux".into()),
        ("apple10", |h| h.families.push("apple10".into())),
    ];
    for (needle, change) in cases {
        let mut host = reference_host();
        change(&mut host);
        let why = r
            .mismatch(&host)
            .unwrap_or_else(|| panic!("{needle}: no mismatch"));
        assert!(why.contains(needle), "{needle}: {why}");
    }
}

#[test]
fn off_the_reference_machine_wins_over_a_missing_budget() {
    let mut host = reference_host();
    host.device_name = "Apple M2".into();
    let r = report(host, &[("forward", 1.0, 1.1)]);
    assert!(
        matches!(judge(&r, None, &reference()), Verdict::NotReference { .. }),
        "a gate with --require-reference must fail off the reference even before budgets exist"
    );
}

#[test]
fn a_pass_exactly_at_its_budget_is_within_it() {
    let r = report(reference_host(), &[("forward", 2.0, 2.5)]);
    let b = budget(&[("forward", limit(Some(2.0), Some(2.5)))]);
    assert_eq!(check(&r, &b), [], "budgets are inclusive");
}
