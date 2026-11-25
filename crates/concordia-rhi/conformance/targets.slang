// Conformance: two color targets and two-channel float targets (M4 4.3b2). A full-screen triangle
// writes `first` to target 0 and `second` to target 1 (`fs_two`), or `first.xy` to one RG target
// (`fs_rg`).
import bindless;

struct TargetsPush { float4 first; float4 second; };
struct VsOut { float4 position : SV_Position; };
struct Two { float4 first : SV_Target0; float4 second : SV_Target1; };

[shader("vertex")]
VsOut vs(uint vid : SV_VertexID) {
    VsOut o;
    o.position = float4(vid == 1 ? 3.0 : -1.0, vid == 2 ? 3.0 : -1.0, 0.5, 1.0);
    return o;
}

[shader("fragment")]
Two fs_two(VsOut i, uniform TargetsPush push) {
    Two o;
    o.first = push.first;
    o.second = push.second;
    return o;
}

[shader("fragment")]
float2 fs_rg(VsOut i, uniform TargetsPush push) : SV_Target { return push.first.xy; }
