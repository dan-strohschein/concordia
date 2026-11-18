// Conformance: a kernel writes a value into a bindless storage texture.
import bindless;

struct WritePush { uint target; uint pad0; uint pad1; uint pad2; float4 value; };

[shader("compute")]
[numthreads(1, 1, 1)]
void write_texture(uint3 id : SV_DispatchThreadID, uniform WritePush push) {
    g_storage.tex[push.target][id.xy] = push.value;
}
