//! Reading the meshlet path's cull codes back (M4 4.4): which entity, view and meshlet each entry
//! slot holds, the per-stage counts `render.stats` reports, and the per-meshlet records
//! `Renderer::read_culling` returns. Each code word is the code in bits 0–7 and the meshlet's
//! triangles in bits 8–15 (`meshlet.slang`).

use concordia_core::id::EntityId;

use crate::cull::CullCode;

/// A run of entry slots: one instance's meshlets in one bucket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Span {
    pub(crate) entity: EntityId,
    /// 0 the camera, 1 + c shadow cascade c.
    pub(crate) view: u32,
    pub(crate) first: u32,
    pub(crate) meshlets: u32,
}

/// Where each entry slot of a frame came from, in slot order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CullLayout {
    pub(crate) spans: Vec<Span>,
    /// Entry slots in all buckets (code words to read back).
    pub(crate) entries: u32,
}

/// What GPU culling decided for one candidate meshlet (`Renderer::read_culling`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CulledMeshlet {
    /// The instance's entity.
    pub entity: EntityId,
    /// The view: 0 the camera, 1 + c shadow cascade c.
    pub view: u32,
    /// The meshlet's index in its mesh.
    pub meshlet: u32,
    /// The decision; `None` if the GPU wrote no known code.
    pub code: Option<CullCode>,
}

/// The per-stage counts of one frame's cull codes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct CullTally {
    /// Camera candidates.
    pub(crate) candidates: u64,
    pub(crate) after_instance_frustum: u64,
    pub(crate) after_frustum: u64,
    pub(crate) after_cone: u64,
    /// Camera meshlets drawn, and their triangles.
    pub(crate) drawn: u64,
    pub(crate) triangles: u64,
    /// Camera instances with at least one meshlet drawn, and with none.
    pub(crate) instances: u64,
    pub(crate) instances_culled: u64,
    /// Meshlets drawn into the shadow cascades.
    pub(crate) shadow_drawn: u64,
}

impl CullLayout {
    /// The layout of a plan's work `items` (instance, mesh row, first entry, bucket): `views`
    /// gives each bucket's view, `instances` each instance row's entity and meshlet count.
    pub(crate) fn new(
        items: &[[u32; 4]],
        views: &[u32],
        instances: &[Option<(EntityId, u32)>],
        entries: u32,
    ) -> Self {
        let spans = items
            .iter()
            .filter_map(|&[instance, _, first, bucket]| {
                let (entity, meshlets) = instances.get(instance as usize).copied().flatten()?;
                let view = *views.get(bucket as usize)?;
                Some(Span {
                    entity,
                    view,
                    first,
                    meshlets,
                })
            })
            .collect();
        Self { spans, entries }
    }
}

/// What the CPU decided for the M1 path's draws, which the GPU counts are added to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct CpuCounts {
    /// M1 draws in the camera's view.
    pub(crate) draws: u64,
    /// M1 draws outside it.
    pub(crate) culled: u64,
    /// Their triangles.
    pub(crate) triangles: u64,
}

/// A frame's code words on their way back: the readback buffer, its layout, the CPU counts.
#[derive(Debug, Clone)]
pub(crate) struct CullReadback {
    pub(crate) buffer: concordia_rhi::Buffer,
    pub(crate) layout: CullLayout,
    pub(crate) cpu: CpuCounts,
}

/// The code in a code word.
pub(crate) fn code_of(word: u32) -> Option<CullCode> {
    CullCode::from_u32(word & 0xff)
}

/// Counts `words` (one per slot of `layout`).
pub(crate) fn tally(words: &[u32], layout: &CullLayout) -> CullTally {
    let mut t = CullTally::default();
    for span in &layout.spans {
        let codes = span_words(words, span).map(|w| (code_of(w), w >> 8 & 0xff));
        if span.view > 0 {
            t.shadow_drawn += codes.filter(|(c, _)| *c == Some(CullCode::Drawn)).count() as u64;
            continue;
        }
        let mut drawn_here = 0;
        for (code, triangles) in codes {
            let reached = |stage: CullCode| code.is_some_and(|c| c > stage);
            t.after_instance_frustum += u64::from(reached(CullCode::Instance));
            t.after_frustum += u64::from(reached(CullCode::Frustum));
            t.after_cone += u64::from(reached(CullCode::Cone));
            if code == Some(CullCode::Drawn) {
                drawn_here += 1;
                t.triangles += u64::from(triangles);
            }
        }
        t.candidates += u64::from(span.meshlets);
        t.drawn += drawn_here;
        match drawn_here {
            0 => t.instances_culled += 1,
            _ => t.instances += 1,
        }
    }
    t
}

/// The code words of `span`'s slots (zero past the end of a short readback).
fn span_words<'a>(words: &'a [u32], span: &Span) -> impl Iterator<Item = u32> + 'a {
    (span.first..span.first + span.meshlets)
        .map(|slot| words.get(slot as usize).copied().unwrap_or(0))
}

/// One record per slot of `layout`, in slot order.
pub(crate) fn records(words: &[u32], layout: &CullLayout) -> Vec<CulledMeshlet> {
    layout
        .spans
        .iter()
        .flat_map(|span| {
            span_words(words, span)
                .enumerate()
                .map(move |(m, w)| CulledMeshlet {
                    entity: span.entity,
                    view: span.view,
                    meshlet: m as u32,
                    code: code_of(w),
                })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(code: CullCode, triangles: u32) -> u32 {
        code as u32 | triangles << 8
    }

    fn layout() -> (CullLayout, [EntityId; 2]) {
        use concordia_core::id::{DeterministicIdGen, IdSource};
        let mut ids = DeterministicIdGen::new(7);
        let (a, b) = (ids.next_id().unwrap(), ids.next_id().unwrap());
        let spans = vec![
            Span {
                entity: a,
                view: 0,
                first: 0,
                meshlets: 3,
            },
            Span {
                entity: b,
                view: 0,
                first: 3,
                meshlets: 2,
            },
            Span {
                entity: a,
                view: 1,
                first: 5,
                meshlets: 3,
            },
        ];
        (CullLayout { spans, entries: 8 }, [a, b])
    }

    #[test]
    fn tally_counts_each_stage_for_the_camera_and_drawn_shadows() {
        let (layout, _) = layout();
        use CullCode::*;
        let words = [
            word(Drawn, 100),
            word(Cone, 90),
            word(Drawn, 28),
            word(Instance, 128),
            word(Instance, 64),
            word(Drawn, 100),
            word(Frustum, 90),
            word(Drawn, 28),
        ];
        assert_eq!(
            tally(&words, &layout),
            CullTally {
                candidates: 5,
                after_instance_frustum: 3,
                after_frustum: 3,
                after_cone: 2,
                drawn: 2,
                triangles: 128,
                instances: 1,
                instances_culled: 1,
                shadow_drawn: 2,
            }
        );
    }

    #[test]
    fn records_name_each_slots_entity_view_and_meshlet() {
        let (layout, [a, b]) = layout();
        let words: Vec<u32> = (0..8).map(|i| word(CullCode::Drawn, i)).collect();
        let r = records(&words, &layout);
        assert_eq!(r.len(), 8);
        assert_eq!(
            r[4],
            CulledMeshlet {
                entity: b,
                view: 0,
                meshlet: 1,
                code: Some(CullCode::Drawn)
            }
        );
        assert_eq!(
            r[7],
            CulledMeshlet {
                entity: a,
                view: 1,
                meshlet: 2,
                code: Some(CullCode::Drawn)
            }
        );
        let unknown = records(&[0; 8], &layout);
        assert!(unknown.iter().all(|m| m.code.is_none()));
    }

    #[test]
    fn layouts_follow_the_work_items() {
        let (want, [a, b]) = layout();
        let items = [[4, 0, 0, 0], [7, 1, 3, 0], [4, 0, 5, 1], [9, 0, 8, 1]];
        let mut instances = vec![None; 10];
        instances[4] = Some((a, 3));
        instances[7] = Some((b, 2));
        let got = CullLayout::new(&items, &[0, 1], &instances, 8);
        assert_eq!(got, want, "an instance without an entity is skipped");
    }

    #[test]
    fn short_readbacks_count_only_what_they_hold() {
        let (layout, _) = layout();
        assert_eq!(
            tally(&[], &layout).candidates,
            5,
            "slots come from the layout"
        );
        assert_eq!(tally(&[], &layout).drawn, 0);
        assert!(records(&[], &layout).iter().all(|m| m.code.is_none()));
    }
}
