//! The per-pixel closure budget: a material with more closures than the budget is downgraded by
//! parameter blending (mixes first, then layers), and each merge is reported.

use concordia_assets::{
    AssetRef, Fuzz, Haze, Material, MaterialError, MaterialNode, Slab, Subsurface, ThinFilm,
};
use concordia_core::math::Vec3;

use crate::bsdf::{f0, slab_transmittance};
use crate::program::MaterialProgram;

/// How many closures a pixel may evaluate (1..=MAX).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClosureBudget(u8);

/// A budget outside 1..=MAX.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("closure budget {0} is outside 1..={max}", max = ClosureBudget::MAX)]
pub struct BudgetError(pub u8);

impl ClosureBudget {
    /// The largest budget a material program supports.
    pub const MAX: u8 = 8;

    /// A budget of `n` closures.
    pub fn new(n: u8) -> Result<Self, BudgetError> {
        if (1..=Self::MAX).contains(&n) {
            Ok(Self(n))
        } else {
            Err(BudgetError(n))
        }
    }

    /// The number of closures.
    pub fn get(self) -> u8 {
        self.0
    }
}

/// Which operator a merge collapsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeKind {
    /// A horizontal mix became one slab.
    Horizontal,
    /// A vertical layer became one slab.
    Vertical,
}

/// One merge: the operator node, the two slabs it combined and what was lost.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClosureMerge {
    /// The operator node, now a slab (or a coverage over one).
    pub node: String,
    /// The operator kind.
    pub kind: MergeKind,
    /// The merged slabs, as `[a, b]` or `[top, bottom]`.
    pub slabs: [String; 2],
    /// Inputs that could not be kept, as `node.field`.
    pub dropped: Vec<String>,
}

/// What fitting a material to a budget did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClosureReport {
    /// The budget.
    pub budget: u8,
    /// Closures before.
    pub before: usize,
    /// Closures after.
    pub after: usize,
    /// Merges in the order they were made.
    pub merges: Vec<ClosureMerge>,
}

impl ClosureReport {
    /// Whether anything was merged.
    pub fn downgraded(&self) -> bool {
        !self.merges.is_empty()
    }
}

/// `material` reduced to at most `budget` closures, and the report (an invalid material is refused).
pub fn fit_budget(
    material: &Material,
    budget: ClosureBudget,
) -> Result<(Material, ClosureReport), MaterialError> {
    let before = closures(material)?;
    let mut fitted = material.clone();
    let mut merges = Vec::new();
    while closures(&fitted)? > usize::from(budget.get()) {
        let Some(candidate) = next_merge(&fitted) else {
            break;
        };
        merges.push(merge(&mut fitted, &candidate));
        prune(&mut fitted);
    }
    let report = ClosureReport {
        budget: budget.get(),
        before,
        after: closures(&fitted)?,
        merges,
    };
    Ok((fitted, report))
}

fn closures(material: &Material) -> Result<usize, MaterialError> {
    Ok(MaterialProgram::compile(material)?.closures())
}

/// An operator whose operands are both (covered) slabs.
struct Candidate {
    node: String,
    kind: MergeKind,
    first: Covered,
    second: Covered,
}

/// A slab seen through a chain of coverage nodes.
#[derive(Clone)]
struct Covered {
    name: String,
    slab: Slab,
    coverage: f32,
}

/// The first mergeable mix in post-order, else the first mergeable layer.
fn next_merge(material: &Material) -> Option<Candidate> {
    let mut order = Vec::new();
    post_order(material, &material.root, &mut order);
    let mut candidates: Vec<Candidate> = order
        .iter()
        .filter_map(|n| candidate(material, n))
        .collect();
    let first_mix = candidates
        .iter()
        .position(|c| c.kind == MergeKind::Horizontal);
    let chosen = first_mix.or((!candidates.is_empty()).then_some(0))?;
    Some(candidates.swap_remove(chosen))
}

fn post_order(material: &Material, name: &str, out: &mut Vec<String>) {
    if out.iter().any(|n| n == name) {
        return;
    }
    for child in material
        .node(name)
        .map(MaterialNode::children)
        .unwrap_or_default()
    {
        post_order(material, child, out);
    }
    out.push(name.to_owned());
}

fn candidate(material: &Material, name: &str) -> Option<Candidate> {
    let (kind, first, second) = match material.node(name)? {
        MaterialNode::Horizontal { a, b, .. } => (MergeKind::Horizontal, a, b),
        MaterialNode::Vertical { top, bottom } => (MergeKind::Vertical, top, bottom),
        _ => return None,
    };
    Some(Candidate {
        node: name.to_owned(),
        kind,
        first: covered(material, first)?,
        second: covered(material, second)?,
    })
}

fn covered(material: &Material, name: &str) -> Option<Covered> {
    match material.node(name)? {
        MaterialNode::Slab(slab) => Some(Covered {
            name: name.to_owned(),
            slab: slab.clone(),
            coverage: 1.0,
        }),
        MaterialNode::Coverage { node, coverage } => covered(material, node).map(|c| Covered {
            coverage: c.coverage * coverage,
            ..c
        }),
        _ => None,
    }
}

/// Replaces the candidate operator with one slab (under a coverage node when partial).
fn merge(material: &mut Material, c: &Candidate) -> ClosureMerge {
    let (slab, coverage, mut dropped) = match c.kind {
        MergeKind::Horizontal => merge_mix(c, mix_of(material, &c.node)),
        MergeKind::Vertical => merge_layer(c),
    };
    if let Some(MaterialNode::Horizontal {
        mix_texture: Some(_),
        ..
    }) = material.node(&c.node)
    {
        dropped.push(format!("{}.mix_texture", c.node));
    }
    replace(material, &c.node, slab, coverage);
    ClosureMerge {
        node: c.node.clone(),
        kind: c.kind,
        slabs: [c.first.name.clone(), c.second.name.clone()],
        dropped,
    }
}

fn mix_of(material: &Material, name: &str) -> f32 {
    match material.node(name) {
        Some(MaterialNode::Horizontal { mix, .. }) => *mix,
        _ => 0.0,
    }
}

fn merge_mix(c: &Candidate, mix: f32) -> (Slab, f32, Vec<String>) {
    let (ca, cb) = (c.first.coverage, c.second.coverage);
    let coverage = ca + (cb - ca) * mix;
    let t = if coverage > 0.0 {
        mix * cb / coverage
    } else {
        mix
    };
    let (slab, dropped) = blend(&c.first, &c.second, t);
    (slab, coverage, dropped)
}

/// A top over a bottom as one slab: the bottom tinted by what passes the top, blended toward the
/// top by its opaque share, with the top's highlight kept as haze.
fn merge_layer(c: &Candidate) -> (Slab, f32, Vec<String>) {
    let (top, ct) = (&c.first.slab, c.first.coverage);
    let through = slab_transmittance(top, 1.0);
    let seen = Vec3::splat(1.0 - ct) + ct * through * through;
    let mut tinted = c.second.clone();
    let tint = rgb(&tinted.slab) * seen;
    tinted.slab.base_color[..3].copy_from_slice(&tint.to_array());
    let opaque = ct * (1.0 - mean(Vec3::from_array(top.transmittance)));
    let (mut slab, mut dropped) = blend(&tinted, &c.first, opaque);
    let weight = (ct * (1.0 - opaque) * mean(f0(top)) / mean(f0(&slab)).max(1e-4)).clamp(0.0, 1.0);
    match slab.haze {
        None if weight > 0.0 => {
            slab.haze = Some(Haze {
                roughness: top.roughness,
                weight,
            })
        }
        Some(_) if weight > 0.0 => dropped.push(format!("{}.specular", c.first.name)),
        _ => {}
    }
    let cb = c.second.coverage;
    (slab, 1.0 - (1.0 - ct) * (1.0 - cb), dropped)
}

fn replace(material: &mut Material, name: &str, slab: Slab, coverage: f32) {
    if coverage >= 1.0 {
        material
            .nodes
            .insert(name.to_owned(), MaterialNode::Slab(slab));
        return;
    }
    let inner = unique(material, &format!("{name}-merged"));
    material
        .nodes
        .insert(inner.clone(), MaterialNode::Slab(slab));
    material.nodes.insert(
        name.to_owned(),
        MaterialNode::Coverage {
            node: inner,
            coverage,
        },
    );
}

fn unique(material: &Material, base: &str) -> String {
    (0..)
        .map(|i| {
            if i == 0 {
                base.to_owned()
            } else {
                format!("{base}-{i}")
            }
        })
        .find(|n| !material.nodes.contains_key(n))
        .unwrap_or_else(|| base.to_owned())
}

/// Removes nodes the root no longer reaches.
fn prune(material: &mut Material) {
    let keep: Vec<String> = material
        .reachable()
        .into_iter()
        .map(str::to_owned)
        .collect();
    material.nodes.retain(|name, _| keep.contains(name));
}

/// Parameter blend of two slabs, `t` of the way from `a` to `b`, and the inputs it drops.
fn blend(a: &Covered, b: &Covered, t: f32) -> (Slab, Vec<String>) {
    let (x, y) = (&a.slab, &b.slab);
    let lerp = |p: f32, q: f32| p + (q - p) * t;
    let lerp_n = |p: &[f32], q: &[f32], out: &mut [f32]| {
        for i in 0..out.len() {
            out[i] = lerp(p[i], q[i]);
        }
    };
    let mut dropped = Vec::new();
    let mut s = Slab {
        metallic: lerp(x.metallic, y.metallic),
        specular: lerp(x.specular, y.specular),
        roughness: lerp(x.roughness, y.roughness),
        normal_scale: lerp(x.normal_scale, y.normal_scale),
        occlusion_strength: lerp(x.occlusion_strength, y.occlusion_strength),
        emissive_strength: 1.0,
        ..Slab::default()
    };
    lerp_n(&x.base_color, &y.base_color, &mut s.base_color);
    lerp_n(&x.transmittance, &y.transmittance, &mut s.transmittance);
    let emitted = |z: &Slab| Vec3::from_array(z.emissive) * z.emissive_strength;
    s.emissive = emitted(x).lerp(emitted(y), t).to_array();
    let mut texture =
        |field: &str, p: &Option<AssetRef>, q: &Option<AssetRef>| -> Option<AssetRef> {
            let (kept, lost) = pick(p.as_ref(), q.as_ref(), t, &a.name, &b.name);
            if let Some(name) = lost {
                dropped.push(format!("{name}.{field}"));
            }
            kept.cloned()
        };
    s.base_color_texture = texture(
        "base_color_texture",
        &x.base_color_texture,
        &y.base_color_texture,
    );
    s.metallic_roughness_texture = texture(
        "metallic_roughness_texture",
        &x.metallic_roughness_texture,
        &y.metallic_roughness_texture,
    );
    s.normal_texture = texture("normal_texture", &x.normal_texture, &y.normal_texture);
    s.occlusion_texture = texture(
        "occlusion_texture",
        &x.occlusion_texture,
        &y.occlusion_texture,
    );
    s.emissive_texture = texture("emissive_texture", &x.emissive_texture, &y.emissive_texture);
    s.haze = feature(
        x.haze,
        y.haze,
        t,
        |h, w| Haze {
            weight: h.weight * w,
            ..h
        },
        |p, q| Haze {
            roughness: lerp(p.roughness, q.roughness),
            weight: lerp(p.weight, q.weight),
        },
    );
    s.fuzz = feature(
        x.fuzz,
        y.fuzz,
        t,
        |f, w| Fuzz {
            color: f.color.map(|c| c * w),
            ..f
        },
        |p, q| Fuzz {
            color: std::array::from_fn(|i| lerp(p.color[i], q.color[i])),
            roughness: lerp(p.roughness, q.roughness),
        },
    );
    s.subsurface = feature(
        x.subsurface,
        y.subsurface,
        t,
        |f, w| Subsurface {
            mean_free_path: f.mean_free_path.map(|m| m * w),
        },
        |p, q| Subsurface {
            mean_free_path: std::array::from_fn(|i| lerp(p.mean_free_path[i], q.mean_free_path[i])),
        },
    );
    let (film, lost) = pick(
        x.thin_film.as_ref(),
        y.thin_film.as_ref(),
        t,
        &a.name,
        &b.name,
    );
    s.thin_film = match (x.thin_film, y.thin_film) {
        (Some(p), Some(q)) => Some(ThinFilm {
            thickness_nm: lerp(p.thickness_nm, q.thickness_nm),
            ior: lerp(p.ior, q.ior),
        }),
        _ => film.copied(),
    };
    if let (Some(name), false) = (lost, x.thin_film.is_some() && y.thin_film.is_some()) {
        dropped.push(format!("{name}.thin_film"));
    }
    (s, dropped)
}

/// Keeps one of two optional inputs: equal ones, the only one when its side weighs ≥ ½, or the
/// heavier side's; returns it and the name of the side whose input was lost.
fn pick<'a, T: PartialEq>(
    p: Option<&'a T>,
    q: Option<&'a T>,
    t: f32,
    a: &str,
    b: &str,
) -> (Option<&'a T>, Option<String>) {
    let b_heavier = t >= 0.5;
    match (p, q) {
        (None, None) => (None, None),
        (Some(x), Some(y)) if x == y => (Some(x), None),
        (Some(_), Some(y)) if b_heavier => (Some(y), Some(a.to_owned())),
        (Some(x), Some(_)) => (Some(x), Some(b.to_owned())),
        (Some(x), None) if !b_heavier => (Some(x), None),
        (Some(_), None) => (None, Some(a.to_owned())),
        (None, Some(y)) if b_heavier => (Some(y), None),
        (None, Some(_)) => (None, Some(b.to_owned())),
    }
}

/// Blends an optional feature: both present → `both`; one present → `scale`d by its side's weight.
fn feature<T: Copy>(
    p: Option<T>,
    q: Option<T>,
    t: f32,
    scale: impl Fn(T, f32) -> T,
    both: impl Fn(T, T) -> T,
) -> Option<T> {
    match (p, q) {
        (Some(x), Some(y)) => Some(both(x, y)),
        (Some(x), None) => Some(scale(x, 1.0 - t)),
        (None, Some(y)) => Some(scale(y, t)),
        (None, None) => None,
    }
}

fn rgb(slab: &Slab) -> Vec3 {
    Vec3::new(slab.base_color[0], slab.base_color[1], slab.base_color[2])
}

fn mean(v: Vec3) -> f32 {
    (v.x + v.y + v.z) / 3.0
}
