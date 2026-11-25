//! Hazard planning (pure; ADR-0088): every texture rests in a home layout chosen from its usage;
//! each step (render pass, dispatch, copy) moves the textures it names out of it and back, and a
//! global memory barrier separates consecutive steps, as Metal's tracked hazards do.

use std::collections::BTreeMap;

use concordia_rhi::{Command, Format, Layout, RenderPassDesc, Texture, TextureUsage};

/// What the planner knows about a texture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TexState {
    /// The layout it rests in between steps.
    pub home: Layout,
    /// False after an aliasing barrier: its next use discards the contents (from `Undefined`).
    pub defined: bool,
}

/// One layout transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Transition {
    /// The texture.
    pub texture: Texture,
    /// Layout before.
    pub from: Layout,
    /// Layout after.
    pub to: Layout,
}

/// What the encoder emits, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Op {
    /// A global memory barrier (all commands, all memory) plus these transitions.
    Barrier(Vec<Transition>),
    /// Translate command `n` of the list.
    Command(usize),
}

/// The layout a texture rests in: storage → `General`, sampled → `ShaderRead`, attachment-only →
/// its attachment layout, copy-only → `General`.
pub(crate) fn home(usage: TextureUsage, format: Format) -> Layout {
    if usage.contains(TextureUsage::STORAGE) {
        Layout::General
    } else if usage.contains(TextureUsage::SAMPLED) {
        Layout::ShaderRead
    } else if usage.contains(TextureUsage::RENDER_TARGET) {
        match format.is_depth() {
            true => Layout::DepthAttachment,
            false => Layout::ColorAttachment,
        }
    } else {
        Layout::General
    }
}

/// The layout a copy uses: `General` when the texture rests there, else `optimal`.
pub(crate) fn copy_layout(home: Layout, optimal: Layout) -> Layout {
    match home {
        Layout::General => Layout::General,
        _ => optimal,
    }
}

/// Plans a validated command list. `textures` holds every texture it names (else `Err(texture)`)
/// and is updated: aliasing barriers leave their texture undefined, and named uses define it.
pub(crate) fn plan(
    commands: &[Command],
    textures: &mut BTreeMap<Texture, TexState>,
) -> Result<Vec<Op>, Texture> {
    let mut planner = Planner {
        textures,
        ops: Vec::new(),
        pending: Vec::new(),
    };
    let mut at = 0;
    while at < commands.len() {
        at = planner.command(commands, at)?;
    }
    planner.flush_returns();
    Ok(planner.ops)
}

struct Planner<'a> {
    textures: &'a mut BTreeMap<Texture, TexState>,
    ops: Vec<Op>,
    /// Transitions back to home layouts owed by the previous step.
    pending: Vec<Transition>,
}

impl Planner<'_> {
    /// Plans the command at `at`; returns the index of the next unplanned command.
    fn command(&mut self, commands: &[Command], at: usize) -> Result<usize, Texture> {
        match &commands[at] {
            Command::BeginRenderPass(desc) => self.pass(commands, at, desc),
            Command::Dispatch { .. }
            | Command::DispatchIndirect { .. }
            | Command::CopyBuffer { .. } => self.step(at, &[]),
            Command::CopyBufferToTexture { dst, .. } => {
                let needed = self.copy_layout(*dst, Layout::CopyDst)?;
                self.step(at, &[(*dst, needed)])
            }
            Command::CopyTextureToBuffer { src, .. } => {
                let needed = self.copy_layout(*src, Layout::CopySrc)?;
                self.step(at, &[(*src, needed)])
            }
            Command::Barrier { textures, .. } => {
                for barrier in textures.iter().filter(|b| b.aliasing.is_some()) {
                    self.state(barrier.texture)?.defined = false;
                }
                Ok(at + 1)
            }
            _ => {
                self.ops.push(Op::Command(at));
                Ok(at + 1)
            }
        }
    }

    /// A render pass is one step: every command up to its `EndRenderPass`, with no barriers inside.
    fn pass(
        &mut self,
        commands: &[Command],
        at: usize,
        desc: &RenderPassDesc,
    ) -> Result<usize, Texture> {
        let mut needs: Vec<(Texture, Layout)> = Vec::new();
        for color in &desc.colors {
            needs.push((color.texture, Layout::ColorAttachment));
            needs.extend(color.resolve.map(|r| (r, Layout::ColorAttachment)));
        }
        needs.extend(
            desc.depth
                .as_ref()
                .map(|d| (d.texture, Layout::DepthAttachment)),
        );
        let end = commands[at..]
            .iter()
            .position(|c| matches!(c, Command::EndRenderPass))
            .map_or(commands.len() - 1, |n| at + n);
        self.enter(&needs)?;
        self.ops.extend((at..=end).map(Op::Command));
        self.leave(&needs)?;
        Ok(end + 1)
    }

    /// A one-command step.
    fn step(&mut self, at: usize, needs: &[(Texture, Layout)]) -> Result<usize, Texture> {
        self.enter(needs)?;
        self.ops.push(Op::Command(at));
        self.leave(needs)?;
        Ok(at + 1)
    }

    /// The barrier before a step: last step's returns, this step's entries, and the global barrier.
    fn enter(&mut self, needs: &[(Texture, Layout)]) -> Result<(), Texture> {
        let mut transitions = std::mem::take(&mut self.pending);
        for (texture, needed) in needs {
            let state = self.state(*texture)?;
            let from = if state.defined {
                state.home
            } else {
                Layout::Undefined
            };
            state.defined = true;
            if from != *needed {
                transitions.push(Transition {
                    texture: *texture,
                    from,
                    to: *needed,
                });
            }
        }
        self.ops.push(Op::Barrier(transitions));
        Ok(())
    }

    /// Owes a return to home for every texture this step moved.
    fn leave(&mut self, needs: &[(Texture, Layout)]) -> Result<(), Texture> {
        for (texture, used) in needs {
            let home = self.state(*texture)?.home;
            if *used != home {
                self.pending.push(Transition {
                    texture: *texture,
                    from: *used,
                    to: home,
                });
            }
        }
        Ok(())
    }

    /// Emits the returns still owed at the end of the list.
    fn flush_returns(&mut self) {
        if !self.pending.is_empty() {
            self.ops
                .push(Op::Barrier(std::mem::take(&mut self.pending)));
        }
    }

    fn copy_layout(&mut self, texture: Texture, optimal: Layout) -> Result<Layout, Texture> {
        Ok(copy_layout(self.state(texture)?.home, optimal))
    }

    fn state(&mut self, texture: Texture) -> Result<&mut TexState, Texture> {
        self.textures.get_mut(&texture).ok_or(texture)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use concordia_rhi::{
        Access, AccessFlags, Buffer, ColorAttachment, DepthAttachment, LoadOp, Stages, StoreOp,
        TextureBarrier,
    };
    use proptest::prelude::*;

    const SAMPLED_RT: Texture = Texture::from_raw(0, 0);
    const STORAGE: Texture = Texture::from_raw(1, 0);
    const RT_ONLY: Texture = Texture::from_raw(2, 0);
    const DEPTH: Texture = Texture::from_raw(3, 0);
    const MSAA: Texture = Texture::from_raw(4, 0);
    const BUF: Buffer = Buffer::from_raw(0, 0);

    fn textures() -> BTreeMap<Texture, TexState> {
        let rt = TextureUsage::RENDER_TARGET | TextureUsage::COPY_SRC;
        [
            (
                SAMPLED_RT,
                home(rt | TextureUsage::SAMPLED, Format::Rgba8Unorm),
            ),
            (
                STORAGE,
                home(
                    TextureUsage::STORAGE | TextureUsage::SAMPLED,
                    Format::Rgba8Unorm,
                ),
            ),
            (RT_ONLY, home(rt, Format::Rgba8Unorm)),
            (
                DEPTH,
                home(TextureUsage::RENDER_TARGET, Format::Depth32Float),
            ),
            (MSAA, home(TextureUsage::RENDER_TARGET, Format::Rgba8Unorm)),
        ]
        .into_iter()
        .map(|(t, home)| {
            (
                t,
                TexState {
                    home,
                    defined: true,
                },
            )
        })
        .collect()
    }

    fn pass(color: Texture, resolve: Option<Texture>, depth: Option<Texture>) -> Command {
        Command::BeginRenderPass(RenderPassDesc {
            colors: vec![ColorAttachment {
                texture: color,
                resolve,
                load: LoadOp::Load,
                store: StoreOp::Store,
            }],
            depth: depth.map(|texture| DepthAttachment {
                texture,
                load: LoadOp::Load,
                store: StoreOp::Store,
            }),
        })
    }

    fn readback(src: Texture) -> Command {
        Command::CopyTextureToBuffer {
            src,
            mip: 0,
            dst: BUF,
            offset: 0,
            bytes_per_row: 16,
        }
    }

    fn t(texture: Texture, from: Layout, to: Layout) -> Transition {
        Transition { texture, from, to }
    }

    #[test]
    fn homes_follow_usage() {
        assert_eq!(
            home(
                TextureUsage::STORAGE | TextureUsage::SAMPLED,
                Format::R32Float
            ),
            Layout::General
        );
        assert_eq!(
            home(
                TextureUsage::SAMPLED | TextureUsage::RENDER_TARGET,
                Format::Rgba8Unorm
            ),
            Layout::ShaderRead
        );
        assert_eq!(
            home(TextureUsage::RENDER_TARGET, Format::Depth32Float),
            Layout::DepthAttachment
        );
        assert_eq!(
            home(
                TextureUsage::RENDER_TARGET | TextureUsage::COPY_SRC,
                Format::Rgba8Unorm
            ),
            Layout::ColorAttachment
        );
        assert_eq!(
            home(TextureUsage::COPY_DST, Format::Rgba8Unorm),
            Layout::General
        );
    }

    #[test]
    fn copy_after_dispatch_waits_for_shader_writes() {
        let list = [
            Command::Dispatch { groups: [1, 1, 1] },
            Command::CopyBuffer {
                src: BUF,
                src_offset: 0,
                dst: BUF,
                dst_offset: 64,
                size: 4,
            },
        ];
        let ops = plan(&list, &mut textures()).unwrap();
        assert_eq!(
            ops,
            [
                Op::Barrier(vec![]),
                Op::Command(0),
                Op::Barrier(vec![]),
                Op::Command(1)
            ]
        );
    }

    #[test]
    fn render_targets_return_to_their_home_layout() {
        let list = [pass(SAMPLED_RT, None, Some(DEPTH)), Command::EndRenderPass];
        let ops = plan(&list, &mut textures()).unwrap();
        assert_eq!(
            ops,
            [
                Op::Barrier(vec![t(
                    SAMPLED_RT,
                    Layout::ShaderRead,
                    Layout::ColorAttachment
                )]),
                Op::Command(0),
                Op::Command(1),
                Op::Barrier(vec![t(
                    SAMPLED_RT,
                    Layout::ColorAttachment,
                    Layout::ShaderRead
                )]),
            ]
        );
    }

    #[test]
    fn copy_from_a_render_target_transitions_and_returns() {
        let list = [
            pass(RT_ONLY, None, None),
            Command::EndRenderPass,
            readback(RT_ONLY),
        ];
        let ops = plan(&list, &mut textures()).unwrap();
        assert_eq!(
            ops,
            [
                Op::Barrier(vec![]),
                Op::Command(0),
                Op::Command(1),
                Op::Barrier(vec![t(RT_ONLY, Layout::ColorAttachment, Layout::CopySrc)]),
                Op::Command(2),
                Op::Barrier(vec![t(RT_ONLY, Layout::CopySrc, Layout::ColorAttachment)]),
            ]
        );
    }

    #[test]
    fn msaa_resolve_targets_are_attachments_for_the_pass() {
        let list = [pass(MSAA, Some(SAMPLED_RT), None), Command::EndRenderPass];
        let ops = plan(&list, &mut textures()).unwrap();
        assert_eq!(
            ops[0],
            Op::Barrier(vec![t(
                SAMPLED_RT,
                Layout::ShaderRead,
                Layout::ColorAttachment
            )])
        );
    }

    #[test]
    fn copies_into_general_textures_stay_general() {
        let list = [Command::CopyBufferToTexture {
            src: BUF,
            offset: 0,
            bytes_per_row: 16,
            dst: STORAGE,
            mip: 0,
        }];
        assert_eq!(
            plan(&list, &mut textures()).unwrap(),
            [Op::Barrier(vec![]), Op::Command(0)]
        );
    }

    #[test]
    fn aliasing_barrier_discards_from_undefined() {
        let none = Access {
            stages: Stages::empty(),
            access: AccessFlags::empty(),
            layout: Layout::Undefined,
        };
        let list = [
            Command::Barrier {
                buffers: vec![],
                textures: vec![TextureBarrier {
                    texture: RT_ONLY,
                    before: none,
                    after: none,
                    aliasing: Some(MSAA),
                }],
            },
            pass(RT_ONLY, None, None),
            Command::EndRenderPass,
        ];
        let mut states = textures();
        let ops = plan(&list, &mut states).unwrap();
        assert_eq!(
            ops[0],
            Op::Barrier(vec![t(RT_ONLY, Layout::Undefined, Layout::ColorAttachment)])
        );
        assert!(states[&RT_ONLY].defined, "the pass defines it again");
    }

    #[test]
    fn draws_inside_one_pass_need_no_barrier() {
        let draw = Command::Draw {
            vertices: 0..3,
            instances: 0..1,
        };
        let list = [
            pass(RT_ONLY, None, None),
            draw.clone(),
            Command::PushConstants(vec![0; 4]),
            draw,
            Command::EndRenderPass,
        ];
        let ops = plan(&list, &mut textures()).unwrap();
        assert_eq!(
            ops.iter().filter(|o| matches!(o, Op::Barrier(_))).count(),
            1,
            "{ops:?}"
        );
    }

    #[test]
    fn unknown_textures_are_reported() {
        let ghost = Texture::from_raw(99, 0);
        assert_eq!(plan(&[readback(ghost)], &mut textures()), Err(ghost));
    }

    fn arb_command() -> impl Strategy<Value = Vec<Command>> {
        let tex = prop::sample::select(vec![SAMPLED_RT, STORAGE, RT_ONLY]);
        prop_oneof![
            Just(vec![Command::Dispatch { groups: [1, 1, 1] }]),
            tex.clone().prop_map(|t| vec![readback(t)]),
            tex.clone()
                .prop_map(|dst| vec![Command::CopyBufferToTexture {
                    src: BUF,
                    offset: 0,
                    bytes_per_row: 16,
                    dst,
                    mip: 0
                }]),
            (tex.clone(), any::<bool>()).prop_map(|(c, d)| vec![
                pass(c, None, d.then_some(DEPTH)),
                Command::Draw {
                    vertices: 0..3,
                    instances: 0..1
                },
                Command::EndRenderPass
            ]),
            tex.prop_map(|t| {
                let none = Access {
                    stages: Stages::empty(),
                    access: AccessFlags::empty(),
                    layout: Layout::Undefined,
                };
                vec![Command::Barrier {
                    buffers: vec![],
                    textures: vec![TextureBarrier {
                        texture: t,
                        before: none,
                        after: none,
                        aliasing: Some(MSAA),
                    }],
                }]
            }),
        ]
    }

    /// The layout each command needs for each texture it names.
    fn needs(
        command: &Command,
        states: &BTreeMap<Texture, TexState>,
    ) -> Vec<(Texture, Vec<Layout>)> {
        let copy = |t: Texture, l: Layout| match states[&t].home {
            Layout::General => vec![Layout::General],
            _ => vec![l],
        };
        match command {
            Command::BeginRenderPass(desc) => {
                let mut v: Vec<_> = desc
                    .colors
                    .iter()
                    .map(|c| (c.texture, vec![Layout::ColorAttachment]))
                    .collect();
                v.extend(
                    desc.depth
                        .iter()
                        .map(|d| (d.texture, vec![Layout::DepthAttachment])),
                );
                v
            }
            Command::CopyTextureToBuffer { src, .. } => vec![(*src, copy(*src, Layout::CopySrc))],
            Command::CopyBufferToTexture { dst, .. } => vec![(*dst, copy(*dst, Layout::CopyDst))],
            _ => vec![],
        }
    }

    proptest! {
        #[test]
        fn every_command_sees_the_layout_it_needs(steps in prop::collection::vec(arb_command(), 0..12)) {
            let list: Vec<Command> = steps.into_iter().flatten().collect();
            let mut states = textures();
            let start = states.clone();
            let ops = plan(&list, &mut states).unwrap();
            let mut actual: BTreeMap<Texture, Layout> = start.iter().map(|(t, s)| (*t, s.home)).collect();
            let mut in_pass = false;
            for op in &ops {
                match op {
                    Op::Barrier(transitions) => {
                        prop_assert!(!in_pass, "no barrier inside a pass: {ops:?}");
                        for tr in transitions {
                            prop_assert!(tr.from == Layout::Undefined || actual[&tr.texture] == tr.from, "{tr:?} from {:?}", actual[&tr.texture]);
                            actual.insert(tr.texture, tr.to);
                        }
                    }
                    Op::Command(n) => {
                        for (texture, layouts) in needs(&list[*n], &start) {
                            prop_assert!(layouts.contains(&actual[&texture]), "{:?} needs {layouts:?} for {texture:?}, has {:?}", list[*n], actual[&texture]);
                        }
                        in_pass = match &list[*n] {
                            Command::BeginRenderPass(_) => true,
                            Command::EndRenderPass => false,
                            _ => in_pass,
                        };
                    }
                }
            }
            let planned: Vec<usize> = ops.iter().filter_map(|o| match o { Op::Command(n) => Some(*n), _ => None }).collect();
            let expected: Vec<usize> = (0..list.len()).filter(|n| !matches!(list[*n], Command::Barrier { .. })).collect();
            prop_assert_eq!(planned, expected, "every command once, in order");
            for (texture, state) in &start {
                prop_assert_eq!(actual[texture], state.home, "{:?} ends at home", texture);
            }
        }
    }
}
