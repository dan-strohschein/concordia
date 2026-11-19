//! Compiled modules cached by content in the DDC (`concordia_core::ddc::Ddc`).

use concordia_core::ddc::{Ddc, DdcKey};

use crate::ShaderCompiler;
use crate::deps::dependencies;
use crate::error::{ShaderError, io};
use crate::request::{CompileRequest, ShaderModule};

/// Stores compiled modules by key (implemented by the project DDC).
pub trait ShaderCache: Send + Sync {
    /// Bytes stored under `key`, if any.
    fn get(&self, key: &str) -> Option<Vec<u8>>;
    /// Stores `bytes` under `key` atomically.
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), ShaderError>;
}

impl ShaderCache for Ddc {
    fn get(&self, key: &str) -> Option<Vec<u8>> {
        Ddc::get(self, &key.parse::<DdcKey>().ok()?).ok().flatten()
    }

    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), ShaderError> {
        let key: DdcKey = key
            .parse()
            .map_err(|e: concordia_core::ddc::DdcError| ShaderError::Io(e.to_string()))?;
        Ddc::put(self, &key, bytes).map_err(|e| ShaderError::Io(e.to_string()))
    }
}

/// The cache key: blake3 of the compiler version, target, stage, entry, sorted defines, and the
/// contents of the root file and every file it depends on.
pub fn cache_key(request: &CompileRequest, version: &str) -> Result<String, ShaderError> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"concordia-shader-v1\0");
    for part in [
        version,
        request.target.slang(),
        request.stage.slang(),
        &request.entry,
    ] {
        hasher.update(part.as_bytes());
        hasher.update(b"\0");
    }
    for (name, value) in &request.defines {
        hasher.update(format!("{name}={value}\0").as_bytes());
    }
    let mut contents: Vec<[u8; 32]> = dependencies(&request.path, &request.include_dirs)?
        .iter()
        .map(|p| {
            std::fs::read(p)
                .map(|b| *blake3::hash(&b).as_bytes())
                .map_err(|e| io(p, e))
        })
        .collect::<Result<_, _>>()?;
    contents.sort_unstable();
    contents.iter().for_each(|h| {
        hasher.update(h);
    });
    Ok(hasher.finalize().to_hex().to_string())
}

/// A compiler that consults a cache first.
#[derive(Debug)]
pub struct CachedCompiler<C, K> {
    inner: C,
    cache: K,
}

impl<C: ShaderCompiler, K: ShaderCache> CachedCompiler<C, K> {
    /// Wraps `inner` with `cache`.
    pub fn new(inner: C, cache: K) -> Self {
        Self { inner, cache }
    }

    /// The wrapped compiler.
    pub fn inner(&self) -> &C {
        &self.inner
    }
}

impl<C: ShaderCompiler, K: ShaderCache> ShaderCompiler for CachedCompiler<C, K> {
    fn compile(&self, request: &CompileRequest) -> Result<ShaderModule, ShaderError> {
        let key = cache_key(request, self.inner.version())?;
        if let Some(module) = self
            .cache
            .get(&key)
            .and_then(|b| serde_json::from_slice(&b).ok())
        {
            return Ok(module);
        }
        let module = self.inner.compile(request)?;
        let bytes = serde_json::to_vec(&module).map_err(|e| ShaderError::Io(e.to_string()))?;
        if let Err(e) = self.cache.put(&key, &bytes) {
            tracing::warn!(target: "concordia::shaders", error = %e, "shader cache write failed");
        }
        Ok(module)
    }

    fn version(&self) -> &str {
        self.inner.version()
    }
}
