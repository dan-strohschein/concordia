//! Helpers for the crate's GPU unit tests: the test device, compute pipelines, runs and downloads.

use std::num::NonZeroU64;
use std::sync::Arc;

use concordia_rhi::{
    Buffer, BufferDesc, BufferUsage, Command, CommandList, ComputePipelineDesc, Device,
    MemoryLocation, Pipeline, QueueKind, Submit,
};

/// The test device, or `None` where none is required and none opens.
pub(crate) fn device() -> Option<Arc<dyn Device>> {
    let backend = concordia_rhi_select::choose_from_env(Default::default()).ok()?;
    match concordia_rhi_select::new_headless(backend) {
        Ok(d) => Some(d.into()),
        Err(e) => {
            assert!(!concordia_testkit::device_required(backend), "{e}");
            None
        }
    }
}

/// A compute pipeline for `entry` of render shader `file`.
pub(crate) fn compute(device: &dyn Device, file: &str, entry: &str) -> Pipeline {
    let toolchain = concordia_shaders::SlangToolchain::locate().expect("run `just slang-fetch`");
    let compiler = concordia_shaders::SlangcCompiler::new(toolchain);
    let module = crate::pipelines::compile(
        device,
        &compiler,
        file,
        entry,
        concordia_shaders::Stage::Compute,
    )
    .unwrap();
    device
        .create_compute_pipeline(&ComputePipelineDesc {
            shader: module.source(),
            threads_per_group: module.reflection.threads_per_group.unwrap(),
            label: entry.into(),
        })
        .unwrap()
}

/// Submits `list` and waits for it.
pub(crate) fn run(device: &dyn Device, list: &CommandList) {
    device
        .submit(Submit {
            queue: QueueKind::Graphics,
            commands: list,
            wait: &[],
            signal: &[],
        })
        .unwrap();
    device.wait_idle().unwrap();
}

/// `commands` as one submitted list.
pub(crate) fn run_commands(device: &dyn Device, commands: impl IntoIterator<Item = Command>) {
    let mut list = CommandList::new("test");
    commands.into_iter().for_each(|c| {
        list.push(c);
    });
    run(device, &list);
}

/// The first `bytes` of `buffer`, as words.
pub(crate) fn download(device: &dyn Device, buffer: Buffer, bytes: u64) -> Vec<u32> {
    let readback = device
        .create_buffer(&BufferDesc {
            size: NonZeroU64::new(bytes).unwrap(),
            usage: BufferUsage::COPY_DST,
            memory: MemoryLocation::Readback,
            label: "readback".into(),
        })
        .unwrap();
    run_commands(
        device,
        [Command::CopyBuffer {
            src: buffer,
            src_offset: 0,
            dst: readback,
            dst_offset: 0,
            size: bytes,
        }],
    );
    let raw = device.read_buffer(readback, 0, bytes).unwrap();
    raw.as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes(*c))
        .collect()
}
