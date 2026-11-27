//! Virtual shadow maps for spot and point lights (M4 4.7c): one map per spot light, six cube faces
//! per point light, each 8192² texels in 64² pages with mips down to one page, laid out in the
//! page table after the sun's levels, coarse mips first.
//!
//! References: Lefohn, Sengupta, Owens, "Resolution-Matched Shadow Maps" (ACM TOG 2007);
//! Fernando et al., "Adaptive Shadow Maps" (SIGGRAPH 2001); Epic's public "Virtual Shadow Maps"
//! documentation (local lights as cube faces with mips), ideas only.

use concordia_core::math::camera::rh::{proj::directx, view::look_to_mat4};
use concordia_core::math::{Mat4, Vec3};

/// Texels per map side at mip 0.
pub const LOCAL_TEXELS: u32 = 8192;
/// Pages per map side at mip 0.
pub const LOCAL_PAGES: u32 = LOCAL_TEXELS / super::PAGE_TEXELS;
/// Mips per map (64² pages down to 1).
pub const LOCAL_MIPS: u32 = 7;
/// Table entries per map (64² + 32² + … + 1²).
pub const MAP_ENTRIES: u32 = 5461;
/// Maps per light slot (a point light's six faces; a spot uses the first).
pub const FACES: u32 = 6;
/// The maps' near plane (m).
pub const NEAR: f32 = 0.05;
/// Widest spot cone (full angle, radians) drawn as one map; wider cones use six faces.
pub const MAX_SPOT_FOV: f32 = 120.0 * std::f32::consts::PI / 180.0;
/// Texels of guard band past a cube face's edge at mip 0.
pub const GUARD_TEXELS: f32 = 2.0;

/// Pages per side of `mip`.
pub fn mip_side(mip: u32) -> u32 {
    debug_assert!(mip < LOCAL_MIPS, "mip {mip}");
    LOCAL_PAGES >> mip
}

/// Where `mip` starts within a map (coarsest first).
pub fn mip_offset(mip: u32) -> u32 {
    (mip + 1..LOCAL_MIPS)
        .map(|m| mip_side(m) * mip_side(m))
        .sum()
}

/// The table entry of page `page` (x, y) of `mip` of map `map`, the maps starting at entry `base`.
pub fn map_entry(base: u32, map: u32, mip: u32, page: [u32; 2]) -> u32 {
    debug_assert!(
        page[0] < mip_side(mip) && page[1] < mip_side(mip),
        "page {page:?}"
    );
    base + map * MAP_ENTRIES + mip_offset(mip) + page[1] * mip_side(mip) + page[0]
}

/// One local map: world → its reverse-Z clip space, and its full field of view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LocalMap {
    /// World → clip (reverse-Z: the near plane at depth 1).
    pub view_proj: Mat4,
    /// Full field of view (radians), square.
    pub fov: f32,
}

/// A map at `position` looking along `dir` with full field of view `fov` out to `range`.
fn map(position: Vec3, dir: Vec3, fov: f32, range: f32) -> LocalMap {
    let up = if dir.y.abs() > 0.99 { Vec3::Z } else { Vec3::Y };
    let view = look_to_mat4(position, dir, up);
    // Reverse-Z: the far plane at depth 0, the near plane at depth 1.
    let proj = directx::perspective(fov, 1.0, range.max(NEAR * 2.0), NEAR);
    LocalMap {
        view_proj: proj * view,
        fov,
    }
}

/// A spot light's map (full field of view twice the outer angle), or `None` when the cone is wider
/// than [`MAX_SPOT_FOV`] (the light then takes six cube faces).
pub fn spot_map(position: Vec3, dir: Vec3, outer: f32, range: f32) -> Option<LocalMap> {
    let fov = 2.0 * outer;
    (fov <= MAX_SPOT_FOV).then(|| map(position, dir.normalize_or(Vec3::NEG_Z), fov, range))
}

/// The cube face axes, in face order (+X, −X, +Y, −Y, +Z, −Z).
pub const FACE_AXES: [Vec3; 6] = [
    Vec3::X,
    Vec3::NEG_X,
    Vec3::Y,
    Vec3::NEG_Y,
    Vec3::Z,
    Vec3::NEG_Z,
];

/// A point light's six faces: 90° plus a guard band of [`GUARD_TEXELS`] at mip 0 on each side.
pub fn cube_faces(position: Vec3, range: f32) -> [LocalMap; 6] {
    let half = (1.0 + 2.0 * GUARD_TEXELS / LOCAL_TEXELS as f32).atan();
    FACE_AXES.map(|axis| map(position, axis, 2.0 * half, range))
}

/// The cube face whose axis is `dir`'s major axis.
pub fn cube_face_of(dir: Vec3) -> u32 {
    let a = dir.abs();
    match (a.x >= a.y && a.x >= a.z, a.y >= a.z) {
        (true, _) => u32::from(dir.x < 0.0),
        (false, true) => 2 + u32::from(dir.y < 0.0),
        (false, false) => 4 + u32::from(dir.z < 0.0),
    }
}

/// The finest mip whose texel is at least `footprint · 2^bias` wide at `distance` from the light,
/// for a map of full field of view `fov`, clamped to the mips.
pub fn local_mip(footprint: f32, distance: f32, fov: f32, bias: f32) -> u32 {
    let texel0 = 2.0 * distance * (fov / 2.0).tan() / LOCAL_TEXELS as f32;
    let mip = ((footprint / texel0.max(1e-12)).log2() + bias).ceil();
    match mip.is_finite() {
        true => (mip.max(0.0) as u32).min(LOCAL_MIPS - 1),
        false if mip > 0.0 => LOCAL_MIPS - 1,
        false => 0,
    }
}

/// A selected light as its slot holds it (M4 4.7c).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlotLight {
    /// World position.
    pub position: Vec3,
    /// Travel direction (spots).
    pub direction: Vec3,
    /// Range (the maps' far plane).
    pub range: f32,
    /// A spot's outer half-angle (`None`: a point light).
    pub spot: Option<f32>,
    /// Its shadow settings.
    pub settings: concordia_render_types::LocalShadowSettings,
}

impl SlotLight {
    /// Its maps: one for a spot that fits [`MAX_SPOT_FOV`], else six cube faces.
    pub fn maps(&self) -> Vec<LocalMap> {
        match self
            .spot
            .and_then(|outer| spot_map(self.position, self.direction, outer, self.range))
        {
            Some(m) => vec![m],
            None => cube_faces(self.position, self.range).to_vec(),
        }
    }
}

/// The locals buffer (`VsmLocal` in `vsm.slang`): a header row (slots, first local depth view,
/// first local table entry, 0), then four rows per slot (position and range; direction and the
/// cosine of the outer angle; kind, first map, field of view bits, PCF radius; depth and normal
/// bias). Empty slots have kind 0.
pub fn local_rows(slots: &[Option<SlotLight>], view_base: u32, entry_base: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(16 * (1 + 4 * slots.len()));
    let mut row = |w: [u32; 4]| w.iter().for_each(|x| out.extend(x.to_le_bytes()));
    row([slots.len() as u32, view_base, entry_base, 0]);
    for (s, slot) in slots.iter().enumerate() {
        let Some(l) = slot else {
            (0..4).for_each(|_| row([0; 4]));
            continue;
        };
        let maps = l.maps();
        let (kind, cos) = match (l.spot, maps.len()) {
            (Some(outer), 1) => (1, outer.cos()),
            _ => (2, -1.0f32),
        };
        let f = |v: f32| v.to_bits();
        row([
            f(l.position.x),
            f(l.position.y),
            f(l.position.z),
            f(l.range),
        ]);
        row([f(l.direction.x), f(l.direction.y), f(l.direction.z), f(cos)]);
        row([
            kind,
            s as u32 * FACES,
            f(maps[0].fov),
            l.settings.pcf.radius(),
        ]);
        row([f(l.settings.depth_bias), f(l.settings.normal_bias), 0, 0]);
    }
    out
}

/// Every slot's map views, slot by slot, six per slot (unused ones the identity).
pub fn local_views(slots: &[Option<SlotLight>]) -> Vec<Mat4> {
    let mut out = Vec::with_capacity(slots.len() * FACES as usize);
    for slot in slots {
        let maps = slot.as_ref().map(SlotLight::maps).unwrap_or_default();
        (0..FACES as usize)
            .for_each(|f| out.push(maps.get(f).map_or(Mat4::IDENTITY, |m| m.view_proj)));
    }
    out
}

/// One frame's local slots: each slot's light (or none) and the slots whose pages must go
/// (their light changed, left or arrived).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LocalFrame {
    pub(crate) slots: Vec<Option<SlotLight>>,
    /// Each slot's light's entity.
    pub(crate) entities: Vec<Option<concordia_core::id::EntityId>>,
    /// Slot indices to release at the frame's start.
    pub(crate) released: Vec<u32>,
    /// Shadowed lights beyond the slots (shaded unshadowed).
    pub(crate) dropped: usize,
}

/// The selected local lights across frames: the selection (its slot count follows the shadowed
/// lights present, rounded up to a power of two and capped) and each slot's light last frame.
#[derive(Debug, Default)]
pub(crate) struct LocalSlots {
    selection: Option<crate::shadow_select::LocalSelection<concordia_core::id::EntityId>>,
    last: Vec<Option<SlotLight>>,
}

impl LocalSlots {
    /// Selects from `candidates` (entity, light, importance) into at most `max` slots.
    pub(crate) fn update(
        &mut self,
        max: u8,
        candidates: &[(concordia_core::id::EntityId, SlotLight, f32)],
    ) -> LocalFrame {
        let want = match candidates.len() {
            0 => 0,
            n => n.next_power_of_two().min(usize::from(max)),
        };
        if self.last.len() != want {
            self.selection = (want > 0).then(|| crate::shadow_select::LocalSelection::new(want));
            self.last = vec![None; want];
        }
        let Some(selection) = &mut self.selection else {
            return LocalFrame {
                slots: Vec::new(),
                entities: Vec::new(),
                released: Vec::new(),
                dropped: candidates.len(),
            };
        };
        let ranked: Vec<_> = candidates.iter().map(|(e, _, i)| (*e, *i)).collect();
        let picked = selection.select(&ranked);
        let slots: Vec<Option<SlotLight>> = (0..want)
            .map(|k| {
                let e = selection.key(k)?;
                candidates
                    .iter()
                    .find(|(c, _, _)| c == e)
                    .map(|(_, l, _)| *l)
            })
            .collect();
        let entities = (0..want).map(|k| selection.key(k).copied()).collect();
        let released = (0..want as u32)
            .filter(|&k| self.last[k as usize] != slots[k as usize])
            .collect();
        self.last.clone_from(&slots);
        LocalFrame {
            slots,
            entities,
            released,
            dropped: picked.dropped,
        }
    }
}
