//! Utility AI (M4 4.12a, M5): response curves, Mark's IAUS compensation, momentum and seeded
//! selection; and the blackboard's typed keys (M2).

use std::collections::BTreeMap;

use concordia_ai::{
    BbType, BbValue, Blackboard, BlackboardError, Consideration, Curve, Input, Selection,
    UtilityOption, choose, score,
};
use concordia_core::rng::SimRng;
use concordia_reflect::Reflect;
use proptest::prelude::*;

proptest! {
    #[test]
    fn utility_response_curves_match_closed_forms(x in -0.5f32..1.5, m in -2.0f32..2.0, k in 0.5f32..4.0, b in -1.0f32..1.0) {
        let clamp = |v: f32| v.clamp(0.0, 1.0);
        let near = |a: f32, b: f32| (a - b).abs() < 1e-5;
        let linear = Curve::Linear { slope: m, intercept: b }.eval(x);
        prop_assert!(near(linear, clamp(m * x + b)));
        let polynomial = Curve::Polynomial { exponent: k, slope: 1.0, x_shift: 0.0, y_shift: 0.0 }.eval(x);
        prop_assert!(near(polynomial, clamp(clamp(x).powf(k))));
        let logistic = Curve::Logistic { steepness: k * 4.0, midpoint: 0.5 }.eval(x);
        prop_assert!(near(logistic, clamp(1.0 / (1.0 + (-(k * 4.0) * (x - 0.5)).exp()))));
        let xc = x.clamp(1e-4, 1.0 - 1e-4);
        let logit = Curve::Logit { scale: 0.1 }.eval(x);
        prop_assert!(near(logit, clamp(0.5 + 0.1 * (xc / (1.0 - xc)).ln())));
        let pw = Curve::Piecewise { points: vec![[0.0, 0.0], [0.5, 1.0], [1.0, 0.5]] };
        let want = if x <= 0.0 { 0.0 } else if x <= 0.5 { x * 2.0 } else if x <= 1.0 { 1.0 - (x - 0.5) } else { 0.5 };
        prop_assert!(near(pw.eval(x), want), "{} vs {want}", pw.eval(x));
    }
}

fn option(name: &str, inputs: &[f32], weight: f32) -> UtilityOption {
    UtilityOption {
        name: name.into(),
        weight,
        considerations: inputs
            .iter()
            .map(|v| Consideration {
                input: Input::Constant(*v),
                curve: Curve::Linear {
                    slope: 1.0,
                    intercept: 0.0,
                },
            })
            .collect(),
    }
}

#[test]
fn utility_compensation_factor_matches_iaus_formula() {
    let bb = Blackboard::default();
    // One consideration: no compensation.
    assert!((score(&option("a", &[0.5], 1.0), &bb) - 0.5).abs() < 1e-6);
    // Three at 0.5: product 0.125; modification 1 − 1/3; makeup (1 − 0.5)·(2/3); final 0.5 + makeup·0.5 each.
    let s = 0.5f32;
    let comp = s + (1.0 - s) * (1.0 - 1.0 / 3.0) * s;
    assert!((score(&option("b", &[0.5, 0.5, 0.5], 1.0), &bb) - comp.powi(3)).abs() < 1e-6);
    // Weight scales; a zero consideration vetoes.
    assert!((score(&option("c", &[0.5], 2.0), &bb) - 1.0).abs() < 1e-6);
    assert_eq!(score(&option("d", &[0.0, 1.0], 1.0), &bb), 0.0);
}

#[test]
fn utility_selection_is_seeded_and_reproducible() {
    let bb = Blackboard::default();
    let options = vec![
        option("a", &[0.9], 1.0),
        option("b", &[0.85], 1.0),
        option("c", &[0.1], 1.0),
    ];
    // Best: the highest score; momentum keeps the current choice unless beaten by more than it.
    let best = Selection::Best { momentum: 0.0 };
    assert_eq!(
        choose(&options, &bb, &best, None, &mut SimRng::new(1))
            .unwrap()
            .index,
        0
    );
    let sticky = Selection::Best { momentum: 0.1 };
    assert_eq!(
        choose(&options, &bb, &sticky, Some(1), &mut SimRng::new(1))
            .unwrap()
            .index,
        1,
        "0.85 × 1.1 beats 0.9"
    );
    // Weighted random among the top two: the same seed, the same picks; never the third.
    let random = Selection::TopN {
        n: 2,
        momentum: 0.0,
    };
    let picks = |seed| {
        let mut rng = SimRng::new(seed);
        (0..200)
            .map(|_| {
                choose(&options, &bb, &random, None, &mut rng)
                    .unwrap()
                    .index
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(picks(5), picks(5));
    assert!(picks(5).iter().all(|&i| i < 2));
    assert!(picks(5).contains(&0) && picks(5).contains(&1));
    // Every option scoring zero: no choice.
    assert!(
        choose(
            &[option("z", &[0.0], 1.0)],
            &bb,
            &best,
            None,
            &mut SimRng::new(1)
        )
        .is_none()
    );
}

#[test]
fn blackboard_schema_is_reflected_and_validates_writes() {
    let mut bb = Blackboard::with_keys(BTreeMap::from([
        ("hp".into(), BbType::Float),
        ("target".into(), BbType::Entity),
    ]));
    bb.set("hp", BbValue::Float(3.0)).unwrap();
    assert!(matches!(
        bb.set("hp", BbValue::Bool(true)),
        Err(BlackboardError::WrongType { .. })
    ));
    assert!(matches!(
        bb.set("mana", BbValue::Float(1.0)),
        Err(BlackboardError::UnknownKey { .. })
    ));
    bb.set("target", BbValue::None).unwrap();
    let v1 = bb.version();
    bb.set("hp", BbValue::Float(3.0)).unwrap();
    assert_eq!(
        bb.version(),
        v1,
        "an unchanged value doesn't bump the version"
    );
    bb.set("hp", BbValue::Float(2.0)).unwrap();
    assert!(bb.version() > v1);
    // Reflected: a component with a schema, round-tripping through JSON.
    assert_eq!(Blackboard::type_info().name, "ai.Blackboard");
    let json = serde_json::to_value(&bb).unwrap();
    let back: Blackboard = serde_json::from_value(json).unwrap();
    assert_eq!(back, bb);
    assert!(concordia_reflect::schema_for(Blackboard::type_info())["type"] == "object");
}
