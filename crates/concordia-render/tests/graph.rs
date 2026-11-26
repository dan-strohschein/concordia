//! The render graph compiled on the CPU: culling, ordering, barriers, aliasing (M1 phase 1.5).

use std::num::NonZeroU32;

use concordia_render::graph::{CompiledGraph, GraphTexture, RenderGraph, Use};
use concordia_rhi::{Format, Layout, SampleCount, Texture, TextureDesc, TextureUsage};
use proptest::prelude::*;

fn desc(label: &str) -> TextureDesc {
    TextureDesc {
        width: NonZeroU32::new(64).unwrap(),
        height: NonZeroU32::new(64).unwrap(),
        mip_levels: NonZeroU32::MIN,
        samples: SampleCount::X1,
        format: Format::Rgba16Float,
        usage: TextureUsage::RENDER_TARGET | TextureUsage::SAMPLED,
        label: label.into(),
    }
}

/// Every texture needs 1 KiB aligned to 256.
fn footprint(_: &TextureDesc) -> (u64, u64) {
    (1024, 256)
}

fn compile(graph: &RenderGraph) -> CompiledGraph {
    graph.compile(&footprint).unwrap()
}

fn swapchain(graph: &mut RenderGraph) -> GraphTexture {
    graph.import_texture(
        "swapchain",
        Texture::from_raw(0, 0),
        desc("swapchain"),
        Layout::Undefined,
        Some(Layout::Present),
    )
}

#[test]
fn unused_pass_is_culled() {
    let mut g = RenderGraph::new();
    let out = swapchain(&mut g);
    let unused = g.create_texture(desc("unused"));
    g.add_pass("dead", |p| p.uses(unused, Use::ColorWrite));
    g.add_pass("final", |p| p.uses(out, Use::ColorWrite));
    let c = compile(&g);
    assert_eq!(c.pass_names(), ["final"]);
}

#[test]
fn side_effect_passes_are_kept() {
    let mut g = RenderGraph::new();
    let t = g.create_texture(desc("t"));
    g.add_pass("readback", |p| {
        p.uses(t, Use::ColorWrite);
        p.side_effect();
    });
    assert_eq!(compile(&g).pass_names(), ["readback"]);
}

#[test]
fn passes_are_ordered_by_dependencies_not_insertion() {
    let mut g = RenderGraph::new();
    let out = swapchain(&mut g);
    let hdr = g.create_texture(desc("hdr"));
    let shadow = g.create_texture(desc("shadow"));
    g.add_pass("tonemap", |p| {
        p.uses(hdr, Use::SampledRead);
        p.uses(out, Use::ColorWrite);
    });
    g.add_pass("forward", |p| {
        p.uses(shadow, Use::SampledRead);
        p.uses(hdr, Use::ColorWrite);
    });
    g.add_pass("shadow", |p| p.uses(shadow, Use::DepthWrite));
    assert_eq!(compile(&g).pass_names(), ["shadow", "forward", "tonemap"]);
}

/// A texture written by two passes with a read between them (M4 4.4b: pass 1's depth, the HZB
/// build, pass 2): the read sees the first write and runs before the second, which waits for it.
#[test]
fn a_read_between_two_writes_runs_between_them() {
    let mut g = RenderGraph::new();
    let out = swapchain(&mut g);
    let depth = g.create_texture(desc("depth"));
    g.add_pass("early", |p| p.uses(depth, Use::DepthWrite));
    g.add_pass("hzb", |p| {
        p.uses(depth, Use::SampledRead);
        p.side_effect();
    });
    g.add_pass("late", |p| {
        p.uses(depth, Use::DepthWrite);
        p.uses(out, Use::ColorWrite);
    });
    assert_eq!(compile(&g).pass_names(), ["early", "hzb", "late"]);
    let barriers = compile(&g).barriers_before("late");
    assert!(
        barriers.iter().any(|b| b.texture == depth),
        "the depth goes back to an attachment: {barriers:?}"
    );
}

#[test]
fn write_then_read_inserts_one_barrier_with_correct_access() {
    let mut g = RenderGraph::new();
    let out = swapchain(&mut g);
    let hdr = g.create_texture(desc("hdr"));
    g.add_pass("forward", |p| p.uses(hdr, Use::ColorWrite));
    g.add_pass("tonemap", |p| {
        p.uses(hdr, Use::SampledRead);
        p.uses(out, Use::ColorWrite);
    });
    let c = compile(&g);
    let barriers: Vec<_> = c
        .barriers_before("tonemap")
        .into_iter()
        .filter(|b| b.texture == hdr)
        .collect();
    assert_eq!(barriers.len(), 1, "{barriers:?}");
    assert_eq!(barriers[0].before.layout, Layout::ColorAttachment);
    assert_eq!(barriers[0].after.layout, Layout::ShaderRead);
}

#[test]
fn read_after_read_inserts_no_barrier() {
    let mut g = RenderGraph::new();
    let out = swapchain(&mut g);
    let t = g.create_texture(desc("t"));
    g.add_pass("write", |p| p.uses(t, Use::ColorWrite));
    g.add_pass("read1", |p| {
        p.uses(t, Use::SampledRead);
        p.uses(out, Use::ColorWrite);
    });
    g.add_pass("read2", |p| {
        p.uses(t, Use::SampledRead);
        p.uses(out, Use::ColorWrite);
    });
    let c = compile(&g);
    assert!(
        c.barriers_before("read2").iter().all(|b| b.texture != t),
        "{:?}",
        c.barriers_before("read2")
    );
}

#[test]
fn non_overlapping_transients_share_memory() {
    let mut g = RenderGraph::new();
    let out = swapchain(&mut g);
    let [a, b, c] = ["a", "b", "c"].map(|n| g.create_texture(desc(n)));
    g.add_pass("p0", |p| p.uses(a, Use::ColorWrite));
    g.add_pass("p1", |p| {
        p.uses(a, Use::SampledRead);
        p.uses(c, Use::ColorWrite);
    });
    g.add_pass("p2", |p| {
        p.uses(c, Use::SampledRead);
        p.uses(b, Use::ColorWrite);
    });
    g.add_pass("p3", |p| {
        p.uses(b, Use::SampledRead);
        p.uses(out, Use::ColorWrite);
    });
    let compiled = compile(&g);
    insta::assert_snapshot!(compiled.memory_plan());
    assert_eq!(
        compiled.heap_size(),
        2048,
        "lifetimes [0,1] [2,3] [1,2] fit two slots"
    );
}

#[test]
fn aliasing_inserts_aliasing_barrier_before_reuse() {
    let mut g = RenderGraph::new();
    let out = swapchain(&mut g);
    let a = g.create_texture(desc("a"));
    let b = g.create_texture(desc("b"));
    g.add_pass("p0", |p| p.uses(a, Use::ColorWrite));
    g.add_pass("p1", |p| {
        p.uses(a, Use::SampledRead);
        p.uses(out, Use::ColorWrite);
    });
    g.add_pass("p2", |p| p.uses(b, Use::ColorWrite));
    g.add_pass("p3", |p| {
        p.uses(b, Use::SampledRead);
        p.uses(out, Use::ColorWrite);
    });
    let c = compile(&g);
    let alias = c
        .barriers_before("p2")
        .into_iter()
        .find(|x| x.texture == b)
        .expect("barrier for b");
    assert_eq!(alias.aliasing, Some(a));
}

#[test]
fn imported_swapchain_ends_in_present_layout() {
    let mut g = RenderGraph::new();
    let out = swapchain(&mut g);
    g.add_pass("final", |p| p.uses(out, Use::ColorWrite));
    let c = compile(&g);
    let last = c.final_barriers();
    assert_eq!(last.len(), 1);
    assert_eq!(
        (last[0].texture, last[0].after.layout),
        (out, Layout::Present)
    );
}

#[test]
fn reading_a_never_written_transient_is_an_error() {
    let mut g = RenderGraph::new();
    let out = swapchain(&mut g);
    let t = g.create_texture(desc("t"));
    g.add_pass("read", |p| {
        p.uses(t, Use::SampledRead);
        p.uses(out, Use::ColorWrite);
    });
    assert!(
        g.compile(&footprint)
            .unwrap_err()
            .to_string()
            .contains("\"t\"")
    );
}

#[test]
fn graph_dump_to_dot_snapshot() {
    let mut g = RenderGraph::new();
    let out = swapchain(&mut g);
    let hdr = g.create_texture(desc("hdr"));
    let depth = g.create_texture(desc("depth"));
    g.add_pass("depth_prepass", |p| p.uses(depth, Use::DepthWrite));
    g.add_pass("forward", |p| {
        p.uses(depth, Use::DepthRead);
        p.uses(hdr, Use::ColorWrite);
    });
    g.add_pass("tonemap", |p| {
        p.uses(hdr, Use::SampledRead);
        p.uses(out, Use::ColorWrite);
    });
    insta::assert_snapshot!(compile(&g).to_dot(&g));
}

/// A random DAG: pass i reads some earlier textures and writes its own; the last pass writes the output.
fn random_graph(edges: &[Vec<bool>]) -> RenderGraph {
    let mut g = RenderGraph::new();
    let out = swapchain(&mut g);
    let textures: Vec<GraphTexture> = (0..edges.len())
        .map(|i| g.create_texture(desc(&format!("t{i}"))))
        .collect();
    for (i, reads) in edges.iter().enumerate() {
        let textures = textures.clone();
        let reads = reads.clone();
        g.add_pass(&format!("p{i}"), move |p| {
            for (j, read) in reads.iter().enumerate().take(i) {
                if *read {
                    p.uses(textures[j], Use::SampledRead);
                }
            }
            p.uses(textures[i], Use::ColorWrite);
        });
    }
    let last = textures.last().copied();
    g.add_pass("present", move |p| {
        if let Some(t) = last {
            p.uses(t, Use::SampledRead);
        }
        p.uses(out, Use::ColorWrite);
    });
    g
}

proptest! {
    #[test]
    fn graph_compile_is_deterministic(edges in proptest::collection::vec(proptest::collection::vec(any::<bool>(), 8), 1..8)) {
        let a = compile(&random_graph(&edges));
        let b = compile(&random_graph(&edges));
        prop_assert_eq!(format!("{a:?}"), format!("{b:?}"));
    }
}

mod execute {
    use std::collections::BTreeMap;

    use concordia_render::graph::{RenderGraph, Use};
    use concordia_rhi::{Command, CommandList, Layout, Texture};

    use super::{desc, footprint, swapchain};

    fn marker(tag: u8) -> concordia_render::graph::Record {
        Box::new(move |ctx| {
            ctx.commands.push(Command::PushConstants(vec![tag]));
            Ok(())
        })
    }

    #[test]
    fn execute_records_barriers_then_passes_in_order_then_final_barriers() {
        let mut g = RenderGraph::new();
        let out = swapchain(&mut g);
        let hdr = g.create_texture(desc("hdr"));
        g.add_pass_recorded(
            "tonemap",
            |p| {
                p.uses(hdr, Use::SampledRead);
                p.uses(out, Use::ColorWrite);
            },
            Some(marker(2)),
        );
        g.add_pass_recorded("forward", |p| p.uses(hdr, Use::ColorWrite), Some(marker(1)));
        let compiled = g.compile(&footprint).unwrap();
        let transients = BTreeMap::from([(hdr, Texture::from_raw(7, 1))]);
        let mut list = CommandList::new("frame");
        compiled.execute(&g, &transients, &mut list).unwrap();
        let kinds: Vec<String> = list
            .commands
            .iter()
            .map(|c| match c {
                Command::PushConstants(tag) => format!("pass{}", tag[0]),
                Command::Barrier { textures, .. } => format!("barrier{}", textures.len()),
                other => format!("{other:?}"),
            })
            .collect();
        assert_eq!(
            kinds,
            ["barrier1", "pass1", "barrier2", "pass2", "barrier1"]
        );
        let Command::Barrier { textures, .. } = list.commands.last().unwrap() else {
            unreachable!()
        };
        assert_eq!(
            (textures[0].texture, textures[0].after.layout),
            (Texture::from_raw(0, 0), Layout::Present)
        );
    }

    #[test]
    fn pass_context_resolves_graph_textures_to_rhi_textures() {
        let mut g = RenderGraph::new();
        let out = swapchain(&mut g);
        let seen = std::sync::Arc::new(std::sync::Mutex::new(None));
        let sink = seen.clone();
        g.add_pass_recorded(
            "final",
            |p| p.uses(out, Use::ColorWrite),
            Some(Box::new(move |ctx| {
                *sink.lock().unwrap() = Some(ctx.texture(out));
                Ok(())
            })),
        );
        let compiled = g.compile(&footprint).unwrap();
        compiled
            .execute(&g, &BTreeMap::new(), &mut CommandList::new("frame"))
            .unwrap();
        assert_eq!(*seen.lock().unwrap(), Some(Texture::from_raw(0, 0)));
    }

    #[test]
    fn missing_transient_texture_is_an_error_naming_it() {
        let mut g = RenderGraph::new();
        let out = swapchain(&mut g);
        let hdr = g.create_texture(desc("hdr"));
        g.add_pass("forward", |p| p.uses(hdr, Use::ColorWrite));
        g.add_pass("tonemap", |p| {
            p.uses(hdr, Use::SampledRead);
            p.uses(out, Use::ColorWrite);
        });
        let compiled = g.compile(&footprint).unwrap();
        let err = compiled
            .execute(&g, &BTreeMap::new(), &mut CommandList::new("frame"))
            .unwrap_err();
        assert!(err.to_string().contains("\"hdr\""), "{err}");
    }
}

#[test]
fn record_errors_abort_execute_and_name_the_pass() {
    use std::collections::BTreeMap;
    let mut g = RenderGraph::new();
    let out = swapchain(&mut g);
    g.add_pass_recorded(
        "broken",
        |p| p.uses(out, Use::ColorWrite),
        Some(Box::new(|_| {
            Err(concordia_rhi::RhiError::DeviceLost("test".into()))
        })),
    );
    let compiled = g.compile(&footprint).unwrap();
    let err = compiled
        .execute(
            &g,
            &BTreeMap::new(),
            &mut concordia_rhi::CommandList::new("frame"),
        )
        .unwrap_err();
    assert!(err.to_string().contains("\"broken\""), "{err}");
}
