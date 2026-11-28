// Concordia's C shim over vendored Manifold (M8 8.1a, ADR-0180).
#include "concordia_manifold.h"

#include <cmath>
#include <vector>

#include <manifold/manifold.h>

using manifold::Manifold;
using manifold::MeshGL;
using manifold::vec3;

struct CmPiece {
    std::vector<float> positions;
    std::vector<uint32_t> triangles;
    std::vector<uint8_t> interior;
    double volume = 0.0;
};

namespace {
bool finite(float x) { return std::isfinite(x) && std::abs(x) < 1.0e9f; }

/// The piece of `m` whose triangles came from anything but `source` are interior.
CmPiece* piece_of(const Manifold& m, int source) {
    MeshGL mesh = m.GetMeshGL();
    auto* piece = new CmPiece;
    piece->volume = m.Volume();
    size_t vertices = mesh.numProp == 0 ? 0 : mesh.vertProperties.size() / mesh.numProp;
    for (size_t v = 0; v < vertices; ++v)
        for (size_t k = 0; k < 3; ++k) piece->positions.push_back(mesh.vertProperties[v * mesh.numProp + k]);
    piece->triangles.assign(mesh.triVerts.begin(), mesh.triVerts.end());
    size_t count = mesh.triVerts.size() / 3;
    piece->interior.assign(count, 0);
    for (size_t r = 0; r + 1 < mesh.runIndex.size() && r < mesh.runOriginalID.size(); ++r) {
        uint8_t cut = static_cast<int>(mesh.runOriginalID[r]) != source ? 1 : 0;
        for (size_t t = mesh.runIndex[r] / 3; t < mesh.runIndex[r + 1] / 3 && t < count; ++t) piece->interior[t] = cut;
    }
    return piece;
}
}  // namespace

CmStatus cm_clip_convex(const float* positions, uint32_t vertex_count, const uint32_t* triangles,
                        uint32_t triangle_count, const float* planes, uint32_t plane_count, CmPiece** out) {
    if (out == nullptr) return CM_INVALID_INPUT;
    *out = nullptr;
    if (positions == nullptr || triangles == nullptr || vertex_count == 0 || triangle_count == 0 ||
        (plane_count > 0 && planes == nullptr))
        return CM_INVALID_INPUT;
    for (size_t i = 0; i < 3 * size_t(vertex_count); ++i)
        if (!finite(positions[i])) return CM_INVALID_INPUT;
    for (size_t i = 0; i < 3 * size_t(triangle_count); ++i)
        if (triangles[i] >= vertex_count) return CM_INVALID_INPUT;
    for (size_t p = 0; p < plane_count; ++p) {
        const float* q = planes + 4 * p;
        if (!finite(q[0]) || !finite(q[1]) || !finite(q[2]) || !finite(q[3])) return CM_INVALID_INPUT;
        if (!(q[0] * q[0] + q[1] * q[1] + q[2] * q[2] > 1.0e-12f)) return CM_INVALID_INPUT;
    }
    try {
        MeshGL mesh;
        mesh.numProp = 3;
        mesh.vertProperties.assign(positions, positions + 3 * size_t(vertex_count));
        mesh.triVerts.assign(triangles, triangles + 3 * size_t(triangle_count));
        Manifold m(mesh);
        if (m.Status() != Manifold::Error::NoError) return CM_NOT_MANIFOLD;
        m = m.AsOriginal();
        int source = m.OriginalID();
        for (size_t p = 0; p < plane_count; ++p) {
            const float* q = planes + 4 * p;
            double length = std::sqrt(double(q[0]) * q[0] + double(q[1]) * q[1] + double(q[2]) * q[2]);
            // Keep n . x <= d: the side of -n, at distance -d / |n| along it.
            m = m.TrimByPlane(vec3(-q[0] / length, -q[1] / length, -q[2] / length), -q[3] / length);
        }
        *out = piece_of(m, source);
        return CM_OK;
    } catch (...) {
        return CM_FAILED;
    }
}

uint32_t cm_piece_vertex_count(const CmPiece* piece) {
    return piece == nullptr ? 0 : static_cast<uint32_t>(piece->positions.size() / 3);
}

uint32_t cm_piece_triangle_count(const CmPiece* piece) {
    return piece == nullptr ? 0 : static_cast<uint32_t>(piece->triangles.size() / 3);
}

double cm_piece_volume(const CmPiece* piece) { return piece == nullptr ? 0.0 : piece->volume; }

void cm_piece_copy(const CmPiece* piece, float* positions, uint32_t* triangles, uint8_t* interior) {
    if (piece == nullptr) return;
    if (positions != nullptr) std::copy(piece->positions.begin(), piece->positions.end(), positions);
    if (triangles != nullptr) std::copy(piece->triangles.begin(), piece->triangles.end(), triangles);
    if (interior != nullptr) std::copy(piece->interior.begin(), piece->interior.end(), interior);
}

void cm_piece_free(CmPiece* piece) { delete piece; }
