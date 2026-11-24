//! The backends a test runs on: the CPU always, and the GPU when this machine has one (required
//! with CONCORDIA_REQUIRE_GPU=1).
#![allow(dead_code)]

use std::sync::Arc;

use concordia_pathtrace::{Backend, GpuTracer};
use concordia_shaders::{SlangToolchain, SlangcCompiler};

pub fn backends() -> Vec<(&'static str, Backend)> {
    let mut all = vec![("cpu", Backend::Cpu)];
    match concordia_rhi_metal::new_headless() {
        Ok(device) => {
            let compiler =
                SlangcCompiler::new(SlangToolchain::locate().expect("run `just slang-fetch`"));
            let tracer =
                GpuTracer::new(Arc::from(device), &compiler).expect("the GPU tracer builds");
            all.push(("gpu", Backend::Gpu(Arc::new(tracer))));
        }
        Err(e) => assert!(
            !concordia_testkit::gpu_required(),
            "CONCORDIA_REQUIRE_GPU=1 but {e}"
        ),
    }
    all
}
