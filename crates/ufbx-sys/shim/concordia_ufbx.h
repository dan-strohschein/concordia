/* Concordia's flat C API over ufbx: plain structs and bounds-checked accessors, no callbacks.
 * Scenes are loaded right-handed, Y-up, in meters (geometry, transforms and animation).
 * Every entry point accepts NULL or out-of-range input and returns NULL, 0, CU_NONE or false. */
#ifndef CONCORDIA_UFBX_H
#define CONCORDIA_UFBX_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define CU_NONE (-1)

typedef struct cu_scene cu_scene;
typedef struct cu_bake cu_bake;

/* UTF-8 text owned by the scene (not NUL-terminated). */
typedef struct cu_str {
    const char *data;
    size_t len;
} cu_str;

/* Bytes owned by the scene. */
typedef struct cu_bytes {
    const uint8_t *data;
    size_t len;
} cu_bytes;

/* What the file declared, before normalization. */
typedef struct cu_settings {
    double original_unit_meters;
    double frames_per_second;
    int32_t original_up_axis; /* ufbx_coordinate_axis: 0 +X, 1 -X, 2 +Y, 3 -Y, 4 +Z, 5 -Z */
} cu_settings;

typedef struct cu_node {
    cu_str name;
    int32_t parent; /* node index, or CU_NONE for the implicit root */
    int32_t mesh;   /* mesh index or CU_NONE */
    uint32_t num_materials;
    bool is_root;
    float translation[3];
    float rotation[4]; /* x, y, z, w */
    float scale[3];
} cu_node;

typedef struct cu_mesh {
    cu_str name;
    uint32_t num_parts;    /* one per material slot */
    uint32_t num_vertices; /* logical vertices (what skin weights index) */
    int32_t skin;          /* skin index or CU_NONE */
    bool has_normals;
    bool has_uvs;
} cu_mesh;

/* One triangle corner. UVs are as stored (V up). */
typedef struct cu_corner {
    float position[3];
    float normal[3];
    float uv[2];
    uint32_t vertex; /* logical vertex */
} cu_corner;

typedef struct cu_skin {
    uint32_t num_clusters;
    uint32_t max_weights;
} cu_skin;

typedef struct cu_cluster {
    int32_t bone;                 /* node index */
    float geometry_to_bone[16];   /* column-major */
} cu_cluster;

typedef struct cu_weight {
    uint32_t cluster;
    float weight;
} cu_weight;

typedef struct cu_material {
    cu_str name;
    float base_color[4];
    float metallic;
    float roughness;
    float emissive[3];
    float emissive_strength;
    int32_t base_color_texture; /* texture file index or CU_NONE */
    int32_t normal_texture;
    int32_t emissive_texture;
} cu_material;

typedef struct cu_texture_file {
    cu_str relative_filename;
    cu_bytes content; /* embedded image bytes (len 0 when external) */
} cu_texture_file;

typedef struct cu_stack {
    cu_str name;
    double time_begin;
    double time_end;
} cu_stack;

typedef struct cu_baked_node {
    int32_t node;
    uint32_t num_keys[3]; /* translation, rotation, scale */
} cu_baked_node;

/* A baked key: vec3 channels use value[0..3], rotations x, y, z, w. */
typedef struct cu_key {
    float time;
    float value[4];
} cu_key;

enum { CU_CHANNEL_TRANSLATION = 0, CU_CHANNEL_ROTATION = 1, CU_CHANNEL_SCALE = 2 };

/* sizeof each struct above, by index in declaration order (cu_str .. cu_key); 0 past the end. */
size_t cu_struct_size(uint32_t which);

/* Loads an FBX (or OBJ) file from memory; on failure writes a message into `error`. */
cu_scene *cu_load(const uint8_t *data, size_t len, char *error, size_t error_len);
void cu_free(cu_scene *scene);
bool cu_settings_get(const cu_scene *scene, cu_settings *out);

size_t cu_node_count(const cu_scene *scene);
bool cu_node_get(const cu_scene *scene, size_t node, cu_node *out);
/* The material in a node's slot (per-instance), or CU_NONE. */
int32_t cu_node_material(const cu_scene *scene, size_t node, size_t slot);

size_t cu_mesh_count(const cu_scene *scene);
bool cu_mesh_get(const cu_scene *scene, size_t mesh, cu_mesh *out);
size_t cu_mesh_part_triangles(const cu_scene *scene, size_t mesh, size_t part);
/* Writes up to `cap` triangulated corners of a part; returns how many were written. */
size_t cu_mesh_part_corners(const cu_scene *scene, size_t mesh, size_t part, cu_corner *out, size_t cap);

bool cu_skin_get(const cu_scene *scene, size_t skin, cu_skin *out);
bool cu_skin_cluster(const cu_scene *scene, size_t skin, size_t cluster, cu_cluster *out);
/* Writes up to `cap` weights of a logical vertex (largest first); returns the vertex's total. */
size_t cu_skin_weights(const cu_scene *scene, size_t skin, size_t vertex, cu_weight *out, size_t cap);

size_t cu_material_count(const cu_scene *scene);
bool cu_material_get(const cu_scene *scene, size_t material, cu_material *out);
size_t cu_texture_file_count(const cu_scene *scene);
bool cu_texture_file_get(const cu_scene *scene, size_t file, cu_texture_file *out);

size_t cu_stack_count(const cu_scene *scene);
bool cu_stack_get(const cu_scene *scene, size_t stack, cu_stack *out);
/* Bakes a stack to linear keys starting at 0, resampled at `rate` keys per second. */
cu_bake *cu_bake_stack(const cu_scene *scene, size_t stack, double rate, char *error, size_t error_len);
void cu_bake_free(cu_bake *bake);
double cu_bake_duration(const cu_bake *bake);
size_t cu_bake_node_count(const cu_bake *bake);
bool cu_bake_node_get(const cu_bake *bake, size_t node, cu_baked_node *out);
/* Writes up to `cap` keys of one channel; returns how many were written. */
size_t cu_bake_keys(const cu_bake *bake, size_t node, uint32_t channel, cu_key *out, size_t cap);

#ifdef __cplusplus
}
#endif

#endif
