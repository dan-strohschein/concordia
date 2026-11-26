//! Conformance cases for depth resolves (M4 4.4b).

use crate::caps::SampleCount;
use crate::command::{
    ClearValue, Command, CommandList, DepthAttachment, DepthResolve, DepthResolveTarget, LoadOp,
    StoreOp,
};
use crate::device::{Device, QueueKind, Submit};
use crate::format::Format;
use crate::handle::Texture;
use crate::pipeline::{CompareOp, DepthState};
use crate::resource::TextureUsage;

use super::Harness;
use super::cases::{clear, pass, triangle, vertices};
use super::util::{compute_entry, download, graphics, push, run, target, texture_desc, upload};

/// Side of the depth targets in pixels.
const N: u32 = 64;
/// The cleared (far, reverse-Z) depth.
const FAR: f32 = 0.25;
/// The triangle's (near) depth.
const NEAR: f32 = 0.75;

fn depth_texture(device: &dyn Device, samples: SampleCount, sampled: bool) -> Texture {
    let mut usage = TextureUsage::RENDER_TARGET;
    if sampled {
        usage |= TextureUsage::SAMPLED | TextureUsage::BINDLESS;
    }
    device
        .create_texture(&texture_desc(N, Format::Depth32Float, samples, usage))
        .expect("depth texture")
}

/// Draws the conformance triangle at [`NEAR`] over a depth cleared to [`FAR`], with `samples`.
fn draw_list(
    device: &dyn Device,
    h: &Harness<'_>,
    samples: SampleCount,
    depth: DepthAttachment,
) -> CommandList {
    let state = DepthState {
        format: Format::Depth32Float,
        compare: CompareOp::GreaterEqual,
        write: true,
    };
    let pipeline = graphics(device, h, "triangle", ("vs", "fs"), samples, Some(state));
    let color = target(device, N, samples);
    let resolve = (samples != SampleCount::X1).then(|| target(device, N, SampleCount::X1));
    let vb = upload(device, &vertices(&triangle(NEAR, [1.0; 4])));
    let mut list = CommandList::new("depth resolve");
    list.push(pass(color, clear([0.0; 4]), resolve, Some(depth)))
        .push(Command::SetPipeline(pipeline))
        .push(push(
            &[device.buffer_index(vb).unwrap().0, 0, 0, 0],
            &[1.0; 4],
        ))
        .push(Command::Draw {
            vertices: 0..3,
            instances: 0..1,
        })
        .push(Command::EndRenderPass);
    list
}

fn attachment(texture: Texture, resolve: Option<Texture>) -> DepthAttachment {
    DepthAttachment {
        texture,
        resolve: resolve.map(|texture| DepthResolveTarget {
            texture,
            filter: DepthResolve::Min,
        }),
        load: LoadOp::Clear(ClearValue::Depth(FAR)),
        store: StoreOp::DontCare,
    }
}

/// Every texel of single-sample depth texture `source`, read by a compute kernel.
fn read_depth(device: &dyn Device, h: &Harness<'_>, source: Texture) -> Vec<f32> {
    let bytes = (N * N * 4) as usize;
    let out = upload(device, &vec![0u8; bytes]);
    let mut list = CommandList::new("read depth");
    list.push(Command::SetPipeline(compute_entry(
        device,
        h,
        "depth",
        "read_depth",
    )))
    .push(push(
        &[
            device.texture_index(source).unwrap().0,
            device.buffer_index(out).unwrap().0,
            N,
        ],
        &[],
    ))
    .push(Command::Dispatch {
        groups: [N / 8, N / 8, 1],
    });
    run(device, &list);
    download(device, out, bytes as u64)
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect()
}

/// The standard 4x sample positions within a pixel (Vulkan's standard locations; Metal's default).
const SAMPLES_4X: [[f32; 2]; 4] = [
    [0.375, 0.125],
    [0.875, 0.375],
    [0.125, 0.625],
    [0.625, 0.875],
];

/// True if framebuffer point (x, y) (y down) lies inside the conformance triangle.
fn inside(x: f32, y: f32) -> bool {
    let n = N as f32;
    let (u, v) = (2.0 * x / n - 1.0, 1.0 - 2.0 * y / n);
    let edge = |a: [f32; 2], b: [f32; 2]| (b[0] - a[0]) * (v - a[1]) - (b[1] - a[1]) * (u - a[0]);
    let [p, q, r] = [[-0.8, -0.8], [0.8, -0.8], [0.0, 0.8]];
    edge(p, q) >= 0.0 && edge(q, r) >= 0.0 && edge(r, p) >= 0.0
}

/// A 4x depth attachment resolved with min keeps each pixel's farthest sample (the smallest value
/// with reverse-Z): a pixel reads the triangle's depth only when all four samples are covered, and
/// edge pixels read the clear, never an average. A resolve from a single-sample attachment is an
/// error, not a crash.
pub fn depth_resolve_min_keeps_the_farthest_sample(device: &dyn Device, h: &Harness<'_>) {
    if !device.caps().depth_resolve_min {
        return;
    }
    let msaa = depth_texture(device, SampleCount::X4, false);
    let resolved = depth_texture(device, SampleCount::X1, true);
    run(
        device,
        &draw_list(device, h, SampleCount::X4, attachment(msaa, Some(resolved))),
    );
    let min = read_depth(device, h, resolved);
    let odd: Vec<_> = min.iter().filter(|d| **d != FAR && **d != NEAR).collect();
    assert!(odd.is_empty(), "min resolves never blend samples: {odd:?}");
    let all_covered = |i: usize| {
        let (x, y) = ((i as u32 % N) as f32, (i as u32 / N) as f32);
        SAMPLES_4X.iter().all(|[sx, sy]| inside(x + sx, y + sy))
    };
    let wrong = (0..min.len())
        .filter(|&i| (min[i] == NEAR) != all_covered(i))
        .count();
    let near = min.iter().filter(|d| **d == NEAR).count();
    // A few samples may sit exactly on an edge, where rasterization rules decide.
    assert!(
        near > 0 && wrong <= 4,
        "{wrong} pixels disagree with the all-samples-covered rule ({near} near)"
    );
    let single = depth_texture(device, SampleCount::X1, true);
    let other = depth_texture(device, SampleCount::X1, true);
    let misuse = draw_list(device, h, SampleCount::X1, attachment(single, Some(other)));
    let result = device.submit(Submit {
        queue: QueueKind::Graphics,
        commands: &misuse,
        wait: &[],
        signal: &[],
    });
    let err = result.expect_err("a single-sample source can't be resolved");
    assert!(err.to_string().contains("multisampled source"), "{err}");
    device.wait_idle().expect("wait idle");
}
