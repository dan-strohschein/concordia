//! The RHI conformance suite on Metal (M1 phase 1.2b). Skips without a device unless CONCORDIA_REQUIRE_GPU=1.

use std::collections::HashMap;
use std::sync::Mutex;

use concordia_rhi::conformance::{GoldenCheck, ShaderProvider, ShaderStage, shader_dir};
use concordia_rhi::{Device, RhiError, ShaderSource};
use concordia_shaders::{
    CompileRequest, ShaderCompiler, ShaderModule, SlangToolchain, SlangcCompiler, Stage, Target,
};
use concordia_testkit::golden::{RgbaImage, Tolerance, assert_golden};
use concordia_testkit::gpu_required;

fn device() -> Option<Box<dyn Device>> {
    match concordia_rhi_metal::new_headless() {
        Ok(device) => Some(device),
        Err(RhiError::NoDevice { reason }) => {
            assert!(
                !gpu_required(),
                "CONCORDIA_REQUIRE_GPU=1 but no device: {reason}"
            );
            None
        }
        Err(other) => panic!("unexpected device error: {other}"),
    }
}

struct Goldens;

impl GoldenCheck for Goldens {
    fn check(&self, name: &str, width: u32, height: u32, rgba: &[u8], mean_flip: f32) {
        let image = RgbaImage::new(width, height, rgba.to_vec()).expect("well-formed capture");
        assert_golden("rhi", name, &image, Tolerance::mean(mean_flip));
    }
}

/// Compiles conformance shaders from Slang to MSL, once per (shader, entry).
#[derive(Default)]
struct Slang {
    compiled: Mutex<HashMap<(String, String), ShaderModule>>,
}

impl ShaderProvider for Slang {
    fn shader(
        &self,
        name: &str,
        entry: &str,
        stage: ShaderStage,
    ) -> (ShaderSource, Option<[u32; 3]>) {
        let key = (name.to_owned(), entry.to_owned());
        let mut compiled = self.compiled.lock().expect("not poisoned");
        let module = compiled.entry(key).or_insert_with(|| {
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
                target: Target::Msl,
                defines: Default::default(),
                include_dirs: vec![concordia_shaders::builtin_dir()],
            };
            compiler
                .compile(&request)
                .unwrap_or_else(|e| panic!("{name}::{entry}: {e}"))
        });
        (module.source(), module.reflection.threads_per_group)
    }
}

concordia_rhi::conformance_tests!(device, Goldens, Slang::default());

#[cfg(target_os = "macos")]
#[test]
fn device_lost_is_reported() {
    let Ok(device) = concordia_rhi_metal::MetalDevice::system_default() else {
        assert!(!gpu_required());
        return;
    };
    device.simulate_device_lost();
    let list = concordia_rhi::CommandList::new("after loss");
    let submit = concordia_rhi::Submit {
        queue: concordia_rhi::QueueKind::Graphics,
        commands: &list,
        wait: &[],
        signal: &[],
    };
    assert!(matches!(
        device.submit(submit),
        Err(RhiError::DeviceLost(_))
    ));
}
