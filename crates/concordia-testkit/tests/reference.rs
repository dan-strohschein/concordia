//! Path-traced references (ADR-0090): `<scene>.reference.ron` round-trips canonically, and blessing
//! is refused in CI and for references too noisy to measure the renderer with.

use concordia_testkit::golden::{FlipScore, Golden, GoldenError, RgbaImage};
use concordia_testkit::reference::{Blessed, ReferenceFile, ReferenceLook};

fn file() -> ReferenceFile {
    ReferenceFile {
        project: "examples/m2-lamplight".into(),
        scene: None,
        view: Some(ReferenceLook {
            eye: [0.0, 1.7, 6.0],
            target: [0.0, 1.5, 0.0],
            fov_y_degrees: 55.0,
        }),
        width: 1280,
        height: 720,
        spp: 4096,
        seed: 1,
        max_bounces: 8,
        blessed: None,
    }
}

fn blessed(noise_mean: f32) -> ReferenceFile {
    ReferenceFile {
        blessed: Some(Blessed {
            backend: "gpu".into(),
            git: "0ee19fb".into(),
            baseline: FlipScore {
                mean: 0.12,
                p99: 0.6,
            },
            noise: FlipScore {
                mean: noise_mean,
                p99: 0.3,
            },
        }),
        ..file()
    }
}

/// A field's value as RON writes it.
fn field(f: &ReferenceFile, what: &str) -> String {
    match what {
        "width" => f.width.to_string(),
        "spp" => f.spp.to_string(),
        "max_bounces" => f.max_bounces.to_string(),
        _ => format!("{:?}", f.project),
    }
}

fn store(ci: bool) -> (tempfile::TempDir, Golden) {
    let dir = tempfile::tempdir().unwrap();
    let suite = dir.path().join("golden/m4_reference");
    std::fs::create_dir_all(&suite).unwrap();
    std::fs::write(
        suite.join("tolerances.ron"),
        "Tolerances(default: (mean: 0.1, p99: 0.9), goldens: {})",
    )
    .unwrap();
    let golden = Golden::new(
        dir.path().join("golden"),
        dir.path().join("failures"),
        false,
        ci,
    );
    (dir, golden)
}

fn image() -> RgbaImage {
    RgbaImage::new(2, 2, vec![128; 16]).unwrap()
}

#[test]
fn a_reference_file_round_trips_canonically() {
    let text = blessed(0.01).to_text().unwrap();
    let back = ReferenceFile::from_text(&text).unwrap();
    assert_eq!(back, blessed(0.01));
    assert_eq!(back.to_text().unwrap(), text, "canonical");
    insta::assert_snapshot!(text);
}

#[test]
fn a_reference_file_rejects_empty_inputs() {
    for (what, bad) in [
        ("width", ReferenceFile { width: 0, ..file() }),
        ("spp", ReferenceFile { spp: 0, ..file() }),
        (
            "max_bounces",
            ReferenceFile {
                max_bounces: 0,
                ..file()
            },
        ),
        (
            "project",
            ReferenceFile {
                project: String::new(),
                ..file()
            },
        ),
    ] {
        let err = bad.validate().unwrap_err();
        assert!(err.to_string().contains(what), "{what}: {err}");
        assert!(bad.to_text().is_err(), "{what}: never written");
        let text = file().to_text().unwrap().replace(
            &format!("{what}: {}", field(&file(), what)),
            &format!("{what}: {}", field(&bad, what)),
        );
        assert!(
            ReferenceFile::from_text(&text).is_err(),
            "{what}: never read"
        );
    }
}

#[test]
fn blessing_a_reference_writes_its_image_and_file() {
    let (dir, golden) = store(false);
    golden
        .bless_reference("street", &blessed(0.01), &image())
        .unwrap();
    let suite = dir.path().join("golden/m4_reference");
    assert!(suite.join("street.png").is_file());
    assert_eq!(golden.reference("street").unwrap(), blessed(0.01));
}

#[test]
fn blessing_a_reference_is_refused_in_ci() {
    let (_dir, golden) = store(true);
    let err = golden
        .bless_reference("street", &blessed(0.01), &image())
        .unwrap_err();
    assert!(matches!(err, GoldenError::BlessInCi), "{err}");
}

#[test]
fn a_reference_noisier_than_a_fifth_of_its_tolerance_is_refused() {
    let (dir, golden) = store(false);
    let err = golden
        .bless_reference("street", &blessed(0.03), &image())
        .unwrap_err();
    assert!(matches!(err, GoldenError::NoisyReference { .. }), "{err}");
    assert!(err.to_string().contains("spp"), "says what to do: {err}");
    assert!(
        !dir.path().join("golden/m4_reference/street.png").exists(),
        "nothing written"
    );
    golden
        .bless_reference("street", &blessed(0.02), &image())
        .unwrap();
}

#[test]
fn an_unblessed_reference_cannot_be_written() {
    let (_dir, golden) = store(false);
    let err = golden
        .bless_reference("street", &file(), &image())
        .unwrap_err();
    assert!(err.to_string().contains("blessed"), "{err}");
}
