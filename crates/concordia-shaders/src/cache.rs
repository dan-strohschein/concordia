//! A content-addressed cache of compiled modules in the DDC layout `<root>/<2 hex>/<64 hex>`.

use std::path::{Path, PathBuf};

use crate::ShaderCompiler;
use crate::deps::dependencies;
use crate::error::{ShaderError, io};
use crate::request::{CompileRequest, ShaderModule};

/// Stores compiled modules by key (the asset DDC implements this from phase 1.4).
pub trait ShaderCache: Send + Sync {
    /// Bytes stored under `key`, if any.
    fn get(&self, key: &str) -> Option<Vec<u8>>;
    /// Stores `bytes` under `key` atomically.
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), ShaderError>;
}

/// A cache directory (`.concordia/ddc` by convention).
#[derive(Debug, Clone)]
pub struct DirCache {
    root: PathBuf,
}

impl DirCache {
    /// A cache rooted at `root` (created on first write).
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }

    fn path(&self, key: &str) -> Option<PathBuf> {
        let valid = key.len() == 64 && key.bytes().all(|b| b.is_ascii_hexdigit());
        valid.then(|| self.root.join(&key[..2]).join(key))
    }
}

impl ShaderCache for DirCache {
    fn get(&self, key: &str) -> Option<Vec<u8>> {
        std::fs::read(self.path(key)?).ok()
    }

    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), ShaderError> {
        let path = self
            .path(key)
            .ok_or_else(|| ShaderError::Io(format!("cache key {key:?} is not 64 hex digits")))?;
        let dir = path.parent().unwrap_or(&self.root);
        std::fs::create_dir_all(dir).map_err(|e| io(dir, e))?;
        let tmp = dir.join(format!("{key}.tmp-{}", std::process::id()));
        std::fs::write(&tmp, bytes).map_err(|e| io(&tmp, e))?;
        std::fs::rename(&tmp, &path).map_err(|e| io(&path, e))
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
