//! Golden images: `tests/golden/<suite>/<name>.png`, compared with LDR-FLIP; blessed with
//! `CONCORDIA_BLESS=1` (`just bless`), never when `CI=true`. Failures leave
//! `<name>.{actual,expected,flip}.png` under `target/golden-failures/<suite>/`.

use std::path::{Path, PathBuf};

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

    fn save(&self, path: &Path) -> Result<(), GoldenError> {
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
}

/// FLIP statistics of a comparison (0 = identical, 1 = maximally different).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlipScore {
    /// Mean error.
    pub mean: f32,
    /// 99th-percentile error.
    pub p99: f32,
}

/// Accepted FLIP error.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tolerance {
    /// Largest accepted mean.
    pub mean: f32,
    /// Largest accepted 99th percentile.
    pub p99: f32,
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
    /// A tolerance with this mean and the default p99.
    pub fn mean(mean: f32) -> Self {
        Self {
            mean,
            ..Self::default()
        }
    }

    /// True if `score` is within tolerance.
    pub fn accepts(&self, score: &FlipScore) -> bool {
        score.mean <= self.mean && score.p99 <= self.p99
    }
}

/// Why a golden check failed.
#[derive(Debug, thiserror::Error)]
pub enum GoldenError {
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

    /// Compares `image` with `<suite>/<name>.png` (or writes it when blessing).
    pub fn check(
        &self,
        suite: &str,
        name: &str,
        image: &RgbaImage,
        tolerance: Tolerance,
    ) -> Result<FlipScore, GoldenError> {
        let path = self.path(suite, name)?;
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
        let failures = self.write_failures(suite, name, &expected, image)?;
        Err(GoldenError::Mismatch {
            path,
            score,
            tolerance,
            failures,
        })
    }

    fn path(&self, suite: &str, name: &str) -> Result<PathBuf, GoldenError> {
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

    fn write_failures(
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

/// Asserts `image` matches its golden under the repository store; panics with the reason.
pub fn assert_golden(suite: &str, name: &str, image: &RgbaImage, tolerance: Tolerance) {
    if let Err(e) = Golden::from_env().check(suite, name, image, tolerance) {
        panic!("golden {suite}/{name}: {e}");
    }
}
