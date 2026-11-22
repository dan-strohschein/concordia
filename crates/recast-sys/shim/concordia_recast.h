/* Concordia's flat C API over vendored Recast and Detour v1.6.0 (zlib), M2 phase 2.5a.
 * Every entry point validates its input and returns an error code (or null) instead of crashing.
 * Nothing calls back into Rust. Polygon references are 64-bit (DT_POLYREF64). */
#ifndef CONCORDIA_RECAST_H
#define CONCORDIA_RECAST_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Status codes. */
#define CR_OK 0
#define CR_EMPTY 1                /* a tile with no walkable polygons */
#define CR_ERR_INPUT (-1)         /* null, out of range, non-finite or inconsistent input */
#define CR_ERR_ALLOC (-2)         /* Recast or Detour ran out of memory */
#define CR_ERR_HEIGHTFIELD (-10)  /* rcCreateHeightfield */
#define CR_ERR_RASTERIZE (-11)    /* rcRasterizeTriangles */
#define CR_ERR_COMPACT (-12)      /* rcBuildCompactHeightfield */
#define CR_ERR_ERODE (-13)        /* rcErodeWalkableArea */
#define CR_ERR_REGIONS (-14)      /* rcBuildDistanceField / rcBuildRegions */
#define CR_ERR_CONTOURS (-15)     /* rcBuildContours */
#define CR_ERR_POLYMESH (-16)     /* rcBuildPolyMesh */
#define CR_ERR_DETAIL (-17)       /* rcBuildPolyMeshDetail */
#define CR_ERR_NAVDATA (-18)      /* dtCreateNavMeshData */
#define CR_ERR_ADD_TILE (-20)     /* dtNavMesh::addTile (bad data, or the slot is taken) */
#define CR_ERR_NOT_FOUND (-21)    /* no polygon near the point, or a stale reference */
#define CR_ERR_QUERY (-22)        /* a Detour query failed */

/* Area ids: 0 is Recast's null area; CR_AREA_BLOCKED is turned into it after marking. */
#define CR_AREA_BLOCKED 1
#define CR_AREA_WALKABLE 63
#define CR_MAX_AREAS 64
#define CR_MAX_VERTS_PER_POLY 6

/* Build settings, in world units unless noted. */
typedef struct cr_config {
    float cell_size;
    float cell_height;
    float agent_height;
    float agent_radius;
    float agent_max_climb;
    float agent_max_slope_deg;
    int32_t tile_size;          /* cells per tile side */
    int32_t region_min_size;    /* cells per side of the smallest kept region */
    int32_t region_merge_size;  /* cells per side under which regions merge */
    float edge_max_len;
    float edge_max_error;
    int32_t verts_per_poly;     /* 3..=6 */
    float detail_sample_dist;
    float detail_sample_max_error;
} cr_config;

/* An axis-aligned box whose walkable cells get `area` (blocked, walkable or a cost area). */
typedef struct cr_box_mark {
    float min[3];
    float max[3];
    uint8_t area;
    uint8_t pad[3];
} cr_box_mark;

/* One tile's input: triangles overlapping it and marks applied in order. */
typedef struct cr_tile_input {
    const float* verts;         /* vert_count * 3 */
    const int32_t* tris;        /* tri_count * 3 */
    const cr_box_mark* marks;   /* mark_count */
    int32_t vert_count;
    int32_t tri_count;
    int32_t mark_count;
    int32_t tile_x;
    int32_t tile_y;
    float bmin[3];              /* tile bounds, without the border */
    float bmax[3];
} cr_tile_input;

/* One navmesh polygon. */
typedef struct cr_poly {
    uint64_t ref;
    float verts[CR_MAX_VERTS_PER_POLY * 3];
    uint8_t vert_count;
    uint8_t area;
    uint8_t pad[6];
} cr_poly;

/* Travel cost per area id (Detour's query filter; every polygon has flag 1). */
typedef struct cr_filter {
    float area_cost[CR_MAX_AREAS];
} cr_filter;

typedef struct cr_navmesh cr_navmesh;
typedef struct cr_query cr_query;

/* Recast/Detour heap blocks currently allocated (counted from the first call into the shim). */
int64_t cr_live_allocations(void);

/* sizeof of the header's structs: 0 cr_config, 1 cr_box_mark, 2 cr_tile_input, 3 cr_poly, 4 cr_filter. */
int32_t cr_struct_size(int32_t which);

/* Builds one tile. On CR_OK, *out_data (free with cr_free) holds Detour tile data. */
int32_t cr_build_tile(const cr_config* config, const cr_tile_input* input, uint8_t** out_data,
                      int32_t* out_size);

/* Frees memory the shim returned. Null is ignored. */
void cr_free(void* data);

/* A tiled navmesh; tiles are added by grid position. Null on bad input. */
cr_navmesh* cr_navmesh_create(const float origin[3], float tile_width, float tile_height,
                              int32_t max_tiles, int32_t max_polys_per_tile);
void cr_navmesh_free(cr_navmesh* mesh);

/* Copies tile data from cr_build_tile into the navmesh. */
int32_t cr_navmesh_add_tile(cr_navmesh* mesh, const uint8_t* data, int32_t size);

/* Tile slots (some may be empty). */
int32_t cr_navmesh_tile_slots(const cr_navmesh* mesh);

/* A filled slot's grid position and polygon count; CR_ERR_NOT_FOUND for an empty slot. */
int32_t cr_navmesh_tile_info(const cr_navmesh* mesh, int32_t slot, int32_t* tile_x, int32_t* tile_y,
                             int32_t* poly_count);

/* One ground polygon of a filled slot. */
int32_t cr_navmesh_poly(const cr_navmesh* mesh, int32_t slot, int32_t index, cr_poly* out);

/* The polygons linked to `ref` (neighbours across edges and tiles); the count, or an error. */
int32_t cr_navmesh_poly_links(const cr_navmesh* mesh, uint64_t ref, uint64_t* out, int32_t cap);

/* A query object over `mesh` (which must outlive it). Null on bad input. */
cr_query* cr_query_create(const cr_navmesh* mesh, int32_t max_nodes);
void cr_query_free(cr_query* query);

/* The nearest polygon and point within the box `center` ± `half_extents`. */
int32_t cr_find_nearest(cr_query* query, const cr_filter* filter, const float center[3],
                        const float half_extents[3], uint64_t* out_ref, float out_point[3]);

/* The polygon corridor from start to end; *partial is 1 when end wasn't reached. */
int32_t cr_find_path(cr_query* query, const cr_filter* filter, uint64_t start_ref, uint64_t end_ref,
                     const float start[3], const float end[3], uint64_t* path, int32_t cap,
                     int32_t* count, uint8_t* partial);

/* Corner points (xyz triples) along a corridor, start and end included; an end outside the
 * corridor's last polygon is clamped to it (a partial path's closest reachable point). */
int32_t cr_straight_path(cr_query* query, const float start[3], const float end[3],
                         const uint64_t* path, int32_t path_count, float* points, int32_t cap,
                         int32_t* count);

/* Walks the navmesh surface from start toward end. *t is in [0, 1] at a wall (with its normal),
 * or greater than 1 when end was reached. */
int32_t cr_raycast(cr_query* query, const cr_filter* filter, uint64_t start_ref, const float start[3],
                   const float end[3], float* t, float normal[3]);

/* Crowd queries (M2 phase 2.5c.1). */

/* Moves from start (on start_ref) toward end along the surface, sliding along walls. Writes the
 * reached point (its height is end's) and the visited polygons, start's first (at most cap). */
int32_t cr_move_along_surface(cr_query* query, const cr_filter* filter, uint64_t start_ref,
                              const float start[3], const float end[3], float out_point[3],
                              uint64_t* visited, int32_t cap, int32_t* count);

/* The surface height of polygon `ref` under pos; CR_EMPTY when pos is outside it. */
int32_t cr_poly_height(cr_query* query, uint64_t ref, const float pos[3], float* height);

/* cr_raycast that also writes the polygons the ray crossed, start's first (at most cap). */
int32_t cr_raycast_path(cr_query* query, const cr_filter* filter, uint64_t start_ref, const float start[3],
                        const float end[3], float* t, float normal[3], uint64_t* path, int32_t cap,
                        int32_t* count);

#ifdef __cplusplus
}
#endif

#endif
