//! A temporary project with generated images and an asset database.
#![allow(dead_code, missing_docs)]

use std::path::{Path, PathBuf};

use concordia_assets::AssetDb;
use concordia_core::id::DeterministicIdGen;

pub struct Project {
    pub dir: tempfile::TempDir,
    pub db: AssetDb,
}

impl Project {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let db = AssetDb::new(dir.path(), Box::new(DeterministicIdGen::new(5))).unwrap();
        Self { dir, db }
    }

    pub fn path(&self, rel: &str) -> PathBuf {
        self.dir.path().join(rel)
    }

    /// Writes a `size`×`size` RGBA gradient PNG at `rel`.
    pub fn png(&self, rel: &str, size: u32, shift: u8) {
        write_png(&self.path(rel), size, shift);
    }
}

pub fn write_png(path: &Path, size: u32, shift: u8) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let pixels: Vec<u8> = (0..size * size)
        .flat_map(|i| [(i % size) as u8 * 8, (i / size) as u8 * 8, shift, 255])
        .collect();
    image::save_buffer(path, &pixels, size, size, image::ColorType::Rgba8).unwrap();
}
