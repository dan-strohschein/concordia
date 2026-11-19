//! `resolve`: the CPU asset data a frame needs, minus what the receiver already holds (M1 1.9d).

mod common;

use common::*;
use concordia_core::math::{Quat, Vec3};
use concordia_render::{RenderWorld, resolve};

#[test]
fn resolve_returns_only_assets_not_yet_held() {
    let mut s = Scene::new();
    let red = s.material("red", unlit([0.8, 0.1, 0.1, 1.0]));
    s.mesh("cube", red, Vec3::ZERO, Quat::IDENTITY, Vec3::ONE);
    let rw = RenderWorld::extract(&s.world);
    let (mesh, material) = (rw.draws[0].mesh.clone(), rw.draws[0].material.clone());
    let all = resolve(&rw, &mut s.assets, &|_| false).unwrap();
    assert!(all.contains(&mesh) && all.contains(&material));
    let rest = resolve(&rw, &mut s.assets, &|r| *r == mesh).unwrap();
    assert!(!rest.contains(&mesh), "a held mesh is not sent again");
    assert!(rest.contains(&material));
    let none = resolve(&rw, &mut s.assets, &|r| *r == mesh || *r == material).unwrap();
    assert!(
        !none.contains(&material),
        "a held material is not sent again either"
    );
}
