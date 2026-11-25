//! `MeshletBlob` v1: the GPU-ready encoded form of a [`MeshletMesh`], and the build's cache key.
//!
//! Little-endian. A 48-byte header (magic `CMLT`, version, max triangles u16, max vertices u16,
//! cone weight f32, meshlet count, vertex-reference count, triangle bytes, source triangles,
//! source vertices, 12 zero bytes), then 32-byte meshlet records (center f32×3, radius f32,
//! cone axis i8×3, cutoff i8, vertex offset u32, triangle offset u32, vertex count u8, triangle
//! count u8, material u16), then vertex references (u32), then triangle runs (u8×3 per
//! triangle, each run 4-byte aligned with zero padding). The layout is canonical: offsets are
//! consecutive, so a valid blob re-encodes to the same bytes.

use concordia_core::ddc::DdcKey;

use crate::{
    ClusterError, ClusterInput, Meshlet, MeshletBuildSettings, MeshletMesh, MeshletSize,
    NormalCone, Sphere,
};

/// The encoded format's version (bumped on any layout change).
pub const FORMAT_VERSION: u32 = 1;
/// The build algorithm's version (piles, bounds fix-ups); bumped whenever output changes.
const BUILD_VERSION: u32 = 1;
const MAGIC: &[u8; 4] = b"CMLT";
const HEADER_BYTES: usize = 48;
const RECORD_BYTES: usize = 32;

impl MeshletMesh {
    /// Encodes to `MeshletBlob` v1, refusing a mesh that isn't in the canonical layout.
    pub fn encode(&self) -> Result<Vec<u8>, ClusterError> {
        self.validate()?;
        let mut out = Vec::with_capacity(
            HEADER_BYTES
                + RECORD_BYTES * self.meshlets.len()
                + 4 * self.vertices.len()
                + self.triangles.len(),
        );
        write_header(self, &mut out);
        for m in &self.meshlets {
            write_record(m, &mut out);
        }
        for v in &self.vertices {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&self.triangles);
        Ok(out)
    }

    /// Decodes and fully validates `MeshletBlob` v1.
    pub fn decode(bytes: &[u8]) -> Result<Self, ClusterError> {
        let mut r = Reader { bytes, at: 0 };
        let (mut mesh, counts) = read_header(&mut r)?;
        let expected = RECORD_BYTES as u64 * u64::from(counts.meshlets)
            + 4 * u64::from(counts.vertices)
            + u64::from(counts.triangle_bytes);
        if (bytes.len() - HEADER_BYTES) as u64 != expected {
            return Err(invalid(format!(
                "{} bytes after the header, the header says {expected}",
                bytes.len() - HEADER_BYTES
            )));
        }
        mesh.meshlets = (0..counts.meshlets)
            .map(|_| read_record(&mut r))
            .collect::<Result<_, _>>()?;
        mesh.vertices = (0..counts.vertices)
            .map(|_| r.u32())
            .collect::<Result<_, _>>()?;
        mesh.triangles = r.take(counts.triangle_bytes as usize)?.to_vec();
        mesh.validate()?;
        Ok(mesh)
    }

    /// Checks the canonical layout and every range (shared by encode and decode).
    fn validate(&self) -> Result<(), ClusterError> {
        let (mut v, mut t, mut triangles, mut material) = (0usize, 0usize, 0u64, 0u16);
        for (i, m) in self.meshlets.iter().enumerate() {
            validate_meshlet(self, m, (v, t, material))
                .map_err(|e| invalid(format!("meshlet {i}: {e}")))?;
            v += usize::from(m.vertex_count);
            t += (3 * usize::from(m.triangle_count)).next_multiple_of(4);
            triangles += u64::from(m.triangle_count);
            material = m.material;
        }
        if v != self.vertices.len() || t != self.triangles.len() {
            return Err(invalid(format!(
                "meshlets cover {v} references and {t} triangle bytes, not {} and {}",
                self.vertices.len(),
                self.triangles.len()
            )));
        }
        if triangles != u64::from(self.source_triangles) {
            return Err(invalid(format!(
                "{triangles} triangles in meshlets, {} in the source",
                self.source_triangles
            )));
        }
        match self.vertices.iter().find(|&&r| r >= self.source_vertices) {
            Some(r) => Err(invalid(format!(
                "vertex reference {r} is past {} source vertices",
                self.source_vertices
            ))),
            None => Ok(()),
        }
    }
}

/// One meshlet at running offsets `(v, t, previous material)`.
fn validate_meshlet(
    mesh: &MeshletMesh,
    m: &Meshlet,
    (v, t, material): (usize, usize, u16),
) -> Result<(), String> {
    let s = &mesh.settings;
    if m.vertex_offset as usize != v || m.triangle_offset as usize != t {
        return Err("offsets are not consecutive".into());
    }
    if m.vertex_count == 0 || u32::from(m.vertex_count) > s.max_vertices() {
        return Err(format!("{} vertices", m.vertex_count));
    }
    if m.triangle_count == 0 || u32::from(m.triangle_count) > s.size().triangles() {
        return Err(format!("{} triangles", m.triangle_count));
    }
    if m.material < material {
        return Err("materials are not ascending".into());
    }
    let b = m.bounds;
    if !(b.center.iter().all(|c| c.is_finite()) && b.radius.is_finite() && b.radius >= 0.0) {
        return Err(format!("bad sphere {b:?}"));
    }
    let run = t..t + (3 * usize::from(m.triangle_count)).next_multiple_of(4);
    let Some(bytes) = mesh.triangles.get(run) else {
        return Err("triangles run past the end".into());
    };
    let (corners, padding) = bytes.split_at(3 * usize::from(m.triangle_count));
    if corners.iter().any(|&c| c >= m.vertex_count) {
        return Err("a corner is past its vertices".into());
    }
    if padding.iter().any(|&p| p != 0) {
        return Err("non-zero padding".into());
    }
    Ok(())
}

fn invalid(message: String) -> ClusterError {
    ClusterError::Decode(message)
}

/// The header's counts.
struct Counts {
    meshlets: u32,
    vertices: u32,
    triangle_bytes: u32,
}

fn write_header(mesh: &MeshletMesh, out: &mut Vec<u8>) {
    let s = &mesh.settings;
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(&(s.size().triangles() as u16).to_le_bytes());
    out.extend_from_slice(&(s.max_vertices() as u16).to_le_bytes());
    out.extend_from_slice(&s.cone_weight().to_le_bytes());
    for word in [
        mesh.meshlets.len() as u32,
        mesh.vertices.len() as u32,
        mesh.triangles.len() as u32,
        mesh.source_triangles,
        mesh.source_vertices,
    ] {
        out.extend_from_slice(&word.to_le_bytes());
    }
    out.resize(HEADER_BYTES, 0);
}

fn read_header(r: &mut Reader<'_>) -> Result<(MeshletMesh, Counts), ClusterError> {
    if r.bytes.len() < HEADER_BYTES {
        return Err(invalid(format!(
            "{} bytes is shorter than the header",
            r.bytes.len()
        )));
    }
    if r.take(4)? != MAGIC {
        return Err(invalid("not meshlet data (bad magic)".into()));
    }
    let version = r.u32()?;
    if version != FORMAT_VERSION {
        return Err(invalid(format!(
            "format version {version}, expected {FORMAT_VERSION}"
        )));
    }
    let size = match r.u16()? {
        64 => MeshletSize::S64,
        128 => MeshletSize::S128,
        other => return Err(invalid(format!("{other} triangles per meshlet"))),
    };
    let max_vertices = u32::from(r.u16()?);
    let settings = MeshletBuildSettings::new(size, max_vertices, r.f32()?)
        .map_err(|e| invalid(e.to_string()))?;
    let counts = Counts {
        meshlets: r.u32()?,
        vertices: r.u32()?,
        triangle_bytes: r.u32()?,
    };
    let mesh = MeshletMesh {
        source_triangles: r.u32()?,
        source_vertices: r.u32()?,
        settings,
        ..MeshletMesh::default()
    };
    if r.take(HEADER_BYTES - r.at)?.iter().any(|&b| b != 0) {
        return Err(invalid("reserved header bytes are not zero".into()));
    }
    Ok((mesh, counts))
}

fn write_record(m: &Meshlet, out: &mut Vec<u8>) {
    for f in m.bounds.center.iter().chain([&m.bounds.radius]) {
        out.extend_from_slice(&f.to_le_bytes());
    }
    out.extend(m.cone.axis.iter().chain([&m.cone.cutoff]).map(|&a| a as u8));
    out.extend_from_slice(&m.vertex_offset.to_le_bytes());
    out.extend_from_slice(&m.triangle_offset.to_le_bytes());
    out.extend_from_slice(&[m.vertex_count, m.triangle_count]);
    out.extend_from_slice(&m.material.to_le_bytes());
}

fn read_record(r: &mut Reader<'_>) -> Result<Meshlet, ClusterError> {
    let bounds = Sphere {
        center: [r.f32()?, r.f32()?, r.f32()?],
        radius: r.f32()?,
    };
    let c = r.take(4)?;
    let cone = NormalCone {
        axis: [c[0] as i8, c[1] as i8, c[2] as i8],
        cutoff: c[3] as i8,
    };
    let vertex_offset = r.u32()?;
    let triangle_offset = r.u32()?;
    let counts = r.take(2)?;
    Ok(Meshlet {
        bounds,
        cone,
        vertex_offset,
        triangle_offset,
        vertex_count: counts[0],
        triangle_count: counts[1],
        material: r.u16()?,
    })
}

/// A bounds-checked little-endian cursor.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], ClusterError> {
        let end = self.at.checked_add(n).filter(|&e| e <= self.bytes.len());
        let end = end.ok_or_else(|| invalid("data ends early".into()))?;
        let out = &self.bytes[self.at..end];
        self.at = end;
        Ok(out)
    }

    fn u16(&mut self) -> Result<u16, ClusterError> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().expect("2 bytes"),
        ))
    }

    fn u32(&mut self) -> Result<u32, ClusterError> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("4 bytes"),
        ))
    }

    fn f32(&mut self) -> Result<f32, ClusterError> {
        Ok(f32::from_bits(self.u32()?))
    }
}

/// The DDC key of building `input` with `settings`: only what the build reads (positions,
/// indices, per-triangle materials with none meaning all 0), the settings and the versions of
/// the format, the build and meshoptimizer. Platform-independent: the output is the same on
/// every OS (`output_hash_is_pinned`).
pub fn cache_key(input: &ClusterInput<'_>, settings: &MeshletBuildSettings) -> DdcKey {
    let positions: Vec<u8> = input
        .positions()
        .iter()
        .flatten()
        .flat_map(|f| f.to_le_bytes())
        .collect();
    let indices: Vec<u8> = input
        .indices()
        .iter()
        .flat_map(|i| i.to_le_bytes())
        .collect();
    let materials: Vec<u8> = (0..input.triangle_count())
        .flat_map(|t| input.material(t).to_le_bytes())
        .collect();
    let settings = [
        settings.size().triangles().to_le_bytes(),
        settings.max_vertices().to_le_bytes(),
        settings.cone_weight().to_le_bytes(),
    ]
    .concat();
    DdcKey::of(&[
        b"concordia-meshlets",
        &FORMAT_VERSION.to_le_bytes(),
        &BUILD_VERSION.to_le_bytes(),
        meshopt_sys::MESHOPTIMIZER_VERSION.as_bytes(),
        &settings,
        &positions,
        &indices,
        &materials,
    ])
}
