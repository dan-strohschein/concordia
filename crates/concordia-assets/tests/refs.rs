//! Asset references and built-in assets (M1 phase 1.4).

use concordia_assets::{AssetRef, Builtin};
use concordia_reflect::{Reflect, schema_for};

#[test]
fn asset_ref_serializes_as_string_and_schema_is_string_with_pattern() {
    let text = "asset:0193a1c2-7e40-7000-8000-000000000001#mesh/Body";
    let r: AssetRef = text.parse().unwrap();
    assert_eq!(serde_json::to_value(&r).unwrap(), serde_json::json!(text));
    assert_eq!(r.sub_path(), Some("mesh/Body"));
    let schema = schema_for(AssetRef::type_info());
    assert_eq!(schema["type"], "string");
    assert!(
        schema["pattern"].as_str().unwrap().contains("asset:"),
        "{schema}"
    );
    for bad in [
        "asset:",
        "asset:not-a-uuid",
        "builtin:mesh/",
        "texture.png",
        "builtin:Mesh/cube",
    ] {
        assert!(bad.parse::<AssetRef>().is_err(), "{bad}");
    }
}

#[test]
fn builtins_have_fixed_names_and_data() {
    for name in [
        "builtin:mesh/cube",
        "builtin:mesh/plane",
        "builtin:mesh/sphere",
        "builtin:material/default",
        "builtin:texture/white",
        "builtin:texture/black",
        "builtin:texture/normal_flat",
    ] {
        let r: AssetRef = name.parse().unwrap();
        assert!(Builtin::of(&r).is_some(), "{name}");
    }
    let cube = Builtin::mesh("cube").unwrap();
    assert_eq!((cube.positions.len(), cube.indices.len()), (24, 36));
    assert!(cube.validate().is_ok());
    assert!(
        "builtin:mesh/teapot"
            .parse::<AssetRef>()
            .ok()
            .and_then(|r| Builtin::of(&r))
            .is_none()
    );
}
