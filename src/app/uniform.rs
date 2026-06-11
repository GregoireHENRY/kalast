use crate::{Mat4, Vec3};

#[repr(C)]
#[derive(Debug, Copy, Clone, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Globals {
    // global color used in fragment if color mode is 1
    pub color: Vec3,

    // Control fragment color
    // - 0: vertex/instance color + lighting + shadow
    // - 1: vertex/instance color, no lighting, show raw color
    // - 2: globals color
    // - 3: same as 0 but without shadow
    // - else: default to 0
    pub color_mode: u32,

    // 0: convert srgb to linear to show raw color
    // 1: use srgb
    pub srgb_mode: u32,
    pub gamma: f32,

    pub ambient_strength: f32,
    pub light_cube_scale: f32,

    pub shadow_resolution: u32,
    pub shadow_bias_scale: f32,
    pub shadow_bias_minimum: f32,
    pub shadow_normal_offset_scale: f32,
    pub shadow_pcf: u32,

    pub extra: u32,

    pub _padding1: u32,
    pub _padding2: u32,
    // pub _padding3: u32,
    // pub _padding4: u32,
}

#[repr(C)]
#[derive(Debug, Copy, Clone, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct View {
    pub camera: Camera,
    pub light: Light,
}

#[repr(C)]
#[derive(Debug, Copy, Clone, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Camera {
    pub view_proj: Mat4,
}

#[repr(C)]
#[derive(Debug, Copy, Clone, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Light {
    pub view_proj: Mat4,
    pub pos: Vec3,
    pub _padding: u32,

    pub color: Vec3,
    pub _padding2: u32,
}

pub struct Uniforms {
    pub globals: super::gpu::UniformBuffer<Globals>,
    pub view: super::gpu::UniformBuffer<View>,
    pub shadow: super::gpu::Texture,
    // pub textures: Vec<super::gpu::Texture>,
}

impl Uniforms {
    pub fn layouts_all(&self) -> Vec<Option<&wgpu::BindGroupLayout>> {
        vec![
            Some(&self.globals.layout),
            Some(&self.view.layout),
            Some(&self.shadow.layout.as_ref().unwrap()),
            // Some(&self.textures[0].layout.as_ref().unwrap()),
        ]
    }

    pub fn layouts_for_shadow(&self) -> Vec<Option<&wgpu::BindGroupLayout>> {
        vec![Some(&self.globals.layout), Some(&self.view.layout)]
    }

    pub fn bindings(&self, device: &wgpu::Device) -> super::pass::Bindings {
        super::pass::Bindings {
            globals: self.globals.bind_group(device),
            view: self.view.bind_group(device),
            shadow: self.shadow.bind_group(device).unwrap(),
            // textures: self.textures[0].bind_group(device).unwrap(),
        }
    }
}
