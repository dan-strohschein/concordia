//! `just bless-reference <name>`: blesses `tests/golden/m4_reference/<name>` (ADR-0090). Ignored,
//! so only that recipe runs it (with CONCORDIA_REFERENCE=<name>); refused in CI.

use concordia_tests::reference::bless;
use concordia_tests::repo_root;

#[test]
#[ignore = "blesses a reference: run with `just bless-reference <name>`"]
fn bless_reference() {
    let name = std::env::var("CONCORDIA_REFERENCE")
        .expect("set CONCORDIA_REFERENCE=<name> (use `just bless-reference <name>`)");
    let root = repo_root();
    let ci = std::env::var("CI").as_deref() == Ok("true");
    std::env::set_current_dir(&root).expect("the repository root");
    let b = bless(
        &name,
        &root.join("tests/golden"),
        &root.join("target/reference-review"),
        ci,
    )
    .unwrap_or_else(|e| panic!("{name}: {e}"));
    println!(
        "{name}: traced on {} in {:.1} s; baseline FLIP mean {:.4} p99 {:.4}; noise FLIP mean {:.4} p99 {:.4}",
        b.blessed.backend,
        b.elapsed_ms / 1000.0,
        b.blessed.baseline.mean,
        b.blessed.baseline.p99,
        b.blessed.noise.mean,
        b.blessed.noise.p99
    );
    println!(
        "review: {}/{name}.{{expected,actual,flip}}.png (reference, renderer, FLIP)",
        b.review
    );
    println!("wrote {}", b.image.display());
}
