// Exclusive prefix sums of `uint` values (M4 4.4), deterministic: integer sums in a fixed tree, no
// atomics. `scan_groups` scans each group of SCAN_GROUP values and writes the group's total; the
// totals are scanned the same way, level after level (`scan.rs`), then `scan_add` adds each
// group's base back down the levels.
import bindless;

static const uint SCAN_GROUP = 256;
// Groups per dispatch row (the most a dimension allows everywhere).
static const uint SCAN_ROW = 65535;

// values: the inputs (`valid` of them; past those they read 0); out: `count` exclusive sums;
// sums: one total per group.
struct ScanPush { uint values; uint out; uint sums; uint count; uint valid; };

groupshared uint s_scan[SCAN_GROUP];

[shader("compute")]
[numthreads(256, 1, 1)]
void scan_groups(uint lane : SV_GroupThreadID, uint3 group : SV_GroupID, uniform ScanPush push) {
    uint g = group.y * SCAN_ROW + group.x;
    uint i = g * SCAN_GROUP + lane;
    uint v = i < push.valid ? g_buffers.ptr[push.values][i] : 0;
    s_scan[lane] = v;
    for (uint step = 1; step < SCAN_GROUP; step *= 2) {
        GroupMemoryBarrierWithGroupSync();
        uint before = lane >= step ? s_scan[lane - step] : 0;
        GroupMemoryBarrierWithGroupSync();
        s_scan[lane] += before;
    }
    if (i < push.count) { g_buffers.ptr[push.out][i] = s_scan[lane] - v; }
    if (lane == SCAN_GROUP - 1 && g * SCAN_GROUP < push.count) { g_buffers.ptr[push.sums][g] = s_scan[lane]; }
}

// out: `count` sums of one level; bases: the scanned totals of its groups.
struct ScanAddPush { uint out; uint bases; uint count; };

[shader("compute")]
[numthreads(256, 1, 1)]
void scan_add(uint lane : SV_GroupThreadID, uint3 group : SV_GroupID, uniform ScanAddPush push) {
    uint g = group.y * SCAN_ROW + group.x;
    uint i = g * SCAN_GROUP + lane;
    if (i < push.count) { g_buffers.ptr[push.out][i] += g_buffers.ptr[push.bases][g]; }
}
