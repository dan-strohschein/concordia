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
        tolerances.validate().expect("backend keys name backends");
        let backend_entries = tolerances.backends.values().flat_map(|b| {
            b.goldens
                .keys()
                .chain(b.paths.keys())
                .chain(b.culling.keys())
        });
        let shared = tolerances
            .goldens
            .keys()
            .chain(tolerances.paths.keys())
            .chain(tolerances.culling.keys());
        for entry in shared.chain(backend_entries) {
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

const WITH_BACKENDS: &str = r#"Tolerances(
    default: (mean: 0.005, p99: 0.2),
    goldens: {
        "loose": (mean: 0.02, p99: 0.3),
        "shared": (mean: 0.03, p99: 0.3),
    },
    backends: {
        "vulkan": (
            default: Some((mean: 0.008, p99: 0.2)),
            goldens: {
                "loose": (mean: 0.04, p99: 0.35),
            },
        ),
    },
)"#;

#[test]
fn backend_tolerances_apply_most_specific_first() {
    use concordia_rhi::Backend::{Metal, Vulkan};
    let dir = tempfile::tempdir().unwrap();
    write_tolerances(dir.path(), "s", WITH_BACKENDS);
    let golden = store(dir.path());
    let t = |backend, name| golden.tolerance_on(Some(backend), "s", name).unwrap();
    let tol = |mean, p99| Tolerance { mean, p99 };
    assert_eq!(
        t(Vulkan, "loose"),
        tol(0.04, 0.35),
        "the backend's own entry"
    );
    assert_eq!(
        t(Vulkan, "shared"),
        tol(0.03, 0.3),
        "the shared entry beats the backend default"
    );
    assert_eq!(t(Vulkan, "other"), tol(0.008, 0.2), "the backend's default");
    assert_eq!(t(Metal, "loose"), tol(0.02, 0.3), "Metal has no overrides");
    assert_eq!(t(Metal, "other"), tol(0.005, 0.2));
    assert_eq!(
        golden.tolerance("s", "loose").unwrap(),
        tol(0.02, 0.3),
        "no backend: the shared rules"
    );
}

#[test]
fn backend_tolerances_round_trip_canonically() {
    use concordia_testkit::golden::Tolerances;
    let file: Tolerances = concordia_scene::read_document(WITH_BACKENDS).unwrap();
    let canonical = concordia_scene::write_document(&file).unwrap();
    assert_eq!(
        concordia_scene::read_document::<Tolerances>(&canonical).unwrap(),
        file
    );
    assert!(canonical.contains("backends:"), "{canonical}");
    let plain: Tolerances = concordia_scene::read_document(FILE).unwrap();
    assert!(plain.backends.is_empty());
    assert!(
        !concordia_scene::write_document(&plain)
            .unwrap()
            .contains("backends"),
        "files without backends stay as they are"
    );
}

#[test]
fn unknown_backends_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    write_tolerances(
        dir.path(),
        "s",
        &WITH_BACKENDS.replace("\"vulkan\"", "\"dx12\""),
    );
    let err = store(dir.path())
        .tolerance_on(Some(concordia_rhi::Backend::Vulkan), "s", "loose")
        .expect_err("dx12 is no backend");
    assert!(err.to_string().contains("dx12"), "{err}");
}

#[test]
fn backend_failures_are_written_under_the_backend() {
    let dir = tempfile::tempdir().unwrap();
    let (base, nudged) = images();
    Golden::new(dir.path().join("golden"), dir.path().join("f"), true, false)
        .check("s", "loose", &base)
        .unwrap();
    let err = Golden::new(
        dir.path().join("golden"),
        dir.path().join("f"),
        false,
        false,
    )
    .check_on(Some(concordia_rhi::Backend::Vulkan), "s", "loose", &nudged)
    .expect_err("outside the default tolerance");
    assert!(matches!(err, GoldenError::Mismatch { .. }), "{err}");
    let out = dir.path().join("f/s/vulkan");
    for file in ["loose.actual.png", "loose.expected.png", "loose.flip.png"] {
        assert!(out.join(file).is_file(), "{file} under {}", out.display());
    }
}

#[test]
fn keep_writes_comparisons_and_scores_for_passing_checks() {
    let dir = tempfile::tempdir().unwrap();
    let (base, _) = images();
    Golden::new(dir.path().join("golden"), dir.path().join("f"), true, false)
        .check("s", "kept", &base)
        .unwrap();
    let quiet = Golden::new(
        dir.path().join("golden"),
        dir.path().join("q"),
        false,
        false,
    );
    quiet.check("s", "kept", &base).unwrap();
    assert!(
        !dir.path().join("q").exists(),
        "passing checks write nothing by default"
    );
    Golden::new(
        dir.path().join("golden"),
        dir.path().join("f"),
        false,
        false,
    )
    .with_keep(true)
    .check_on(Some(concordia_rhi::Backend::Vulkan), "s", "kept", &base)
    .unwrap();
    let out = dir.path().join("f/s/vulkan");
    assert!(out.join("kept.actual.png").is_file());
    let score = std::fs::read_to_string(out.join("kept.score.ron")).unwrap();
    assert!(
        score.contains("passed: true") && score.contains("mean: 0.0"),
        "{score}"
    );
}

#[test]
fn blessing_is_refused_on_vulkan() {
    let dir = tempfile::tempdir().unwrap();
    let (base, _) = images();
    let err = Golden::new(dir.path().join("golden"), dir.path().join("f"), true, false)
        .check_on(Some(concordia_rhi::Backend::Vulkan), "s", "x", &base)
        .expect_err("Vulkan compares against Metal's blessed images (D4)");
    assert!(matches!(err, GoldenError::BlessOnBackend { .. }), "{err}");
    assert!(!dir.path().join("golden/s/x.png").exists());
}

#[test]
fn keep_writes_the_actual_image_of_a_missing_golden() {
    let dir = tempfile::tempdir().unwrap();
    let (base, _) = images();
    let quiet = Golden::new(
        dir.path().join("golden"),
        dir.path().join("q"),
        false,
        false,
    );
    let err = quiet.check("s", "new", &base).expect_err("no golden yet");
    assert!(matches!(err, GoldenError::Missing { .. }), "{err}");
    assert!(
        !dir.path().join("q").exists(),
        "nothing written without keep"
    );
    let err = Golden::new(
        dir.path().join("golden"),
        dir.path().join("f"),
        false,
        false,
    )
    .with_keep(true)
    .check_on(Some(concordia_rhi::Backend::Vulkan), "s", "new", &base)
    .expect_err("still missing");
    assert!(matches!(err, GoldenError::Missing { .. }), "{err}");
    let actual = dir.path().join("f/s/vulkan/new.actual.png");
    assert!(actual.is_file(), "the image to review before blessing");
    assert!(
        !dir.path().join("golden/s/new.png").exists(),
        "keep never blesses"
    );
}

const WITH_PATHS: &str = r#"Tolerances(
    default: (mean: 0.005, p99: 0.2),
    goldens: {},
    paths: {
        "scene": (mean: 0.002, p99: 0.1),
    },
    backends: {
        "vulkan": (
            default: None,
            goldens: {},
            paths: {
                "scene": (mean: 0.004, p99: 0.1),
            },
        ),
    },
)"#;

const WITH_CULLING: &str = r#"Tolerances(
    default: (mean: 0.005, p99: 0.2),
    goldens: {},
    paths: {
        "scene": (mean: 0.002, p99: 0.1),
    },
    culling: {
        "scene": (mean: 0.001, p99: 0.05),
    },
    backends: {
        "vulkan": (
            default: None,
            goldens: {},
            culling: {
                "scene": (mean: 0.003, p99: 0.05),
            },
        ),
    },
)"#;

/// A scene rendered with culling on and off agrees within its golden's `culling` entry (M4 4.4),
/// per backend like the others, and independently of its `paths` entry.
#[test]
fn culling_tolerances_are_read_per_golden_and_backend() {
    use concordia_testkit::golden::Comparison;
    let dir = tempfile::tempdir().unwrap();
    write_tolerances(dir.path(), "s", WITH_CULLING);
    let golden = store(dir.path());
    let culling = |backend, name| {
        golden
            .comparison_tolerance_on(backend, "s", name, Comparison::Culling)
            .unwrap()
    };
    let t = |mean, p99| Some(Tolerance { mean, p99 });
    assert_eq!(culling(None, "scene"), t(0.001, 0.05));
    assert_eq!(
        culling(Some(concordia_rhi::Backend::Vulkan), "scene"),
        t(0.003, 0.05)
    );
    assert_eq!(culling(None, "other"), None);
    assert_eq!(
        golden
            .path_tolerance_on(Some(concordia_rhi::Backend::Vulkan), "s", "scene")
            .unwrap(),
        t(0.002, 0.1),
        "paths are their own entries"
    );
}

/// Two draw paths of one golden's scene agree within its `paths` entry (M4 4.3b: the compute
/// fallback against the mesh-shader path), per backend like golden entries; none without one.
#[test]
fn path_tolerances_are_read_per_golden_and_backend() {
    let dir = tempfile::tempdir().unwrap();
    write_tolerances(dir.path(), "s", WITH_PATHS);
    let golden = store(dir.path());
    let paths = |backend, name| golden.path_tolerance_on(backend, "s", name).unwrap();
    assert_eq!(
        paths(None, "scene"),
        Some(Tolerance {
            mean: 0.002,
            p99: 0.1
        })
    );
    assert_eq!(
        paths(Some(concordia_rhi::Backend::Vulkan), "scene"),
        Some(Tolerance {
            mean: 0.004,
            p99: 0.1
        })
    );
    assert_eq!(
        paths(Some(concordia_rhi::Backend::Metal), "scene"),
        Some(Tolerance {
            mean: 0.002,
            p99: 0.1
        })
    );
    assert_eq!(paths(None, "other"), None, "no entry, no path tolerance");
    assert_eq!(
        golden.path_tolerance_on(None, "no_file", "x").unwrap(),
        None
    );
    // Files without `paths` still read.
    write_tolerances(dir.path(), "old", FILE);
    assert_eq!(
        golden.path_tolerance_on(None, "old", "loose").unwrap(),
        None
    );
}
