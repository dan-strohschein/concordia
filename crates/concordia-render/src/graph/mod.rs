//! The render graph: passes declare how they use textures; `compile` orders and culls passes,
//! derives barriers and plans transient memory (all on the CPU, deterministic).

mod compile;
mod dot;

use concordia_rhi::{Access, AccessFlags, Layout, Stages, Texture, TextureDesc};

pub use compile::{CompiledGraph, GraphBarrier, GraphError, Placement};

/// A texture in the graph (transient or imported).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GraphTexture(pub(crate) u32);

/// How a pass uses a texture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Use {
    /// Color attachment write.
    ColorWrite,
    /// Depth attachment write (and test).
    DepthWrite,
    /// Depth test only.
    DepthRead,
    /// Sampled in fragment or compute shaders.
    SampledRead,
    /// Read as a storage texture.
    StorageRead,
    /// Written as a storage texture.
    StorageWrite,
    /// Copy source.
    CopySrc,
    /// Copy destination.
    CopyDst,
}

impl Use {
    /// True if the use writes the texture.
    pub fn is_write(self) -> bool {
        matches!(
            self,
            Self::ColorWrite | Self::DepthWrite | Self::StorageWrite | Self::CopyDst
        )
    }

    /// The RHI access this use needs.
    pub fn access(self) -> Access {
        let (stages, access, layout) = match self {
            Self::ColorWrite => (
                Stages::COLOR_OUTPUT,
                AccessFlags::COLOR_WRITE,
                Layout::ColorAttachment,
            ),
            Self::DepthWrite => (
                Stages::DEPTH,
                AccessFlags::DEPTH_READ | AccessFlags::DEPTH_WRITE,
                Layout::DepthAttachment,
            ),
            Self::DepthRead => (
                Stages::DEPTH,
                AccessFlags::DEPTH_READ,
                Layout::DepthAttachment,
            ),
            Self::SampledRead => (
                Stages::FRAGMENT | Stages::COMPUTE,
                AccessFlags::SHADER_READ,
                Layout::ShaderRead,
            ),
            Self::StorageRead => (Stages::COMPUTE, AccessFlags::SHADER_READ, Layout::General),
            Self::StorageWrite => (
                Stages::COMPUTE,
                AccessFlags::SHADER_READ | AccessFlags::SHADER_WRITE,
                Layout::General,
            ),
            Self::CopySrc => (Stages::COPY, AccessFlags::COPY_READ, Layout::CopySrc),
            Self::CopyDst => (Stages::COPY, AccessFlags::COPY_WRITE, Layout::CopyDst),
        };
        Access {
            stages,
            access,
            layout,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum ResourceKind {
    Transient,
    Imported {
        texture: Texture,
        initial: Layout,
        final_layout: Option<Layout>,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct Resource {
    pub(crate) desc: TextureDesc,
    pub(crate) kind: ResourceKind,
}

/// Records what a pass does when executed (an error aborts `execute`, naming the pass).
pub type Record =
    Box<dyn Fn(&mut PassContext<'_>) -> Result<(), concordia_rhi::RhiError> + Send + Sync>;

pub(crate) struct Pass {
    pub(crate) name: String,
    pub(crate) uses: Vec<(GraphTexture, Use)>,
    pub(crate) side_effect: bool,
    pub(crate) record: Option<Record>,
}

/// Declares a pass's texture uses.
pub struct PassBuilder<'a> {
    pass: &'a mut Pass,
}

impl PassBuilder<'_> {
    /// The pass uses `texture` this way.
    pub fn uses(&mut self, texture: GraphTexture, how: Use) {
        self.pass.uses.push((texture, how));
    }

    /// Keep the pass even if nothing reads its outputs (readbacks, captures).
    pub fn side_effect(&mut self) {
        self.pass.side_effect = true;
    }
}

/// What a pass's record function gets: the command list and real textures.
pub struct PassContext<'a> {
    /// Commands for the frame.
    pub commands: &'a mut concordia_rhi::CommandList,
    pub(crate) textures: &'a std::collections::BTreeMap<GraphTexture, Texture>,
}

impl PassContext<'_> {
    /// The RHI texture behind a graph texture.
    pub fn texture(&self, texture: GraphTexture) -> Texture {
        self.textures[&texture]
    }
}

/// A frame's passes and textures.
#[derive(Default)]
pub struct RenderGraph {
    pub(crate) resources: Vec<Resource>,
    pub(crate) passes: Vec<Pass>,
}

impl std::fmt::Debug for RenderGraph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names: Vec<&str> = self.passes.iter().map(|p| p.name.as_str()).collect();
        f.debug_struct("RenderGraph")
            .field("passes", &names)
            .field("textures", &self.resources.len())
            .finish()
    }
}

impl RenderGraph {
    /// An empty graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// A transient texture, allocated (possibly aliased) by the graph for this frame.
    pub fn create_texture(&mut self, desc: TextureDesc) -> GraphTexture {
        self.add(Resource {
            desc,
            kind: ResourceKind::Transient,
        })
    }

    /// An existing texture (swapchain, capture target, history); `final_layout` is applied after its last use.
    pub fn import_texture(
        &mut self,
        name: &str,
        texture: Texture,
        mut desc: TextureDesc,
        initial: Layout,
        final_layout: Option<Layout>,
    ) -> GraphTexture {
        desc.label = name.to_owned();
        self.add(Resource {
            desc,
            kind: ResourceKind::Imported {
                texture,
                initial,
                final_layout,
            },
        })
    }

    fn add(&mut self, resource: Resource) -> GraphTexture {
        self.resources.push(resource);
        GraphTexture(u32::try_from(self.resources.len() - 1).unwrap_or(u32::MAX))
    }

    /// Adds a pass that only declares uses (planning and tests).
    pub fn add_pass(&mut self, name: &str, setup: impl FnOnce(&mut PassBuilder<'_>)) {
        self.add_pass_recorded(name, setup, None);
    }

    /// Adds a pass with a function that records its commands at execution.
    pub fn add_pass_recorded(
        &mut self,
        name: &str,
        setup: impl FnOnce(&mut PassBuilder<'_>),
        record: Option<Record>,
    ) {
        let mut pass = Pass {
            name: name.to_owned(),
            uses: Vec::new(),
            side_effect: false,
            record,
        };
        setup(&mut PassBuilder { pass: &mut pass });
        self.passes.push(pass);
    }

    /// A texture's description.
    pub fn texture_desc(&self, t: GraphTexture) -> &TextureDesc {
        &self.resources[t.0 as usize].desc
    }

    pub(crate) fn name(&self, t: GraphTexture) -> &str {
        &self.resources[t.0 as usize].desc.label
    }
}
