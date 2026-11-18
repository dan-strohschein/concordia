// Concordia's bindless ABI (ADR-0040, ADR-0045). Import this module and declare no other globals.
// Push constants are the entry point's only `uniform` parameter, named `push`:
//   void main(..., uniform MyPush push)
// Slots: 0 buffers (device pointers), 1 sampled textures, 2 storage textures (same table), 3 push constants.
module bindless;

public static const uint BINDLESS_CAPACITY = 16384;

public struct BufferTable { public uint* ptr[BINDLESS_CAPACITY]; };
public struct TextureTable { public Texture2D<float4> tex[BINDLESS_CAPACITY]; };
public struct StorageTable { public RWTexture2D<float4> tex[BINDLESS_CAPACITY]; };

public ParameterBlock<BufferTable> g_buffers;
public ParameterBlock<TextureTable> g_textures;
public ParameterBlock<StorageTable> g_storage;
