//! The glTF 2.0 importer (`.gltf`, `.glb`): meshes, metallic-roughness materials, textures, the
//! node hierarchy as a prefab, skins and animation clips (stored for M2), lights and cameras.
//!
//! Sub-assets are named by glTF names (index fallback), so re-exports that reorder nodes keep ids.

mod anim;
mod extras;
mod material;
mod mesh;
mod names;
mod prefab;
mod source;

use concordia_assets::{AssetError, ImportInput, ImportOutput, Importer};
use concordia_import_model::{MeshletContext, ModelSettings};

pub use concordia_import_model::{EntitySummary, PrefabSummary, generate_tangents, load_prefab};

/// Extensions the importer understands (anything else in `extensionsRequired` is an error).
pub const SUPPORTED_EXTENSIONS: [&str; 5] = [
    "KHR_lights_punctual",
    "KHR_materials_emissive_strength",
    "KHR_materials_unlit",
    "KHR_texture_transform",
    "KHR_mesh_quantization",
];

/// The glTF importer.
#[derive(Debug, Clone, Copy, Default)]
pub struct GltfImporter;

impl Importer for GltfImporter {
    fn name(&self) -> &'static str {
        "gltf"
    }

    fn version(&self) -> u32 {
        // 3 (2.4.1): `Skinned` on skinned renderers, bone names on clip channels.
        // 4 (2.4.4): clip events from the import settings.
        // 5 (M4 4.2): meshlets (`meshlets: auto | true | false`).
        5
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["gltf", "glb"]
    }

    fn default_settings(&self) -> serde_json::Value {
        ModelSettings::defaults()
    }

    fn import(&self, input: ImportInput<'_>) -> Result<ImportOutput, AssetError> {
        let fail = |reason: String| AssetError::Import {
            path: input.path.to_path_buf(),
            reason,
        };
        let gltf = gltf::Gltf::from_slice_without_validation(input.bytes)
            .map_err(|e| fail(e.to_string()))?;
        if let Some(ext) = gltf
            .document
            .extensions_required()
            .find(|e| !SUPPORTED_EXTENSIONS.contains(e))
        {
            return Err(fail(format!(
                "required extension {ext} is not supported (supported: {})",
                SUPPORTED_EXTENSIONS.join(", ")
            )));
        }
        let buffers = source::buffers(&gltf, input.read_relative).map_err(fail)?;
        let names = names::Names::of(&gltf.document);
        let mut output = ImportOutput::default();
        let images = material::textures(
            &gltf.document,
            &buffers,
            input.read_relative,
            &names,
            &mut output,
        )
        .map_err(fail)?;
        let settings = ModelSettings::from_json(input.settings).map_err(&fail)?;
        material::materials(
            &gltf.document,
            &images,
            &names,
            settings.emission_scale(),
            input.sub_ids,
            &mut output,
        )?;
        let meshlets = MeshletContext {
            mode: settings.meshlets,
            jobs: input.jobs,
            cache: input.derived,
        };
        mesh::meshes(&gltf.document, &buffers, &names, &meshlets, &mut output).map_err(fail)?;
        extras::lights_and_cameras(&gltf.document, &names, &mut output)?;
        let stem = input
            .path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("gltf");
        let entities = prefab::prefab(&gltf.document, &names, stem, input.sub_ids, &mut output)?;
        anim::skins_and_clips(
            &gltf.document,
            &buffers,
            &names,
            &entities,
            &settings,
            &mut output,
        )
        .map_err(fail)?;
        for sub in output.artifacts.keys().filter(|k| !k.is_empty()) {
            input.sub_ids.id(sub);
        }
        Ok(output)
    }
}
