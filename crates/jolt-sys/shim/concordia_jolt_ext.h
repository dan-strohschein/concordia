/* Concordia's C API over Jolt Physics (crates/jolt-sys, ADR-0041). Positions are single precision. */
#ifndef CONCORDIA_JOLT_EXT_H
#define CONCORDIA_JOLT_EXT_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Jolt's version (from Jolt/Core/Core.h). */
void concordia_jolt_version(uint32_t* major, uint32_t* minor, uint32_t* patch);

/* Registers types and routes Jolt's allocations through counters. Idempotent and thread-safe. */
bool concordia_jolt_init(void);

/* Jolt heap blocks allocated and not yet freed since concordia_jolt_init. */
int64_t concordia_jolt_live_allocations(void);

/* Number of user collision layers; each body is on one. */
#define CJ_LAYERS 16

typedef struct CjWorld CjWorld;
typedef struct CjCharacter CjCharacter;

typedef struct CjWorldDesc {
    uint32_t max_bodies;
    /* Total threads used by a step, including the caller (1 = no workers). */
    uint32_t threads;
    float gravity[3];
    /* collide[i] has bit j set if layer i collides with layer j (kept symmetric by the caller). */
    uint16_t collide[CJ_LAYERS];
} CjWorldDesc;

typedef enum CjShapeKind {
    CJ_SHAPE_BOX = 0,      /* dims = half extents */
    CJ_SHAPE_SPHERE = 1,   /* dims[0] = radius */
    CJ_SHAPE_CAPSULE = 2,  /* dims[0] = half height of the cylinder part, dims[1] = radius */
    CJ_SHAPE_CYLINDER = 3, /* dims[0] = half height, dims[1] = radius */
    CJ_SHAPE_CONVEX_HULL = 4, /* points (x, y, z) in body space, point_count of them */
    CJ_SHAPE_COMPOUND = 5,    /* convex hulls: part i is points [part_ends[i-1], part_ends[i]) */
} CjShapeKind;

typedef enum CjMotion {
    CJ_MOTION_STATIC = 0,
    CJ_MOTION_KINEMATIC = 1,
    CJ_MOTION_DYNAMIC = 2,
} CjMotion;

typedef struct CjBodyDesc {
    uint64_t user;
    CjShapeKind shape;
    float dims[3];
    float offset[3];
    float position[3];
    float rotation[4]; /* x, y, z, w */
    float linear_velocity[3];
    float angular_velocity[3];
    CjMotion motion;
    uint8_t layer;
    bool sensor;
    bool ccd;
    float mass; /* <= 0: from shape density */
    float friction;
    float restitution;
    float linear_damping;
    float angular_damping;
    float gravity_scale;
    const float* points;       /* CONVEX_HULL and COMPOUND: 3 floats per point (else null) */
    uint32_t point_count;
    const uint32_t* part_ends; /* COMPOUND: increasing end indices into points (else null) */
    uint32_t part_count;
} CjBodyDesc;

typedef struct CjBodyState {
    float position[3];
    float rotation[4];
    float linear_velocity[3];
    float angular_velocity[3];
} CjBodyState;

typedef struct CjContactEvent {
    uint64_t a; /* user value of the body with the lower Jolt id */
    uint64_t b;
    bool started; /* false: ended */
    bool sensor;
    /* Started only: closing speed along the normal times the pair's reduced mass (N s), and the
     * first contact point (world space). */
    float strength;
    float point[3];
} CjContactEvent;

typedef struct CjRayHit {
    uint64_t user;
    float fraction;
    float point[3];
    float normal[3];
} CjRayHit;

typedef struct CjCharacterDesc {
    float height; /* total capsule height */
    float radius;
    float max_slope_radians;
    float step_height;
    float position[3]; /* feet */
    uint8_t layer;
    uint64_t user; /* user data of its inner body, which only sensors see */
} CjCharacterDesc;

#define CJ_NO_BODY 0xffffffffu

CjWorld* cj_world_create(const CjWorldDesc* desc);
void cj_world_destroy(CjWorld* world);
/* Returns a body id, or CJ_NO_BODY if the desc is invalid or the world is full. */
uint32_t cj_body_add(CjWorld* world, const CjBodyDesc* desc);
void cj_body_remove(CjWorld* world, uint32_t body);
bool cj_body_state(CjWorld* world, uint32_t body, CjBodyState* out);
void cj_body_move_kinematic(CjWorld* world, uint32_t body, const float position[3], const float rotation[4], float dt);
/* Returns Jolt's EPhysicsUpdateError bits (0 = ok). */
uint32_t cj_world_step(CjWorld* world, float dt, int32_t collision_steps);
/* Copies up to cap pending begin/end events into out (unordered), removes them, and returns how many were copied. */
size_t cj_world_drain_events(CjWorld* world, CjContactEvent* out, size_t cap);
size_t cj_world_pending_events(CjWorld* world);
/* Nearest hit along origin + t * dir_len for t in [0, 1] on layers in layer_mask. */
/* The triangles of the convex hull of `count` points (3 floats each), 9 floats per triangle into
 * `out` (room for `cap` triangles). Returns the hull's triangle count (only the first `cap` are
 * written), or 0 if the points have no hull (fewer than 4, all in a plane, not finite). */
size_t cj_convex_hull_triangles(const float* points, uint32_t count, float* out, size_t cap);

/* A soft body (M8 8.0d; the cloth solver's test reference): vertices at rest in world space,
 * per-vertex inverse masses (0 pins a vertex), triangles (3 indices each). Stretch, shear and bend
 * constraints come from the faces with these compliances (bend < 0: none). */
typedef struct CjSoftBodyDesc {
    const float* positions;
    const float* inv_masses;
    uint32_t vertex_count;
    const uint32_t* faces;
    uint32_t face_count;
    float compliance;
    float shear_compliance;
    float bend_compliance;
    uint32_t iterations;
    float gravity_factor;
} CjSoftBodyDesc;

/* Adds a soft body; CJ_NO_BODY if the desc is invalid (no vertices, an index out of range,
 * non-finite values). */
uint32_t cj_soft_body_add(CjWorld* world, const CjSoftBodyDesc* desc);

/* The soft body's vertex positions in world space into `out` (3 floats each, room for `cap`);
 * returns its vertex count, or 0 if `body` isn't a soft body. */
size_t cj_soft_body_positions(CjWorld* world, uint32_t body, float* out, size_t cap);

bool cj_raycast(CjWorld* world, const float origin[3], const float dir_len[3], uint16_t layer_mask, CjRayHit* out);
/* Serializes body state; returns the size needed (copies only if cap is large enough). */
size_t cj_world_save(CjWorld* world, uint8_t* out, size_t cap);
bool cj_world_restore(CjWorld* world, const uint8_t* data, size_t len);

CjCharacter* cj_character_create(CjWorld* world, const CjCharacterDesc* desc);
void cj_character_destroy(CjCharacter* character);
/* Moves with the given horizontal velocity, walking stairs up to step_height; applies gravity while airborne. */
void cj_character_move(CjWorld* world, CjCharacter* character, const float velocity[3], float dt);
void cj_character_position(const CjCharacter* character, float out[3]);
bool cj_character_on_ground(const CjCharacter* character);

#ifdef __cplusplus
}
#endif

#endif
