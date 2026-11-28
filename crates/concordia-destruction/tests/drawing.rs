//! How fragments are drawn (M8 8.5b): a breakable's `MeshRenderer` gives way to a
//! `PieceRenderer` of the recipe's baked meshes, and every part split off draws its own pieces of
//! the same meshes.

mod common;

use common::{Run, fragments, middle, root, run};
use concordia_assets::{AssetRef, MeshRenderer, Mobility, PieceRenderer};
use concordia_destruction::Fragment;
use concordia_terra::World;

fn stone() -> AssetRef {
    "asset:0193a1c2-7e40-7000-8000-0000000000e1"
        .parse()
        .unwrap()
}

fn painted(world: &mut World) {
    let r = MeshRenderer {
        mesh: "builtin:mesh/cube".parse().unwrap(),
        material: stone(),
        cast_shadows: false,
        mobility: Mobility::Movable,
    };
    world.insert(root(), r).unwrap();
}

#[test]
fn a_breakable_draws_all_its_pieces_with_its_own_material() {
    let world = run(
        &Run {
            setup: Some(painted),
            ..Run::default()
        },
        2,
    );
    assert!(
        world.get::<MeshRenderer>(root()).is_none(),
        "the source mesh gives way to its pieces"
    );
    let r = world.get::<PieceRenderer>(root()).cloned().unwrap();
    let looks = common::looks();
    let want = |mesh: &AssetRef, material: AssetRef| MeshRenderer {
        mesh: mesh.clone(),
        material,
        cast_shadows: false,
        mobility: Mobility::Movable,
    };
    assert_eq!(
        r,
        PieceRenderer {
            meshes: vec![
                want(looks.surface.as_ref().unwrap(), stone()),
                want(
                    looks.interior.as_ref().unwrap(),
                    common::interior_material()
                ),
            ],
            pieces: 4,
            shown: vec![0, 1, 2, 3],
        }
    );
}

#[test]
fn each_part_draws_only_its_own_pieces() {
    let mut world = run(
        &Run {
            setup: Some(painted),
            blows: vec![middle(2)],
            ..Run::default()
        },
        4,
    );
    let parts = fragments(&mut world);
    assert_eq!(parts.len(), 2, "{parts:?}");
    let whole = common::looks();
    for (pieces, id) in parts {
        let r = world.get::<PieceRenderer>(id).cloned().unwrap();
        assert_eq!(r.shown, pieces, "{id}");
        assert_eq!(r.pieces, 4);
        assert_eq!(
            r.meshes[0].mesh,
            whole.surface.clone().unwrap(),
            "shared meshes"
        );
        assert_eq!(r.meshes[0].material, stone());
        assert_eq!(world.get::<Fragment>(id).unwrap().pieces, pieces);
    }
}

#[test]
fn without_a_mesh_renderer_or_baked_meshes_pieces_use_the_defaults_or_nothing() {
    // No MeshRenderer: the default material; the interior's own material still applies.
    let world = run(&Run::default(), 2);
    let r = world.get::<PieceRenderer>(root()).cloned().unwrap();
    assert_eq!(r.meshes[0].material, MeshRenderer::default().material);
    assert_eq!(r.meshes[1].material, common::interior_material());
    // A breakable loaded without baked meshes (no renderer) is still simulated, and undrawn.
    let bare = common::run_with(&Run::default(), 2, common::source_without_looks());
    assert!(bare.get::<Fragment>(root()).is_some());
    assert!(bare.get::<PieceRenderer>(root()).is_none());
}
