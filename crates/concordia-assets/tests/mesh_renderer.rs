//! `MeshRenderer.mobility` (M4 4.7b): a hint for the shadow cache, `Auto` unless written.

use concordia_assets::{MeshRenderer, Mobility};

#[test]
fn mobility_defaults_to_auto_and_roundtrips() {
    assert_eq!(MeshRenderer::default().mobility, Mobility::Auto);
    let old: MeshRenderer = serde_json::from_value(serde_json::json!({
        "mesh": "builtin:mesh/cube",
        "material": "builtin:material/default",
    }))
    .unwrap();
    assert_eq!(
        old.mobility,
        Mobility::Auto,
        "existing scenes load unchanged"
    );
    for m in [Mobility::Auto, Mobility::Static, Mobility::Movable] {
        let r = MeshRenderer {
            mobility: m,
            ..MeshRenderer::default()
        };
        let text = serde_json::to_value(&r).unwrap();
        assert_eq!(serde_json::from_value::<MeshRenderer>(text).unwrap(), r);
    }
    assert_eq!(
        serde_json::to_value(Mobility::Movable).unwrap(),
        serde_json::json!("Movable")
    );
    assert!(serde_json::from_value::<Mobility>(serde_json::json!("Dynamic")).is_err());
}
