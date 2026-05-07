use image::{GenericImageView, ImageBuffer, Rgba};
use wgpu::util::DeviceExt;

use crate::{Mat4, Vec3};

pub const SHADER_MAIN: wgpu::ShaderModuleDescriptor =
    wgpu::include_wgsl!("../../shaders/shader_main.wgsl");

pub const SHADER_TRIANGLE: wgpu::ShaderModuleDescriptor =
    wgpu::include_wgsl!("../../shaders/triangle.wgsl");

pub const SHADER_TRIANGLE_COLOR_XY: wgpu::ShaderModuleDescriptor =
    wgpu::include_wgsl!("../../shaders/triangle_color_xy.wgsl");

pub const SHADER_VERTICES_COLOR: wgpu::ShaderModuleDescriptor =
    wgpu::include_wgsl!("../../shaders/vertices_color.wgsl");

pub const SHADER_VERTICES_TEX: wgpu::ShaderModuleDescriptor =
    wgpu::include_wgsl!("../../shaders/vertices_tex.wgsl");

pub const SHADER_MESH: wgpu::ShaderModuleDescriptor =
    wgpu::include_wgsl!("../../shaders/mesh.wgsl");

pub const SHADER_MESH_SHADOW: wgpu::ShaderModuleDescriptor =
    wgpu::include_wgsl!("../../shaders/mesh_shadow.wgsl");

pub const SHADER_DEPTH_RENDER: wgpu::ShaderModuleDescriptor =
    wgpu::include_wgsl!("../../shaders/depth_render.wgsl");

pub const SHADER_LIGHT_RENDER: wgpu::ShaderModuleDescriptor =
    wgpu::include_wgsl!("../../shaders/light_render.wgsl");

pub const SHADER_SHADOW: wgpu::ShaderModuleDescriptor =
    wgpu::include_wgsl!("../../shaders/shadow.wgsl");

pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

pub struct Pipelines {
    pub main: RenderPipeline,
    pub more: Vec<RenderPipeline>,
}
pub struct RenderPipeline {
    pub inner: wgpu::RenderPipeline,
}

impl RenderPipeline {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        cull_mode: Option<wgpu::Face>,
        shader: wgpu::ShaderModuleDescriptor,
        bind_group_layouts: &[Option<&wgpu::BindGroupLayout>],
        depth_stencil: bool,
        fragment: bool,
    ) -> Self {
        // wireframe: bool,

        let shader = device.create_shader_module(shader);

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            bind_group_layouts,
            ..Default::default()
        });

        // let polygon_mode = wireframe
        //     .then(|| wgpu::PolygonMode::Line)
        //     .unwrap_or_else(|| wgpu::PolygonMode::Fill);

        let depth_stencil = (depth_stencil).then(|| wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState {
                // constant: 2, // bilinear filtering
                // slope_scale: 2.0,
                constant: 0, // bilinear filtering
                slope_scale: 0.0,
                clamp: 0.0,
            },
        });

        let fragment = if fragment {
            Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            })
        } else {
            None
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[crate::mesh::Vertex::desc(), MeshBuffer::desc()],
                compilation_options: Default::default(),
            },
            fragment,
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
            label: None,
        });

        Self { inner: pipeline }
    }
}

pub struct UniformBuffer<U: bytemuck::NoUninit> {
    pub uniform: U,
    pub buffer: wgpu::Buffer,
    pub layout: wgpu::BindGroupLayout,
}

impl<U: bytemuck::NoUninit> UniformBuffer<U> {
    pub fn new(device: &wgpu::Device, uniform: U) -> Self {
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            contents: bytemuck::cast_slice(&[uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            label: None,
        });

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
            label: None,
        });

        Self {
            uniform,
            buffer,
            layout,
        }
    }

    pub fn bind_group(&self, device: &wgpu::Device) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &self.layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: self.buffer.as_entire_binding(),
            }],
            label: None,
        })
    }
}

impl crate::mesh::Vertex {
    pub const ATTRIBS: [wgpu::VertexAttribute; 8] = wgpu::vertex_attr_array![
        0 => Float32x3,
        1 => Float32x2,
        2 => Float32x3,
        3 => Float32x3,
        4 => Float32x3,
        5 => Float32x3,
        6 => Uint32,
        7 => Uint32,
    ];

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct InstanceInput {
    pub mat: Mat4,

    normal: Mat4,
    //

    // instance color used in vertex if color mode is 1
    // pub color: Vec3,

    // Control vertex instance color
    // - 0: vertex color
    // - 1: instance color
    // pub color_mode: u32,
}

impl Default for InstanceInput {
    fn default() -> Self {
        Self {
            mat: Mat4::IDENTITY,
            normal: Mat4::IDENTITY,
            // color: Vec3::new(1.0, 1.0, 1.0),
            // color_mode: 0,
        }
    }
}

impl InstanceInput {
    pub fn new(mat: Mat4) -> Self {
        let mut instance = Self {
            mat,
            ..Default::default()
        };
        instance.compute_normal();
        instance
    }

    pub fn compute_normal(&mut self) {
        self.normal = self.mat.inverse().transpose();
    }
}

pub struct MeshBuffer {
    pub n_vertices: u32,
    pub n_indices: u32,
    pub is_flat: bool,

    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,

    pub instance_buffer: wgpu::Buffer,
}

impl MeshBuffer {
    // matrix model
    pub const ATTRIBS: [wgpu::VertexAttribute; 8] = wgpu::vertex_attr_array![
        8  => Float32x4,
        9  => Float32x4,
        10 => Float32x4,
        11 => Float32x4,
        12  => Float32x4,
        13  => Float32x4,
        14 => Float32x4,
        15 => Float32x4,
        // 16 => Float32x3,
        // 17 => Uint32,
    ];

    pub fn new(
        device: &wgpu::Device,
        vertices: &[crate::mesh::Vertex],
        indices: &[u32],
        instance: &InstanceInput,
        is_flat: bool,
    ) -> Self {
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
            label: None,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
            label: None,
        });

        let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            contents: bytemuck::bytes_of(instance),
            usage: wgpu::BufferUsages::VERTEX,
            label: None,
        });

        Self {
            n_vertices: vertices.len() as _,
            n_indices: indices.len() as _,
            is_flat,

            vertex_buffer,
            index_buffer,

            instance_buffer,
        }
    }

    // pub fn n_indices(&self) -> u32 {
    //     (self.index_buffer.size() / 4) as _
    // }

    pub fn update_instance_buffer(&mut self, device: &wgpu::Device, instance: &InstanceInput) {
        self.instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            contents: bytemuck::bytes_of(instance),
            usage: wgpu::BufferUsages::VERTEX,
            label: None,
        });
    }

    pub fn render(&self, pass: &mut wgpu::RenderPass) {
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);

        if self.is_flat {
            pass.draw(0..self.n_vertices, 0..1);
        } else {
            pass.draw_indexed(0..self.n_indices, 0, 0..1);
        }
    }

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<InstanceInput>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &Self::ATTRIBS,
        }
    }
}

pub struct Texture {
    pub inner: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    pub layout: Option<wgpu::BindGroupLayout>,
}

impl Texture {
    pub fn new_image_from_bytes(device: &wgpu::Device, queue: &wgpu::Queue, bytes: &[u8]) -> Self {
        let image = image::load_from_memory(bytes).unwrap();
        let rgba = image.to_rgba8();
        let dimensions = image.dimensions();
        let size = wgpu::Extent3d {
            width: dimensions.0,
            height: dimensions.1,
            depth_or_array_layers: 1,
        };

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
            label: None,
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * dimensions.0),
                rows_per_image: Some(dimensions.1),
            },
            size,
        );

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
            label: None,
        });

        Self {
            inner: texture,
            view,
            sampler,
            layout: Some(layout),
        }
    }

    pub fn load_image<P: AsRef<std::path::Path>>(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        path: P,
    ) -> Self {
        Self::new_image_from_bytes(device, queue, &std::fs::read(path.as_ref()).unwrap())
    }

    pub fn create_depth_texture_shadow_pass(
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> Self {
        let mut texture = Self::create_depth_texture(
            device,
            width,
            height,
            wgpu::FilterMode::Linear,
            Some(wgpu::CompareFunction::LessEqual),
        );

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    count: None,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    visibility: wgpu::ShaderStages::FRAGMENT,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    count: None,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    visibility: wgpu::ShaderStages::FRAGMENT,
                },
            ],
            label: None,
        });
        texture.layout = Some(layout);

        texture
    }

    pub fn create_depth_texture_shadow_debug(
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> Self {
        let mut texture =
            Self::create_depth_texture(device, width, height, wgpu::FilterMode::Nearest, None);

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    count: None,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    visibility: wgpu::ShaderStages::FRAGMENT,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    count: None,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    visibility: wgpu::ShaderStages::FRAGMENT,
                },
            ],
            label: None,
        });
        texture.layout = Some(layout);

        texture
    }

    pub fn create_depth_texture_render_debug(
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> Self {
        let mut texture =
            Self::create_depth_texture(device, width, height, wgpu::FilterMode::Nearest, None);

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    count: None,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    visibility: wgpu::ShaderStages::FRAGMENT,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    count: None,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    visibility: wgpu::ShaderStages::FRAGMENT,
                },
            ],
            label: None,
        });
        texture.layout = Some(layout);

        texture
    }

    pub fn create_depth_texture(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        mag_min_filter: wgpu::FilterMode,
        compare: Option<wgpu::CompareFunction>,
    ) -> Self {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
            label: None,
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: mag_min_filter,
            min_filter: mag_min_filter,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            compare,
            lod_min_clamp: 0.0,
            lod_max_clamp: 100.0,
            ..Default::default()
        });

        Self {
            inner: texture,
            view,
            sampler,
            layout: None,
        }
    }

    pub fn bind_group(&self, device: &wgpu::Device) -> Option<wgpu::BindGroup> {
        self.layout.as_ref().map(|layout| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&self.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
                label: None,
            })
        })
    }
}

pub fn color_vec3(c: &wgpu::Color) -> Vec3 {
    Vec3::new(c.r as _, c.g as _, c.b as _)
}

pub fn export_frame(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
) {
    let bytes_per_pixel = 4;
    let unpadded_bytes_per_row = bytes_per_pixel * width;
    let padding = (256 - unpadded_bytes_per_row % 256) % 256;
    let padded_bytes_per_row = unpadded_bytes_per_row + padding;

    let buffer_size = (padded_bytes_per_row * height) as wgpu::BufferAddress;

    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        size: buffer_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
        label: None,
    });

    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );

    queue.submit(Some(encoder.finish()));

    let buffer_slice = buffer.slice(..);
    buffer_slice.map_async(wgpu::MapMode::Read, |_| {});
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .unwrap();

    let data = buffer_slice.get_mapped_range();

    let mut pixels = vec![0u8; (width * height * 4) as usize];

    for y in 0..height as usize {
        let src_offset = y * padded_bytes_per_row as usize;
        let dst_offset = y * (width as usize * 4);

        pixels[dst_offset..dst_offset + (width as usize * 4)]
            .copy_from_slice(&data[src_offset..src_offset + (width as usize * 4)]);
    }

    drop(data);
    buffer.unmap();

    let img = ImageBuffer::<Rgba<u8>, _>::from_raw(width, height, pixels).unwrap();

    let mut ii = 0usize;
    let mut path;

    loop {
        path = std::path::PathBuf::from(format!("out/frames/{ii}.png"));

        if !path.exists() {
            break;
        }
        ii += 1;
    }

    img.save(path).unwrap();
}
