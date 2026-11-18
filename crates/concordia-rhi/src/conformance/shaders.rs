//! Conformance shaders (MSL until phase 1.3 moves them to Slang). Binding model: bindless buffer
//! pointers at buffer(0), sampled textures at buffer(1), storage textures at buffer(2), push constants at buffer(3).

use crate::caps::Backend;
use crate::pipeline::ShaderSource;

const PRELUDE: &str = r#"
#include <metal_stdlib>
using namespace metal;
struct BufferTable { device uint* ptr[16384]; };
struct TextureTable { texture2d<float> tex[16384]; };
struct StorageTable { texture2d<float, access::read_write> tex[16384]; };
struct VsOut { float4 position [[position]]; float4 color; };
"#;

const FILL: &str = r#"
struct Push { uint out; uint count; };
kernel void fill(constant BufferTable& b [[buffer(0)]], constant Push& p [[buffer(3)]],
                 uint i [[thread_position_in_grid]]) {
    if (i < p.count) b.ptr[p.out][i] = i * 3 + 1;
}
"#;

const SUM: &str = r#"
struct Push { uint src; uint dst; uint count; };
kernel void sum(constant BufferTable& b [[buffer(0)]], constant Push& p [[buffer(3)]],
                uint i [[thread_position_in_grid]]) {
    if (i != 0) return;
    uint total = 0;
    for (uint k = 0; k < p.count; ++k) total += b.ptr[p.src][k];
    b.ptr[p.dst][0] = total;
}
"#;

const TRIANGLE: &str = r#"
struct Push { uint vertices; uint pad0; uint pad1; uint pad2; float4 tint; };
vertex VsOut vs(uint vid [[vertex_id]], constant BufferTable& b [[buffer(0)]], constant Push& p [[buffer(3)]]) {
    device float4* v = (device float4*)b.ptr[p.vertices];
    VsOut out;
    out.position = v[vid * 2];
    out.color = v[vid * 2 + 1] * p.tint;
    return out;
}
fragment float4 fs(VsOut in [[stage_in]]) { return in.color; }
"#;

const WRITE_TEXTURE: &str = r#"
struct Push { uint target; uint pad0; uint pad1; uint pad2; float4 value; };
kernel void write_texture(constant StorageTable& s [[buffer(2)]], constant Push& p [[buffer(3)]],
                          uint2 i [[thread_position_in_grid]]) {
    s.tex[p.target].write(p.value, i);
}
"#;

const SAMPLE_TEXTURE: &str = r#"
struct Push { uint source; uint pad0; uint pad1; uint pad2; float4 unused; };
vertex VsOut fullscreen(uint vid [[vertex_id]]) {
    float2 uv = float2((vid << 1) & 2, vid & 2);
    VsOut out;
    out.position = float4(uv * 2.0 - 1.0, 0.0, 1.0);
    out.color = float4(uv, 0.0, 1.0);
    return out;
}
fragment float4 sample_texture(VsOut in [[stage_in]], constant TextureTable& t [[buffer(1)]], constant Push& p [[buffer(3)]]) {
    return t.tex[p.source].read(uint2(0, 0));
}
"#;

/// A named conformance shader for the device's backend.
pub(crate) fn shader(backend: Backend, name: &str, entry: &str) -> ShaderSource {
    let body = match name {
        "fill" => FILL,
        "sum" => SUM,
        "triangle" => TRIANGLE,
        "write_texture" => WRITE_TEXTURE,
        "sample_texture" => SAMPLE_TEXTURE,
        other => panic!("no conformance shader {other:?}"),
    };
    match backend {
        Backend::Metal => ShaderSource::Msl {
            source: format!("{PRELUDE}{body}"),
            entry: entry.to_owned(),
        },
        Backend::Vulkan => panic!("Vulkan conformance shaders arrive with Slang (phase 1.3)"),
    }
}
