/* Concordia's flat C API over ufbx (see concordia_ufbx.h). Every index is checked before use. */
#include "concordia_ufbx.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "ufbx.h"

struct cu_bake {
    ufbx_baked_anim *anim;
};

static const ufbx_scene *as_scene(const cu_scene *scene) { return (const ufbx_scene *)scene; }

static cu_str str_of(ufbx_string s) {
    cu_str out = {s.data, s.length};
    return out;
}

static void write_error(char *dst, size_t len, const char *message) {
    if (dst != NULL && len > 0) {
        snprintf(dst, len, "%s", message);
    }
}

static int32_t index_or_none(size_t index, size_t count) {
    return (index < count && index <= INT32_MAX) ? (int32_t)index : CU_NONE;
}

size_t cu_struct_size(uint32_t which) {
    static const size_t sizes[] = {
        sizeof(cu_str),     sizeof(cu_bytes),    sizeof(cu_settings),     sizeof(cu_node),
        sizeof(cu_mesh),    sizeof(cu_corner),   sizeof(cu_skin),         sizeof(cu_cluster),
        sizeof(cu_weight),  sizeof(cu_material), sizeof(cu_texture_file), sizeof(cu_stack),
        sizeof(cu_baked_node), sizeof(cu_key),
    };
    return which < sizeof(sizes) / sizeof(sizes[0]) ? sizes[which] : 0;
}

cu_scene *cu_load(const uint8_t *data, size_t len, char *error, size_t error_len) {
    if (data == NULL || len == 0) {
        write_error(error, error_len, "empty input");
        return NULL;
    }
    ufbx_load_opts opts;
    memset(&opts, 0, sizeof(opts));
    opts.target_axes = ufbx_axes_right_handed_y_up;
    opts.target_unit_meters = 1.0;
    opts.space_conversion = UFBX_SPACE_CONVERSION_MODIFY_GEOMETRY;
    opts.geometry_transform_handling = UFBX_GEOMETRY_TRANSFORM_HANDLING_MODIFY_GEOMETRY;
    opts.handedness_conversion_axis = UFBX_MIRROR_AXIS_Z;
    opts.generate_missing_normals = true;
    opts.normalize_normals = true;
    opts.clean_skin_weights = true;
    opts.use_blender_pbr_material = true;
    opts.node_depth_limit = 256;
    ufbx_error err;
    ufbx_scene *scene = ufbx_load_memory(data, len, &opts, &err);
    if (scene == NULL) {
        char message[512];
        ufbx_format_error(message, sizeof(message), &err);
        write_error(error, error_len, message);
    }
    return (cu_scene *)scene;
}

void cu_free(cu_scene *scene) {
    if (scene != NULL) {
        ufbx_free_scene((ufbx_scene *)scene);
    }
}

bool cu_settings_get(const cu_scene *scene, cu_settings *out) {
    const ufbx_scene *s = as_scene(scene);
    if (s == NULL || out == NULL) {
        return false;
    }
    out->original_unit_meters = s->settings.original_unit_meters;
    out->frames_per_second = s->settings.frames_per_second;
    out->original_up_axis = (int32_t)s->settings.axes.up; /* the file's axes, kept by ufbx */
    return true;
}

size_t cu_node_count(const cu_scene *scene) {
    const ufbx_scene *s = as_scene(scene);
    return s == NULL ? 0 : s->nodes.count;
}

static const ufbx_node *node_at(const ufbx_scene *s, size_t node) {
    return (s != NULL && node < s->nodes.count) ? s->nodes.data[node] : NULL;
}

bool cu_node_get(const cu_scene *scene, size_t node, cu_node *out) {
    const ufbx_scene *s = as_scene(scene);
    const ufbx_node *n = node_at(s, node);
    if (n == NULL || out == NULL) {
        return false;
    }
    out->name = str_of(n->name);
    out->parent = n->parent != NULL ? index_or_none(n->parent->typed_id, s->nodes.count) : CU_NONE;
    out->mesh = n->mesh != NULL ? index_or_none(n->mesh->typed_id, s->meshes.count) : CU_NONE;
    out->num_materials = (uint32_t)n->materials.count;
    out->is_root = n->is_root;
    ufbx_transform t = n->local_transform;
    float translation[3] = {(float)t.translation.x, (float)t.translation.y, (float)t.translation.z};
    float rotation[4] = {(float)t.rotation.x, (float)t.rotation.y, (float)t.rotation.z, (float)t.rotation.w};
    float scale[3] = {(float)t.scale.x, (float)t.scale.y, (float)t.scale.z};
    memcpy(out->translation, translation, sizeof(translation));
    memcpy(out->rotation, rotation, sizeof(rotation));
    memcpy(out->scale, scale, sizeof(scale));
    return true;
}

int32_t cu_node_material(const cu_scene *scene, size_t node, size_t slot) {
    const ufbx_scene *s = as_scene(scene);
    const ufbx_node *n = node_at(s, node);
    if (n == NULL || slot >= n->materials.count || n->materials.data[slot] == NULL) {
        return CU_NONE;
    }
    return index_or_none(n->materials.data[slot]->typed_id, s->materials.count);
}

size_t cu_mesh_count(const cu_scene *scene) {
    const ufbx_scene *s = as_scene(scene);
    return s == NULL ? 0 : s->meshes.count;
}

static const ufbx_mesh *mesh_at(const ufbx_scene *s, size_t mesh) {
    return (s != NULL && mesh < s->meshes.count) ? s->meshes.data[mesh] : NULL;
}

bool cu_mesh_get(const cu_scene *scene, size_t mesh, cu_mesh *out) {
    const ufbx_scene *s = as_scene(scene);
    const ufbx_mesh *m = mesh_at(s, mesh);
    if (m == NULL || out == NULL) {
        return false;
    }
    out->name = str_of(m->name);
    out->num_parts = (uint32_t)m->material_parts.count;
    out->num_vertices = (uint32_t)m->num_vertices;
    out->skin = CU_NONE;
    if (m->skin_deformers.count > 0 && m->skin_deformers.data[0] != NULL) {
        out->skin = index_or_none(m->skin_deformers.data[0]->typed_id, s->skin_deformers.count);
    }
    out->has_normals = m->vertex_normal.exists;
    out->has_uvs = m->vertex_uv.exists;
    return true;
}

static const ufbx_mesh_part *part_at(const ufbx_mesh *m, size_t part) {
    return (m != NULL && part < m->material_parts.count) ? &m->material_parts.data[part] : NULL;
}

size_t cu_mesh_part_triangles(const cu_scene *scene, size_t mesh, size_t part) {
    const ufbx_mesh_part *p = part_at(mesh_at(as_scene(scene), mesh), part);
    return p == NULL ? 0 : p->num_triangles;
}

static void vec3_at(const ufbx_vertex_vec3 *attr, size_t index, float out[3]) {
    out[0] = out[1] = out[2] = 0.0f;
    if (!attr->exists || index >= attr->indices.count) {
        return;
    }
    uint32_t value = attr->indices.data[index];
    if (value >= attr->values.count) {
        return;
    }
    ufbx_vec3 v = attr->values.data[value];
    out[0] = (float)v.x;
    out[1] = (float)v.y;
    out[2] = (float)v.z;
}

static void vec2_at(const ufbx_vertex_vec2 *attr, size_t index, float out[2]) {
    out[0] = out[1] = 0.0f;
    if (!attr->exists || index >= attr->indices.count) {
        return;
    }
    uint32_t value = attr->indices.data[index];
    if (value >= attr->values.count) {
        return;
    }
    out[0] = (float)attr->values.data[value].x;
    out[1] = (float)attr->values.data[value].y;
}

static cu_corner corner_at(const ufbx_mesh *m, uint32_t index) {
    cu_corner c;
    vec3_at(&m->vertex_position, index, c.position);
    vec3_at(&m->vertex_normal, index, c.normal);
    vec2_at(&m->vertex_uv, index, c.uv);
    c.vertex = index < m->vertex_indices.count ? m->vertex_indices.data[index] : 0;
    return c;
}

size_t cu_mesh_part_corners(const cu_scene *scene, size_t mesh, size_t part, cu_corner *out, size_t cap) {
    const ufbx_mesh *m = mesh_at(as_scene(scene), mesh);
    const ufbx_mesh_part *p = part_at(m, part);
    if (p == NULL || out == NULL || cap == 0 || m->max_face_triangles == 0) {
        return 0;
    }
    size_t tri_cap = m->max_face_triangles * 3;
    uint32_t *tri = (uint32_t *)malloc(tri_cap * sizeof(uint32_t));
    if (tri == NULL) {
        return 0;
    }
    size_t written = 0;
    for (size_t f = 0; f < p->face_indices.count && written < cap; f++) {
        uint32_t face_index = p->face_indices.data[f];
        if (face_index >= m->faces.count || m->faces.data[face_index].num_indices < 3) {
            continue;
        }
        uint32_t n = ufbx_triangulate_face(tri, tri_cap, m, m->faces.data[face_index]);
        for (size_t k = 0; k < (size_t)n * 3 && written < cap; k++) {
            out[written++] = corner_at(m, tri[k]);
        }
    }
    free(tri);
    return written;
}

static const ufbx_skin_deformer *skin_at(const ufbx_scene *s, size_t skin) {
    return (s != NULL && skin < s->skin_deformers.count) ? s->skin_deformers.data[skin] : NULL;
}

bool cu_skin_get(const cu_scene *scene, size_t skin, cu_skin *out) {
    const ufbx_skin_deformer *d = skin_at(as_scene(scene), skin);
    if (d == NULL || out == NULL) {
        return false;
    }
    out->num_clusters = (uint32_t)d->clusters.count;
    out->max_weights = (uint32_t)d->max_weights_per_vertex;
    return true;
}

bool cu_skin_cluster(const cu_scene *scene, size_t skin, size_t cluster, cu_cluster *out) {
    const ufbx_scene *s = as_scene(scene);
    const ufbx_skin_deformer *d = skin_at(s, skin);
    if (d == NULL || out == NULL || cluster >= d->clusters.count) {
        return false;
    }
    const ufbx_skin_cluster *c = d->clusters.data[cluster];
    if (c == NULL || c->bone_node == NULL) {
        return false;
    }
    out->bone = index_or_none(c->bone_node->typed_id, s->nodes.count);
    const ufbx_matrix *g = &c->geometry_to_bone;
    const ufbx_vec3 cols[4] = {g->cols[0], g->cols[1], g->cols[2], g->cols[3]};
    for (int col = 0; col < 4; col++) {
        out->geometry_to_bone[col * 4 + 0] = (float)cols[col].x;
        out->geometry_to_bone[col * 4 + 1] = (float)cols[col].y;
        out->geometry_to_bone[col * 4 + 2] = (float)cols[col].z;
        out->geometry_to_bone[col * 4 + 3] = col == 3 ? 1.0f : 0.0f;
    }
    return true;
}

size_t cu_skin_weights(const cu_scene *scene, size_t skin, size_t vertex, cu_weight *out, size_t cap) {
    const ufbx_skin_deformer *d = skin_at(as_scene(scene), skin);
    if (d == NULL || vertex >= d->vertices.count) {
        return 0;
    }
    ufbx_skin_vertex v = d->vertices.data[vertex];
    if ((size_t)v.weight_begin + v.num_weights > d->weights.count) {
        return 0;
    }
    for (size_t i = 0; out != NULL && i < v.num_weights && i < cap; i++) {
        ufbx_skin_weight w = d->weights.data[v.weight_begin + i];
        out[i].cluster = w.cluster_index;
        out[i].weight = (float)w.weight;
    }
    return v.num_weights;
}

size_t cu_material_count(const cu_scene *scene) {
    const ufbx_scene *s = as_scene(scene);
    return s == NULL ? 0 : s->materials.count;
}

static int32_t texture_file(const ufbx_scene *s, const ufbx_material_map *map) {
    const ufbx_texture *t = map->texture;
    if (t == NULL || !t->has_file) {
        return CU_NONE;
    }
    return index_or_none(t->file_index, s->texture_files.count);
}

bool cu_material_get(const cu_scene *scene, size_t material, cu_material *out) {
    const ufbx_scene *s = as_scene(scene);
    if (s == NULL || out == NULL || material >= s->materials.count) {
        return false;
    }
    const ufbx_material *m = s->materials.data[material];
    const ufbx_material_pbr_maps *pbr = &m->pbr;
    double base = pbr->base_factor.has_value ? pbr->base_factor.value_real : 1.0;
    double opacity = pbr->opacity.has_value ? pbr->opacity.value_real : 1.0;
    out->name = str_of(m->name);
    out->base_color[0] = (float)(pbr->base_color.value_vec4.x * base);
    out->base_color[1] = (float)(pbr->base_color.value_vec4.y * base);
    out->base_color[2] = (float)(pbr->base_color.value_vec4.z * base);
    out->base_color[3] = (float)opacity;
    out->metallic = (float)pbr->metalness.value_real;
    out->roughness = (float)pbr->roughness.value_real;
    out->emissive[0] = (float)pbr->emission_color.value_vec3.x;
    out->emissive[1] = (float)pbr->emission_color.value_vec3.y;
    out->emissive[2] = (float)pbr->emission_color.value_vec3.z;
    out->emissive_strength = (float)pbr->emission_factor.value_real;
    out->base_color_texture = texture_file(s, &pbr->base_color);
    out->normal_texture = texture_file(s, &pbr->normal_map);
    out->emissive_texture = texture_file(s, &pbr->emission_color);
    return true;
}

size_t cu_texture_file_count(const cu_scene *scene) {
    const ufbx_scene *s = as_scene(scene);
    return s == NULL ? 0 : s->texture_files.count;
}

bool cu_texture_file_get(const cu_scene *scene, size_t file, cu_texture_file *out) {
    const ufbx_scene *s = as_scene(scene);
    if (s == NULL || out == NULL || file >= s->texture_files.count) {
        return false;
    }
    const ufbx_texture_file *f = &s->texture_files.data[file];
    out->relative_filename = str_of(f->relative_filename);
    out->content.data = (const uint8_t *)f->content.data;
    out->content.len = f->content.data != NULL ? f->content.size : 0;
    return true;
}

size_t cu_stack_count(const cu_scene *scene) {
    const ufbx_scene *s = as_scene(scene);
    return s == NULL ? 0 : s->anim_stacks.count;
}

bool cu_stack_get(const cu_scene *scene, size_t stack, cu_stack *out) {
    const ufbx_scene *s = as_scene(scene);
    if (s == NULL || out == NULL || stack >= s->anim_stacks.count) {
        return false;
    }
    const ufbx_anim_stack *a = s->anim_stacks.data[stack];
    out->name = str_of(a->name);
    out->time_begin = a->time_begin;
    out->time_end = a->time_end;
    return true;
}

cu_bake *cu_bake_stack(const cu_scene *scene, size_t stack, double rate, char *error, size_t error_len) {
    const ufbx_scene *s = as_scene(scene);
    if (s == NULL || stack >= s->anim_stacks.count || !(rate > 0.0)) {
        write_error(error, error_len, "no such animation stack, or a rate that is not positive");
        return NULL;
    }
    ufbx_bake_opts opts;
    memset(&opts, 0, sizeof(opts));
    opts.trim_start_time = true;
    opts.resample_rate = rate;
    ufbx_error err;
    ufbx_baked_anim *anim = ufbx_bake_anim(s, s->anim_stacks.data[stack]->anim, &opts, &err);
    if (anim == NULL) {
        char message[512];
        ufbx_format_error(message, sizeof(message), &err);
        write_error(error, error_len, message);
        return NULL;
    }
    cu_bake *bake = (cu_bake *)malloc(sizeof(cu_bake));
    if (bake == NULL) {
        ufbx_free_baked_anim(anim);
        write_error(error, error_len, "out of memory");
        return NULL;
    }
    bake->anim = anim;
    return bake;
}

void cu_bake_free(cu_bake *bake) {
    if (bake != NULL) {
        ufbx_free_baked_anim(bake->anim);
        free(bake);
    }
}

double cu_bake_duration(const cu_bake *bake) {
    return bake == NULL ? 0.0 : bake->anim->playback_duration;
}

size_t cu_bake_node_count(const cu_bake *bake) {
    return bake == NULL ? 0 : bake->anim->nodes.count;
}

static const ufbx_baked_node *baked_at(const cu_bake *bake, size_t node) {
    return (bake != NULL && node < bake->anim->nodes.count) ? &bake->anim->nodes.data[node] : NULL;
}

bool cu_bake_node_get(const cu_bake *bake, size_t node, cu_baked_node *out) {
    const ufbx_baked_node *n = baked_at(bake, node);
    if (n == NULL || out == NULL) {
        return false;
    }
    out->node = (int32_t)n->typed_id;
    out->num_keys[0] = (uint32_t)n->translation_keys.count;
    out->num_keys[1] = (uint32_t)n->rotation_keys.count;
    out->num_keys[2] = (uint32_t)n->scale_keys.count;
    return true;
}

static void vec3_keys(ufbx_baked_vec3_list keys, cu_key *out, size_t n) {
    for (size_t i = 0; i < n; i++) {
        out[i].time = (float)keys.data[i].time;
        out[i].value[0] = (float)keys.data[i].value.x;
        out[i].value[1] = (float)keys.data[i].value.y;
        out[i].value[2] = (float)keys.data[i].value.z;
        out[i].value[3] = 0.0f;
    }
}

size_t cu_bake_keys(const cu_bake *bake, size_t node, uint32_t channel, cu_key *out, size_t cap) {
    const ufbx_baked_node *n = baked_at(bake, node);
    if (n == NULL || out == NULL) {
        return 0;
    }
    if (channel == CU_CHANNEL_ROTATION) {
        size_t count = n->rotation_keys.count < cap ? n->rotation_keys.count : cap;
        for (size_t i = 0; i < count; i++) {
            ufbx_baked_quat k = n->rotation_keys.data[i];
            out[i].time = (float)k.time;
            out[i].value[0] = (float)k.value.x;
            out[i].value[1] = (float)k.value.y;
            out[i].value[2] = (float)k.value.z;
            out[i].value[3] = (float)k.value.w;
        }
        return count;
    }
    if (channel != CU_CHANNEL_TRANSLATION && channel != CU_CHANNEL_SCALE) {
        return 0;
    }
    ufbx_baked_vec3_list keys = channel == CU_CHANNEL_TRANSLATION ? n->translation_keys : n->scale_keys;
    size_t count = keys.count < cap ? keys.count : cap;
    vec3_keys(keys, out, count);
    return count;
}
