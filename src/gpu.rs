//! A headless GPU context, shared by the compute paths.
//!
//! Exists so the model's stages can be mixed freely. The thermophysical model
//! and the radiance conversion each run on the CPU or the GPU independently,
//! and all four combinations have to work:
//!
//! | TPM | radiance | how the temperatures cross |
//! |---|---|---|
//! | CPU | CPU | never leave numpy |
//! | GPU | CPU | `GpuTpm::surface`, one readback |
//! | CPU | GPU | `GpuRadiance::set_temperatures`, one upload |
//! | GPU | GPU | **nothing moves** -- radiance binds the TPM's own buffer |
//!
//! That last row only works if both were built on the *same* device, which is
//! what this type is for. Build one `Context` and hand it to both; construct
//! them separately and they each get their own device, which still works but
//! pays a round trip through host memory every step.

use std::sync::Arc;

pub struct Context {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

impl Context {
    pub fn new() -> Result<Arc<Self>, String> {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .map_err(|e| format!("no GPU adapter: {e}"))?;

        // The adapter's real limits, not the conservative cross-backend
        // defaults: a 3.1M-facet column set is a 0.43 GB buffer, well past the
        // 256 MiB the default allows.
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            required_limits: adapter.limits(),
            ..Default::default()
        }))
        .map_err(|e| format!("no GPU device: {e}"))?;

        Ok(Arc::new(Self { device, queue }))
    }

    /// Split a linear invocation count into a dispatch respecting the 65,535
    /// workgroups-per-dimension cap. Returns `(groups_x, groups_y, stride)`,
    /// `stride` being the width in invocations so a shader can rebuild a
    /// linear index.
    pub fn dispatch_2d(total: u64) -> (u32, u32, u32) {
        const WG: u64 = 64;
        const MAX: u64 = 65_535;
        let groups = total.div_ceil(WG).max(1);
        let gx = groups.min(MAX);
        let gy = groups.div_ceil(gx);
        (gx as u32, gy as u32, (gx * WG) as u32)
    }

    /// Read a storage buffer back into a `Vec<f32>`. Blocking.
    pub fn read_f32(&self, src: &wgpu::Buffer, staging: &wgpu::Buffer, len: u64) -> Vec<f32> {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        encoder.copy_buffer_to_buffer(src, 0, staging, 0, len * 4);
        self.queue.submit([encoder.finish()]);

        let slice = staging.slice(..len * 4);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = self.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        });
        let out = bytemuck::cast_slice::<u8, f32>(&slice.get_mapped_range()).to_vec();
        staging.unmap();
        out
    }
}
