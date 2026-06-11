use crate::app::gpu;

pub fn create_render_target(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });

    let view = texture.create_view(&Default::default());

    (texture, view)
}

pub struct Pass {
    pub pipeline: gpu::RenderPipeline,
    pub render_texture: wgpu::Texture,
    pub render_view: wgpu::TextureView,
}

impl Pass {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        config: &crate::app::config::Config,
        layouts: &[Option<&wgpu::BindGroupLayout>],
    ) -> Self {
        let pipeline = gpu::RenderPipeline::new(
            &device,
            format,
            None, // Some(wgpu::Face::Back),
            gpu::SHADER_MESH_SHADOW,
            layouts,
            true,
            true,
        );

        let (render_texture, render_view) =
            create_render_target(device, format, config.width, config.height);

        Self {
            pipeline,
            render_texture,
            render_view,
        }
    }

    pub fn resize(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat, width: u32, height: u32) {
        let (render_texture, render_view) = create_render_target(device, format, width, height);
        self.render_texture = render_texture;
        self.render_view = render_view;
    }

    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        depth_view: &wgpu::TextureView,
        light: &super::light_cube::Pass,
        meshes: &[gpu::MeshBuffer],
        bindings: &super::Bindings,
        config: &crate::app::config::Config,
    ) {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.render_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(config.background),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });

        if config.debug_light_cube_show {
            light.render(&mut render_pass, &meshes[0], bindings);
        }

        render_pass.set_pipeline(&self.pipeline.inner);

        bindings.all(&mut render_pass);

        for mesh in &meshes[1..] {
            mesh.render(&mut render_pass);
        }
    }
}
