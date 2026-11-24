//! The RHI conformance suite's [`ShaderProvider`]: compiles `concordia_rhi::conformance::shader_dir()`
//! with the pinned Slang toolchain for one target, once per (shader, entry).

use std::collections::HashMap;
use std::sync::Mutex;

use concordia_rhi::ShaderSource;
use concordia_rhi::conformance::{ShaderProvider, ShaderStage, shader_dir};

use crate::{
    CompileRequest, ShaderCompiler, ShaderModule, SlangToolchain, SlangcCompiler, Stage, Target,
};

/// Conformance shaders for one backend's target (tests only: panics when compilation fails).
#[derive(Debug)]
pub struct SlangShaders {
    target: Target,
    compiled: Mutex<HashMap<(String, String), ShaderModule>>,
}

impl SlangShaders {
    /// A provider compiling for `target`.
    pub fn new(target: Target) -> Self {
        Self {
            target,
            compiled: Mutex::default(),
        }
    }

    /// How many (shader, entry) pairs have been compiled.
    pub fn compiled_count(&self) -> usize {
        self.compiled.lock().map_or(0, |c| c.len())
    }

    fn compile(&self, name: &str, entry: &str, stage: ShaderStage) -> ShaderModule {
        let compiler =
            SlangcCompiler::new(SlangToolchain::locate().expect("run `just slang-fetch`"));
        let request = CompileRequest {
            path: shader_dir().join(format!("{name}.slang")),
            entry: entry.to_owned(),
            stage: match stage {
                ShaderStage::Compute => Stage::Compute,
                ShaderStage::Vertex => Stage::Vertex,
                ShaderStage::Fragment => Stage::Fragment,
            },
            target: self.target,
            defines: Default::default(),
            include_dirs: vec![crate::builtin_dir()],
        };
        compiler
            .compile(&request)
            .unwrap_or_else(|e| panic!("{name}::{entry}: {e}"))
    }
}

impl ShaderProvider for SlangShaders {
    fn shader(
        &self,
        name: &str,
        entry: &str,
        stage: ShaderStage,
    ) -> (ShaderSource, Option<[u32; 3]>) {
        let key = (name.to_owned(), entry.to_owned());
        let mut compiled = self.compiled.lock().expect("not poisoned");
        let module = compiled
            .entry(key)
            .or_insert_with(|| self.compile(name, entry, stage));
        (module.source(), module.reflection.threads_per_group)
    }
}
