//! The provider interface: a request and a seed in, file bytes out.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::GenError;

/// What a provider makes.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, concordia_reflect::Reflect)]
pub enum GenKind {
    /// Meshes (.glb).
    mesh,
    /// Textures (.png).
    texture,
    /// Sound effects (.wav).
    sfx,
    /// Music (.wav).
    music,
    /// Anything a Blender script builds (.glb).
    blender,
}

/// Where a provider works.
#[derive(Debug, Clone, Copy)]
pub struct GenContext<'a> {
    /// The project root (scripts are read from it; scratch space is under `.concordia/gen`).
    pub project: &'a std::path::Path,
}

/// One generated file and what to report about it.
#[derive(Debug, Clone, PartialEq)]
pub struct Generated {
    /// The file's bytes.
    pub bytes: Vec<u8>,
    /// Facts for the caller, e.g. triangle counts and bounds.
    pub info: Value,
    /// Import settings to merge into the sidecar (e.g. a sound's loop points).
    pub settings: Option<Value>,
    /// Files the output depends on beyond the request (project path, BLAKE3), e.g. a script.
    pub inputs: Vec<(String, String)>,
}

/// Makes content from a request; the same request and seed give the same bytes.
pub trait ContentProvider: Send + Sync {
    /// Its id, e.g. `mesh`.
    fn id(&self) -> &str;
    /// What it makes.
    fn kind(&self) -> GenKind;
    /// Its version: a change that may change output bumps it.
    fn version(&self) -> u32;
    /// The file extensions it writes (without dots).
    fn extensions(&self) -> Vec<&str>;
    /// The JSON Schema of its request.
    fn request_schema(&self) -> Value;
    /// Whether it runs in-process from code (not a plugin or an external tool).
    fn procedural(&self) -> bool {
        true
    }
    /// Generates the file.
    fn generate(
        &self,
        request: &Value,
        seed: u64,
        ctx: &GenContext<'_>,
    ) -> Result<Generated, GenError>;
}

/// BLAKE3 of a provider, its version, the request (canonical JSON), the seed and any input
/// files (path and hash).
pub fn inputs_hash(
    provider: &str,
    version: u32,
    request: &Value,
    seed: u64,
    inputs: &[(String, String)],
) -> String {
    let mut h = blake3::Hasher::new();
    let request = request.to_string();
    let fixed: [&[u8]; 4] = [
        provider.as_bytes(),
        &version.to_le_bytes(),
        request.as_bytes(),
        &seed.to_le_bytes(),
    ];
    let files = inputs
        .iter()
        .flat_map(|(p, hash)| [p.as_bytes(), hash.as_bytes()]);
    for part in fixed.into_iter().chain(files) {
        h.update(&(part.len() as u64).to_le_bytes());
        h.update(part);
    }
    h.finalize().to_hex().to_string()
}
