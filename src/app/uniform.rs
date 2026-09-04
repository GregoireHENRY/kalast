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

    /// Colour facets from `Mesh::values` through the colormap instead of from
    /// their vertex colour. Orthogonal to `color_mode`, which still decides
    /// whether the result is lit: `value_mode = 1` with `color_mode = 0` is a
    /// lit data map, with `color_mode = 1` a flat one.
    pub value_mode: u32,
    /// Range the colormap spans. Values outside are clamped, not wrapped, so
    /// an outlier saturates rather than aliasing to the far end of the scale.
    pub value_min: f32,
    pub value_max: f32,

    pub _padding1: u32,
    // WGSL rounds the struct up to a multiple of 16; without this the Rust
    // side is 92 bytes against the shader's 96 and the bind group is
    // rejected at draw time.
    pub _padding2: [u32; 1],
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

/// Number of entries in the colour lookup table.
///
/// 256 is what matplotlib hands out by default, so a colormap passed straight
/// from Python needs no resampling.
pub const COLORMAP_SIZE: usize = 256;

/// The colour lookup table, as a uniform rather than a texture.
///
/// A uniform because the colorbar has to read the *same* table: a texture
/// would need a sampler in a second pipeline and two chances to bind the wrong
/// one, and 4 KB sits comfortably inside the 64 KB uniform limit.
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Colormap {
    /// `rgb` in xyz; `w` unused, present because uniform arrays are stride-16.
    pub lut: [crate::Vec4; COLORMAP_SIZE],
}

impl Default for Colormap {
    fn default() -> Self {
        // Greyscale, so a mesh with values but no colormap set still reads as
        // data rather than as a single flat colour.
        let mut lut = [crate::Vec4::ZERO; COLORMAP_SIZE];
        for (i, e) in lut.iter_mut().enumerate() {
            let t = i as f32 / (COLORMAP_SIZE - 1) as f32;
            *e = crate::Vec4::new(t, t, t, 1.0);
        }
        Self { lut }
    }
}

pub struct Uniforms {
    pub globals: super::gpu::UniformBuffer<Globals>,
    pub view: super::gpu::UniformBuffer<View>,
    pub colormap: super::gpu::UniformBuffer<Colormap>,
    pub shadow: super::gpu::Texture,
    // pub textures: Vec<super::gpu::Texture>,
}

impl Uniforms {
    pub fn layouts_all(&self) -> Vec<Option<&wgpu::BindGroupLayout>> {
        vec![
            Some(&self.globals.layout),
            Some(&self.view.layout),
            Some(&self.shadow.layout.as_ref().unwrap()),
            Some(&self.colormap.layout),
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
            colormap: self.colormap.bind_group(device),
            // textures: self.textures[0].bind_group(device).unwrap(),
        }
    }
}
