//! Golden tolerances live in text, per suite (M4 phase 4.1a, D2): `tolerances.ron` beside the
//! PNGs; `Tolerance::default()` applies only where a suite has no file.

use std::path::{Path, PathBuf};

use concordia_testkit::golden::{Golden, GoldenError, RgbaImage, Tolerance};

fn store(dir: &Path) -> Golden {
    Golden::new(dir.join("golden"), dir.join("failures"), false, false)
}

fn write_tolerances(dir: &Path, suite: &str, text: &str) {
    let path = dir.join("golden").join(suite);
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(path.join("tolerances.ron"), text).unwrap();
}

const FILE: &str = r#"Tolerances(
    default: (mean: 0.005, p99: 0.2),
    goldens: {
        "loose": (mean: 0.02, p99: 0.3),
    },
)"#;

#[test]
fn tolerance_file_overrides_the_default() {
    let dir = tempfile::tempdir().unwrap();
    write_tolerances(dir.path(), "s", FILE);
    let golden = store(dir.path());
    let t = |name| golden.tolerance("s", name).unwrap();
    assert_eq!(
        t("loose"),
        Tolerance {
            mean: 0.02,
            p99: 0.3
        }
    );
    assert_eq!(
        t("other"),
        Tolerance {
            mean: 0.005,
            p99: 0.2
        },
        "the file's default"
    );
    assert_eq!(
        golden.tolerance("no_file", "x").unwrap(),
        Tolerance::default(),
        "no file: the code's default"
    );
}

#[test]
fn tolerance_file_rejects_unknown_fields_and_out_of_range() {
    for (bad, why) in [
        (FILE.replace("p99: 0.3", "p99: 1.5"), "1.5"),
        (FILE.replace("mean: 0.02", "mean: -0.1"), "-0.1"),
        (FILE.replace("goldens:", "golden:"), "golden"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        write_tolerances(dir.path(), "s", &bad);
        let err = store(dir.path()).tolerance("s", "loose").expect_err(why);
        assert!(
            matches!(err, GoldenError::Tolerances { .. }),
            "{why}: {err}"
        );
        assert!(err.to_string().contains("tolerances.ron"), "{why}: {err}");
    }
}

/// A gradient, and the same gradient one step brighter in a band (a small FLIP error).
fn images() -> (RgbaImage, RgbaImage) {
    let mut pixels = Vec::new();
    for y in 0..64u32 {
        for x in 0..64u32 {
            pixels.extend_from_slice(&[(x * 4) as u8, (y * 4) as u8, 128, 255]);
        }
    }
    let base = RgbaImage::new(64, 64, pixels).unwrap();
    let mut nudged = base.clone();
    for p in nudged.pixels_mut().chunks_mut(4).take(64 * 24) {
        p[0] = p[0].saturating_add(24);
    }
    (base, nudged)
}

#[test]
fn check_compares_within_the_files_tolerance() {
    let dir = tempfile::tempdir().unwrap();
    let (base, nudged) = images();
    Golden::new(dir.path().join("golden"), dir.path().join("f"), true, false)
        .check("s", "loose", &base)
        .unwrap();
    let strict = store(dir.path()).check("s", "loose", &nudged);
    assert!(
        matches!(strict, Err(GoldenError::Mismatch { .. })),
        "{strict:?}"
    );
    write_tolerances(
        dir.path(),
        "s",
        &FILE.replace("(mean: 0.02, p99: 0.3)", "(mean: 1.0, p99: 1.0)"),
    );
    store(dir.path())
        .check("s", "loose", &nudged)
        .expect("the file's looser tolerance accepts it");
}

fn repo_golden() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden")
}

/// Every suite has a tolerances file, and every entry names a golden that exists.
#[test]
fn every_tolerance_entry_names_an_existing_golden() {
    let golden = Golden::new(repo_golden(), std::env::temp_dir(), false, false);
    for suite in std::fs::read_dir(repo_golden()).unwrap() {
        let suite = suite.unwrap().path();
        if !suite.is_dir() {
            continue;
        }
        let name = suite.file_name().unwrap().to_str().unwrap().to_owned();
        let file = suite.join("tolerances.ron");
        assert!(file.is_file(), "{name} has no tolerances.ron");
        golden.tolerance(&name, "any").expect("a valid file");
        let text = std::fs::read_to_string(&file).unwrap();
        let tolerances: concordia_testkit::golden::Tolerances =
            concordia_scene::read_document(&text).unwrap();
        for entry in tolerances.goldens.keys() {
            assert!(
                suite.join(format!("{entry}.png")).is_file(),
                "{name}/tolerances.ron names {entry}, which has no PNG"
            );
        }
        assert_eq!(
            concordia_scene::write_document(&tolerances).unwrap(),
            text,
            "{name}/tolerances.ron is canonical"
        );
    }
}
