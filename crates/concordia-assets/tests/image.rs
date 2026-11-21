//! The image importer and DDC behaviour (M1 phase 1.4).

mod common;

use std::path::Path;

use common::Project;
use concordia_assets::{AssetRef, TextureFormat};

#[test]
fn png_imports_as_srgb_rgba8_with_a_full_mip_chain() {
    let mut p = Project::new();
    p.png("t.png", 16, 0);
    let id = p.db.import(Path::new("t.png")).unwrap().id;
    let tex =
        p.db.load_texture(&format!("asset:{id}").parse::<AssetRef>().unwrap())
            .unwrap();
    assert_eq!(
        (tex.width, tex.height, tex.format),
        (16, 16, TextureFormat::Rgba8Srgb)
    );
    let sizes: Vec<usize> = tex.mips.iter().map(Vec::len).collect();
    assert_eq!(sizes, [16 * 16 * 4, 8 * 8 * 4, 4 * 4 * 4, 2 * 2 * 4, 4]);
}

#[test]
fn settings_in_the_sidecar_change_the_import() {
    let mut p = Project::new();
    p.png("n.png", 4, 0);
    let id = p.db.import(Path::new("n.png")).unwrap().id;
    let sidecar = p.path("n.png.import.ron");
    let text = std::fs::read_to_string(&sidecar).unwrap();
    std::fs::write(
        &sidecar,
        text.replace("\"Srgb\"", "\"Linear\"")
            .replace("\"generate_mips\": true", "\"generate_mips\": false"),
    )
    .unwrap();
    let report = p.db.import(Path::new("n.png")).unwrap();
    assert!(!report.cached, "new settings, new key");
    let tex =
        p.db.load_texture(&format!("asset:{id}").parse().unwrap())
            .unwrap();
    assert_eq!(
        (tex.format, tex.mips.len()),
        (TextureFormat::Rgba8Linear, 1)
    );
}

#[test]
fn hdr_imports_as_rgba16_float() {
    let mut p = Project::new();
    let pixels: Vec<image::Rgb<f32>> = (0..64)
        .map(|i| image::Rgb([i as f32 * 0.25, 1.0, 4.0]))
        .collect();
    let file = std::fs::File::create(p.path("sky.hdr")).unwrap();
    image::codecs::hdr::HdrEncoder::new(file)
        .encode(&pixels, 8, 8)
        .unwrap();
    let id = p.db.import(Path::new("sky.hdr")).unwrap().id;
    let tex =
        p.db.load_texture(&format!("asset:{id}").parse().unwrap())
            .unwrap();
    assert_eq!((tex.format, tex.width), (TextureFormat::Rgba16Float, 8));
}

#[test]
fn missing_ddc_entry_triggers_reimport_not_error() {
    let mut p = Project::new();
    p.png("t.png", 8, 0);
    let id = p.db.import(Path::new("t.png")).unwrap().id;
    std::fs::remove_dir_all(p.path(".concordia/ddc")).unwrap();
    let tex =
        p.db.load_texture(&format!("asset:{id}").parse().unwrap())
            .unwrap();
    assert_eq!(tex.width, 8);
}

#[test]
fn import_is_deterministic() {
    let mut a = Project::new();
    let mut b = Project::new();
    a.png("t.png", 8, 3);
    b.png("t.png", 8, 3);
    let ta = a.db.import(Path::new("t.png")).unwrap();
    let tb = b.db.import(Path::new("t.png")).unwrap();
    let blob = |p: &Project, key: &str| {
        std::fs::read(p.path(".concordia/ddc").join(&key[..2]).join(key)).unwrap()
    };
    assert_eq!(ta.key, tb.key);
    assert_eq!(blob(&a, &ta.key.to_string()), blob(&b, &tb.key.to_string()));
}

#[test]
fn hot_reload_bumps_handle_generation_on_source_change() {
    let mut p = Project::new();
    p.png("t.png", 8, 0);
    let id = p.db.import(Path::new("t.png")).unwrap().id;
    let before = p.db.generation(id);
    p.png("t.png", 8, 200);
    let reloaded = p.db.on_files_changed(&[p.path("t.png")]);
    assert_eq!(reloaded, [id]);
    assert_eq!(p.db.generation(id), before + 1);
    assert!(p.db.on_files_changed(&[p.path("unrelated.txt")]).is_empty());
}

#[test]
fn importing_changed_content_bumps_the_generation_once() {
    let mut p = Project::new();
    p.png("t.png", 8, 0);
    let id = p.db.import(Path::new("t.png")).unwrap().id;
    let first = p.db.generation(id);
    p.db.import(Path::new("t.png")).unwrap();
    assert_eq!(p.db.generation(id), first, "unchanged content keeps it");
    p.png("t.png", 8, 90);
    p.db.import(Path::new("t.png")).unwrap();
    assert_eq!(p.db.generation(id), first + 1, "a direct import bumps it");
    p.png("t.png", 8, 180);
    p.db.on_files_changed(&[p.path("t.png")]);
    assert_eq!(p.db.generation(id), first + 2, "hot reload bumps it once");
}
