//! Parsing and printing assertions (M3 phase 3.2a, grammar v1 in the M3 plan).

use concordia_assert::*;

/// The design's example parses to: within 600 ticks, the distance between the `player` and
/// `door` tags is less than 2 meters (snapshot of the tree), and prints back to the same text.
#[test]
fn parses_within_distance_example_from_design() {
    let text = r#"within 600 ticks: distance(tag("player"), tag("door")) < 2m"#;
    let assertion = parse(text).unwrap();
    insta::assert_debug_snapshot!(assertion);
    assert_eq!(assertion.to_string(), text);
}

/// Every form of the grammar parses and prints back canonically: temporal operators, `for`
/// spans, `after`, seconds, units, `and` binding tighter than `or`, `not`, parentheses,
/// quantified fields with vector components, counts, `exists`, events and every comparison.
#[test]
fn every_form_round_trips() {
    for text in [
        r#"always: count(with(Health)) >= 1"#,
        r#"always for 120 ticks: name("hero").Health.current > 0"#,
        r#"never for 2.5s: event("PlayerDied")"#,
        r#"eventually: exists(tag("coin")) == 0"#,
        r#"at 10s: any(tag("lamp")).Lamp.lit"#,
        r#"after 60 ticks, within 30 ticks: all(with(Wisp)).Transform.translation.y <= -1.5"#,
        r#"within 600 ticks: not event("Hit") and count(tag("enemy")) != 3 or distance(name("a"), id("018bcfe5-6800-7000-a424-000000000011")) > 90deg"#,
        r#"within 600 ticks: (event("A") or event("B")) and event("C")"#,
        r#"never: not (event("A") and event("B"))"#,
        r#"always: name("hero").Transform.rotation.w < 1 and 3s >= 2 ticks"#,
    ] {
        let parsed = parse(text).unwrap_or_else(|e| panic!("{text}: {e}"));
        assert_eq!(parsed.to_string(), text, "{parsed:?}");
    }
    // `and` binds tighter than `or`: a or (b and c).
    let p = parse(r#"always: event("a") or event("b") and event("c")"#).unwrap();
    assert!(
        matches!(p.pred, Pred::Or(_, ref r) if matches!(**r, Pred::And(..))),
        "{p:?}"
    );
}

/// A misspelled unit is pointed at: "within 600 " is 11 columns, so `tickz` is columns 12–16,
/// where a duration unit (`ticks` or `s`) was expected.
#[test]
fn parse_error_points_at_offending_span() {
    let err = parse(r#"within 600 tickz: distance(tag("a"), tag("b")) < 2m"#).unwrap_err();
    assert_eq!((err.start, err.end), (12, 16), "{err}");
    assert_eq!(err.found, "tickz");
    assert!(err.expected.contains(&"ticks".to_owned()), "{err}");
    assert!(err.expected.contains(&"s".to_owned()), "{err}");
    let err = parse("within 600 ticks distance").unwrap_err();
    assert_eq!(err.expected, [":"], "{err}");
    let err = parse(r#"always: name("a"#).unwrap_err();
    assert!(err.to_string().contains("string"), "{err}");
}

mod strategies {
    use concordia_assert::*;
    use proptest::prelude::*;

    fn word() -> impl Strategy<Value = String> {
        "[A-Za-z][A-Za-z0-9_]{0,7}"
    }

    fn sel() -> impl Strategy<Value = Sel> {
        prop_oneof![
            "[a-z ]{0,6}".prop_map(Sel::Tag),
            "[a-z ]{0,6}".prop_map(Sel::Name),
            word().prop_map(Sel::With),
        ]
    }

    fn num() -> impl Strategy<Value = f64> {
        (-1000i32..1000, 0u32..3).prop_map(|(n, d)| f64::from(n) / 10f64.powi(d as i32))
    }

    fn expr() -> impl Strategy<Value = Expr> {
        let unit = prop_oneof![
            Just(Unit::None),
            Just(Unit::M),
            Just(Unit::S),
            Just(Unit::Ticks),
            Just(Unit::Deg)
        ];
        prop_oneof![
            (num(), unit).prop_map(|(value, unit)| Expr::Num { value, unit }),
            (sel(), sel()).prop_map(|(a, b)| Expr::Distance(a, b)),
            sel().prop_map(Expr::Count),
            (
                prop_oneof![Just(Quant::One), Just(Quant::Any), Just(Quant::All)],
                sel(),
                proptest::collection::vec(word(), 1..4)
            )
                .prop_map(|(quant, sel, path)| Expr::Field { quant, sel, path }),
        ]
    }

    fn pred() -> impl Strategy<Value = Pred> {
        let cmp = prop_oneof![
            Just(Cmp::Lt),
            Just(Cmp::Le),
            Just(Cmp::Gt),
            Just(Cmp::Ge),
            Just(Cmp::Eq),
            Just(Cmp::Ne)
        ];
        let leaf = prop_oneof![
            (expr(), cmp, expr()).prop_map(|(lhs, op, rhs)| Pred::Cmp { lhs, op, rhs }),
            "[A-Za-z]{1,8}".prop_map(|e| Pred::Is(Expr::Event(e))),
            sel().prop_map(|s| Pred::Is(Expr::Exists(s))),
        ];
        leaf.prop_recursive(3, 12, 2, |inner| {
            prop_oneof![
                (inner.clone(), inner.clone())
                    .prop_map(|(a, b)| Pred::And(Box::new(a), Box::new(b))),
                (inner.clone(), inner.clone())
                    .prop_map(|(a, b)| Pred::Or(Box::new(a), Box::new(b))),
                inner.prop_map(|p| Pred::Not(Box::new(p))),
            ]
        })
    }

    fn dur() -> impl Strategy<Value = Dur> {
        prop_oneof![
            (1u64..10_000).prop_map(Dur::Ticks),
            (1u32..1000).prop_map(|n| Dur::Seconds(f64::from(n) / 4.0)),
        ]
    }

    fn temporal() -> impl Strategy<Value = Temporal> {
        let base = prop_oneof![
            dur().prop_map(Temporal::Within),
            proptest::option::of(dur()).prop_map(Temporal::Always),
            proptest::option::of(dur()).prop_map(Temporal::Never),
            Just(Temporal::Eventually),
            dur().prop_map(Temporal::At),
        ];
        (proptest::option::of(dur()), base).prop_map(|(after, t)| match after {
            Some(d) => Temporal::After(d, Box::new(t)),
            None => t,
        })
    }

    proptest! {
        /// Printing then parsing gives back the same tree.
        #[test]
        fn prop_pretty_print_then_parse_is_identity(temporal in temporal(), pred in pred()) {
            let a = Assertion { temporal, pred };
            let text = a.to_string();
            match parse(&text) {
                Ok(parsed) => prop_assert_eq!(parsed, a, "{}", text),
                Err(e) => prop_assert!(false, "{}: {}", text, e),
            }
        }
    }
}
