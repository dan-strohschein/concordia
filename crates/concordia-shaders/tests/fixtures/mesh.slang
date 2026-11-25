// A mesh shader with and without an amplification (task) stage (M4 4.1v(c), D4's spike).

struct MeshVertex
{
    float4 position : SV_Position;
    float3 color : COLOR0;
};

struct Payload
{
    uint triangles;
};

[shader("amplification")]
[numthreads(1, 1, 1)]
void as_main()
{
    Payload payload;
    payload.triangles = 1;
    DispatchMesh(1, 1, 1, payload);
}

[shader("mesh")]
[numthreads(1, 1, 1)]
[outputtopology("triangle")]
void ms_main(out indices uint3 triangles[1], out vertices MeshVertex vertices[3])
{
    SetMeshOutputCounts(3, 1);
    vertices[0].position = float4(-0.5, -0.5, 0.0, 1.0);
    vertices[1].position = float4(0.5, -0.5, 0.0, 1.0);
    vertices[2].position = float4(0.0, 0.5, 0.0, 1.0);
    for (uint i = 0; i < 3; i++)
        vertices[i].color = float3(1.0, 0.5, 0.25);
    triangles[0] = uint3(0, 1, 2);
}
