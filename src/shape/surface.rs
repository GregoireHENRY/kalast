//! The shape model whole, around its maplets: what of it each image sees
//! lit, where maplets go to cover it, and what the rest of it hides of a
//! maplet from a camera or from the Sun.
//!
//! A maplet knows its own relief and nothing beyond it. Over a whole body --
//! a comet's two lobes, its neck, its cliffs -- the terrain beyond a maplet
//! shadows it and stands between it and a camera, and which images are
//! worth a maplet depends on which see it at all. [`Surface`] answers both
//! with rays against the shape model, and [`plan`] lays maplets over it,
//! each at the finest spacing its images allow.

use std::collections::{HashMap, VecDeque};

use super::bvh::Bvh;
use super::maplet::{frame, Maplet};
use super::par_map;
use super::view::View;
use super::{F, V3};

/// A shape model and the rays against it.
pub struct Surface {
    pub positions: Vec<V3>,
    pub indices: Vec<u32>,
    bvh: Bvh,
    /// How far above a point its rays start: a quarter of a typical edge,
    /// clear of the facets around it.
    lift: F,
}

/// Points over a surface, one or two a cube of it: the point of its
/// facets nearest their mean, their mean normal, their area. Two where
/// the surface crosses the cube twice -- a thin ridge's two sides.
pub struct Samples {
    pub points: Vec<V3>,
    pub normals: Vec<V3>,
    pub areas: Vec<F>,
}

/// A point in the frame, in front of the camera.
pub const IN_FRAME: u8 = 1;
/// Facing the camera.
pub const FACING: u8 = 2;
/// Nothing between it and the camera.
pub const SEEN: u8 = 4;
/// Facing the Sun, nothing between them.
pub const LIT: u8 = 8;

/// What a camera sees of a point.
#[derive(Clone, Copy, Debug, Default)]
pub struct Seen {
    /// [`IN_FRAME`], [`FACING`], [`SEEN`], [`LIT`]; the last two only for a
    /// point in the frame and facing the camera.
    pub flags: u8,
    /// A pixel's size there, in the surface's unit, facing the camera.
    pub scale: F,
    /// The Sun's and the camera's angles to the point's normal, radians.
    pub incidence: F,
    pub emission: F,
}

/// Where a maplet goes: its centre on the surface, its normal, its
/// spacing.
#[derive(Clone, Copy, Debug)]
pub struct Tile {
    pub centre: V3,
    pub normal: V3,
    pub spacing: F,
}

impl Surface {
    pub fn new(positions: Vec<V3>, indices: Vec<u32>) -> Result<Surface, String> {
        if indices.len() % 3 != 0 || indices.is_empty() {
            return Err("the indices come in threes".into());
        }
        if let Some(i) = indices.iter().find(|&&i| i as usize >= positions.len()) {
            return Err(format!("index {i} for {} vertices", positions.len()));
        }
        let bvh = Bvh::new(&positions, &indices);
        let mut edges: Vec<F> = indices
            .chunks_exact(3)
            .step_by((indices.len() / 3 / 10000).max(1))
            .map(|t| (positions[t[1] as usize] - positions[t[0] as usize]).length())
            .collect();
        edges.sort_by(F::total_cmp);
        let lift = 0.25 * edges[edges.len() / 2];
        Ok(Surface { positions, indices, bvh, lift })
    }

    fn corners(&self, f: usize) -> [V3; 3] {
        let t = &self.indices[3 * f..3 * f + 3];
        [self.positions[t[0] as usize], self.positions[t[1] as usize], self.positions[t[2] as usize]]
    }

    /// The surface in cubes `size` on a side: a sample a cube, two where
    /// facets facing away from the cube's mean normal make a tenth of its
    /// area or more.
    pub fn samples(&self, size: F) -> Samples {
        let count = self.indices.len() / 3;
        let origin = self.positions.iter().fold(V3::splat(F::INFINITY), |a, p| a.min(*p));
        let keyed: Vec<(u64, V3, V3)> = par_map(count, |f| {
            let t = self.corners(f);
            let c = (t[0] + t[1] + t[2]) / 3.0;
            let a = 0.5 * (t[1] - t[0]).cross(t[2] - t[0]);
            let q = ((c - origin) / size).floor();
            let key = (q.x as u64) | ((q.y as u64) << 21) | ((q.z as u64) << 42);
            (key, c, a)
        });
        let mut order: Vec<u32> = (0..count as u32).collect();
        order.sort_unstable_by_key(|&f| keyed[f as usize].0);
        let mut out = Samples { points: vec![], normals: vec![], areas: vec![] };
        let mut start = 0;
        while start < order.len() {
            let key = keyed[order[start] as usize].0;
            let mut end = start;
            while end < order.len() && keyed[order[end] as usize].0 == key {
                end += 1;
            }
            let cube = &order[start..end];
            let total: V3 = cube.iter().map(|&f| keyed[f as usize].2).sum();
            for side in [1.0, -1.0] {
                let part: Vec<u32> = cube.iter().copied().filter(|&f| side * keyed[f as usize].2.dot(total) >= 0.0).collect();
                let area: F = part.iter().map(|&f| keyed[f as usize].2.length()).sum();
                let all: F = cube.iter().map(|&f| keyed[f as usize].2.length()).sum();
                if part.is_empty() || (side < 0.0 && area < 0.1 * all) || area <= 0.0 {
                    continue;
                }
                let mean = part.iter().map(|&f| keyed[f as usize].1 * keyed[f as usize].2.length()).sum::<V3>() / area;
                let normal = part.iter().map(|&f| keyed[f as usize].2).sum::<V3>().normalize_or_zero();
                let nearest = part
                    .iter()
                    .min_by(|&&a, &&b| {
                        (keyed[a as usize].1 - mean).length_squared().total_cmp(&(keyed[b as usize].1 - mean).length_squared())
                    })
                    .unwrap();
                out.points.push(keyed[*nearest as usize].1);
                out.normals.push(normal);
                out.areas.push(area);
            }
            start = end;
        }
        out
    }

    /// What `camera` sees of `points` on the surface, each facing along
    /// its normal: in its frame, facing it, nothing of the surface between
    /// them, and lit. The camera's image and mask are not used.
    pub fn sees(&self, points: &[V3], normals: &[V3], camera: &View) -> Vec<Seen> {
        let none = |_: u32| false;
        par_map(points.len(), |k| {
            let (p, n) = (points[k], normals[k]);
            let mut seen = Seen::default();
            let Some((x, y, depth)) = camera.project(p) else {
                return seen;
            };
            let to = camera.position - p;
            let distance = to.length();
            let (e, sun) = (to / distance, camera.sun);
            let (mu, mu0) = (n.dot(e), n.dot(sun));
            seen.scale = depth / camera.focal;
            seen.incidence = mu0.clamp(-1.0, 1.0).acos();
            seen.emission = mu.clamp(-1.0, 1.0).acos();
            if x >= 0.0 && y >= 0.0 && x < camera.width as F && y < camera.height as F {
                seen.flags |= IN_FRAME;
            }
            if mu > 0.0 {
                seen.flags |= FACING;
            }
            if seen.flags == IN_FRAME | FACING {
                let from = p + n * self.lift;
                if !self.bvh.blocked(from, e, distance, &none) {
                    seen.flags |= SEEN;
                }
                if mu0 > 0.0 && !self.bvh.blocked(from, sun, F::INFINITY, &none) {
                    seen.flags |= LIT;
                }
            }
            seen
        })
    }

    /// The pixels of `view` that show `maplet` as its own grid can tell it:
    /// not hidden by the rest of the surface, nor in its shadow, and
    /// `margin` (the surface's unit) around those that are, for the
    /// shape model's error -- 1 where they are, 0 elsewhere on the maplet,
    /// 1 off it. The surface within the maplet's square, as far as its own
    /// window above and below, is the maplet's own and left aside.
    pub fn mask(&self, maplet: &Maplet, view: &View, margin: F) -> Vec<u8> {
        let n = maplet.n;
        let [u, v, w] = maplet.axes;
        let half = 0.5 * (n - 1) as F * maplet.spacing + maplet.spacing;
        let window = half.max(10.0 * maplet.spacing);
        let own = |f: u32| {
            let t = self.corners(f as usize);
            let d = (t[0] + t[1] + t[2]) / 3.0 - maplet.centre;
            d.dot(u).abs() <= half && d.dot(v).abs() <= half && d.dot(w).abs() < window
        };
        let points = maplet.points();
        let sun = view.sun;
        let bad: Vec<bool> = par_map(points.len(), |k| {
            let p = points[k];
            let to = view.position - p;
            let distance = to.length();
            self.bvh.blocked(p, to / distance, distance, &own) || self.bvh.blocked(p, sun, F::INFINITY, &own)
        });
        let mut valid = vec![1u8; view.width * view.height];
        // A pixel's size at the maplet, for the margin.
        let Some((_, _, depth)) = view.project(maplet.centre) else {
            return valid;
        };
        let reach = (margin * view.focal / depth).ceil().max(0.0) as i64;
        let projected: Vec<Option<(F, F, F)>> = points.iter().map(|p| view.project(*p)).collect();
        let (wi, hi) = (view.width as i64, view.height as i64);
        // Each cell with a bad corner, and `reach` pixels around it.
        for j in 0..n - 1 {
            for i in 0..n - 1 {
                let corners = [j * n + i, j * n + i + 1, (j + 1) * n + i, (j + 1) * n + i + 1];
                if !corners.iter().any(|&k| bad[k]) {
                    continue;
                }
                let (mut x0, mut y0, mut x1, mut y1) = (F::INFINITY, F::INFINITY, F::NEG_INFINITY, F::NEG_INFINITY);
                for &k in &corners {
                    if let Some((x, y, _)) = projected[k] {
                        x0 = x0.min(x);
                        y0 = y0.min(y);
                        x1 = x1.max(x);
                        y1 = y1.max(y);
                    }
                }
                if !x0.is_finite() {
                    continue;
                }
                let (xa, ya) = ((x0.floor() as i64 - reach).max(0), (y0.floor() as i64 - reach).max(0));
                let (xb, yb) = ((x1.ceil() as i64 + reach).min(wi - 1), (y1.ceil() as i64 + reach).min(hi - 1));
                for y in ya..=yb {
                    for x in xa..=xb {
                        valid[(y * wi + x) as usize] = 0;
                    }
                }
            }
        }
        valid
    }
}

/// Points of a surface in cubes `cell` on a side, to find those near a
/// place without looking at all of them.
struct Grid {
    cell: F,
    cubes: HashMap<(i64, i64, i64), Vec<u32>>,
}

impl Grid {
    fn new(points: &[V3], cell: F) -> Grid {
        let mut cubes: HashMap<(i64, i64, i64), Vec<u32>> = HashMap::new();
        for (k, p) in points.iter().enumerate() {
            cubes.entry(Grid::key(*p, cell)).or_default().push(k as u32);
        }
        Grid { cell, cubes }
    }

    fn key(p: V3, cell: F) -> (i64, i64, i64) {
        let q = (p / cell).floor();
        (q.x as i64, q.y as i64, q.z as i64)
    }

    /// The points within `radius` of `p`.
    fn near(&self, points: &[V3], p: V3, radius: F) -> Vec<u32> {
        let r = (radius / self.cell).ceil() as i64;
        let (a, b, c) = Grid::key(p, self.cell);
        let mut out = vec![];
        for x in a - r..=a + r {
            for y in b - r..=b + r {
                for z in c - r..=c + r {
                    if let Some(ks) = self.cubes.get(&(x, y, z)) {
                        out.extend(ks.iter().copied().filter(|&k| (points[k as usize] - p).length() <= radius));
                    }
                }
            }
        }
        out
    }
}

/// Maplets of `n` points on a side over a surface's `samples`, each at the
/// finest spacing its images allow: `finest`, a sample's, not finite where
/// none does. Finest first, each spacing's maplets laid edge to edge from
/// one another as a lattice grows -- one, then its four neighbours a
/// maplet's width less `overlap` of it apart, on the surface as it turns --
/// until every sample of that spacing or finer lies on one, as each maplet
/// would see it: within the part of its square it answers for, facing
/// within 60 degrees of its normal, and not under an overhang. A maplet is
/// laid where at least half of what it covers allows its spacing, and not
/// where most of it is covered already. A sample a maplet cannot see as
/// a height field -- a cliff under a plain's maplet -- gets one of its own
/// along its own normal.
pub fn plan(samples: &Samples, finest: &[F], n: usize, overlap: F) -> Result<Vec<Tile>, String> {
    let count = samples.points.len();
    if finest.len() != count || samples.normals.len() != count || samples.areas.len() != count {
        return Err(format!("{count} samples, {} spacings", finest.len()));
    }
    if n < 3 || !(0.0..0.9).contains(&overlap) {
        return Err(format!("{n} points a side, overlap {overlap}"));
    }
    let mut levels: Vec<F> = finest.iter().copied().filter(|s| s.is_finite() && *s > 0.0).collect();
    levels.sort_by(F::total_cmp);
    levels.dedup_by(|a, b| (*a - *b).abs() <= 1e-9 * b.abs());
    let mut areas: Vec<F> = samples.areas.clone();
    areas.sort_by(F::total_cmp);
    // A sample's extent, for what lies above it.
    let size = areas.get(count / 2).map_or(0.0, |a| a.sqrt());
    let (points, normals) = (&samples.points, &samples.normals);
    let mut covered = vec![false; count];
    let mut tiles = vec![];
    // The samples each maplet answers for.
    let mut answers: Vec<Vec<usize>> = vec![];
    let up = (60.0 * super::RPD).cos();
    for &s in &levels {
        let width = (n - 1) as F * s;
        let step = width * (1.0 - overlap);
        let window = (0.5 * width).max(10.0 * s);
        let grid = Grid::new(points, 0.5 * step);
        let allows = |k: usize| finest[k] <= s * (1.0 + 1e-9);
        // Seeds in an order that keeps neighbours together.
        let mut order: Vec<usize> = (0..count).filter(|&k| allows(k) && !covered[k]).collect();
        order.sort_by_key(|&k| morton(points[k], step));
        let mut queue = VecDeque::new();
        for seed in order {
            if covered[seed] {
                continue;
            }
            // A seed takes a maplet of its own whatever is covered around
            // it -- along its own normal if the mean's leaves it out; those
            // grown from it, only where they cover something new.
            queue.push_back((points[seed], Some(seed)));
            while let Some((p, seed)) = queue.pop_front() {
                let q = match seed {
                    Some(k) => k,
                    None => {
                        let Some(q) = grid
                            .near(points, p, 0.5 * step)
                            .into_iter()
                            .min_by(|&a, &b| (points[a as usize] - p).length().total_cmp(&(points[b as usize] - p).length()))
                        else {
                            continue;
                        };
                        q as usize
                    }
                };
                let centre = points[q];
                let around = grid.near(points, centre, 0.5 * width * std::f64::consts::SQRT_2 + size);
                let mut mean = V3::ZERO;
                for &k in &around {
                    let k = k as usize;
                    if (points[k] - centre).length() <= 0.5 * width && normals[k].dot(normals[q]) > 0.0 {
                        mean += normals[k] * samples.areas[k];
                    }
                }
                let tries = if seed.is_some() { vec![mean, normals[q]] } else { vec![mean] };
                for normal in tries {
                    let Ok(axes) = frame(normal) else {
                        continue;
                    };
                    let members = cover(points, normals, &around, centre, axes, width, step, window, size, up);
                    let total: F = members.iter().map(|&k| samples.areas[k]).sum();
                    let fine: F = members.iter().filter(|&&k| allows(k)).map(|&k| samples.areas[k]).sum();
                    let fresh: F = members.iter().filter(|&&k| !covered[k]).map(|&k| samples.areas[k]).sum();
                    let wanted = match seed {
                        Some(k) => members.contains(&k),
                        None => fresh >= 0.4 * total,
                    };
                    if !(total > 0.0) || fine < 0.5 * total || !wanted {
                        continue;
                    }
                    for &k in &members {
                        covered[k] = true;
                    }
                    let [u, v, w] = axes;
                    tiles.push(Tile { centre, normal: w, spacing: s });
                    answers.push(members);
                    for (a, b) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                        queue.push_back((centre + u * (a * step) + v * (b * step), None));
                    }
                    break;
                }
            }
        }
    }
    // The lattice's seams leave maplets whose every sample another
    // answers for too: the last laid first.
    let mut times = vec![0u32; count];
    for members in &answers {
        for &k in members {
            times[k] += 1;
        }
    }
    let mut keep = vec![true; tiles.len()];
    for t in (0..tiles.len()).rev() {
        if answers[t].iter().all(|&k| times[k] >= 2) {
            keep[t] = false;
            for &k in &answers[t] {
                times[k] -= 1;
            }
        }
    }
    Ok(tiles.into_iter().zip(keep).filter_map(|(t, k)| k.then_some(t)).collect())
}

/// The samples among `around` a maplet at `centre` along `axes` answers
/// for: within the middle `step` of its square, facing within the angle of
/// `up`'s cosine of its normal, not under an overhang.
#[allow(clippy::too_many_arguments)]
fn cover(points: &[V3], normals: &[V3], around: &[u32], centre: V3, axes: [V3; 3], width: F, step: F, window: F, size: F, up: F) -> Vec<usize> {
    let [u, v, w] = axes;
    // What lies highest over each cell of the square, to tell the samples
    // under an overhang.
    let cells = ((width / size.max(1e-12)).ceil() as usize).clamp(1, 256);
    let cell = width / cells as F;
    let mut top = vec![F::NEG_INFINITY; cells * cells];
    let local: Vec<(usize, F, F, F)> = around
        .iter()
        .map(|&k| {
            let d = points[k as usize] - centre;
            (k as usize, d.dot(u), d.dot(v), d.dot(w))
        })
        .filter(|&(_, x, y, z)| x.abs() < 0.5 * width && y.abs() < 0.5 * width && z.abs() < window)
        .collect();
    let at = |x: F, y: F| {
        let i = (((x + 0.5 * width) / cell) as usize).min(cells - 1);
        let j = (((y + 0.5 * width) / cell) as usize).min(cells - 1);
        j * cells + i
    };
    for &(k, x, y, z) in &local {
        if normals[k].dot(w) > 0.0 {
            let c = at(x, y);
            top[c] = top[c].max(z);
        }
    }
    local
        .iter()
        .filter(|&&(k, x, y, z)| x.abs() <= 0.5 * step && y.abs() <= 0.5 * step && normals[k].dot(w) >= up && z >= top[at(x, y)] - 2.0 * size)
        .map(|&(k, ..)| k)
        .collect()
}

/// The images a maplet takes of `candidates`, as MPCD keeps them: finest
/// first, each only if it differs from every one taken by `angle` (radians)
/// of Sun or of view, or by `ratio` of scale, or if most of what it sees of
/// the maplet -- `seen`, sorted sample indices, one list an image, or none
/// -- the one it repeats does not; `cap` at most. `scales` a pixel's size,
/// `suns` toward the Sun and `views` toward the camera, unit. Indices into
/// the candidates, in the order taken.
pub fn choose(scales: &[F], suns: &[V3], views: &[V3], seen: Option<&[Vec<u32>]>, cap: usize, angle: F, ratio: F) -> Vec<usize> {
    let mut order: Vec<usize> = (0..scales.len()).collect();
    order.sort_by(|&a, &b| scales[a].total_cmp(&scales[b]));
    let close = angle.cos();
    // The share of `a`'s ground that `b` sees as well.
    let shared = |a: usize, b: usize| -> F {
        let Some(seen) = seen else {
            return 1.0;
        };
        let (x, y) = (&seen[a], &seen[b]);
        if x.is_empty() {
            return 1.0;
        }
        let (mut i, mut j, mut both) = (0, 0, 0);
        while i < x.len() && j < y.len() {
            match x[i].cmp(&y[j]) {
                std::cmp::Ordering::Less => i += 1,
                std::cmp::Ordering::Greater => j += 1,
                std::cmp::Ordering::Equal => {
                    both += 1;
                    i += 1;
                    j += 1;
                }
            }
        }
        both as F / x.len() as F
    };
    let mut taken: Vec<usize> = vec![];
    for a in order {
        if taken.len() >= cap {
            break;
        }
        let repeats = taken.iter().any(|&b| {
            suns[a].dot(suns[b]) > close
                && views[a].dot(views[b]) > close
                && scales[a].max(scales[b]) < ratio * scales[a].min(scales[b])
                && shared(a, b) > 0.7
        });
        if !repeats {
            taken.push(a);
        }
    }
    taken
}

/// The Morton code of `p` in cubes of `cell`: nearby points, nearby codes.
fn morton(p: V3, cell: F) -> u64 {
    let spread = |x: u64| {
        let mut x = x & 0x1fffff;
        x = (x | x << 32) & 0x1f00000000ffff;
        x = (x | x << 16) & 0x1f0000ff0000ff;
        x = (x | x << 8) & 0x100f00f00f00f00f;
        x = (x | x << 4) & 0x10c30c30c30c30c3;
        (x | x << 2) & 0x1249249249249249
    };
    let q = (p / cell).floor() + V3::splat((1 << 20) as F);
    spread(q.x as u64) | spread(q.y as u64) << 1 | spread(q.z as u64) << 2
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sphere of radius `r`, `k` rings.
    fn sphere(r: F, k: usize) -> (Vec<V3>, Vec<u32>) {
        let mut p = vec![V3::new(0.0, 0.0, r)];
        for i in 1..k {
            let th = std::f64::consts::PI * i as F / k as F;
            for j in 0..2 * k {
                let ph = std::f64::consts::PI * j as F / k as F;
                p.push(V3::new(r * th.sin() * ph.cos(), r * th.sin() * ph.sin(), r * th.cos()));
            }
        }
        p.push(V3::new(0.0, 0.0, -r));
        let m = 2 * k as u32;
        let ring = |i: u32, j: u32| 1 + (i - 1) * m + j % m;
        let mut idx = vec![];
        for j in 0..m {
            idx.extend_from_slice(&[0, ring(1, j), ring(1, j + 1)]);
        }
        for i in 1..k as u32 - 1 {
            for j in 0..m {
                idx.extend_from_slice(&[ring(i, j), ring(i + 1, j), ring(i + 1, j + 1), ring(i, j), ring(i + 1, j + 1), ring(i, j + 1)]);
            }
        }
        let last = p.len() as u32 - 1;
        for j in 0..m {
            idx.extend_from_slice(&[last, ring(k as u32 - 1, j + 1), ring(k as u32 - 1, j)]);
        }
        (p, idx)
    }

    #[test]
    fn a_camera_sees_the_near_side_lit_where_the_sun_is() {
        let (p, idx) = sphere(1.0, 60);
        let surface = Surface::new(p, idx).unwrap();
        let samples = surface.samples(0.1);
        let camera = View {
            name: String::new(),
            image: vec![],
            width: 1000,
            height: 1000,
            valid: vec![],
            position: V3::new(10.0, 0.0, 0.0),
            // Looking down -x: x along +y, y along -z.
            axes: super::super::M3::from_cols(V3::Y, -V3::Z, -V3::X).transpose(),
            focal: 2000.0,
            centre: [500.0, 500.0],
            sun: V3::Y,
            weight: 1.0,
        };
        let seen = surface.sees(&samples.points, &samples.normals, &camera);
        for (k, s) in seen.iter().enumerate() {
            let q = samples.points[k];
            let facing = q.x > 0.15;
            if facing && q.x > 0.3 {
                assert_eq!(s.flags & (FACING | IN_FRAME | SEEN), FACING | IN_FRAME | SEEN, "{q}");
                if q.y.abs() > 0.1 {
                    assert_eq!(s.flags & LIT != 0, q.y > 0.0, "{q} {}", s.flags);
                }
            }
            if q.x < -0.15 {
                assert_eq!(s.flags & SEEN, 0);
            }
        }
    }

    #[test]
    fn maplets_cover_a_sphere_once_over() {
        let (p, idx) = sphere(1.0, 120);
        let surface = Surface::new(p, idx).unwrap();
        let samples = surface.samples(0.02);
        let finest: Vec<F> = samples.points.iter().map(|q| if q.z > 0.0 { 0.005 } else { 0.01 }).collect();
        let tiles = plan(&samples, &finest, 41, 0.2).unwrap();
        let fine = tiles.iter().filter(|t| t.spacing < 0.007).count();
        let coarse = tiles.len() - fine;
        // A hemisphere is 2 pi; a fine maplet answers for (0.2 x 0.8)^2,
        // a coarse one (0.4 x 0.8)^2, the lattice's seams and turns more.
        let ideal_fine = 2.0 * std::f64::consts::PI / (0.16 * 0.16);
        let ideal_coarse = 2.0 * std::f64::consts::PI / (0.32 * 0.32);
        assert!((fine as F) > 0.9 * ideal_fine && (fine as F) < 2.0 * ideal_fine, "{fine} fine, ideally {ideal_fine:.0}");
        assert!((coarse as F) > 0.7 * ideal_coarse && (coarse as F) < 2.2 * ideal_coarse, "{coarse} coarse, ideally {ideal_coarse:.0}");
        // Every sample within some maplet's square; at its spacing or finer
        // but where a maplet would be mostly coarser ground.
        let mut finer = 0;
        for (q, s) in samples.points.iter().zip(&finest) {
            let on = |t: &&Tile| {
                let [u, v, _] = frame(t.normal).unwrap();
                let d = *q - t.centre;
                let half = 0.5 * 40.0 * t.spacing;
                d.dot(u).abs() <= half && d.dot(v).abs() <= half && d.dot(t.normal) > -0.1
            };
            assert!(tiles.iter().any(|t| on(&t)), "{q} is on no maplet");
            finer += tiles.iter().any(|t| on(&t) && t.spacing <= *s * 1.0001) as usize;
        }
        assert!(finer as F > 0.98 * samples.points.len() as F, "{finer} of {}", samples.points.len());
    }
}
