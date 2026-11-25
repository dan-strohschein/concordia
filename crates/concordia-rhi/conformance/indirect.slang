// Conformance: indirect draws. `vs` pulls (position, color) pairs and moves each instance right by
// `step`, coloring instance 0 by its vertices and later ones green; `write_args` stores draw
// arguments the way a culling pass would.
import bindless;

struct IndirectPush { uint vertices; float step; uint args; uint count; uint instances; uint pad0; uint pad1; uint pad2; };
struct VsOut { float4 position : SV_Position; float4 color : COLOR; };

[shader("vertex")]
VsOut vs(uint vid : SV_VertexID, uint iid : SV_InstanceID, uniform IndirectPush push) {
    float4* v = (float4*)g_buffers.ptr[push.vertices];
    VsOut o;
    o.position = v[vid * 2] + float4(float(iid) * push.step, 0.0, 0.0, 0.0);
    o.color = iid == 0 ? v[vid * 2 + 1] : float4(0.0, 1.0, 0.0, 1.0);
    return o;
}

[shader("fragment")]
float4 fs(VsOut i) : SV_Target { return i.color; }

[shader("compute")]
[numthreads(1, 1, 1)]
void write_args(uint3 id : SV_DispatchThreadID, uniform IndirectPush push) {
    uint* args = g_buffers.ptr[push.args];
    args[0] = push.count;
    args[1] = push.instances;
    args[2] = 0;
    args[3] = 0;
}
