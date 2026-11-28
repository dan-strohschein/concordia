/* Concordia's C shim over vendored Manifold (M8 8.1a, ADR-0180): clipping a closed mesh by a
 * convex cell, the operation fracture is made of. Every function validates its input. */
#pragma once
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct CmPiece CmPiece;

typedef enum CmStatus {
    CM_OK = 0,
    CM_INVALID_INPUT = 1, /* null pointers, empty mesh, out-of-range index, non-finite value, zero normal */
    CM_NOT_MANIFOLD = 2,  /* the mesh isn't closed and manifold */
    CM_FAILED = 3,        /* Manifold failed (an exception) */
} CmStatus;

/* Clips the closed mesh (3 floats per vertex, 3 indices per triangle) by the convex cell of
 * `plane_count` half-spaces n . x <= d (4 floats each: nx, ny, nz, d; n need not be unit). On
 * CM_OK, *out holds the piece (free it with cm_piece_free); otherwise *out is null. */
CmStatus cm_clip_convex(const float* positions, uint32_t vertex_count, const uint32_t* triangles,
                        uint32_t triangle_count, const float* planes, uint32_t plane_count, CmPiece** out);

uint32_t cm_piece_vertex_count(const CmPiece* piece);
uint32_t cm_piece_triangle_count(const CmPiece* piece);
double cm_piece_volume(const CmPiece* piece);

/* Copies the piece: 3 floats per vertex, 3 indices per triangle, and per triangle 1 if it lies
 * on a cut (an interior face) or 0 if it comes from the mesh's surface. */
void cm_piece_copy(const CmPiece* piece, float* positions, uint32_t* triangles, uint8_t* interior);

void cm_piece_free(CmPiece* piece);

#ifdef __cplusplus
}
#endif
