//! The asset database: sidecars, imports through the DDC, loads, listing and hot reload.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use concordia_core::ddc::{Ddc, DdcKey};
use concordia_core::id::{AssetId, IdSource, TypedId};
use concordia_reflect::Reflect;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::builtin::{Builtin, BuiltinAsset};
use crate::data::{MaterialData, MeshData, TextureData};
use crate::error::{AssetError, io};
use crate::image_importer::ImageImporter;
use crate::importer::{AssetKind, ImportInput, Importer, SubIds, encode};
use crate::key::{PLATFORM, artifact_key, import_key};
use crate::reference::AssetRef;
use crate::sidecar::Sidecar;

/// What an import produced.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportReport {
    /// The asset id (from the sidecar).
    pub id: AssetId,
    /// The DDC key of this import.
    pub key: DdcKey,
    /// True if the DDC already had the result.
    pub cached: bool,
    /// Sub-asset ids by path.
    pub sub_assets: BTreeMap<String, AssetId>,
}

/// One known asset (for `asset.list` and pickers).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
pub struct AssetInfo {
    /// Asset id.
    pub id: AssetId,
    /// Project-relative source path.
    pub path: String,
    /// Importer name.
    pub importer: String,
    /// Sub-asset paths (refer to them as `asset:<id>#<path>`).
    pub sub_assets: Vec<String>,
}

/// The artifact list stored under an import's own key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ImportIndex {
    artifacts: Vec<(String, AssetKind)>,
}

#[derive(Debug, Clone)]
struct Record {
    path: PathBuf,
    sidecar: Sidecar,
}

/// A project's assets.
pub struct AssetDb {
    root: PathBuf,
    ddc: Ddc,
    ids: Box<dyn IdSource + Send>,
    importers: Vec<Arc<dyn Importer>>,
    records: BTreeMap<AssetId, Record>,
    generations: BTreeMap<AssetId, u64>,
    /// Each asset's import key as of its last import (sources are not re-hashed per load).
    keys: BTreeMap<AssetId, DdcKey>,
    /// Small artifacts already read, by asset and sub-asset.
    memory: ArtifactMemory,
}

/// An artifact's kind and bytes.
type KeptArtifact = (AssetKind, Arc<[u8]>);

/// Recently loaded artifacts kept in memory (small ones only, within a total budget).
#[derive(Default)]
struct ArtifactMemory {
    artifacts: BTreeMap<(AssetId, String), KeptArtifact>,
    bytes: usize,
}

impl ArtifactMemory {
    /// Largest artifact kept.
    const MAX_ARTIFACT: usize = 4 << 20;
    /// Total kept before the memory is emptied.
    const BUDGET: usize = 256 << 20;

    fn get(&self, id: AssetId, sub: &str) -> Option<(AssetKind, Vec<u8>)> {
        let (kind, bytes) = self.artifacts.get(&(id, sub.to_owned()))?;
        Some((*kind, bytes.to_vec()))
    }

    fn keep(&mut self, id: AssetId, sub: &str, kind: AssetKind, bytes: &[u8]) {
        if bytes.len() > Self::MAX_ARTIFACT {
            return;
        }
        if self.bytes + bytes.len() > Self::BUDGET {
            self.artifacts.clear();
            self.bytes = 0;
        }
        self.bytes += bytes.len();
        self.artifacts
            .insert((id, sub.to_owned()), (kind, Arc::from(bytes)));
    }

    fn forget(&mut self, id: AssetId) {
        let stale: Vec<_> = self
            .artifacts
            .keys()
            .filter(|(a, _)| *a == id)
            .cloned()
            .collect();
        for key in stale {
            if let Some((_, b)) = self.artifacts.remove(&key) {
                self.bytes -= b.len();
            }
        }
    }
}

impl std::fmt::Debug for AssetDb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AssetDb")
            .field("root", &self.root)
            .field("assets", &self.records.len())
            .finish()
    }
}

impl AssetDb {
    /// The database of the project at `root` (DDC at `.concordia/ddc`), with the image importer, scanned.
    pub fn new(root: &Path, ids: Box<dyn IdSource + Send>) -> Result<Self, AssetError> {
        let mut db = Self {
            root: root.to_path_buf(),
            ddc: Ddc::new(root.join(".concordia/ddc")),
            ids,
            importers: vec![Arc::new(ImageImporter)],
            records: BTreeMap::new(),
            generations: BTreeMap::new(),
            keys: BTreeMap::new(),
            memory: ArtifactMemory::default(),
        };
        db.scan()?;
        Ok(db)
    }

    /// Adds an importer (later registrations win for shared extensions).
    pub fn register(&mut self, importer: Arc<dyn Importer>) {
        self.importers.insert(0, importer);
    }

    /// Rebuilds the id → path index from every sidecar in the project.
    pub fn scan(&mut self) -> Result<(), AssetError> {
        self.records.clear();
        let mut stack = vec![self.root.clone()];
        while let Some(dir) = stack.pop() {
            let entries = std::fs::read_dir(&dir).map_err(|e| io(&dir, e))?;
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().into_owned();
                if path.is_dir() {
                    if !matches!(
                        name.as_str(),
                        ".concordia" | "target" | ".git" | "node_modules"
                    ) {
                        stack.push(path);
                    }
                } else if let Some(source) = name.strip_suffix(".import.ron") {
                    let sidecar = Sidecar::read(&path)?;
                    let rel = path.parent().unwrap_or(&self.root).join(source);
                    let rel = rel.strip_prefix(&self.root).unwrap_or(&rel).to_path_buf();
                    self.records
                        .insert(sidecar.id, Record { path: rel, sidecar });
                }
            }
        }
        Ok(())
    }

    /// Imports `path` (project-relative): creates or updates its sidecar and fills the DDC.
    pub fn import(&mut self, path: &Path) -> Result<ImportReport, AssetError> {
        let rel = relative(path)?;
        let abs = self.root.join(&rel);
        let bytes = std::fs::read(&abs).map_err(|e| io(&abs, e))?;
        let importer = self.importer_for(&rel)?;
        let sidecar_path = Sidecar::path_for(&abs);
        let mut sidecar = match sidecar_path.exists() {
            true => Sidecar::read(&sidecar_path)?,
            false => self.new_sidecar(importer.as_ref())?,
        };
        let key = import_key(
            importer.name(),
            importer.version(),
            PLATFORM,
            &sidecar.settings,
            &bytes,
        );
        let cached = self
            .ddc
            .get(&key)
            .map_err(|e| AssetError::Io(e.to_string()))?
            .is_some();
        if !cached {
            sidecar.sub_assets = self.run(importer.as_ref(), &rel, &bytes, &sidecar, &key)?;
        }
        sidecar.write(&sidecar_path)?;
        let report = ImportReport {
            id: sidecar.id,
            key,
            cached,
            sub_assets: sidecar.sub_assets.clone(),
        };
        self.keys.insert(sidecar.id, report.key);
        self.memory.forget(sidecar.id);
        self.records
            .insert(sidecar.id, Record { path: rel, sidecar });
        Ok(report)
    }

    fn new_sidecar(&mut self, importer: &dyn Importer) -> Result<Sidecar, AssetError> {
        let uuid = self
            .ids
            .next_uuid()
            .map_err(|e| AssetError::Io(e.to_string()))?;
        let id = AssetId::try_from_uuid(uuid).map_err(|e| AssetError::Io(e.to_string()))?;
        Ok(Sidecar {
            id,
            importer: importer.name().to_owned(),
            importer_version: importer.version(),
            settings: importer.default_settings(),
            sub_assets: BTreeMap::new(),
        })
    }

    /// Runs the importer and stores its artifacts plus their index; returns the sub-asset ids.
    fn run(
        &self,
        importer: &dyn Importer,
        rel: &Path,
        bytes: &[u8],
        sidecar: &Sidecar,
        key: &DdcKey,
    ) -> Result<BTreeMap<String, AssetId>, AssetError> {
        let dir = self.root.join(rel.parent().unwrap_or(Path::new("")));
        let read_relative = move |name: &str| -> Result<Vec<u8>, AssetError> {
            let file = dir.join(relative(Path::new(name))?);
            std::fs::read(&file).map_err(|e| io(&file, e))
        };
        let mut sub_ids = SubIds::new(sidecar.id, sidecar.sub_assets.clone());
        let output = importer.import(ImportInput {
            path: rel,
            bytes,
            settings: &sidecar.settings,
            read_relative: &read_relative,
            sub_ids: &mut sub_ids,
        })?;
        let ddc_err = |e: concordia_core::ddc::DdcError| AssetError::Io(e.to_string());
        for (sub, artifact) in &output.artifacts {
            self.ddc
                .put(&artifact_key(key, sub), &artifact.bytes)
                .map_err(ddc_err)?;
        }
        let index = ImportIndex {
            artifacts: output
                .artifacts
                .iter()
                .map(|(s, a)| (s.clone(), a.kind))
                .collect(),
        };
        self.ddc.put(key, &encode(&index)?).map_err(ddc_err)?;
        tracing::debug!(target: "concordia::assets", path = %rel.display(), artifacts = index.artifacts.len(), "imported");
        Ok(sub_ids.into_map())
    }

    fn importer_for(&self, rel: &Path) -> Result<Arc<dyn Importer>, AssetError> {
        let extension = rel
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        self.importers
            .iter()
            .find(|i| i.extensions().contains(&extension.as_str()))
            .cloned()
            .ok_or_else(|| {
                let known: Vec<&str> = self
                    .importers
                    .iter()
                    .flat_map(|i| i.extensions().iter().copied())
                    .collect();
                AssetError::NoImporter {
                    path: rel.to_path_buf(),
                    extension,
                    known: known.join(", "),
                }
            })
    }

    /// The artifact a reference names, re-importing if the DDC lacks it.
    pub fn load_artifact(
        &mut self,
        reference: &AssetRef,
    ) -> Result<(AssetKind, Vec<u8>), AssetError> {
        let AssetRef::Asset { id, sub } = reference else {
            return Err(AssetError::NotFound(format!(
                "{reference} is built-in; use Builtin::of"
            )));
        };
        let record = self
            .records
            .get(id)
            .ok_or_else(|| AssetError::NotFound(reference.to_string()))?
            .clone();
        let sub = sub.clone().unwrap_or_default();
        if let Some(hit) = self.memory.get(*id, &sub) {
            return Ok(hit);
        }
        for attempt in 0..2 {
            let report = match (attempt, self.keys.get(id)) {
                (0, Some(key)) => *key,
                (0, None) => {
                    let key = self.key_of(&record)?;
                    self.keys.insert(*id, key);
                    key
                }
                _ => self.import(&record.path)?.key,
            };
            let index: Option<ImportIndex> = self
                .ddc
                .get(&report)
                .ok()
                .flatten()
                .and_then(|b| postcard::from_bytes(&b).ok());
            let kind = index
                .as_ref()
                .and_then(|i| i.artifacts.iter().find(|(s, _)| *s == sub).map(|(_, k)| *k));
            let bytes = self.ddc.get(&artifact_key(&report, &sub)).ok().flatten();
            if let (Some(kind), Some(bytes)) = (kind, bytes) {
                self.memory.keep(*id, &sub, kind, &bytes);
                return Ok((kind, bytes));
            }
            if attempt == 0 && index.is_some() && kind.is_none() {
                return Err(AssetError::NotFound(reference.to_string()));
            }
        }
        Err(AssetError::NotFound(reference.to_string()))
    }

    fn key_of(&self, record: &Record) -> Result<DdcKey, AssetError> {
        let abs = self.root.join(&record.path);
        let bytes = std::fs::read(&abs).map_err(|e| io(&abs, e))?;
        let importer = self.importer_for(&record.path)?;
        Ok(import_key(
            importer.name(),
            importer.version(),
            PLATFORM,
            &record.sidecar.settings,
            &bytes,
        ))
    }

    fn load_as<T: DeserializeOwned>(
        &mut self,
        reference: &AssetRef,
        want: AssetKind,
    ) -> Result<T, AssetError> {
        let (kind, bytes) = self.load_artifact(reference)?;
        if kind != want {
            return Err(AssetError::Decode(format!(
                "{reference} is a {kind:?}, not a {want:?}"
            )));
        }
        postcard::from_bytes(&bytes).map_err(|e| AssetError::Decode(format!("{reference}: {e}")))
    }

    /// A texture (imported or built-in).
    pub fn load_texture(&mut self, reference: &AssetRef) -> Result<TextureData, AssetError> {
        match Builtin::of(reference) {
            Some(BuiltinAsset::Texture(t)) => Ok(t),
            Some(_) => Err(AssetError::Decode(format!("{reference} is not a texture"))),
            None => self.load_as(reference, AssetKind::Texture),
        }
    }

    /// A mesh (imported or built-in).
    pub fn load_mesh(&mut self, reference: &AssetRef) -> Result<MeshData, AssetError> {
        match Builtin::of(reference) {
            Some(BuiltinAsset::Mesh(m)) => Ok(m),
            Some(_) => Err(AssetError::Decode(format!("{reference} is not a mesh"))),
            None => self.load_as(reference, AssetKind::Mesh),
        }
    }

    /// A material (imported or built-in).
    pub fn load_material(&mut self, reference: &AssetRef) -> Result<MaterialData, AssetError> {
        match Builtin::of(reference) {
            Some(BuiltinAsset::Material(m)) => Ok(m),
            Some(_) => Err(AssetError::Decode(format!("{reference} is not a material"))),
            None => self.load_as(reference, AssetKind::Material),
        }
    }

    /// Any postcard-encoded artifact of the expected kind (skins, clips, lights, cameras).
    pub fn load<T: DeserializeOwned>(
        &mut self,
        reference: &AssetRef,
        kind: AssetKind,
    ) -> Result<T, AssetError> {
        self.load_as(reference, kind)
    }

    /// Where an asset's source lives (project-relative).
    pub fn path_of(&self, id: AssetId) -> Option<&Path> {
        self.records.get(&id).map(|r| r.path.as_path())
    }

    /// Bumped each time hot reload re-imports the asset.
    pub fn generation(&self, id: AssetId) -> u64 {
        self.generations.get(&id).copied().unwrap_or(0)
    }

    /// Re-imports assets whose source or sidecar changed; returns their ids (failures are logged and skipped).
    pub fn on_files_changed(&mut self, changed: &[PathBuf]) -> Vec<AssetId> {
        let canonical = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
        let changed: Vec<PathBuf> = changed.iter().map(|p| canonical(p)).collect();
        let touched: Vec<(AssetId, PathBuf)> = self
            .records
            .iter()
            .filter(|(_, r)| {
                let abs = canonical(&self.root.join(&r.path));
                changed.contains(&abs) || changed.contains(&Sidecar::path_for(&abs))
            })
            .map(|(id, r)| (*id, r.path.clone()))
            .collect();
        let mut reloaded = Vec::new();
        for (id, path) in touched {
            match self.import(&path) {
                Ok(_) => {
                    *self.generations.entry(id).or_default() += 1;
                    reloaded.push(id);
                }
                Err(e) => {
                    tracing::warn!(target: "concordia::assets", path = %path.display(), error = %e, "re-import failed")
                }
            }
        }
        reloaded
    }

    /// Every known asset, by id.
    pub fn list(&self) -> Vec<AssetInfo> {
        self.records
            .iter()
            .map(|(id, r)| AssetInfo {
                id: *id,
                path: r.path.to_string_lossy().replace('\\', "/"),
                importer: r.sidecar.importer.clone(),
                sub_assets: r.sidecar.sub_assets.keys().cloned().collect(),
            })
            .collect()
    }
}

/// A project-relative path without `..`, roots or prefixes.
fn relative(path: &Path) -> Result<PathBuf, AssetError> {
    let ok = !path.as_os_str().is_empty()
        && path
            .components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
    match ok {
        true => Ok(path
            .components()
            .filter(|c| matches!(c, Component::Normal(_)))
            .collect()),
        false => Err(AssetError::Path(path.display().to_string())),
    }
}
