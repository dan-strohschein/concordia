//! Golden-image harness: FLIP compare, failure artifacts, blessing rules (M1 phase 1.2b; planned for 1.5a).

use concordia_testkit::golden::{Golden, GoldenError, RgbaImage, Tolerance, flip_score};

fn gradient(width: u32, height: u32, shift: u32) -> RgbaImage {
    let mut pixels = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let v = (((x + shift) * 255) / width).min(255) as u8;
            pixels.extend_from_slice(&[v, (y * 255 / height) as u8, 128, 255]);
        }
    }
    RgbaImage::new(width, height, pixels).unwrap()
}

fn store(dir: &tempfile::TempDir, bless: bool, ci: bool) -> Golden {
    Golden::new(
        dir.path().join("golden"),
        dir.path().join("failures"),
        bless,
        ci,
    )
}

#[test]
fn identical_images_have_zero_flip() {
    let score = flip_score(&gradient(64, 64, 0), &gradient(64, 64, 0)).unwrap();
    assert_eq!(score.mean, 0.0);
    assert_eq!(score.p99, 0.0);
}

#[test]
fn one_pixel_change_is_below_default_tolerance_but_large_shift_is_not() {
    let base = gradient(128, 128, 0);
    let mut one = base.clone();
    one.pixels_mut()[0..3].copy_from_slice(&[255, 0, 0]);
    assert!(Tolerance::default().accepts(&flip_score(&base, &one).unwrap()));
    let shifted = gradient(128, 128, 40);
    assert!(!Tolerance::default().accepts(&flip_score(&base, &shifted).unwrap()));
}

#[test]
fn image_size_mismatch_is_an_error_not_a_panic() {
    assert!(flip_score(&gradient(8, 8, 0), &gradient(8, 9, 0)).is_err());
    assert!(RgbaImage::new(2, 2, vec![0; 15]).is_err());
}

#[test]
fn missing_golden_fails_with_bless_hint() {
    let dir = tempfile::tempdir().unwrap();
    let err = store(&dir, false, false)
        .check("suite", "new", &gradient(8, 8, 0), Tolerance::default())
        .unwrap_err();
    assert!(matches!(err, GoldenError::Missing { .. }), "{err}");
    assert!(err.to_string().contains("just bless"), "{err}");
}

#[test]
fn bless_overwrites_only_when_enabled_and_never_in_ci() {
    let dir = tempfile::tempdir().unwrap();
    let image = gradient(16, 16, 0);
    let err = store(&dir, true, true)
        .check("s", "n", &image, Tolerance::default())
        .unwrap_err();
    assert!(matches!(err, GoldenError::BlessInCi), "{err}");
    store(&dir, true, false)
        .check("s", "n", &image, Tolerance::default())
        .unwrap();
    assert!(dir.path().join("golden/s/n.png").exists());
    let changed = gradient(16, 16, 9);
    assert!(
        store(&dir, false, false)
            .check("s", "n", &changed, Tolerance::default())
            .is_err()
    );
    store(&dir, true, false)
        .check("s", "n", &changed, Tolerance::default())
        .unwrap();
    store(&dir, false, false)
        .check("s", "n", &changed, Tolerance::default())
        .unwrap();
}

#[test]
fn failure_writes_actual_expected_and_flip_heatmap() {
    let dir = tempfile::tempdir().unwrap();
    store(&dir, true, false)
        .check("s", "n", &gradient(32, 32, 0), Tolerance::default())
        .unwrap();
    let err = store(&dir, false, false)
        .check("s", "n", &gradient(32, 32, 20), Tolerance::default())
        .unwrap_err();
    assert!(matches!(err, GoldenError::Mismatch { .. }), "{err}");
    for kind in ["actual", "expected", "flip"] {
        assert!(
            dir.path().join(format!("failures/s/n.{kind}.png")).exists(),
            "{kind}"
        );
    }
}

#[test]
fn golden_names_are_validated() {
    let dir = tempfile::tempdir().unwrap();
    let err = store(&dir, true, false)
        .check("../escape", "n", &gradient(4, 4, 0), Tolerance::default())
        .unwrap_err();
    assert!(matches!(err, GoldenError::Name(_)), "{err}");
}
