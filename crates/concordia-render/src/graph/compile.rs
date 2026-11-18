//! Compiling a graph: culling, dependency order, barriers, lifetimes and transient memory.

use std::collections::{BTreeMap, BTreeSet};

use concordia_rhi::{Access, AccessFlags, Layout, Stages, TextureDesc};

use concordia_rhi::{Command, CommandList, Texture, TextureBarrier};

use super::{GraphTexture, PassContext, RenderGraph, ResourceKind};

/// The graph cannot be compiled.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum GraphError {
    /// A transient texture is read but no pass writes it.
    #[error("texture {0:?} is read but never written")]
    NeverWritten(String),
    /// Passes depend on each other in a cycle.
    #[error("passes form a dependency cycle involving {0:?}")]
    Cycle(String),
    /// `execute` was not given an RHI texture for a transient.
    #[error("no RHI texture was provided for transient texture {0:?}")]
    MissingTransient(String),
    /// A pass's record function failed.
    #[error("pass {pass:?} failed to record: {error}")]
    Record {
        /// The pass.
        pass: String,
        /// The device error.
        error: concordia_rhi::RhiError,
    },
}

/// A barrier before a pass (or after the last one).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphBarrier {
    /// The texture.
    pub texture: GraphTexture,
    /// Access before.
    pub before: Access,
    /// Access after.
    pub after: Access,
    /// The texture that previously occupied the same memory, if this starts an aliased use.
    pub aliasing: Option<GraphTexture>,
}

/// Where a transient texture lives in the frame heap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    /// Byte offset.
    pub offset: u64,
    /// Byte size.
    pub size: u64,
    /// First and last pass position using it.
    pub lifetime: (usize, usize),
}

/// A compiled graph: pass order, barriers and the transient memory plan.
#[derive(Debug, Clone, PartialEq)]
pub struct CompiledGraph {
    pub(crate) order: Vec<usize>,
    pub(crate) names: Vec<String>,
    pub(crate) barriers: Vec<Vec<GraphBarrier>>,
    pub(crate) final_barriers: Vec<GraphBarrier>,
    pub(crate) placements: BTreeMap<GraphTexture, Placement>,
    pub(crate) labels: BTreeMap<GraphTexture, String>,
    pub(crate) heap_size: u64,
}

impl CompiledGraph {
    /// Executed pass names in order.
    pub fn pass_names(&self) -> Vec<&str> {
        self.names.iter().map(String::as_str).collect()
    }

    /// Barriers recorded before the named pass.
    pub fn barriers_before(&self, pass: &str) -> Vec<GraphBarrier> {
        self.names
            .iter()
            .position(|n| n == pass)
            .map(|i| self.barriers[i].clone())
            .unwrap_or_default()
    }

    /// Barriers after the last pass (imported textures' final layouts).
    pub fn final_barriers(&self) -> &[GraphBarrier] {
        &self.final_barriers
    }

    /// Bytes of transient memory the frame needs.
    pub fn heap_size(&self) -> u64 {
        self.heap_size
    }

    /// Transient placements.
    pub fn placements(&self) -> &BTreeMap<GraphTexture, Placement> {
        &self.placements
    }

    /// Records the frame: each pass's barriers, then its commands; then the final barriers.
    /// `transients` maps every transient texture to the RHI texture placed for it (see `placements`).
    pub fn execute(
        &self,
        graph: &RenderGraph,
        transients: &BTreeMap<GraphTexture, Texture>,
        commands: &mut CommandList,
    ) -> Result<(), GraphError> {
        let textures = self.resolve(graph, transients)?;
        for (pos, &p) in self.order.iter().enumerate() {
            push_barriers(commands, &self.barriers[pos], &textures);
            if let Some(record) = &graph.passes[p].record {
                record(&mut PassContext {
                    commands,
                    textures: &textures,
                })
                .map_err(|error| GraphError::Record {
                    pass: graph.passes[p].name.clone(),
                    error,
                })?;
            }
        }
        push_barriers(commands, &self.final_barriers, &textures);
        Ok(())
    }

    /// Imported textures plus the caller's transients; every transient used must be present.
    fn resolve(
        &self,
        graph: &RenderGraph,
        transients: &BTreeMap<GraphTexture, Texture>,
    ) -> Result<BTreeMap<GraphTexture, Texture>, GraphError> {
        let mut textures = BTreeMap::new();
        for (i, r) in graph.resources.iter().enumerate() {
            let t = GraphTexture(i as u32);
            match r.kind {
                ResourceKind::Imported { texture, .. } => {
                    textures.insert(t, texture);
                }
                ResourceKind::Transient if self.placements.contains_key(&t) => {
                    let texture = transients
                        .get(&t)
                        .ok_or_else(|| GraphError::MissingTransient(r.desc.label.clone()))?;
                    textures.insert(t, *texture);
                }
                ResourceKind::Transient => {}
            }
        }
        Ok(textures)
    }

    /// The pass indices (in `RenderGraph` insertion order) in execution order.
    pub(crate) fn order(&self) -> &[usize] {
        &self.order
    }

    /// One line per transient: name, offset, size and lifetime (for snapshots and agents).
    pub fn memory_plan(&self) -> String {
        let mut lines: Vec<String> = self
            .placements
            .iter()
            .map(|(t, p)| {
                format!(
                    "{}: offset {} size {} passes {}..={}",
                    self.labels[t], p.offset, p.size, p.lifetime.0, p.lifetime.1
                )
            })
            .collect();
        lines.push(format!("heap: {} bytes", self.heap_size));
        lines.join("\n")
    }
}

impl RenderGraph {
    /// Compiles the graph; `footprint` gives a transient texture's (size, alignment) in a heap.
    pub fn compile(
        &self,
        footprint: &dyn Fn(&TextureDesc) -> (u64, u64),
    ) -> Result<CompiledGraph, GraphError> {
        self.check_written()?;
        let order = self.ordered(&self.needed())?;
        let positions = self.lifetimes(&order);
        let (placements, aliases, heap_size) = self.allocate(&positions, footprint);
        let (barriers, final_barriers) = self.barriers(&order, &aliases);
        Ok(CompiledGraph {
            names: order.iter().map(|p| self.passes[*p].name.clone()).collect(),
            labels: (0..self.resources.len())
                .map(|i| (GraphTexture(i as u32), self.resources[i].desc.label.clone()))
                .collect(),
            order,
            barriers,
            final_barriers,
            placements,
            heap_size,
        })
    }

    fn is_transient(&self, t: GraphTexture) -> bool {
        matches!(self.resources[t.0 as usize].kind, ResourceKind::Transient)
    }

    fn check_written(&self) -> Result<(), GraphError> {
        let written: BTreeSet<GraphTexture> = self
            .passes
            .iter()
            .flat_map(|p| p.uses.iter())
            .filter(|(_, u)| u.is_write())
            .map(|(t, _)| *t)
            .collect();
        let unwritten = self
            .passes
            .iter()
            .flat_map(|p| p.uses.iter())
            .find(|(t, u)| !u.is_write() && self.is_transient(*t) && !written.contains(t));
        match unwritten {
            Some((t, _)) => Err(GraphError::NeverWritten(self.name(*t).to_owned())),
            None => Ok(()),
        }
    }

    /// Passes that must run: side effects, writers of imported textures, and what they read, transitively.
    fn needed(&self) -> BTreeSet<usize> {
        let mut needed: BTreeSet<usize> = (0..self.passes.len())
            .filter(|&i| {
                self.passes[i].side_effect
                    || self.passes[i]
                        .uses
                        .iter()
                        .any(|(t, u)| u.is_write() && !self.is_transient(*t))
            })
            .collect();
        loop {
            let reads: BTreeSet<GraphTexture> = needed
                .iter()
                .flat_map(|&i| self.passes[i].uses.iter())
                .filter(|(_, u)| !u.is_write())
                .map(|(t, _)| *t)
                .collect();
            let more: Vec<usize> = (0..self.passes.len())
                .filter(|i| {
                    !needed.contains(i)
                        && self.passes[*i]
                            .uses
                            .iter()
                            .any(|(t, u)| u.is_write() && reads.contains(t))
                })
                .collect();
            if more.is_empty() {
                return needed;
            }
            needed.extend(more);
        }
    }

    /// Writers of a texture run in insertion order; readers run after every writer.
    fn edges(&self, needed: &BTreeSet<usize>) -> BTreeMap<usize, BTreeSet<usize>> {
        let mut deps: BTreeMap<usize, BTreeSet<usize>> =
            needed.iter().map(|&p| (p, BTreeSet::new())).collect();
        for t in 0..self.resources.len() as u32 {
            let t = GraphTexture(t);
            let writers: Vec<usize> = needed
                .iter()
                .copied()
                .filter(|&p| {
                    self.passes[p]
                        .uses
                        .iter()
                        .any(|(x, u)| *x == t && u.is_write())
                })
                .collect();
            for w in writers.windows(2) {
                deps.entry(w[1]).or_default().insert(w[0]);
            }
            for &p in needed {
                let reads = self.passes[p]
                    .uses
                    .iter()
                    .any(|(x, u)| *x == t && !u.is_write());
                if reads && !writers.contains(&p) {
                    deps.entry(p).or_default().extend(writers.iter().copied());
                }
            }
        }
        deps
    }

    /// Kahn's algorithm, always picking the earliest-inserted ready pass.
    fn ordered(&self, needed: &BTreeSet<usize>) -> Result<Vec<usize>, GraphError> {
        let mut deps = self.edges(needed);
        let mut order = Vec::with_capacity(needed.len());
        while !deps.is_empty() {
            let ready = deps.iter().find(|(_, d)| d.is_empty()).map(|(p, _)| *p);
            let Some(next) = ready else {
                let stuck = deps
                    .keys()
                    .next()
                    .map(|p| self.passes[*p].name.clone())
                    .unwrap_or_default();
                return Err(GraphError::Cycle(stuck));
            };
            deps.remove(&next);
            deps.values_mut().for_each(|d| {
                d.remove(&next);
            });
            order.push(next);
        }
        Ok(order)
    }

    /// First and last position (in `order`) of each transient texture.
    fn lifetimes(&self, order: &[usize]) -> BTreeMap<GraphTexture, (usize, usize)> {
        let mut spans: BTreeMap<GraphTexture, (usize, usize)> = BTreeMap::new();
        for (pos, &p) in order.iter().enumerate() {
            for (t, _) in &self.passes[p].uses {
                if self.is_transient(*t) {
                    let span = spans.entry(*t).or_insert((pos, pos));
                    span.1 = pos;
                }
            }
        }
        spans
    }

    /// First-fit slots by lifetime start: a slot is reused once its occupant's lifetime has ended.
    #[allow(clippy::type_complexity)]
    fn allocate(
        &self,
        lifetimes: &BTreeMap<GraphTexture, (usize, usize)>,
        footprint: &dyn Fn(&TextureDesc) -> (u64, u64),
    ) -> (
        BTreeMap<GraphTexture, Placement>,
        BTreeMap<GraphTexture, GraphTexture>,
        u64,
    ) {
        struct Slot {
            offset: u64,
            size: u64,
            free_after: usize,
            occupant: GraphTexture,
        }
        let mut by_start: Vec<(GraphTexture, (usize, usize))> =
            lifetimes.iter().map(|(t, l)| (*t, *l)).collect();
        by_start.sort_by_key(|(t, (start, _))| (*start, *t));
        let (mut slots, mut placements, mut aliases, mut heap) =
            (Vec::<Slot>::new(), BTreeMap::new(), BTreeMap::new(), 0u64);
        for (t, (start, end)) in by_start {
            let (size, align) = footprint(&self.resources[t.0 as usize].desc);
            let reuse = slots
                .iter_mut()
                .find(|s| s.free_after < start && s.size >= size && s.offset % align.max(1) == 0);
            let offset = match reuse {
                Some(slot) => {
                    aliases.insert(t, slot.occupant);
                    slot.free_after = end;
                    slot.occupant = t;
                    slot.offset
                }
                None => {
                    let offset = heap.div_ceil(align.max(1)) * align.max(1);
                    heap = offset + size;
                    slots.push(Slot {
                        offset,
                        size,
                        free_after: end,
                        occupant: t,
                    });
                    offset
                }
            };
            placements.insert(
                t,
                Placement {
                    offset,
                    size,
                    lifetime: (start, end),
                },
            );
        }
        (placements, aliases, heap)
    }

    /// A transient's first use transitions from `Undefined`; later conflicting uses get barriers;
    /// read-after-read with the same layout needs none.
    fn barriers(
        &self,
        order: &[usize],
        aliases: &BTreeMap<GraphTexture, GraphTexture>,
    ) -> (Vec<Vec<GraphBarrier>>, Vec<GraphBarrier>) {
        let undefined = |layout| Access {
            stages: Stages::empty(),
            access: AccessFlags::empty(),
            layout,
        };
        let mut state: BTreeMap<GraphTexture, (Access, bool)> = BTreeMap::new();
        for (i, r) in self.resources.iter().enumerate() {
            if let ResourceKind::Imported { initial, .. } = r.kind {
                state.insert(GraphTexture(i as u32), (undefined(initial), false));
            }
        }
        let mut per_pass = Vec::with_capacity(order.len());
        for &p in order {
            let mut barriers = Vec::new();
            for (t, how) in &self.passes[p].uses {
                let after = how.access();
                match state.get(t).copied() {
                    None => {
                        let aliasing = aliases.get(t).copied();
                        barriers.push(GraphBarrier {
                            texture: *t,
                            before: undefined(Layout::Undefined),
                            after,
                            aliasing,
                        });
                    }
                    Some((before, wrote))
                        if wrote || how.is_write() || before.layout != after.layout =>
                    {
                        barriers.push(GraphBarrier {
                            texture: *t,
                            before,
                            after,
                            aliasing: None,
                        });
                    }
                    Some((before, _)) => {
                        state.insert(*t, (before.merge(after), false));
                        continue;
                    }
                }
                state.insert(*t, (after, how.is_write()));
            }
            per_pass.push(barriers);
        }
        let finals = self
            .resources
            .iter()
            .enumerate()
            .filter_map(|(i, r)| match r.kind {
                ResourceKind::Imported {
                    final_layout: Some(layout),
                    ..
                } => {
                    let t = GraphTexture(i as u32);
                    let (before, _) = state.get(&t).copied()?;
                    (before.layout != layout).then_some(GraphBarrier {
                        texture: t,
                        before,
                        after: undefined(layout),
                        aliasing: None,
                    })
                }
                _ => None,
            })
            .collect();
        (per_pass, finals)
    }
}

/// One `Barrier` command for a group, if it is non-empty.
fn push_barriers(
    commands: &mut CommandList,
    barriers: &[GraphBarrier],
    textures: &BTreeMap<GraphTexture, Texture>,
) {
    if barriers.is_empty() {
        return;
    }
    let textures = barriers
        .iter()
        .map(|b| TextureBarrier {
            texture: textures[&b.texture],
            before: b.before,
            after: b.after,
            aliasing: b.aliasing.and_then(|a| textures.get(&a).copied()),
        })
        .collect();
    commands.push(Command::Barrier {
        buffers: Vec::new(),
        textures,
    });
}
