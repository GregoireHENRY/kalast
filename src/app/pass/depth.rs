use crate::app::gpu;
use crate::{Vec2, Vec3};

const DEPTH_VERTICES: &[crate::mesh::Vertex] = &[
    crate::mesh::Vertex {
        pos: Vec3::new(0.0, 0.0, 0.0),
        tex: Vec2::new(0.0, 1.0),
        ..crate::mesh::Vertex::default()
    },
    crate::mesh::Vertex {
        pos: Vec3::new(1.0, 0.0, 0.0),
        tex: Vec2::new(1.0, 1.0),
        ..crate::mesh::Vertex::default()
    },
    crate::mesh::Vertex {
        pos: Vec3::new(1.0, 1.0, 0.0),
        tex: Vec2::new(1.0, 0.0),
        ..crate::mesh::Vertex::default()
    },
    crate::mesh::Vertex {
        pos: Vec3::new(0.0, 1.0, 0.0),
        tex: Vec2::new(0.0, 0.0),
        ..crate::mesh::Vertex::default()
    },
];

const DEPTH_INDICES: &[u32] = &[0, 1, 2, 0, 2, 3];

pub struct Pass {
    pub mesh: gpu::MeshBuffer,
    pub texture: gpu::Texture,
    pub bind_group: wgpu::BindGroup,

    pub pipeline: gpu::RenderPipeline,
}

impl Pass {
    pub fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> Self {
        let mesh = gpu::MeshBuffer::new(
            device,
            DEPTH_VERTICES,
            DEPTH_INDICES,
            &gpu::InstanceInput::default(),
            false,
            &[],
        );

        let texture = gpu::Texture::create_depth_texture_render_debug(device, width, height);
        let layouts = &[Some(texture.layout.as_ref().unwrap())];
        let bind_group = texture.bind_group(device).unwrap();

        let pipeline = gpu::RenderPipeline::new(
            &device,
            format,
            None,
            gpu::SHADER_DEPTH_RENDER,
            layouts,
            false,
            true,
            1,
            true,
        );

        Self {
            pipeline,

            mesh,

            texture,
            bind_group,
        }
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.texture = gpu::Texture::create_depth_texture_render_debug(device, width, height);
        self.bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: self.texture.layout.as_ref().unwrap(),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&self.texture.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.texture.sampler),
                },
            ],
            label: None,
        });
    }

    pub fn render(&self, view: &wgpu::TextureView, encoder: &mut wgpu::CommandEncoder) {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
            multiview_mask: None,
            label: None,
        });

        render_pass.set_pipeline(&self.pipeline.inner);

        render_pass.set_bind_group(0, &self.bind_group, &[]);

        self.mesh.render(&mut render_pass);
    }
}
