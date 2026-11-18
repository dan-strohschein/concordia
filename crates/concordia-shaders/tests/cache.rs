//! Content-addressed shader cache (M1 phase 1.3).

mod common;

use std::sync::atomic::{AtomicUsize, Ordering};

use common::{request, toolchain};
use concordia_shaders::{
    CachedCompiler, CompileRequest, DirCache, ShaderCompiler, ShaderError, ShaderModule,
    SlangcCompiler, Stage, Target, cache_key,
};

/// Counts calls to the real compiler.
struct Counting {
    inner: SlangcCompiler,
    calls: AtomicUsize,
}

impl ShaderCompiler for Counting {
    fn compile(&self, request: &CompileRequest) -> Result<ShaderModule, ShaderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.inner.compile(request)
    }

    fn version(&self) -> &str {
        self.inner.version()
    }
}

fn copy_fixtures(dir: &tempfile::TempDir) -> CompileRequest {
    for f in ["fill.slang", "common/tint.slang"] {
        let to = dir.path().join(f);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::copy(common::fixtures().join(f), to).unwrap();
    }
    let mut req = request("fill.slang", "fill", Stage::Compute, Target::Msl);
    req.path = dir.path().join("fill.slang");
    req.include_dirs = vec![concordia_shaders::builtin_dir(), dir.path().to_path_buf()];
    req
}

#[test]
fn cache_key_changes_when_included_file_changes() {
    let dir = tempfile::tempdir().unwrap();
    let req = copy_fixtures(&dir);
    let before = cache_key(&req, "2026.22").unwrap();
    assert_eq!(cache_key(&req, "2026.22").unwrap(), before, "stable");
    std::fs::write(
        dir.path().join("common/tint.slang"),
        "module tint;\npublic uint tint_offset() { return 1; }\n",
    )
    .unwrap();
    let after = cache_key(&req, "2026.22").unwrap();
    assert_ne!(
        after, before,
        "an imported module's contents are part of the key"
    );
    let mut other_define = req.clone();
    other_define.defines.insert("FAST".into(), "1".into());
    assert_ne!(cache_key(&other_define, "2026.22").unwrap(), after);
    assert_ne!(
        cache_key(&req, "2026.23").unwrap(),
        after,
        "the toolchain version is part of the key"
    );
}

#[test]
fn cache_hit_skips_compiler() {
    let dir = tempfile::tempdir().unwrap();
    let req = copy_fixtures(&dir);
    let cache_dir = dir.path().join(".concordia/ddc");
    let counting = Counting {
        inner: SlangcCompiler::new(toolchain()),
        calls: AtomicUsize::new(0),
    };
    let cached = CachedCompiler::new(counting, DirCache::new(&cache_dir));
    let first = cached.compile(&req).unwrap();
    let second = cached.compile(&req).unwrap();
    assert_eq!(first, second);
    assert_eq!(cached.inner().calls.load(Ordering::SeqCst), 1);
    let key = cache_key(&req, cached.version()).unwrap();
    assert!(
        cache_dir.join(&key[..2]).join(&key).exists(),
        "DDC layout <2 hex>/<64 hex>"
    );
}
