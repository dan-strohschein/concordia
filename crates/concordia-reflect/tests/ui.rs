//! Compile-fail cases for `#[derive(Reflect)]` (M0 phase 0.2).

#[test]
fn derive_rejects_unsupported_input() {
    trybuild::TestCases::new().compile_fail("tests/ui/*.rs");
}
