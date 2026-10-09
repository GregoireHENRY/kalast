//! Text with a hard edge, for `hud.antialias` off.
//!
//! The text library draws glyphs from coverage, smooth at every edge, and
//! takes no shader of its own. So the text is drawn on a transparent layer
//! instead, and copied onto the image by a full-screen triangle that keeps a
//! pixel where the glyph covers half of it and drops it elsewhere.

use crate::app::gpu;

/// The copy's pipeline, and the layers it copies from: one a size, since the
/// text is drawn at the image's size and at the window's, two at most.
pub struct TextLayer {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
    layers: Vec<Layer>,
}

struct Layer {
    size: (u32, u32),
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
}

impl TextLayer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("text layer"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let shader = device.create_shader_module(gpu::SHADER_TEXT_LAYER);
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("text layer"),
            bind_group_layouts: &[Some(&layout)],
            ..Default::default()
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("text layer"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            // Written whole or not at all, so nothing to blend.
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        Self { pipeline, layout, format, layers: Vec::new() }
    }

    /// The layer for text `size` pixels, made the first time that size is
    /// asked for: which one, for `view` and `copy`.
    pub fn prepare(&mut self, device: &wgpu::Device, size: (u32, u32)) -> usize {
        let size = (size.0.max(1), size.1.max(1));
        if let Some(i) = self.layers.iter().position(|l| l.size == size) {
            return i;
        }
        if self.layers.len() >= 2 {
            self.layers.remove(0);
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("text layer"),
            size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            // The image's own format, which the text brush draws in.
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("text layer"),
            layout: &self.layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) }],
        });
        self.layers.push(Layer { size, _texture: texture, view, bind_group });
        self.layers.len() - 1
    }

    /// Where the text is drawn: layer `i`, cleared to nothing first.
    pub fn view(&self, i: usize) -> &wgpu::TextureView {
        &self.layers[i].view
    }

    /// Layer `i` onto `target`, with a hard edge.
    pub fn copy(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, i: usize) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("text layer"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.layers[i].bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}
