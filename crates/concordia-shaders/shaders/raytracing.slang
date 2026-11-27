// Acceleration structures by bindless index (M4 4.8c, ADR-0040 amended): import it only in
// shaders that trace rays (it needs the ray-query capability). Metal buffer 4, Vulkan set 3;
// slots 0-3 stay as `bindless` has them.
module raytracing;
import bindless;

public static const uint ACCEL_CAPACITY = 256;

public struct AccelTable { public RaytracingAccelerationStructure accel[ACCEL_CAPACITY]; };

[[vk::binding(0, 3)]] public ParameterBlock<AccelTable> g_accels : register(b4, space3);
