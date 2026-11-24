//! A minimal glTF 2.0 binary (GLB) writer: one mesh, one node, positions, normals, uvs (and
//! tangents when the mesh has them), 32-bit indices. Keys are sorted, so the bytes depend only
//! on the mesh.

use concordia_assets::MeshData;
use serde_json::{Value, json};

use crate::error::GenError;

const FLOAT: u32 = 5126;
const UNSIGNED_INT: u32 = 5125;
const ARRAY_BUFFER: u32 = 34962;
const ELEMENT_ARRAY_BUFFER: u32 = 34963;

/// The binary buffer and its views and accessors, built attribute by attribute.
#[derive(Default)]
struct Bin {
    bytes: Vec<u8>,
    views: Vec<Value>,
    accessors: Vec<Value>,
}

impl Bin {
    /// Appends one accessor's data (4-byte aligned) and returns its index.
    fn push(&mut self, data: &[u8], target: u32, accessor: Value) -> usize {
        while !self.bytes.len().is_multiple_of(4) {
            self.bytes.push(0);
        }
        self.views.push(json!({
            "buffer": 0, "byteOffset": self.bytes.len(), "byteLength": data.len(), "target": target
        }));
        self.bytes.extend_from_slice(data);
        let mut a = accessor;
        a["bufferView"] = json!(self.views.len() - 1);
        self.accessors.push(a);
        self.accessors.len() - 1
    }

    fn floats<const N: usize>(&mut self, values: &[[f32; N]], kind: &str, bounds: bool) -> usize {
        let data: Vec<u8> = values
            .iter()
            .flatten()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let mut accessor = json!({"componentType": FLOAT, "count": values.len(), "type": kind});
        if bounds {
            let lo: Vec<f32> = (0..N)
                .map(|i| values.iter().map(|v| v[i]).fold(f32::MAX, f32::min))
                .collect();
            let hi: Vec<f32> = (0..N)
                .map(|i| values.iter().map(|v| v[i]).fold(f32::MIN, f32::max))
                .collect();
            accessor["min"] = json!(lo);
            accessor["max"] = json!(hi);
        }
        self.push(&data, ARRAY_BUFFER, accessor)
    }
}

/// `mesh` as a GLB file.
pub fn write_glb(mesh: &MeshData) -> Result<Vec<u8>, GenError> {
    let n = mesh.positions.len();
    if n == 0 || mesh.indices.is_empty() {
        return Err(GenError::Output("the mesh is empty".into()));
    }
    if mesh.normals.len() != n
        || mesh.uv0.len() != n
        || mesh.indices.iter().any(|&i| i as usize >= n)
    {
        return Err(GenError::Output(
            "the mesh's attributes don't line up".into(),
        ));
    }
    let mut bin = Bin::default();
    let mut attributes = json!({
        "POSITION": bin.floats(&mesh.positions, "VEC3", true),
        "NORMAL": bin.floats(&mesh.normals, "VEC3", false),
        "TEXCOORD_0": bin.floats(&mesh.uv0, "VEC2", false),
    });
    if mesh.tangents.len() == n {
        attributes["TANGENT"] = json!(bin.floats(&mesh.tangents, "VEC4", false));
    }
    let index_bytes: Vec<u8> = mesh.indices.iter().flat_map(|i| i.to_le_bytes()).collect();
    let indices = bin.push(
        &index_bytes,
        ELEMENT_ARRAY_BUFFER,
        json!({"componentType": UNSIGNED_INT, "count": mesh.indices.len(), "type": "SCALAR"}),
    );
    while !bin.bytes.len().is_multiple_of(4) {
        bin.bytes.push(0);
    }
    let doc = json!({
        "asset": {"version": "2.0", "generator": "concordia-gen"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"mesh": 0, "name": "mesh"}],
        "meshes": [{"name": "mesh", "primitives": [{"attributes": attributes, "indices": indices, "mode": 4}]}],
        "accessors": bin.accessors,
        "bufferViews": bin.views,
        "buffers": [{"byteLength": bin.bytes.len()}],
    });
    let mut text = serde_json::to_vec(&doc).map_err(|e| GenError::Output(e.to_string()))?;
    while !text.len().is_multiple_of(4) {
        text.push(b' ');
    }
    let total = 12 + 8 + text.len() + 8 + bin.bytes.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(text.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&text);
    out.extend_from_slice(&(bin.bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(b"BIN\0");
    out.extend_from_slice(&bin.bytes);
    Ok(out)
}
