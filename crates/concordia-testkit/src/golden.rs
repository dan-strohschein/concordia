//! Golden images: `tests/golden/<suite>/<name>.png`, compared with LDR-FLIP; blessed with
//! `CONCORDIA_BLESS=1` (`just bless`), never when `CI=true`. Failures leave
//! `<name>.{actual,expected,flip}.png` under `target/golden-failures/<suite>/`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

/// An 8-bit RGBA image (sRGB-encoded, as captured).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbaImage {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl RgbaImage {
    /// An image from tightly packed RGBA rows.
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self, GoldenError> {
        let expected = width as usize * height as usize * 4;
        match width > 0 && height > 0 && pixels.len() == expected {
            true => Ok(Self {
                width,
                height,
                pixels,
            }),
            false => Err(GoldenError::Size(format!(
                "{width}x{height} RGBA needs {expected} bytes, got {}",
                pixels.len()
            ))),
        }
    }

    /// Width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// RGBA bytes.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// RGBA bytes, mutable.
    pub fn pixels_mut(&mut self) -> &mut [u8] {
        &mut self.pixels
    }

    fn rgb(&self) -> Vec<u8> {
        self.pixels
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect()
    }

    pub(crate) fn save(&self, path: &Path) -> Result<(), GoldenError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| io(path, e))?;
        }
        image::save_buffer(
            path,
            &self.pixels,
            self.width,
            self.height,
            image::ColorType::Rgba8,
        )
        .map_err(|e| GoldenError::Io(format!("{}: {e}", path.display())))
    }

    fn load(path: &Path) -> Result<Self, GoldenError> {
        let img =
            image::open(path).map_err(|e| GoldenError::Io(format!("{}: {e}", path.display())))?;
        let rgba = img.to_rgba8();
        Self::new(rgba.width(), rgba.height(), rgba.into_raw())
    }

    /// Decodes PNG bytes (e.g. a `capture.screenshot` result).
    pub fn from_png(bytes: &[u8]) -> Result<Self, GoldenError> {
        let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
            .map_err(|e| GoldenError::Io(format!("PNG: {e}")))?;
        let rgba = img.to_rgba8();
        Self::new(rgba.width(), rgba.height(), rgba.into_raw())
    }
}

/// FLIP statistics of a comparison (0 = identical, 1 = maximally different).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct FlipScore {
    /// Mean error.
    pub mean: f32,
    /// 99th-percentile error.
    pub p99: f32,
}

/// Accepted FLIP error (per suite in `tests/golden/<suite>/tolerances.ron`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Tolerance {
    /// Largest accepted mean.
    #[reflect(range(min = 0.0, max = 1.0))]
    pub mean: f32,
    /// Largest accepted 99th percentile.
    #[reflect(range(min = 0.0, max = 1.0))]
    pub p99: f32,
}

/// `tests/golden/<suite>/tolerances.ron`: the suite's default and per-golden tolerances.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Tolerances {
    /// For goldens not listed.
    pub default: Tolerance,
    /// By golden name.
    pub goldens: BTreeMap<String, Tolerance>,
}

impl Default for Tolerance {
    fn default() -> Self {
        Self {
            mean: 0.01,
            p99: 0.25,
        }
    }
}

impl Tolerance {
    /// True if `score` is within tolerance.
    pub fn accepts(&self, score: &FlipScore) -> bool {
        score.mean <= self.mean && score.p99 <= self.p99
    }
}

/// Why a golden check failed.
#[derive(Debug, thiserror::Error)]
pub enum GoldenError {
    /// A suite's `tolerances.ron` is unreadable or invalid.
    #[error("{path}: {message}")]
    Tolerances {
        /// The file.
        path: PathBuf,
        /// Why.
        message: String,
    },
    /// No golden exists yet.
    #[error("no golden at {path}; render it, check it by eye, then run `just bless`")]
    Missing {
        /// Expected path.
        path: PathBuf,
    },
    /// The image differs beyond tolerance.
    #[error("{path}: FLIP mean {:.4} p99 {:.4} exceeds tolerance mean {:.4} p99 {:.4}; see {failures}", score.mean, score.p99, tolerance.mean, tolerance.p99)]
    Mismatch {
        /// Golden path.
        path: PathBuf,
        /// Measured error.
        score: FlipScore,
        /// Allowed error.
        tolerance: Tolerance,
        /// Where actual/expected/flip images were written.
        failures: String,
    },
    /// Blessing was requested while `CI=true`.
    #[error("refusing to bless golden images in CI (CI=true); bless locally with `just bless`")]
    BlessInCi,
    /// A reference's own noise is too large to measure the renderer against it.
    #[error("{name}: noise FLIP mean {:.4} exceeds a fifth of the tolerance ({:.4}); trace it with more spp", noise.mean, tolerance.mean / 5.0)]
    NoisyReference {
        /// Reference name.
        name: String,
        /// FLIP between two seeds.
        noise: FlipScore,
        /// The suite's tolerance for it.
        tolerance: Tolerance,
    },
    /// A reference file is invalid.
    #[error("{0}")]
    Reference(String),
    /// Images or buffers have the wrong size.
    #[error("image size: {0}")]
    Size(String),
    /// A suite or name is not a plain file name.
    #[error("invalid golden name: {0}")]
    Name(String),
    /// Reading or writing a PNG failed.
    #[error("golden io: {0}")]
    Io(String),
}

fn io(path: &Path, e: std::io::Error) -> GoldenError {
    GoldenError::Io(format!("{}: {e}", path.display()))
}

/// FLIP between two equally sized images (alpha ignored).
pub fn flip_score(expected: &RgbaImage, actual: &RgbaImage) -> Result<FlipScore, GoldenError> {
    if (expected.width, expected.height) != (actual.width, actual.height) {
        return Err(GoldenError::Size(format!(
            "expected {}x{}, got {}x{}",
            expected.width, expected.height, actual.width, actual.height
        )));
    }
    let map = flip_map(expected, actual);
    let mut pool = nv_flip::FlipPool::from_image(&map);
    Ok(FlipScore {
        mean: pool.mean(),
        p99: pool.get_percentile(0.99, false),
    })
}

fn flip_map(expected: &RgbaImage, actual: &RgbaImage) -> nv_flip::FlipImageFloat {
    let reference =
        nv_flip::FlipImageRgb8::with_data(expected.width, expected.height, &expected.rgb());
    let test = nv_flip::FlipImageRgb8::with_data(actual.width, actual.height, &actual.rgb());
    nv_flip::flip(reference, test, nv_flip::DEFAULT_PIXELS_PER_DEGREE)
}

/// A golden store: where goldens and failure artifacts live, and whether to bless.
#[derive(Debug, Clone)]
pub struct Golden {
    root: PathBuf,
    failures: PathBuf,
    bless: bool,
    ci: bool,
}

impl Golden {
    /// A store at explicit paths (tests).
    pub fn new(root: PathBuf, failures: PathBuf, bless: bool, ci: bool) -> Self {
        Self {
            root,
            failures,
            bless,
            ci,
        }
    }

    /// The repository store: `tests/golden`, `target/golden-failures`, `CONCORDIA_BLESS=1`, `CI=true`.
    pub fn from_env() -> Self {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        Self::new(
            repo.join("tests/golden"),
            repo.join("target/golden-failures"),
            std::env::var("CONCORDIA_BLESS").as_deref() == Ok("1"),
            std::env::var("CI").as_deref() == Ok("true"),
        )
    }

    /// The tolerance for golden `name` in `suite`: its entry in the suite's `tolerances.ron`, else
    /// the file's default, else [`Tolerance::default`] when the suite has no file.
    pub fn tolerance(&self, suite: &str, name: &str) -> Result<Tolerance, GoldenError> {
        let path = self.path(suite, name)?.with_file_name("tolerances.ron");
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Tolerance::default()),
            Err(e) => return Err(tolerances_error(&path, e)),
        };
        let file: Tolerances =
            concordia_scene::read_document(&text).map_err(|e| tolerances_error(&path, e))?;
        Ok(file.goldens.get(name).copied().unwrap_or(file.default))
    }

    /// Compares `image` with `<suite>/<name>.png` within its tolerance (or writes it when blessing).
    pub fn check(
        &self,
        suite: &str,
        name: &str,
        image: &RgbaImage,
    ) -> Result<FlipScore, GoldenError> {
        let path = self.path(suite, name)?;
        let tolerance = self.tolerance(suite, name)?;
        if self.bless {
            if self.ci {
                return Err(GoldenError::BlessInCi);
            }
            image.save(&path)?;
            return Ok(FlipScore {
                mean: 0.0,
                p99: 0.0,
            });
        }
        if !path.exists() {
            return Err(GoldenError::Missing { path });
        }
        let expected = RgbaImage::load(&path)?;
        let score = flip_score(&expected, image)?;
        if tolerance.accepts(&score) {
            return Ok(score);
        }
        let failures = self.write_comparison(suite, name, &expected, image)?;
        Err(GoldenError::Mismatch {
            path,
            score,
            tolerance,
            failures,
        })
    }

    /// True when running in CI (blessing is refused there).
    pub fn is_ci(&self) -> bool {
        self.ci
    }

    pub(crate) fn path(&self, suite: &str, name: &str) -> Result<PathBuf, GoldenError> {
        for part in [suite, name] {
            let ok = !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '@'));
            if !ok {
                return Err(GoldenError::Name(format!(
                    "{part:?} must be [A-Za-z0-9_@-]+"
                )));
            }
        }
        Ok(self.root.join(suite).join(format!("{name}.png")))
    }

    /// Writes `<name>.actual.png`, `.expected.png` and the FLIP heat map `.flip.png` under the
    /// failures directory's `suite`; returns that directory.
    pub fn write_comparison(
        &self,
        suite: &str,
        name: &str,
        expected: &RgbaImage,
        actual: &RgbaImage,
    ) -> Result<String, GoldenError> {
        let dir = self.failures.join(suite);
        actual.save(&dir.join(format!("{name}.actual.png")))?;
        expected.save(&dir.join(format!("{name}.expected.png")))?;
        let heat = flip_map(expected, actual).apply_color_lut(&nv_flip::magma_lut());
        let rgba: Vec<u8> = heat
            .to_vec()
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect();
        RgbaImage::new(expected.width, expected.height, rgba)?
            .save(&dir.join(format!("{name}.flip.png")))?;
        Ok(dir.display().to_string())
    }
}

fn tolerances_error(path: &Path, e: impl std::fmt::Display) -> GoldenError {
    GoldenError::Tolerances {
        path: path.to_path_buf(),
        message: e.to_string(),
    }
}

/// Asserts `image` matches its golden under the repository store; panics with the reason.
pub fn assert_golden(suite: &str, name: &str, image: &RgbaImage) {
    if let Err(e) = Golden::from_env().check(suite, name, image) {
        panic!("golden {suite}/{name}: {e}");
    }
}

/// The RHI conformance suite's golden check: `name` in suite `rhi` (`tests/golden/rhi/`).
#[cfg(feature = "rhi")]
#[derive(Debug, Clone, Copy, Default)]
pub struct RhiGoldens;

#[cfg(feature = "rhi")]
impl concordia_rhi::conformance::GoldenCheck for RhiGoldens {
    fn check(&self, name: &str, width: u32, height: u32, rgba: &[u8]) {
        let image = RgbaImage::new(width, height, rgba.to_vec()).expect("well-formed capture");
        assert_golden("rhi", name, &image);
    }
}
