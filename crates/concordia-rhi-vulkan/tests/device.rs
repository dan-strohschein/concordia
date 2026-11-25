//! Vulkan-only device behaviour (M4 4.1v(a)). Device cases skip without Vulkan unless
//! CONCORDIA_REQUIRE_VULKAN=1; they run under validation and fail on any validation error.

use std::num::NonZeroU64;

use concordia_rhi::{
    BufferDesc, BufferUsage, CapsMask, Command, CommandList, ComputePipelineDesc, Device,
    MemoryLocation, MeshPipelineDesc, QueueKind, RhiError, ShaderSource, Submit, TimerScope,
};
use concordia_rhi_vulkan::{Options, Validation, ValidationLog, VulkanDevice, new_headless};
use concordia_testkit::vulkan_required;

fn open(mask: CapsMask) -> Option<(VulkanDevice, ValidationLog)> {
    let required = vulkan_required();
    let log = ValidationLog::new();
    let options = Options {
        validation: if required {
            Validation::Required
        } else {
            Validation::IfAvailable
        },
        mask,
        log: Some(log.clone()),
    };
    match VulkanDevice::new(&options) {
        Ok(device) => Some((device, log)),
        Err(RhiError::NoDevice { reason }) => {
            assert!(
                !required,
                "CONCORDIA_REQUIRE_VULKAN=1 but no device: {reason}"
            );
            None
        }
        Err(other) => panic!("unexpected device error: {other}"),
    }
}

fn assert_clean(log: &ValidationLog) {
    let errors = log.take();
    assert!(
        errors.is_empty(),
        "validation errors:\n{}",
        errors.join("\n")
    );
}

fn buffer(
    device: &dyn Device,
    size: u64,
    usage: BufferUsage,
    memory: MemoryLocation,
) -> concordia_rhi::Buffer {
    let desc = BufferDesc {
        size: NonZeroU64::new(size).unwrap(),
        usage,
        memory,
        label: "test".into(),
    };
    device.create_buffer(&desc).unwrap()
}

fn run(device: &dyn Device, list: &CommandList) -> Result<(), RhiError> {
    device.submit(Submit {
        queue: QueueKind::Graphics,
        commands: list,
        wait: &[],
        signal: &[],
    })
}

/// Runs everywhere: on a machine without a Vulkan loader (the author's Mac) this is the skip path.
#[test]
fn without_a_loader_or_device_the_backend_is_no_device() {
    match new_headless() {
        Ok(device) => assert_eq!(device.caps().backend, concordia_rhi::Backend::Vulkan),
        Err(RhiError::NoDevice { reason }) => {
            assert!(
                !vulkan_required(),
                "CONCORDIA_REQUIRE_VULKAN=1 but no device: {reason}"
            );
            assert!(!reason.is_empty());
        }
        Err(other) => panic!("a missing Vulkan must be NoDevice, got {other}"),
    }
}

#[test]
fn the_validation_layer_is_on_when_required() {
    let Some((device, _)) = open(CapsMask::default()) else {
        return;
    };
    if vulkan_required() {
        assert!(device.validation_enabled());
    }
}

#[test]
fn validation_errors_reach_the_log() {
    let Some((device, log)) = open(CapsMask::default()) else {
        return;
    };
    if !device.validation_enabled() {
        return;
    }
    device.report_validation_error_for_tests("deliberate test error");
    let errors = log.take();
    assert!(
        errors.iter().any(|e| e.contains("deliberate test error")),
        "{errors:?}"
    );
}

#[test]
fn destroyed_resources_outlive_their_last_submission() {
    let Some((device, log)) = open(CapsMask::default()) else {
        return;
    };
    let src = buffer(&device, 256, BufferUsage::COPY_SRC, MemoryLocation::Upload);
    let dst = buffer(
        &device,
        256,
        BufferUsage::COPY_DST,
        MemoryLocation::Readback,
    );
    device.write_buffer(src, 0, &[7; 256]).unwrap();
    let mut list = CommandList::new("copy then destroy");
    list.push(Command::CopyBuffer {
        src,
        src_offset: 0,
        dst,
        dst_offset: 0,
        size: 256,
    });
    run(&device, &list).unwrap();
    device.destroy_buffer(src);
    assert!(matches!(
        device.read_buffer(src, 0, 4),
        Err(RhiError::StaleHandle { .. })
    ));
    device.wait_idle().unwrap();
    assert_eq!(device.read_buffer(dst, 0, 256).unwrap(), vec![7; 256]);
    assert_clean(&log);
}

#[test]
fn masked_timestamps_report_no_scopes() {
    let mask = CapsMask {
        timestamps: true,
        ..CapsMask::default()
    };
    let Some((device, log)) = open(mask) else {
        return;
    };
    assert!(!device.caps().timestamp_granularity.times_passes());
    let a = buffer(&device, 64, BufferUsage::COPY_SRC, MemoryLocation::Upload);
    let b = buffer(&device, 64, BufferUsage::COPY_DST, MemoryLocation::Readback);
    let mut list = CommandList::new("masked");
    list.push(Command::BeginTimer(TimerScope(1)))
        .push(Command::CopyBuffer {
            src: a,
            src_offset: 0,
            dst: b,
            dst_offset: 0,
            size: 64,
        })
        .push(Command::EndTimer);
    let (tx, rx) = std::sync::mpsc::channel();
    device
        .submit_timed(
            Submit {
                queue: QueueKind::Graphics,
                commands: &list,
                wait: &[],
                signal: &[],
            },
            Box::new(move |t| tx.send(t).unwrap()),
        )
        .unwrap();
    device.wait_idle().unwrap();
    let timing = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
    assert!(timing.scopes.is_empty(), "{timing:?}");
    assert_clean(&log);
}

#[test]
fn timeline_signals_must_grow() {
    let Some((device, log)) = open(CapsMask::default()) else {
        return;
    };
    let timeline = device.create_timeline(0).unwrap();
    let empty = CommandList::new("tick");
    let submit = |value| {
        device.submit(Submit {
            queue: QueueKind::Graphics,
            commands: &empty,
            wait: &[],
            signal: &[(timeline, value)],
        })
    };
    submit(5).unwrap();
    assert!(matches!(submit(3), Err(RhiError::Invalid { .. })));
    assert!(
        device
            .wait_timeline(timeline, 5, std::time::Duration::from_secs(5))
            .unwrap()
    );
    assert_clean(&log);
}

#[test]
fn wrong_shader_language_mesh_pipelines_and_surfaces_are_errors() {
    let Some((device, log)) = open(CapsMask::default()) else {
        return;
    };
    let msl = ShaderSource::Msl {
        source: "kernel void k() {}".into(),
        entry: "k".into(),
    };
    let desc = ComputePipelineDesc {
        shader: msl.clone(),
        threads_per_group: [1, 1, 1],
        label: "msl".into(),
    };
    assert!(matches!(
        device.create_compute_pipeline(&desc),
        Err(RhiError::Invalid { .. })
    ));
    let mesh = MeshPipelineDesc {
        object: None,
        mesh: msl.clone(),
        fragment: msl,
        color_formats: vec![],
        depth: None,
        label: "mesh".into(),
    };
    assert!(matches!(
        device.create_mesh_pipeline(&mesh),
        Err(RhiError::Unsupported(_))
    ));
    assert!(!device.caps().mesh_shaders);
    assert_clean(&log);
}

#[test]
fn out_of_range_copies_and_cpu_access_are_errors() {
    let Some((device, log)) = open(CapsMask::default()) else {
        return;
    };
    let a = buffer(&device, 64, BufferUsage::COPY_SRC, MemoryLocation::Upload);
    let gpu = buffer(&device, 64, BufferUsage::COPY_DST, MemoryLocation::GpuOnly);
    let mut list = CommandList::new("overflow");
    list.push(Command::CopyBuffer {
        src: a,
        src_offset: 32,
        dst: gpu,
        dst_offset: 0,
        size: 64,
    });
    assert!(matches!(run(&device, &list), Err(RhiError::Invalid { .. })));
    assert!(matches!(
        device.write_buffer(a, 60, &[0; 8]),
        Err(RhiError::Invalid { .. })
    ));
    assert!(matches!(
        device.read_buffer(gpu, 0, 4),
        Err(RhiError::Invalid { .. })
    ));
    device.wait_idle().unwrap();
    assert_clean(&log);
}
