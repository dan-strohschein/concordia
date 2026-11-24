//! Path-traced references (ADR-0090): `tests/golden/m4_reference/<scene>.png` and its
//! `<scene>.reference.ron` (the inputs to render it again, and what was measured when blessed).

use concordia_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::golden::{FlipScore, Golden, GoldenError, RgbaImage};

/// The golden suite references live in.
pub const SUITE: &str = "m4_reference";

/// A free viewpoint for a reference (perspective).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct ReferenceLook {
    /// Camera position.
    pub eye: [f32; 3],
    /// Point looked at.
    pub target: [f32; 3],
    /// Vertical field of view in degrees.
    #[reflect(range(min = 1.0, max = 179.0))]
    pub fov_y_degrees: f32,
}

/// What was measured when a reference was blessed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Blessed {
    /// Where it was traced: gpu or cpu.
    pub backend: String,
    /// The commit it was traced at.
    pub git: String,
    /// The renderer's FLIP against the reference (the baseline features must beat).
    pub baseline: FlipScore,
    /// FLIP between two seeds of the reference (its grain).
    pub noise: FlipScore,
}

/// `<scene>.reference.ron`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct ReferenceFile {
    /// Project directory, relative to the repository.
    pub project: String,
    /// Scene file in the project (default: its main scene).
    #[serde(default)]
    pub scene: Option<String>,
    /// Viewpoint (default: the scene's highest-priority camera).
    #[serde(default)]
    pub view: Option<ReferenceLook>,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Samples per pixel.
    pub spp: u32,
    /// Seed.
    pub seed: u32,
    /// Bounces.
    pub max_bounces: u32,
    /// Set by blessing.
    #[serde(default)]
    pub blessed: Option<Blessed>,
}

impl ReferenceFile {
    /// Parses and validates a reference file.
    pub fn from_text(text: &str) -> Result<Self, GoldenError> {
        let file: Self = concordia_scene::read_document(text).map_err(invalid)?;
        file.validate()?;
        Ok(file)
    }

    /// The canonical text (refused for an invalid file).
    pub fn to_text(&self) -> Result<String, GoldenError> {
        self.validate()?;
        concordia_scene::write_document(self).map_err(invalid)
    }

    /// A project, a nonempty image and at least one sample and bounce.
    pub fn validate(&self) -> Result<(), GoldenError> {
        let empty = [
            ("project", self.project.is_empty()),
            ("width", self.width == 0),
            ("height", self.height == 0),
            ("spp", self.spp == 0),
            ("max_bounces", self.max_bounces == 0),
        ];
        match empty.iter().find(|(_, bad)| *bad) {
            Some((what, _)) => Err(GoldenError::Reference(format!(
                "{what} must be set (nonzero)"
            ))),
            None => Ok(()),
        }
    }
}

fn invalid(e: impl std::fmt::Display) -> GoldenError {
    GoldenError::Reference(e.to_string())
}

impl Golden {
    /// The reference file `<name>.reference.ron`.
    pub fn reference(&self, name: &str) -> Result<ReferenceFile, GoldenError> {
        let path = self.reference_path(name)?;
        let text = std::fs::read_to_string(&path)
            .map_err(|e| GoldenError::Io(format!("{}: {e}", path.display())))?;
        ReferenceFile::from_text(&text)
            .map_err(|e| GoldenError::Reference(format!("{}: {e}", path.display())))
    }

    /// Writes a blessed reference's image and file; refused in CI, without a `blessed` record, and
    /// when its noise exceeds a fifth of its tolerance's mean (it would measure the grain).
    pub fn bless_reference(
        &self,
        name: &str,
        file: &ReferenceFile,
        image: &RgbaImage,
    ) -> Result<(), GoldenError> {
        if self.is_ci() {
            return Err(GoldenError::BlessInCi);
        }
        let Some(blessed) = &file.blessed else {
            return Err(GoldenError::Reference(format!(
                "{name}: only a blessed reference is written"
            )));
        };
        let tolerance = self.tolerance(SUITE, name)?;
        if blessed.noise.mean > tolerance.mean / 5.0 {
            return Err(GoldenError::NoisyReference {
                name: name.to_owned(),
                noise: blessed.noise,
                tolerance,
            });
        }
        let text = file.to_text()?;
        let path = self.reference_path(name)?;
        image.save(&path.with_file_name(format!("{name}.png")))?;
        std::fs::write(&path, text).map_err(|e| GoldenError::Io(format!("{}: {e}", path.display())))
    }

    fn reference_path(&self, name: &str) -> Result<std::path::PathBuf, GoldenError> {
        Ok(self
            .path(SUITE, name)?
            .with_file_name(format!("{name}.reference.ron")))
    }
}
