// D4's spike (M4 4.1v(c)): one triangle from a mesh shader, alone and launched by an
// amplification (task) shader. Not a production pipeline (4.3b).

struct MeshVertex
{
    float4 position : SV_Position;
    float3 color : COLOR0;
};

struct Payload
{
    float3 color;
};

void emit(float3 color, out indices uint3 triangles[1], out vertices MeshVertex vertices[3])
{
    SetMeshOutputCounts(3, 1);
    vertices[0].position = float4(-0.5, -0.5, 0.0, 1.0);
    vertices[1].position = float4(0.5, -0.5, 0.0, 1.0);
    vertices[2].position = float4(0.0, 0.5, 0.0, 1.0);
    for (uint i = 0; i < 3; i++)
        vertices[i].color = color;
    triangles[0] = uint3(0, 1, 2);
}

[shader("mesh")]
[numthreads(1, 1, 1)]
[outputtopology("triangle")]
void ms_main(out indices uint3 triangles[1], out vertices MeshVertex vertices[3])
{
    emit(float3(1.0, 0.5, 0.25), triangles, vertices);
}

[shader("amplification")]
[numthreads(1, 1, 1)]
void as_main()
{
    Payload payload;
    payload.color = float3(0.25, 1.0, 0.5);
    DispatchMesh(1, 1, 1, payload);
}

[shader("mesh")]
[numthreads(1, 1, 1)]
[outputtopology("triangle")]
void ms_tasked(in payload Payload payload, out indices uint3 triangles[1], out vertices MeshVertex vertices[3])
{
    emit(payload.color, triangles, vertices);
}

[shader("fragment")]
float4 fs_main(MeshVertex input) : SV_Target
{
    return float4(input.color, 1.0);
}
