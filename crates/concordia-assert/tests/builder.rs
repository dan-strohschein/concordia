//! The Rust builder makes the same trees as the text (M3 phase 3.2a).

use concordia_assert::builder::*;
use concordia_assert::parse;

#[test]
fn builder_matches_the_text_form() {
    let built = within(
        600.ticks(),
        distance(tag("player"), tag("door")).lt(2.0.m()),
    );
    let parsed = parse(r#"within 600 ticks: distance(tag("player"), tag("door")) < 2m"#).unwrap();
    assert_eq!(built, parsed);
    let built = after(
        1.0.secs(),
        always_for(
            30.ticks(),
            field(name("hero"), "Health.current")
                .gt(0.0)
                .and(!event("PlayerDied"))
                .or(exists(tag("shield"))),
        ),
    );
    let parsed = parse(
        r#"after 1s, always for 30 ticks: name("hero").Health.current > 0 and not event("PlayerDied") or exists(tag("shield"))"#,
    )
    .unwrap();
    assert_eq!(built, parsed);
    assert_eq!(
        never(all(with("Wisp"), "Transform.translation.y").le(count(tag("x")))),
        parse(r#"never: all(with(Wisp)).Transform.translation.y <= count(tag("x"))"#).unwrap()
    );
}
