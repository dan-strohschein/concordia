// Concordia's shim over Recast and Detour v1.6.0 (zlib), M2 phase 2.5a.
// The tile build follows the steps of RecastDemo's Sample_TileMesh (zlib), with Concordia's
// area marking: marks apply in order with a temporary blocked id, then blocked becomes the null
// area before erosion, so agents keep their radius from blocked cells.
#include "concordia_recast.h"

#include <atomic>
#include <cmath>
#include <cstdlib>
#include <cstring>
#include <mutex>

#include "DetourAlloc.h"
#include "DetourNavMesh.h"
#include "DetourNavMeshBuilder.h"
#include "DetourNavMeshQuery.h"
#include "Recast.h"
#include "RecastAlloc.h"

static_assert(sizeof(dtPolyRef) == 8, "DT_POLYREF64 must be defined for every translation unit");

namespace {

std::atomic<int64_t> g_live{0};

void* counted_alloc(size_t size) {
    void* p = std::malloc(size);
    if (p) g_live.fetch_add(1, std::memory_order_relaxed);
    return p;
}

void counted_free(void* p) {
    if (!p) return;
    g_live.fetch_sub(1, std::memory_order_relaxed);
    std::free(p);
}

void* rc_alloc(size_t size, rcAllocHint) { return counted_alloc(size); }
void* dt_alloc(size_t size, dtAllocHint) { return counted_alloc(size); }

void install_allocators() {
    static std::once_flag once;
    std::call_once(once, [] {
        rcAllocSetCustom(rc_alloc, counted_free);
        dtAllocSetCustom(dt_alloc, counted_free);
    });
}

bool finite3(const float* v) { return v && std::isfinite(v[0]) && std::isfinite(v[1]) && std::isfinite(v[2]); }

bool positive(float v) { return std::isfinite(v) && v > 0.0f; }

bool valid_config(const cr_config* c) {
    return c && positive(c->cell_size) && positive(c->cell_height) && positive(c->agent_height) &&
           std::isfinite(c->agent_radius) && c->agent_radius >= 0.0f && std::isfinite(c->agent_max_climb) &&
           c->agent_max_climb >= 0.0f && std::isfinite(c->agent_max_slope_deg) && c->agent_max_slope_deg > 0.0f &&
           c->agent_max_slope_deg < 90.0f && c->tile_size > 0 && c->tile_size <= 4096 && c->region_min_size >= 0 &&
           c->region_merge_size >= 0 && std::isfinite(c->edge_max_len) && c->edge_max_len >= 0.0f &&
           std::isfinite(c->edge_max_error) && c->edge_max_error >= 0.0f && c->verts_per_poly >= 3 &&
           c->verts_per_poly <= CR_MAX_VERTS_PER_POLY && std::isfinite(c->detail_sample_dist) &&
           c->detail_sample_dist >= 0.0f && std::isfinite(c->detail_sample_max_error) &&
           c->detail_sample_max_error >= 0.0f;
}

bool valid_input(const cr_tile_input* in) {
    if (!in || in->vert_count < 0 || in->tri_count < 0 || in->mark_count < 0) return false;
    if ((in->vert_count > 0 && !in->verts) || (in->tri_count > 0 && !in->tris) || (in->mark_count > 0 && !in->marks))
        return false;
    if (!finite3(in->bmin) || !finite3(in->bmax)) return false;
    for (int i = 0; i < 3; ++i)
        if (!(in->bmin[i] < in->bmax[i])) return false;
    for (int i = 0; i < in->vert_count * 3; ++i)
        if (!std::isfinite(in->verts[i])) return false;
    for (int i = 0; i < in->tri_count * 3; ++i)
        if (in->tris[i] < 0 || in->tris[i] >= in->vert_count) return false;
    for (int i = 0; i < in->mark_count; ++i)
        if (!finite3(in->marks[i].min) || !finite3(in->marks[i].max)) return false;
    return true;
}

// Owns the intermediate Recast objects of one tile build.
struct TileBuild {
    rcHeightfield* solid = nullptr;
    rcCompactHeightfield* chf = nullptr;
    rcContourSet* cset = nullptr;
    rcPolyMesh* pmesh = nullptr;
    rcPolyMeshDetail* dmesh = nullptr;
    unsigned char* areas = nullptr;

    ~TileBuild() {
        rcFreeHeightField(solid);
        rcFreeCompactHeightfield(chf);
        rcFreeContourSet(cset);
        rcFreePolyMesh(pmesh);
        rcFreePolyMeshDetail(dmesh);
        rcFree(areas);
    }
};

rcConfig recast_config(const cr_config* c, const cr_tile_input* in) {
    rcConfig cfg;
    std::memset(&cfg, 0, sizeof(cfg));
    cfg.cs = c->cell_size;
    cfg.ch = c->cell_height;
    cfg.walkableSlopeAngle = c->agent_max_slope_deg;
    cfg.walkableHeight = (int)std::ceil(c->agent_height / cfg.ch);
    cfg.walkableClimb = (int)std::floor(c->agent_max_climb / cfg.ch);
    cfg.walkableRadius = (int)std::ceil(c->agent_radius / cfg.cs);
    cfg.maxEdgeLen = (int)(c->edge_max_len / cfg.cs);
    cfg.maxSimplificationError = c->edge_max_error;
    cfg.minRegionArea = c->region_min_size * c->region_min_size;
    cfg.mergeRegionArea = c->region_merge_size * c->region_merge_size;
    cfg.maxVertsPerPoly = c->verts_per_poly;
    cfg.tileSize = c->tile_size;
    cfg.borderSize = cfg.walkableRadius + 3;
    cfg.width = cfg.tileSize + cfg.borderSize * 2;
    cfg.height = cfg.tileSize + cfg.borderSize * 2;
    cfg.detailSampleDist = c->detail_sample_dist;
    cfg.detailSampleMaxError = c->detail_sample_max_error;
    rcVcopy(cfg.bmin, in->bmin);
    rcVcopy(cfg.bmax, in->bmax);
    cfg.bmin[0] -= cfg.borderSize * cfg.cs;
    cfg.bmin[2] -= cfg.borderSize * cfg.cs;
    cfg.bmax[0] += cfg.borderSize * cfg.cs;
    cfg.bmax[2] += cfg.borderSize * cfg.cs;
    return cfg;
}

// Marks in order (blocked as a temporary id), then blocked becomes the null area.
void apply_marks(rcContext* ctx, const cr_tile_input* in, rcCompactHeightfield* chf) {
    for (int i = 0; i < in->mark_count; ++i) {
        const cr_box_mark& m = in->marks[i];
        rcMarkBoxArea(ctx, m.min, m.max, m.area, *chf);
    }
    for (int i = 0; i < chf->spanCount; ++i)
        if (chf->areas[i] == CR_AREA_BLOCKED) chf->areas[i] = RC_NULL_AREA;
}

int32_t voxelize(rcContext* ctx, const rcConfig& cfg, const cr_tile_input* in, TileBuild& b) {
    b.solid = rcAllocHeightfield();
    if (!b.solid) return CR_ERR_ALLOC;
    if (!rcCreateHeightfield(ctx, *b.solid, cfg.width, cfg.height, cfg.bmin, cfg.bmax, cfg.cs, cfg.ch))
        return CR_ERR_HEIGHTFIELD;
    b.areas = (unsigned char*)rcAlloc(in->tri_count > 0 ? in->tri_count : 1, RC_ALLOC_TEMP);
    if (!b.areas) return CR_ERR_ALLOC;
    std::memset(b.areas, 0, in->tri_count > 0 ? in->tri_count : 1);
    rcMarkWalkableTriangles(ctx, cfg.walkableSlopeAngle, in->verts, in->vert_count, in->tris, in->tri_count,
                            b.areas);
    if (!rcRasterizeTriangles(ctx, in->verts, in->vert_count, in->tris, b.areas, in->tri_count, *b.solid,
                              cfg.walkableClimb))
        return CR_ERR_RASTERIZE;
    rcFilterLowHangingWalkableObstacles(ctx, cfg.walkableClimb, *b.solid);
    rcFilterLedgeSpans(ctx, cfg.walkableHeight, cfg.walkableClimb, *b.solid);
    rcFilterWalkableLowHeightSpans(ctx, cfg.walkableHeight, *b.solid);
    return CR_OK;
}

int32_t partition(rcContext* ctx, const rcConfig& cfg, const cr_tile_input* in, TileBuild& b) {
    b.chf = rcAllocCompactHeightfield();
    if (!b.chf) return CR_ERR_ALLOC;
    if (!rcBuildCompactHeightfield(ctx, cfg.walkableHeight, cfg.walkableClimb, *b.solid, *b.chf))
        return CR_ERR_COMPACT;
    apply_marks(ctx, in, b.chf);
    if (!rcErodeWalkableArea(ctx, cfg.walkableRadius, *b.chf)) return CR_ERR_ERODE;
    if (!rcBuildDistanceField(ctx, *b.chf)) return CR_ERR_REGIONS;
    if (!rcBuildRegions(ctx, *b.chf, cfg.borderSize, cfg.minRegionArea, cfg.mergeRegionArea))
        return CR_ERR_REGIONS;
    return CR_OK;
}

int32_t polygonize(rcContext* ctx, const rcConfig& cfg, TileBuild& b) {
    b.cset = rcAllocContourSet();
    if (!b.cset) return CR_ERR_ALLOC;
    if (!rcBuildContours(ctx, *b.chf, cfg.maxSimplificationError, cfg.maxEdgeLen, *b.cset))
        return CR_ERR_CONTOURS;
    if (b.cset->nconts == 0) return CR_EMPTY;
    b.pmesh = rcAllocPolyMesh();
    if (!b.pmesh) return CR_ERR_ALLOC;
    if (!rcBuildPolyMesh(ctx, *b.cset, cfg.maxVertsPerPoly, *b.pmesh)) return CR_ERR_POLYMESH;
    if (b.pmesh->npolys == 0) return CR_EMPTY;
    b.dmesh = rcAllocPolyMeshDetail();
    if (!b.dmesh) return CR_ERR_ALLOC;
    if (!rcBuildPolyMeshDetail(ctx, *b.pmesh, *b.chf, cfg.detailSampleDist, cfg.detailSampleMaxError, *b.dmesh))
        return CR_ERR_DETAIL;
    return CR_OK;
}

int32_t pack(const cr_config* c, const rcConfig& cfg, const cr_tile_input* in, TileBuild& b, uint8_t** out_data,
             int32_t* out_size) {
    for (int i = 0; i < b.pmesh->npolys; ++i) b.pmesh->flags[i] = 1;
    dtNavMeshCreateParams p;
    std::memset(&p, 0, sizeof(p));
    p.verts = b.pmesh->verts;
    p.vertCount = b.pmesh->nverts;
    p.polys = b.pmesh->polys;
    p.polyAreas = b.pmesh->areas;
    p.polyFlags = b.pmesh->flags;
    p.polyCount = b.pmesh->npolys;
    p.nvp = b.pmesh->nvp;
    p.detailMeshes = b.dmesh->meshes;
    p.detailVerts = b.dmesh->verts;
    p.detailVertsCount = b.dmesh->nverts;
    p.detailTris = b.dmesh->tris;
    p.detailTriCount = b.dmesh->ntris;
    p.walkableHeight = c->agent_height;
    p.walkableRadius = c->agent_radius;
    p.walkableClimb = c->agent_max_climb;
    p.tileX = in->tile_x;
    p.tileY = in->tile_y;
    p.tileLayer = 0;
    rcVcopy(p.bmin, b.pmesh->bmin);
    rcVcopy(p.bmax, b.pmesh->bmax);
    p.cs = cfg.cs;
    p.ch = cfg.ch;
    p.buildBvTree = true;
    unsigned char* data = nullptr;
    int size = 0;
    if (!dtCreateNavMeshData(&p, &data, &size)) return CR_ERR_NAVDATA;
    *out_data = data;
    *out_size = size;
    return CR_OK;
}

dtQueryFilter make_filter(const cr_filter* f) {
    dtQueryFilter filter;
    filter.setIncludeFlags(1);
    filter.setExcludeFlags(0);
    for (int i = 0; i < CR_MAX_AREAS; ++i) filter.setAreaCost(i, f->area_cost[i]);
    return filter;
}

bool valid_filter(const cr_filter* f) {
    if (!f) return false;
    for (int i = 0; i < CR_MAX_AREAS; ++i)
        if (!std::isfinite(f->area_cost[i]) || f->area_cost[i] < 0.0f) return false;
    return true;
}

int32_t query_status(dtStatus s) {
    if (dtStatusSucceed(s)) return CR_OK;
    if (dtStatusDetail(s, DT_INVALID_PARAM)) return CR_ERR_NOT_FOUND;
    if (dtStatusDetail(s, DT_OUT_OF_MEMORY)) return CR_ERR_ALLOC;
    return CR_ERR_QUERY;
}

}  // namespace

struct cr_navmesh {
    dtNavMesh* mesh;
};

struct cr_query {
    dtNavMeshQuery* query;
    const dtNavMesh* mesh;
};

extern "C" {

int64_t cr_live_allocations(void) {
    install_allocators();
    return g_live.load(std::memory_order_relaxed);
}

int32_t cr_struct_size(int32_t which) {
    switch (which) {
        case 0: return (int32_t)sizeof(cr_config);
        case 1: return (int32_t)sizeof(cr_box_mark);
        case 2: return (int32_t)sizeof(cr_tile_input);
        case 3: return (int32_t)sizeof(cr_poly);
        case 4: return (int32_t)sizeof(cr_filter);
        default: return -1;
    }
}

int32_t cr_build_tile(const cr_config* config, const cr_tile_input* input, uint8_t** out_data,
                      int32_t* out_size) {
    install_allocators();
    if (!out_data || !out_size || !valid_config(config) || !valid_input(input)) return CR_ERR_INPUT;
    *out_data = nullptr;
    *out_size = 0;
    if (input->tri_count == 0) return CR_EMPTY;
    rcContext ctx(false);
    const rcConfig cfg = recast_config(config, input);
    TileBuild b;
    int32_t code = voxelize(&ctx, cfg, input, b);
    if (code == CR_OK) code = partition(&ctx, cfg, input, b);
    if (code == CR_OK) code = polygonize(&ctx, cfg, b);
    if (code == CR_OK) code = pack(config, cfg, input, b, out_data, out_size);
    return code;
}

void cr_free(void* data) { dtFree(data); }

cr_navmesh* cr_navmesh_create(const float origin[3], float tile_width, float tile_height, int32_t max_tiles,
                              int32_t max_polys_per_tile) {
    install_allocators();
    if (!finite3(origin) || !positive(tile_width) || !positive(tile_height) || max_tiles <= 0 ||
        max_polys_per_tile <= 0)
        return nullptr;
    dtNavMeshParams p;
    std::memset(&p, 0, sizeof(p));
    rcVcopy(p.orig, origin);
    p.tileWidth = tile_width;
    p.tileHeight = tile_height;
    p.maxTiles = max_tiles;
    p.maxPolys = max_polys_per_tile;
    dtNavMesh* mesh = dtAllocNavMesh();
    if (!mesh) return nullptr;
    if (dtStatusFailed(mesh->init(&p))) {
        dtFreeNavMesh(mesh);
        return nullptr;
    }
    cr_navmesh* out = (cr_navmesh*)dtAlloc(sizeof(cr_navmesh), DT_ALLOC_PERM);
    if (!out) {
        dtFreeNavMesh(mesh);
        return nullptr;
    }
    out->mesh = mesh;
    return out;
}

void cr_navmesh_free(cr_navmesh* mesh) {
    if (!mesh) return;
    dtFreeNavMesh(mesh->mesh);
    dtFree(mesh);
}

int32_t cr_navmesh_add_tile(cr_navmesh* mesh, const uint8_t* data, int32_t size) {
    if (!mesh || !data || size <= (int32_t)sizeof(dtMeshHeader)) return CR_ERR_INPUT;
    const dtMeshHeader* header = (const dtMeshHeader*)data;
    if (header->magic != DT_NAVMESH_MAGIC || header->version != DT_NAVMESH_VERSION) return CR_ERR_ADD_TILE;
    if (mesh->mesh->getTileAt(header->x, header->y, header->layer)) return CR_ERR_ADD_TILE;
    unsigned char* copy = (unsigned char*)dtAlloc(size, DT_ALLOC_PERM);
    if (!copy) return CR_ERR_ALLOC;
    std::memcpy(copy, data, size);
    if (dtStatusFailed(mesh->mesh->addTile(copy, size, DT_TILE_FREE_DATA, 0, nullptr))) {
        dtFree(copy);
        return CR_ERR_ADD_TILE;
    }
    return CR_OK;
}

int32_t cr_navmesh_tile_slots(const cr_navmesh* mesh) { return mesh ? mesh->mesh->getMaxTiles() : 0; }

int32_t cr_navmesh_tile_info(const cr_navmesh* mesh, int32_t slot, int32_t* tile_x, int32_t* tile_y,
                             int32_t* poly_count) {
    if (!mesh || !tile_x || !tile_y || !poly_count || slot < 0 || slot >= mesh->mesh->getMaxTiles())
        return CR_ERR_INPUT;
    const dtNavMesh* nav = mesh->mesh;
    const dtMeshTile* tile = nav->getTile(slot);
    if (!tile || !tile->header) return CR_ERR_NOT_FOUND;
    *tile_x = tile->header->x;
    *tile_y = tile->header->y;
    *poly_count = tile->header->polyCount;
    return CR_OK;
}

int32_t cr_navmesh_poly(const cr_navmesh* mesh, int32_t slot, int32_t index, cr_poly* out) {
    if (!mesh || !out || slot < 0 || slot >= mesh->mesh->getMaxTiles()) return CR_ERR_INPUT;
    const dtNavMesh* nav = mesh->mesh;
    const dtMeshTile* tile = nav->getTile(slot);
    if (!tile || !tile->header) return CR_ERR_NOT_FOUND;
    if (index < 0 || index >= tile->header->polyCount) return CR_ERR_INPUT;
    const dtPoly& poly = tile->polys[index];
    std::memset(out, 0, sizeof(*out));
    out->ref = nav->getPolyRefBase(tile) | (dtPolyRef)index;
    out->area = poly.getArea();
    out->vert_count = poly.vertCount;
    for (int v = 0; v < poly.vertCount && v < CR_MAX_VERTS_PER_POLY; ++v)
        rcVcopy(&out->verts[v * 3], &tile->verts[poly.verts[v] * 3]);
    return CR_OK;
}

int32_t cr_navmesh_poly_links(const cr_navmesh* mesh, uint64_t ref, uint64_t* out, int32_t cap) {
    if (!mesh || !out || cap < 0) return CR_ERR_INPUT;
    const dtMeshTile* tile = nullptr;
    const dtPoly* poly = nullptr;
    if (dtStatusFailed(mesh->mesh->getTileAndPolyByRef(ref, &tile, &poly))) return CR_ERR_NOT_FOUND;
    int32_t n = 0;
    for (unsigned int k = poly->firstLink; k != DT_NULL_LINK; k = tile->links[k].next) {
        if (n < cap) out[n] = tile->links[k].ref;
        ++n;
    }
    return n < cap ? n : cap;
}

cr_query* cr_query_create(const cr_navmesh* mesh, int32_t max_nodes) {
    install_allocators();
    if (!mesh || max_nodes <= 0 || max_nodes > 65535) return nullptr;
    dtNavMeshQuery* q = dtAllocNavMeshQuery();
    if (!q) return nullptr;
    if (dtStatusFailed(q->init(mesh->mesh, max_nodes))) {
        dtFreeNavMeshQuery(q);
        return nullptr;
    }
    cr_query* out = (cr_query*)dtAlloc(sizeof(cr_query), DT_ALLOC_PERM);
    if (!out) {
        dtFreeNavMeshQuery(q);
        return nullptr;
    }
    out->query = q;
    out->mesh = mesh->mesh;
    return out;
}

void cr_query_free(cr_query* query) {
    if (!query) return;
    dtFreeNavMeshQuery(query->query);
    dtFree(query);
}

int32_t cr_find_nearest(cr_query* query, const cr_filter* filter, const float center[3],
                        const float half_extents[3], uint64_t* out_ref, float out_point[3]) {
    if (!query || !valid_filter(filter) || !finite3(center) || !finite3(half_extents) || !out_ref || !out_point)
        return CR_ERR_INPUT;
    const dtQueryFilter f = make_filter(filter);
    dtPolyRef ref = 0;
    const dtStatus s = query->query->findNearestPoly(center, half_extents, &f, &ref, out_point);
    if (dtStatusFailed(s)) return query_status(s);
    *out_ref = ref;
    return ref ? CR_OK : CR_ERR_NOT_FOUND;
}

int32_t cr_find_path(cr_query* query, const cr_filter* filter, uint64_t start_ref, uint64_t end_ref,
                     const float start[3], const float end[3], uint64_t* path, int32_t cap, int32_t* count,
                     uint8_t* partial) {
    if (!query || !valid_filter(filter) || !finite3(start) || !finite3(end) || !path || cap <= 0 || !count ||
        !partial)
        return CR_ERR_INPUT;
    if (!query->mesh->isValidPolyRef(start_ref) || !query->mesh->isValidPolyRef(end_ref)) return CR_ERR_NOT_FOUND;
    const dtQueryFilter f = make_filter(filter);
    int n = 0;
    const dtStatus s = query->query->findPath(start_ref, end_ref, start, end, &f, path, &n, cap);
    if (dtStatusFailed(s)) return query_status(s);
    *count = n;
    *partial = (dtStatusDetail(s, DT_PARTIAL_RESULT) || (n > 0 && path[n - 1] != end_ref)) ? 1 : 0;
    return CR_OK;
}

int32_t cr_straight_path(cr_query* query, const float start[3], const float end[3], const uint64_t* path,
                         int32_t path_count, float* points, int32_t cap, int32_t* count) {
    if (!query || !finite3(start) || !finite3(end) || !path || path_count <= 0 || !points || cap < 2 || !count)
        return CR_ERR_INPUT;
    for (int i = 0; i < path_count; ++i)
        if (!query->mesh->isValidPolyRef(path[i])) return CR_ERR_NOT_FOUND;
    int n = 0;
    const dtStatus s =
        query->query->findStraightPath(start, end, path, path_count, points, nullptr, nullptr, &n, cap, 0);
    if (dtStatusFailed(s)) return query_status(s);
    *count = n;
    return CR_OK;
}

int32_t cr_raycast(cr_query* query, const cr_filter* filter, uint64_t start_ref, const float start[3],
                   const float end[3], float* t, float normal[3]) {
    if (!query || !valid_filter(filter) || !finite3(start) || !finite3(end) || !t || !normal) return CR_ERR_INPUT;
    if (!query->mesh->isValidPolyRef(start_ref)) return CR_ERR_NOT_FOUND;
    const dtQueryFilter f = make_filter(filter);
    dtRaycastHit hit;
    std::memset(&hit, 0, sizeof(hit));
    const dtStatus s = query->query->raycast(start_ref, start, end, &f, 0, &hit, 0);
    if (dtStatusFailed(s)) return query_status(s);
    *t = hit.t;
    rcVcopy(normal, hit.hitNormal);
    return CR_OK;
}

}
