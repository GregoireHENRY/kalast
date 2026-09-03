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

    // 0: shaded mesh only
    // 1: wireframe only
    // 2: wireframe over the shaded mesh
    pub wireframe_mode: u32,
    // Half-width in pixels (screen space).
    pub wireframe_width: f32,

    // Must start on a 16-byte boundary (WGSL vec3 alignment), which the four
    // u32/f32 rows above land on exactly. Keep new scalars ahead of it.
    pub wireframe_color: Vec3,

    pub _padding1: u32,
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

/// Most shadow maps the renderer will fit at once, one per body.
///
/// Fixed because it sizes a uniform array, and a uniform cannot be resized
/// per frame. Scenes with more bodies than this fall back to sharing the last
/// layer, which is degraded rather than wrong.
pub const MAX_SHADOW_LAYERS: usize = 8;

#[repr(C)]
#[derive(Debug, Copy, Clone, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Light {
    /// The layer currently being rendered *into*. Rewritten before each
    /// shadow pass, and read by `shadow.wgsl`, which draws one layer at a
    /// time and so needs a single matrix rather than the array.
    pub view_proj: Mat4,

    /// One matrix per body, for sampling in the main pass. Each is aimed at
    /// its own body and sized to it -- that is the resolution win -- while
    /// reaching far enough along the sunlight direction to contain whatever
    /// else lies between the Sun and that body, so mutual shadowing survives.
    pub view_proj_layers: [Mat4; MAX_SHADOW_LAYERS],

    /// Per layer: `(normal_offset_scale, bias_scale, bias_minimum, unused)`.
    ///
    /// The bias has to be per layer for the same reason the matrices do. Each
    /// layer covers a different world extent at the same texel count, so one
    /// texel is a different distance in each, and a single shared bias would
    /// be right for at most one body -- too small on the coarse layers (acne)
    /// and too large on the fine ones (detached shadows). Resolved on the CPU
    /// so a user-pinned value still wins, exactly as before.
    pub layer_bias: [crate::Vec4; MAX_SHADOW_LAYERS],

    pub pos: Vec3,
    /// How many of `view_proj_layers` are valid this frame.
    pub n_layers: u32,

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
