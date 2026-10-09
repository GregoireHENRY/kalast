//! Horizon maps: for every facet of a body, how high its terrain rises in
//! each of `AZIMUTHS` directions (`body.horizon_map`).
//!
//! A body's own shadows otherwise come from drawing it into its own shadow
//! layer every frame -- on Mars's 12.9M facets at AFC's closest approach,
//! most of the frame's shadow pass. Its horizons hold the same, worked out
//! once: a facet is lit where the Sun stands above its horizon, and with the
//! Sun a disc, by the part of the disc above it -- a penumbra for every
//! shadow of the terrain at no cost per frame. The body's layer then holds
//! only the other bodies that can shadow it.
//!
//! Worked out on the GPU (`shaders/horizon.wgsl`) from the mesh itself: its
//! radius, longitude by latitude, into a coarse grid over the whole body and
//! a fine one where its facets are smaller than the coarse cells; then from
//! each facet's centre a march along the surface in each direction, the
//! fine grid near and the coarse far, until nothing farther could rise
//! above what was seen. For a body each direction from whose centre crosses
//! its surface once: a planet, most asteroids. An overhang is a facet facing
//! its centre, and is said on the console.
//!
//! Held to the shadow map by `tests/test_horizon_map.py`.

use crate::Vec3;

/// Directions a horizon is known in: one every 11.25 degrees, two to a
/// word of `HorizonMap::buffer`. Kept with `AZIMUTHS` in `horizon.wgsl` and
/// `HORIZON_AZIMUTHS` in `mesh_shadow.wgsl`.
pub const AZIMUTHS: u32 = 32;

/// The coarse grid's finest cell, radians: 1/16 degree, 5760 by 2880.
const COARSE_FINEST: f64 = std::f64::consts::PI / 2880.0;
/// The most cells the fine grid takes: 256 MB of radii.
const FINE_CELLS: f64 = 64.0e6;

/// Cells of the tops' grid, a quarter of a degree each, longitude from -180
/// and latitude from the north pole: `TOP_CELLS` in `horizon.wgsl`,
/// `HORIZON_TOPS` in `mesh_shadow.wgsl`.
pub const TOP_CELLS: (u32, u32) = (1440, 720);

pub struct HorizonMap {
    /// Per facet, `AZIMUTHS / 2` words: the sine of the horizon's elevation
    /// as a snorm16 per azimuth, the first of each pair in the low half.
    /// Then per cell of `TOP_CELLS`, as an i32, the highest of the horizons
    /// of the facets whose centres are in it: where the Sun stands above
    /// that, the shading does not read the facet's.
    pub buffer: wgpu::Buffer,
    /// The sine of the highest of them all: a Sun above it lights every
    /// facet, and the shading does not read the map.
    pub top: f32,
}

/// `Params` in `horizon.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    n_facets: u32,
    stride: u32,
    grid: u32,
    _pad: u32,
    coarse_size: [u32; 2],
    fine_size: [u32; 2],
    fine_lo: [f32; 2],
    fine_cell: [f32; 2],
    fine_reach: f32,
    first_step: f32,
    growth: f32,
    max_step: f32,
    _pad2: [u32; 4],
}

fn lon_lat(p: Vec3) -> (f64, f64) {
    let (x, y, z) = (p.x as f64, p.y as f64, p.z as f64);
    (y.atan2(x), (z / (x * x + y * y + z * z).sqrt()).clamp(-1.0, 1.0).asin())
}

/// The smallest longitude interval holding all of `lons`, as its start and
/// width, to a tenth of a degree: round the widest run of tenths none falls
/// in.
fn lon_span(lons: impl Iterator<Item = f64>) -> (f64, f64) {
    use std::f64::consts::PI;
    const BINS: usize = 3600;
    let mut seen = vec![false; BINS];
    for lon in lons {
        seen[(((lon + PI) / (2.0 * PI) * BINS as f64) as usize).min(BINS - 1)] = true;
    }
    let Some(first) = seen.iter().position(|&s| s) else {
        return (-PI, 2.0 * PI);
    };
    // The longest run of empty bins, going round from a full one.
    let (mut best, mut best_end, mut run) = (0, first, 0);
    for k in 1..=BINS {
        let i = (first + k) % BINS;
        if seen[i] {
            if run > best {
                (best, best_end) = (run, i);
            }
            run = 0;
        } else {
            run += 1;
        }
    }
    let bin = 2.0 * PI / BINS as f64;
    (-PI + best_end as f64 * bin, (BINS - best) as f64 * bin)
}

/// The median of `values`, without sorting them all.
fn median(mut values: Vec<f64>) -> f64 {
    let mid = values.len() / 2;
    *values.select_nth_unstable_by(mid, f64::total_cmp).1
}

impl HorizonMap {
    /// The map of the mesh `positions`, `indices` (three per facet), in its
    /// own frame, centred on its origin, z its spin axis.
    pub fn build(device: &wgpu::Device, queue: &wgpu::Queue, positions: &[Vec3], indices: &[u32]) -> Self {
        use std::f64::consts::PI;
        let started = std::time::Instant::now();
        let n_facets = indices.len() / 3;

        // The body's size, and its facets': the coarse grid about half a
        // typical facet, no finer than `COARSE_FINEST`.
        let radii: Vec<f64> = positions.iter().map(|p| p.length() as f64).collect();
        let mean_radius = radii.iter().sum::<f64>() / radii.len().max(1) as f64;
        let corner = |f: usize, k: usize| positions[indices[3 * f + k] as usize];
        let edge = |f: usize| {
            let (a, b, c) = (corner(f, 0), corner(f, 1), corner(f, 2));
            ((a - b).length().max((b - c).length()).max((c - a).length())) as f64
        };
        let edges: Vec<f64> = (0..n_facets).map(edge).collect();
        let overhangs = (0..n_facets)
            .filter(|&f| {
                let (a, b, c) = (corner(f, 0), corner(f, 1), corner(f, 2));
                (b - a).cross(c - a).dot(a + b + c) < 0.0
            })
            .count();
        let widest = edges.iter().cloned().fold(0.0, f64::max);
        let coarse_cell = (median(edges.clone()) / mean_radius / 2.0).max(COARSE_FINEST);
        let coarse_size = [((2.0 * PI / coarse_cell).round() as u32).clamp(64, 5760), 0];
        let coarse_size = [coarse_size[0], coarse_size[0] / 2];
        let coarse_cell = 2.0 * PI / coarse_size[0] as f64;

        // The fine grid over the facets smaller than two coarse cells,
        // at half the median of them, as fine as `FINE_CELLS` allows.
        let small: Vec<usize> = (0..n_facets).filter(|&f| edges[f] < 2.0 * coarse_cell * mean_radius).collect();
        let (fine_size, fine_lo, fine_cell) = if small.len() > n_facets / 100 && !small.is_empty() {
            let centres: Vec<(f64, f64)> = small
                .iter()
                .map(|&f| lon_lat(corner(f, 0) + corner(f, 1) + corner(f, 2)))
                .collect();
            let (lon0, width) = lon_span(centres.iter().map(|c| c.0));
            let lat0 = centres.iter().map(|c| c.1).fold(f64::INFINITY, f64::min);
            let lat1 = centres.iter().map(|c| c.1).fold(f64::NEG_INFINITY, f64::max);
            let want = median(small.iter().map(|&f| edges[f]).collect()) / mean_radius / 2.0;
            // Two coarse cells of margin, so the march leaves it on terrain
            // the coarse grid has too.
            let (lon0, width) = (lon0 - 2.0 * coarse_cell, (width + 4.0 * coarse_cell).min(2.0 * PI));
            let (lat0, lat1) = ((lat0 - 2.0 * coarse_cell).max(-0.5 * PI), (lat1 + 2.0 * coarse_cell).min(0.5 * PI));
            let mean_cos = (0.5 * (lat0 + lat1)).cos().max(0.05);
            let area = width * (lat1 - lat0) / mean_cos;
            let cell = want.max((area / FINE_CELLS).sqrt()).min(coarse_cell / 2.0);
            let size = [((width / cell).ceil() as u32).max(2), (((lat1 - lat0) / cell).ceil() as u32).max(2)];
            (size, [lon0 as f32, lat0 as f32], [(width / size[0] as f64) as f32, ((lat1 - lat0) / size[1] as f64) as f32])
        } else {
            ([1, 1], [0.0, 0.0], [1.0, 1.0])
        };

        // Per 4 x 4 degree cell, the highest radius anything within reach
        // of it rises to: as far as a summit at the body's greatest relief
        // could still be seen over the curve.
        let r_max = radii.iter().cloned().fold(0.0, f64::max);
        let r_min = radii.iter().cloned().fold(f64::INFINITY, f64::min);
        let reach = (2.0 * mean_radius * (r_max - r_min)).sqrt() / mean_radius + 4.0f64.to_radians() * 1.5;
        let mut own = vec![0.0f64; 90 * 45];
        for p in positions {
            let (lon, lat) = lon_lat(*p);
            let i = (((lon + PI) / (2.0 * PI) * 90.0) as usize).min(89);
            let j = (((0.5 * PI - lat) / PI * 45.0) as usize).min(44);
            own[j * 90 + i] = own[j * 90 + i].max(p.length() as f64);
        }
        let centre = |i: usize, j: usize| {
            let (lon, lat) = (-PI + (i as f64 + 0.5) * PI / 45.0, 0.5 * PI - (j as f64 + 0.5) * PI / 45.0);
            [lat.cos() * lon.cos(), lat.cos() * lon.sin(), lat.sin()]
        };
        let mut highest = vec![0.0f32; 90 * 45];
        for j in 0..45 {
            for i in 0..90 {
                let a = centre(i, j);
                let mut top = own[j * 90 + i];
                for jj in 0..45 {
                    for ii in 0..90 {
                        let b = centre(ii, jj);
                        let cos = (a[0] * b[0] + a[1] * b[1] + a[2] * b[2]).clamp(-1.0, 1.0);
                        if cos.acos() <= reach {
                            top = top.max(own[jj * 90 + ii]);
                        }
                    }
                }
                highest[j * 90 + i] = if top > 0.0 { top as f32 } else { r_max as f32 };
            }
        }

        let fine_step = if fine_size[0] > 1 { fine_cell[0].min(fine_cell[1]) as f64 } else { coarse_cell };
        let params = |grid: u32, stride: u32, n: u32| Params {
            n_facets: n,
            stride,
            grid,
            _pad: 0,
            coarse_size,
            fine_size,
            fine_lo,
            fine_cell,
            fine_reach: (64.0 * fine_step * mean_radius) as f32,
            first_step: (0.5 * fine_step.min(coarse_cell) * mean_radius) as f32,
            growth: 0.04,
            max_step: (2.0 * coarse_cell * mean_radius) as f32,
            _pad2: [0; 4],
        };

        use wgpu::util::DeviceExt;
        let storage = |label, contents: &[u8]| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents,
                usage: wgpu::BufferUsages::STORAGE,
            })
        };
        let flat: Vec<f32> = positions.iter().flat_map(|p| [p.x as f32, p.y as f32, p.z as f32]).collect();
        let positions_buffer = storage("horizon positions", bytemuck::cast_slice(&flat));
        drop(flat);
        let indices_buffer = storage("horizon indices", bytemuck::cast_slice(indices));
        let highest_buffer = storage("horizon highest", bytemuck::cast_slice(&highest));
        // The facets along a Morton curve through their centres, 10 bits an
        // axis over the body's box.
        let mut order: Vec<(u32, u32)> = (0..n_facets)
            .map(|f| {
                let c = (corner(f, 0) + corner(f, 1) + corner(f, 2)) / 3.0;
                let q = |x: crate::Float| (((x as f64 / r_max + 1.0) * 0.5 * 1023.0).clamp(0.0, 1023.0)) as u32;
                let spread = |mut v: u32| {
                    v = (v | (v << 16)) & 0x0300_00ff;
                    v = (v | (v << 8)) & 0x0300_f00f;
                    v = (v | (v << 4)) & 0x030c_30c3;
                    (v | (v << 2)) & 0x0924_9249
                };
                (spread(q(c.x)) | (spread(q(c.y)) << 1) | (spread(q(c.z)) << 2), f as u32)
            })
            .collect();
        order.sort_unstable_by_key(|o| o.0);
        let order: Vec<u32> = order.into_iter().map(|o| o.1).collect();
        let order_buffer = storage("horizon order", bytemuck::cast_slice(&order));
        drop(order);
        let words = n_facets as u64 * (AZIMUTHS / 2) as u64;
        let tops = (TOP_CELLS.0 * TOP_CELLS.1) as u64;
        let top = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("horizon top"),
            contents: bytemuck::cast_slice(&vec![-32767i32; 1 + tops as usize]),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        });
        let top_read = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("horizon top read"),
            size: (1 + tops) * 4,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let grid = |label, cells: u64| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: cells.max(1) * 4,
                usage: wgpu::BufferUsages::STORAGE,
                mapped_at_creation: false,
            })
        };
        // Cleared, which is `EMPTY`: radius 0.
        let coarse = grid("horizon coarse grid", coarse_size[0] as u64 * coarse_size[1] as u64);
        let fine = grid("horizon fine grid", fine_size[0] as u64 * fine_size[1] as u64);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("horizon map"),
            size: (words + tops) * 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let module = device.create_shader_module(crate::app::gpu::SHADER_HORIZON);
        let pipeline = |entry: &str| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: None,
                module: &module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let (raster, fill, march) = (pipeline("raster"), pipeline("fill"), pipeline("march"));
        // A dispatch of `n` invocations of 64, in rows of at most 65,535
        // workgroups.
        let rows = |n: u64| {
            let groups = n.div_ceil(64).max(1);
            let x = groups.min(65_535) as u32;
            (x, groups.div_ceil(x as u64) as u32, x * 64)
        };
        // Each entry point's layout holds only the bindings it reads.
        let group = |pipeline: &wgpu::ComputePipeline, used: &[u32], p: Params| {
            let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("horizon params"),
                contents: bytemuck::bytes_of(&p),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let layout = pipeline.get_bind_group_layout(0);
            let all = [
                (0, uniform.as_entire_binding()),
                (1, positions_buffer.as_entire_binding()),
                (2, indices_buffer.as_entire_binding()),
                (3, coarse.as_entire_binding()),
                (4, fine.as_entire_binding()),
                (5, buffer.as_entire_binding()),
                (6, highest_buffer.as_entire_binding()),
                (7, order_buffer.as_entire_binding()),
                (8, top.as_entire_binding()),
            ];
            let entries: Vec<wgpu::BindGroupEntry> = all
                .into_iter()
                .filter(|(b, _)| used.contains(b))
                .map(|(binding, resource)| wgpu::BindGroupEntry { binding, resource })
                .collect();
            device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("horizon"), layout: &layout, entries: &entries })
        };

        // Each step its own submit, timed (`KALAST_HORIZON_TIMES=1`).
        let timed = std::env::var_os("KALAST_HORIZON_TIMES").is_some();
        let cpu = started.elapsed().as_secs_f64();
        let mut steps: Vec<(&str, f64)> = Vec::new();
        let mut run = |name: &'static str, record: &dyn Fn(&mut wgpu::ComputePass)| {
            let t = std::time::Instant::now();
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("horizon map") });
            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some(name), timestamp_writes: None });
                record(&mut pass);
            }
            let submission = queue.submit([encoder.finish()]);
            if timed {
                let _ = device.poll(wgpu::PollType::Wait { submission_index: Some(submission), timeout: None });
                steps.push((name, t.elapsed().as_secs_f64()));
            }
        };
        let (x, y, stride) = rows(n_facets as u64);
        for (g, size) in [(0u32, coarse_size), (1, fine_size)] {
            if g == 1 && fine_size[0] < 2 {
                continue;
            }
            let rasters = group(&raster, &[0, 1, 2, 3, 4], params(g, stride, n_facets as u32));
            run(if g == 0 { "raster coarse" } else { "raster fine" }, &|pass| {
                pass.set_pipeline(&raster);
                pass.set_bind_group(0, Some(&rasters), &[]);
                pass.dispatch_workgroups(x, y, 1);
            });
            let filler = group(&fill, &[0, 3, 4], params(g, stride, n_facets as u32));
            run("fill", &|pass| {
                pass.set_pipeline(&fill);
                pass.set_bind_group(0, Some(&filler), &[]);
                for _ in 0..3 {
                    pass.dispatch_workgroups(size[0].div_ceil(8), size[1].div_ceil(8), 1);
                }
            });
        }
        let (x, y, stride) = rows(words);
        let marcher = group(&march, &[0, 1, 2, 3, 4, 5, 6, 7, 8], params(0, stride, n_facets as u32));
        run("march", &|pass| {
            pass.set_pipeline(&march);
            pass.set_bind_group(0, Some(&marcher), &[]);
            pass.dispatch_workgroups(x, y, 1);
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("horizon top") });
        encoder.copy_buffer_to_buffer(&top, 0, &top_read, 0, (1 + tops) * 4);
        queue.submit([encoder.finish()]);
        top_read.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        let _ = device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None });
        let read: Vec<i32> = top_read.slice(..).get_mapped_range().map_or(vec![32767; 1 + tops as usize], |v| bytemuck::cast_slice(&v).to_vec());
        let top = read[0] as f32 / 32767.0;
        // Each cell's highest over the cells a facet's centre can be in from a
        // point of it -- a facet as wide as the widest, either way: a pixel
        // near a cell's edge can be a facet's whose centre is in the next.
        let (w, h) = (TOP_CELLS.0 as usize, TOP_CELLS.1 as usize);
        let cell = 360.0 / w as f64;
        let reach = (((widest / mean_radius).to_degrees() / cell).ceil() as usize) + 1;
        let mut dilated = vec![-32767i32; w * h];
        for j in 0..h {
            let lat = 90.0 - (j as f64 + 0.5) * cell;
            let across = ((reach as f64 / lat.to_radians().cos().max(1.0e-3)).ceil() as usize).min(w / 2);
            for i in 0..w {
                let mut m = -32767;
                for jj in j.saturating_sub(reach)..(j + reach + 1).min(h) {
                    for di in 0..=2 * across {
                        let ii = (i + w + di - across) % w;
                        m = m.max(read[1 + jj * w + ii]);
                    }
                }
                dilated[j * w + i] = m;
            }
        }
        queue.write_buffer(&buffer, words * 4, bytemuck::cast_slice(&dilated));
        if timed {
            let times: Vec<String> = steps.iter().map(|(n, t)| format!("{n} {t:.2} s")).collect();
            println!("[horizon] setup {cpu:.2} s; {}", times.join(", "));
        }

        println!(
            "[horizon] {n_facets} facets x {AZIMUTHS} azimuths in {:.1} s: grids {}x{} and {}x{} ({:.2} and {:.2} deg), \
             {:.0} MB, the highest horizon {:.1} deg",
            started.elapsed().as_secs_f64(),
            coarse_size[0],
            coarse_size[1],
            fine_size[0],
            fine_size[1],
            coarse_cell.to_degrees(),
            (fine_cell[0] as f64).to_degrees(),
            words as f64 * 4.0 / 1.0e6,
            (top as f64).clamp(-1.0, 1.0).asin().to_degrees(),
        );
        if overhangs > 0 {
            println!(
                "[horizon] {overhangs} facets face the body's centre: an overhang, which a horizon map cannot hold, \
                 so the shadows there may be wrong"
            );
        }
        Self { buffer, top }
    }
}
