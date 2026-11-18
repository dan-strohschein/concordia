//! Buffer and image bytes: GLB blob, data URIs or files next to the source.

use base64::Engine as _;

/// Bytes of every buffer, by index.
pub(crate) fn buffers(
    gltf: &gltf::Gltf,
    read_relative: &dyn Fn(&str) -> Result<Vec<u8>, concordia_assets::AssetError>,
) -> Result<Vec<Vec<u8>>, String> {
    gltf.document
        .buffers()
        .map(|buffer| match buffer.source() {
            gltf::buffer::Source::Bin => gltf
                .blob
                .clone()
                .ok_or_else(|| "GLB has no binary chunk".to_owned()),
            gltf::buffer::Source::Uri(uri) => uri_bytes(uri, read_relative),
        })
        .collect()
}

/// Bytes of a `data:` URI or a relative file.
pub(crate) fn uri_bytes(
    uri: &str,
    read_relative: &dyn Fn(&str) -> Result<Vec<u8>, concordia_assets::AssetError>,
) -> Result<Vec<u8>, String> {
    match uri.strip_prefix("data:") {
        Some(rest) => {
            let (_, data) = rest
                .split_once(";base64,")
                .ok_or_else(|| format!("unsupported data URI {}…", &uri[..uri.len().min(40)]))?;
            base64::engine::general_purpose::STANDARD
                .decode(data)
                .map_err(|e| format!("base64: {e}"))
        }
        None => read_relative(&percent_decode(uri)).map_err(|e| e.to_string()),
    }
}

fn percent_decode(uri: &str) -> String {
    let bytes = uri.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match (bytes[i], bytes.get(i + 1..i + 3)) {
            (b'%', Some(hex)) => {
                match u8::from_str_radix(std::str::from_utf8(hex).unwrap_or("zz"), 16) {
                    Ok(b) => {
                        out.push(b);
                        i += 3;
                        continue;
                    }
                    Err(_) => out.push(b'%'),
                }
            }
            (b, _) => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
