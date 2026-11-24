// Compiles the material module with nothing but itself and a stub table source: proof that it
// has no renderer dependencies (M4 4.1b1). Not run; only compiled.
import material;

struct ConstantTables : IAlbedoTables {
    float2 direct_albedo(float nov, float roughness) { return float2(1.0 - roughness, 0.0); }
    float sheen_albedo(float nov, float roughness) { return 0.5; }
};

RWStructuredBuffer<float4> g_out;

[shader("compute")]
[numthreads(1, 1, 1)]
void main(uint3 id : SV_DispatchThreadID) {
    Closure c = {};
    c.base = float3(0.8, 0.2, 0.1);
    c.roughness = 0.5;
    c.a = alpha_of(c.roughness);
    c.f0 = f0_of(c);
    c.n = float3(0.0, 0.0, 1.0);
    c.nov = 1.0;
    ConstantTables tables;
    float3 v = float3(0.0, 0.0, 1.0);
    float3 l = normalize(float3(0.3, 0.0, 1.0));
    Entry lit = layer(cover(nothing(), 0.5), nothing());
    lit.color = eval_slab(c, v, l) + transmittance(tables, c, 0.7);
    g_out[id.x] = float4(lit.color, specular_energy(tables, c, 0.5).x);
}
