//! The FBX importer (`.fbx`, through ufbx): meshes, materials (one `Slab` each), textures, the node
//! hierarchy as a prefab, skins, and animation stacks as clips, in the glTF importer's asset types.
//!
//! ufbx normalizes every file to the engine's convention (right-handed, Y-up, meters) while loading,
//! so geometry, node transforms, bind matrices and keys all arrive converted.

#![deny(unsafe_code)]

mod anim;
mod ffi;
mod material;
mod mesh;
mod names;
mod prefab;
mod skin;

use concordia_assets::{AssetError, ImportInput, ImportOutput, Importer};
use concordia_import_model::ModelSettings;

/// The FBX importer.
#[derive(Debug, Clone, Copy, Default)]
pub struct FbxImporter;

impl Importer for FbxImporter {
    fn name(&self) -> &'static str {
        "fbx"
    }

    fn version(&self) -> u32 {
        // 2 (2.4.1): `Skinned` on skinned renderers, bone names and the frame rate on clips.
        // 3 (2.4.4): clip events from the import settings.
        // 4 (M4 4.2): meshlets (`meshlets: auto | true | false`).
        4
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["fbx"]
    }

    fn default_settings(&self) -> serde_json::Value {
        ModelSettings::defaults()
    }

    fn import(&self, input: ImportInput<'_>) -> Result<ImportOutput, AssetError> {
        let fail = |reason: String| AssetError::Import {
            path: input.path.to_path_buf(),
            reason,
        };
        let scene = ffi::Scene::load(input.bytes).map_err(|e| fail(format!("FBX: {e}")))?;
        let nodes = scene.nodes().map_err(fail)?;
        let meshes = scene.meshes().map_err(fail)?;
        let materials = scene.materials().map_err(fail)?;
        let files = scene.texture_files().map_err(fail)?;
        let stacks = scene.stacks().map_err(fail)?;
        let names = names::Names::of(&nodes, &meshes, &materials, &files, &stacks);
        let mut output = ImportOutput::default();
        let images =
            material::textures(&files, &materials, &names, input.read_relative, &mut output)
                .map_err(fail)?;
        let settings = ModelSettings::from_json(input.settings).map_err(&fail)?;
        let scale = settings.emission_scale();
        material::materials(
            &materials,
            &images,
            &names,
            scale,
            input.sub_ids,
            &mut output,
        )?;
        let stem = input
            .path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("fbx");
        let entities = prefab::prefab(&nodes, &meshes, &names, stem, input.sub_ids, &mut output)?;
        let clusters =
            skin::skins(&scene, &meshes, &names, &entities, &mut output).map_err(fail)?;
        let meshlets = concordia_import_model::MeshletContext {
            mode: settings.meshlets,
            jobs: input.jobs,
            cache: input.derived,
        };
        mesh::meshes(&scene, &meshes, &clusters, &names, &meshlets, &mut output).map_err(fail)?;
        anim::clips(&scene, &stacks, &names, &entities, &settings, &mut output).map_err(fail)?;
        for sub in output.artifacts.keys().filter(|k| !k.is_empty()) {
            input.sub_ids.id(sub);
        }
        Ok(output)
    }
}
