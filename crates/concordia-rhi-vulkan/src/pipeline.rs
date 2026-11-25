//! Compute and graphics pipelines from SPIR-V, on the device's one pipeline layout, with dynamic
//! rendering (no render pass objects).

use std::ffi::CString;

use ash::vk;
use concordia_rhi::{ComputePipelineDesc, GraphicsPipelineDesc, RhiCaps, RhiError, ShaderSource};

use crate::convert;
use crate::spirv::{self, Model};

/// What a pipeline binds to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Compute,
    Graphics,
}

/// A live pipeline.
pub(crate) struct VkPipeline {
    pub raw: vk::Pipeline,
    pub kind: Kind,
}

/// A checked shader module and its entry point name.
struct Module<'d> {
    device: &'d ash::Device,
    raw: vk::ShaderModule,
    entry: CString,
}

impl Drop for Module<'_> {
    fn drop(&mut self) {
        // SAFETY: modules are only needed while their pipeline is created.
        unsafe { self.device.destroy_shader_module(self.raw, None) };
    }
}

fn compile_error(label: &str, log: impl Into<String>) -> RhiError {
    RhiError::PipelineCompile {
        label: label.to_owned(),
        log: log.into(),
    }
}

fn module<'d>(
    device: &'d ash::Device,
    source: &ShaderSource,
    model: Model,
    label: &str,
) -> Result<Module<'d>, RhiError> {
    let ShaderSource::SpirV { words, entry } = source else {
        return Err(RhiError::invalid(
            "shader source",
            format!(
                "{label:?}: the Vulkan backend takes SPIR-V (compile for Target::SpirV), not MSL"
            ),
        ));
    };
    spirv::check(words, entry, model).map_err(|log| compile_error(label, log))?;
    let entry = CString::new(entry.as_str())
        .map_err(|_| compile_error(label, "entry point name contains a nul byte"))?;
    let info = vk::ShaderModuleCreateInfo::default().code(words);
    // SAFETY: `words` passed the SPIR-V header and framing check above.
    let raw = unsafe { device.create_shader_module(&info, None) }
        .map_err(|e| compile_error(label, format!("vkCreateShaderModule: {e}")))?;
    Ok(Module { device, raw, entry })
}

fn stage<'a>(
    module: &'a Module<'_>,
    flag: vk::ShaderStageFlags,
) -> vk::PipelineShaderStageCreateInfo<'a> {
    vk::PipelineShaderStageCreateInfo::default()
        .stage(flag)
        .module(module.raw)
        .name(&module.entry)
}

/// A compute pipeline (the group size comes from the shader's `LocalSize`).
pub(crate) fn compute(
    device: &ash::Device,
    layout: vk::PipelineLayout,
    desc: &ComputePipelineDesc,
) -> Result<VkPipeline, RhiError> {
    if desc.threads_per_group.contains(&0) {
        return Err(RhiError::invalid(
            "compute pipeline",
            format!("{:?}: zero threads per group", desc.label),
        ));
    }
    let kernel = module(device, &desc.shader, Model::Compute, &desc.label)?;
    let info = vk::ComputePipelineCreateInfo::default()
        .stage(stage(&kernel, vk::ShaderStageFlags::COMPUTE))
        .layout(layout);
    // SAFETY: the module and layout are live; `info` is valid for the call.
    let made = unsafe { device.create_compute_pipelines(vk::PipelineCache::null(), &[info], None) };
    let raw = made
        .map_err(|(_, e)| compile_error(&desc.label, format!("vkCreateComputePipelines: {e}")))?;
    Ok(VkPipeline {
        raw: raw[0],
        kind: Kind::Compute,
    })
}

fn check_graphics(desc: &GraphicsPipelineDesc, caps: &RhiCaps) -> Result<(), RhiError> {
    let bad = |reason: String| {
        Err(RhiError::invalid(
            "graphics pipeline",
            format!("{:?}: {reason}", desc.label),
        ))
    };
    if desc.color_formats.len() > 8 {
        return bad(format!(
            "{} color targets; at most 8",
            desc.color_formats.len()
        ));
    }
    if desc.color_formats.is_empty() && desc.depth.is_none() {
        return bad("no color or depth target".into());
    }
    if desc.fragment.is_none() && !desc.color_formats.is_empty() {
        return bad("color targets need a fragment shader".into());
    }
    if let Some(f) = desc.color_formats.iter().find(|f| f.is_depth()) {
        return bad(format!("{f:?} is a depth format in a color slot"));
    }
    if desc.depth.is_some_and(|d| !d.format.is_depth()) {
        return bad("the depth state's format is not a depth format".into());
    }
    if !caps.msaa_sample_counts.contains(&desc.samples) {
        return Err(RhiError::Unsupported(format!(
            "{}× MSAA on {}",
            desc.samples.get(),
            caps.device_name
        )));
    }
    Ok(())
}

/// A graphics pipeline: vertex pulling, triangle lists, dynamic viewport and scissor.
pub(crate) fn graphics(
    device: &ash::Device,
    layout: vk::PipelineLayout,
    desc: &GraphicsPipelineDesc,
    caps: &RhiCaps,
) -> Result<VkPipeline, RhiError> {
    check_graphics(desc, caps)?;
    let vertex = module(device, &desc.vertex, Model::Vertex, &desc.label)?;
    let fragment = desc
        .fragment
        .as_ref()
        .map(|f| module(device, f, Model::Fragment, &desc.label))
        .transpose()?;
    let mut stages = vec![stage(&vertex, vk::ShaderStageFlags::VERTEX)];
    stages.extend(
        fragment
            .as_ref()
            .map(|f| stage(f, vk::ShaderStageFlags::FRAGMENT)),
    );
    let formats: Vec<vk::Format> = desc
        .color_formats
        .iter()
        .map(|f| convert::format(*f))
        .collect();
    let depth_format = desc
        .depth
        .map_or(vk::Format::UNDEFINED, |d| convert::format(d.format));
    let mut rendering = vk::PipelineRenderingCreateInfo::default()
        .color_attachment_formats(&formats)
        .depth_attachment_format(depth_format);
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();
    let assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
    let viewport = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    let raster = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(convert::cull(desc.cull))
        .front_face(convert::FRONT_FACE)
        .line_width(1.0);
    let multisample = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(convert::samples(desc.samples));
    let depth = desc
        .depth
        .map_or_else(vk::PipelineDepthStencilStateCreateInfo::default, |d| {
            vk::PipelineDepthStencilStateCreateInfo::default()
                .depth_test_enable(true)
                .depth_write_enable(d.write)
                .depth_compare_op(convert::compare(d.compare))
        });
    let blends = vec![convert::blend(desc.blend); formats.len()];
    let blend = vk::PipelineColorBlendStateCreateInfo::default().attachments(&blends);
    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);
    let info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&assembly)
        .viewport_state(&viewport)
        .rasterization_state(&raster)
        .multisample_state(&multisample)
        .depth_stencil_state(&depth)
        .color_blend_state(&blend)
        .dynamic_state(&dynamic)
        .layout(layout)
        .push_next(&mut rendering);
    // SAFETY: the modules, layout and every state struct are live and valid for the call.
    let made =
        unsafe { device.create_graphics_pipelines(vk::PipelineCache::null(), &[info], None) };
    let raw = made
        .map_err(|(_, e)| compile_error(&desc.label, format!("vkCreateGraphicsPipelines: {e}")))?;
    Ok(VkPipeline {
        raw: raw[0],
        kind: Kind::Graphics,
    })
}
