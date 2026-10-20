//! Maplets back into their shape model: one mesh, each maplet's surface
//! where it answers for the ground, the shape model's elsewhere.
//!
//! Maplets overlap, at their own spacings each, and the shape model lies
//! under all of them. A point of the surface belongs to the finest maplet
//! over it, and among those to the one it is nearest the middle of -- each
//! maplet's square measured in its own half-widths; a maplet keeps the
//! cells whose middle is its own, and a cell more all round, so that two
//! meet without a crack. The shape model loses each facet whose corners
//! all belong to maplets, and keeps the rest: where the maplets stop, it
//! carries on, the two overlapping by a facet rather than parting.

use std::collections::HashMap;

use super::maplet::Maplet;
use super::{par_map, F, V3};

/// A maplet as the composition reads it.
struct Placed<'a> {
    maplet: &'a Maplet,
    half: F,
    /// Off its surface farther than this, a point is not its own: an
    /// overhang over it, a wall beyond.
    depth: F,
}

impl Placed<'_> {
    /// `p`'s place in the maplet's square, its share of the half-width,
    /// if the maplet's surface is there and near it.
    fn reach(&self, p: V3) -> Option<F> {
        let m = self.maplet;
        let [x, y, z] = m.local(p - m.centre);
        let (ax, ay) = (x.abs(), y.abs());
        if ax > self.half || ay > self.half {
            return None;
        }
        let c = (m.n - 1) as F / 2.0;
        let (fi, fj) = (x / m.spacing + c, y / m.spacing + c);
        let (i, j) = ((fi.floor() as usize).min(m.n - 2), (fj.floor() as usize).min(m.n - 2));
        let (tx, ty) = (fi - i as F, fj - j as F);
        let at = |a: usize, b: usize| m.reference[b * m.n + a];
        let corners = [at(i, j), at(i + 1, j), at(i, j + 1), at(i + 1, j + 1)];
        if corners.iter().any(|r| !r.is_finite()) {
            return None;
        }
        let h = |a: usize, b: usize| m.heights[b * m.n + a];
        let height = (1.0 - ty) * ((1.0 - tx) * h(i, j) + tx * h(i + 1, j)) + ty * ((1.0 - tx) * h(i, j + 1) + tx * h(i + 1, j + 1));
        ((z - height).abs() <= self.depth).then_some(ax.max(ay) / self.half)
    }
}

/// The maplets each point of the surface could be in: by cubes of the
/// widest maplet's width.
struct Index {
    cell: F,
    cubes: HashMap<(i64, i64, i64), Vec<usize>>,
}

impl Index {
    fn key(&self, p: V3) -> (i64, i64, i64) {
        let q = (p / self.cell).floor();
        (q.x as i64, q.y as i64, q.z as i64)
    }

    /// The maplet `p` belongs to: the finest over it, then the one it is
    /// most in the middle of.
    fn owner(&self, placed: &[Placed], p: V3) -> Option<usize> {
        let (a, b, c) = self.key(p);
        let mut best: Option<(F, F, usize)> = None;
        for x in a - 1..=a + 1 {
            for y in b - 1..=b + 1 {
                for z in c - 1..=c + 1 {
                    for &t in self.cubes.get(&(x, y, z)).into_iter().flatten() {
                        if let Some(share) = placed[t].reach(p) {
                            let key = (placed[t].maplet.spacing, share, t);
                            if best.is_none_or(|(s, r, _)| key.0 < s * (1.0 - 1e-9) || (key.0 <= s * (1.0 + 1e-9) && key.1 < r)) {
                                best = Some(key);
                            }
                        }
                    }
                }
            }
        }
        best.map(|b| b.2)
    }
}

/// The shape model `positions`, `indices`, with `maplets` in it: `(vertices,
/// indices, albedo)`, the shape model's vertices first, then each maplet's
/// it keeps; the albedo 1 on the shape model's. `spacing`, if coarser than
/// a maplet's, draws it at that: an image a few times coarser than the
/// maplets sees no more of them, and a whole lobe's maplets at their own
/// spacing would make a mesh no renderer holds.
pub fn compose<P: Copy + Into<V3> + Sync>(positions: &[P], indices: &[u32], maplets: &[Maplet], spacing: F) -> (Vec<V3>, Vec<u32>, Vec<F>) {
    let drawn: Vec<Maplet> = maplets
        .iter()
        .map(|m| {
            let width = (m.n - 1) as F * m.spacing;
            if spacing > m.spacing {
                let cells = ((width / spacing).round() as usize).max(2);
                m.resampled(cells + 1, width / cells as F)
            } else {
                m.clone()
            }
        })
        .collect();
    let placed: Vec<Placed> = drawn
        .iter()
        .map(|m| {
            let half = 0.5 * (m.n - 1) as F * m.spacing;
            Placed { maplet: m, half, depth: 10.0 * m.spacing + 0.05 * half }
        })
        .collect();
    let widest = placed.iter().map(|p| 2.0 * p.half).fold(0.0, F::max).max(1e-12);
    let mut index = Index { cell: widest * std::f64::consts::SQRT_2, cubes: HashMap::new() };
    for (t, p) in placed.iter().enumerate() {
        let key = index.key(p.maplet.centre);
        index.cubes.entry(key).or_default().push(t);
    }
    let base: Vec<V3> = positions.iter().map(|p| (*p).into()).collect();
    // The shape model's vertices that maplets answer for.
    let taken: Vec<bool> = par_map(base.len(), |v| index.owner(&placed, base[v]).is_some());
    let mut out_indices: Vec<u32> = indices
        .chunks_exact(3)
        .filter(|t| !t.iter().all(|&v| taken[v as usize]))
        .flatten()
        .copied()
        .collect();
    let mut vertices = base;
    let mut albedo = vec![1.0; vertices.len()];
    // Each maplet's own cells, and one more all round.
    let kept: Vec<Vec<bool>> = par_map(placed.len(), |t| {
        let m = placed[t].maplet;
        let n = m.n;
        let points = m.points();
        let known = |k: usize| m.reference[k].is_finite();
        let mut own = vec![false; (n - 1) * (n - 1)];
        for j in 0..n - 1 {
            for i in 0..n - 1 {
                let k = [j * n + i, j * n + i + 1, (j + 1) * n + i, (j + 1) * n + i + 1];
                if k.iter().all(|&k| known(k)) {
                    let middle = (points[k[0]] + points[k[1]] + points[k[2]] + points[k[3]]) / 4.0;
                    own[j * (n - 1) + i] = index.owner(&placed, middle) == Some(t);
                }
            }
        }
        let cells = n - 1;
        let mut grown = own.clone();
        for j in 0..cells {
            for i in 0..cells {
                if own[j * cells + i] {
                    continue;
                }
                let near = (j.saturating_sub(1)..=(j + 1).min(cells - 1)).any(|b| (i.saturating_sub(1)..=(i + 1).min(cells - 1)).any(|a| own[b * cells + a]));
                let k = [j * n + i, j * n + i + 1, (j + 1) * n + i, (j + 1) * n + i + 1];
                grown[j * cells + i] = near && k.iter().all(|&k| known(k));
            }
        }
        grown
    });
    for (p, cells) in placed.iter().zip(&kept) {
        let m = p.maplet;
        let n = m.n;
        let first = vertices.len() as u32;
        vertices.extend(m.points());
        albedo.extend(m.albedo.iter().copied());
        for j in 0..n - 1 {
            for i in 0..n - 1 {
                if cells[j * (n - 1) + i] {
                    let a = first + (j * n + i) as u32;
                    let (b, c, d) = (a + 1, a + n as u32, a + n as u32 + 1);
                    out_indices.extend_from_slice(&[a, b, d, a, d, c]);
                }
            }
        }
    }
    // The vertices no facet uses, left out.
    let mut used = vec![false; vertices.len()];
    for &v in &out_indices {
        used[v as usize] = true;
    }
    let mut map = vec![u32::MAX; vertices.len()];
    let (mut kept_vertices, mut kept_albedo) = (vec![], vec![]);
    for (v, u) in used.iter().enumerate() {
        if *u {
            map[v] = kept_vertices.len() as u32;
            kept_vertices.push(vertices[v]);
            kept_albedo.push(albedo[v]);
        }
    }
    for v in out_indices.iter_mut() {
        *v = map[*v as usize];
    }
    (kept_vertices, out_indices, kept_albedo)
}

/// The maplets' relief laid into the shape model itself: each vertex moved
/// along its own normal to where that line meets each maplet's surface over
/// it -- the maplets weighed toward their middles, `(1 - s^2)^2` of the
/// share `s` of the half-width along each side; a maplet facing more than
/// 78 deg away from the vertex, or whose surface the line meets farther off
/// than `compose`'s depth, not its. The moves are smoothed over the mesh,
/// `smooth` rounds of each the mean of its own and its neighbours', and held
/// to `step` (the surface's unit) if positive, so that a surface moved far
/// gets there in rounds, its neighbours with it, rather than folding. The
/// facets are the shape model's, and the mesh as closed as it was: what the
/// next round of maplets is laid on. Then `relax` rounds of each vertex
/// moved halfway toward its neighbours' middle across the surface, which
/// keeps the triangles even; and where a facet is turned over -- maplets
/// that disagree, a sliver tipped -- its corners' moves averaged with their
/// neighbours' until none is. With each
/// vertex's move along its normal, outward positive, and the maplets'
/// summed weight on it -- 0 where none is.
pub fn fuse<P: Copy + Into<V3> + Sync>(positions: &[P], indices: &[u32], maplets: &[Maplet], relax: usize, smooth: usize, step: F) -> (Vec<V3>, Vec<F>, Vec<F>) {
    let base: Vec<V3> = positions.iter().map(|p| (*p).into()).collect();
    let placed: Vec<Placed> = maplets
        .iter()
        .map(|m| {
            let half = 0.5 * (m.n - 1) as F * m.spacing;
            Placed { maplet: m, half, depth: 10.0 * m.spacing + 0.05 * half }
        })
        .collect();
    let widest = placed.iter().map(|p| 2.0 * p.half).fold(0.0, F::max).max(1e-12);
    let mut index = Index { cell: widest * std::f64::consts::SQRT_2, cubes: HashMap::new() };
    for (t, p) in placed.iter().enumerate() {
        let key = index.key(p.maplet.centre);
        index.cubes.entry(key).or_default().push(t);
    }
    let normals = vertex_normals(&base, indices);
    let taper = |s: F| (1.0 - s * s).max(0.0).powi(2);
    // A maplet's height at `(x, y)` of its square, if known there.
    let height_at = |m: &Maplet, half: F, x: F, y: F| -> Option<F> {
        if x.abs() > half || y.abs() > half {
            return None;
        }
        let mid = (m.n - 1) as F / 2.0;
        let (fi, fj) = (x / m.spacing + mid, y / m.spacing + mid);
        let (i, j) = ((fi.floor().max(0.0) as usize).min(m.n - 2), (fj.floor().max(0.0) as usize).min(m.n - 2));
        let (tx, ty) = (fi - i as F, fj - j as F);
        let known = |a: usize, b: usize| m.reference[b * m.n + a].is_finite();
        if !(known(i, j) && known(i + 1, j) && known(i, j + 1) && known(i + 1, j + 1)) {
            return None;
        }
        let h = |a: usize, b: usize| m.heights[b * m.n + a];
        Some((1.0 - ty) * ((1.0 - tx) * h(i, j) + tx * h(i + 1, j)) + ty * ((1.0 - tx) * h(i, j + 1) + tx * h(i + 1, j + 1)))
    };
    let moves: Vec<(F, F)> = par_map(base.len(), |k| {
        let (p, n) = (base[k], normals[k]);
        let (a, b, c) = index.key(p);
        let (mut sum, mut weight) = (0.0, 0.0);
        for x in a - 1..=a + 1 {
            for y in b - 1..=b + 1 {
                for z in c - 1..=c + 1 {
                    for &t in index.cubes.get(&(x, y, z)).into_iter().flatten() {
                        let pl = &placed[t];
                        let m = pl.maplet;
                        let w = m.axes[2];
                        let cos = n.dot(w);
                        if cos < 0.2 {
                            continue;
                        }
                        // Along the normal to the maplet's surface: where the
                        // line's height over the maplet meets the maplet's
                        // height under it, a few fixed-point steps.
                        let mut s = 0.0;
                        let mut met = None;
                        for _ in 0..8 {
                            let [lx, ly, lz] = m.local(p + n * s - m.centre);
                            let Some(h) = height_at(m, pl.half, lx, ly) else {
                                break;
                            };
                            let ds = (h - lz) / cos;
                            s += ds;
                            if ds.abs() < 1e-3 * m.spacing {
                                met = Some(s);
                                break;
                            }
                        }
                        let Some(s) = met else {
                            continue;
                        };
                        if s.abs() > pl.depth {
                            continue;
                        }
                        let [lx, ly, _] = m.local(p + n * s - m.centre);
                        let share = taper(lx / pl.half) * taper(ly / pl.half);
                        sum += s * share;
                        weight += share;
                    }
                }
            }
        }
        if weight > 0.0 {
            (sum / weight, weight)
        } else {
            (0.0, 0.0)
        }
    });
    let neighbours = {
        let mut n: Vec<Vec<u32>> = vec![vec![]; base.len()];
        for t in indices.chunks_exact(3) {
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                n[a as usize].push(b);
                n[b as usize].push(a);
            }
        }
        n
    };
    // The moves along the normals, smoothed where maplets cover, held to a
    // step.
    let covered: Vec<bool> = moves.iter().map(|(_, w)| *w > 0.0).collect();
    let mut along: Vec<F> = moves.iter().map(|(s, _)| *s).collect();
    for _ in 0..smooth {
        let before = along.clone();
        for (k, near) in neighbours.iter().enumerate() {
            if !covered[k] || near.is_empty() {
                continue;
            }
            let (mut total, mut count) = (before[k], 1.0);
            for &j in near {
                if covered[j as usize] {
                    total += before[j as usize];
                    count += 1.0;
                }
            }
            along[k] = total / count;
        }
    }
    if step > 0.0 {
        for a in along.iter_mut() {
            *a = a.clamp(-step, step);
        }
    }
    let mut vertices: Vec<V3> = base.iter().zip(&along).zip(&normals).map(|((p, a), n)| *p + *n * *a).collect();
    for _ in 0..relax {
        vertices = relaxed(&vertices, indices);
    }
    // A facet turned over by the move -- its corners moved by maplets that
    // disagree, or a sliver the relaxing tipped -- folds the surface: the
    // moves of its corners averaged with their neighbours' until none is.
    let mut shift: Vec<V3> = vertices.iter().zip(&base).map(|(v, p)| *v - *p).collect();
    let normal_of = |p: &[V3], t: &[u32]| (p[t[1] as usize] - p[t[0] as usize]).cross(p[t[2] as usize] - p[t[0] as usize]);
    for _ in 0..400 {
        let moved: Vec<V3> = base.iter().zip(&shift).map(|(p, d)| *p + *d).collect();
        let mut bad = vec![false; base.len()];
        let mut any = false;
        for t in indices.chunks_exact(3) {
            if normal_of(&base, t).dot(normal_of(&moved, t)) <= 0.0 {
                any = true;
                for &k in t {
                    bad[k as usize] = true;
                }
            }
        }
        if !any {
            break;
        }
        let before = shift.clone();
        for (k, near) in neighbours.iter().enumerate() {
            if bad[k] && !near.is_empty() {
                let mean = near.iter().map(|&j| before[j as usize]).sum::<V3>() / near.len() as F;
                shift[k] = 0.5 * (before[k] + mean);
            }
        }
    }
    // What averaging leaves turned over goes back where it was, a facet's
    // corners at a time, until none is: the mesh as it came is untangled.
    loop {
        let moved: Vec<V3> = base.iter().zip(&shift).map(|(p, d)| *p + *d).collect();
        let mut any = false;
        for t in indices.chunks_exact(3) {
            if normal_of(&base, t).dot(normal_of(&moved, t)) <= 0.0 {
                any = true;
                for &k in t {
                    shift[k as usize] = V3::ZERO;
                }
            }
        }
        if !any {
            break;
        }
    }
    let vertices: Vec<V3> = base.iter().zip(&shift).map(|(p, d)| *p + *d).collect();
    let moved = base.iter().zip(&vertices).zip(&normals).map(|((a, b), n)| (*b - *a).dot(*n)).collect();
    (vertices, moved, moves.into_iter().map(|(_, w)| w).collect())
}

/// The maplets merged into a closed mesh made again: the surface where a
/// field crosses zero, the maplets' where they are and the mesh's
/// elsewhere, extracted as [`Hull::mesh`](super::Hull::mesh) extracts a
/// hull ([`march`](super::march)): cubes of `size`, smoothed by `smooth`,
/// `relax` rounds after. A maplet's field is the distance along its normal
/// from its surface, positive below it -- inside the body -- weighed toward
/// its middle and fading out at `depth` from its surface; where the
/// maplets weigh less than a half, the mesh's own distance makes up the
/// rest -- inside it by a ray's crossings, the distance to its nearest
/// vertex. Unlike [`fuse`], whatever the maplets do -- sink a saddle by
/// hundreds of metres, part its walls -- the mesh that comes out is closed
/// and does not fold: what the next round of maplets is laid on.
pub fn merge<P: Copy + Into<V3> + Sync>(
    positions: &[P],
    indices: &[u32],
    maplets: &[Maplet],
    size: F,
    smooth: F,
    relax: usize,
    depth: F,
) -> Result<(Vec<V3>, Vec<u32>, super::HullReport), String> {
    let base: Vec<V3> = positions.iter().map(|p| (*p).into()).collect();
    if base.is_empty() || indices.len() < 3 {
        return Err("an empty mesh".into());
    }
    let placed: Vec<Placed> = maplets
        .iter()
        .map(|m| Placed { maplet: m, half: 0.5 * (m.n - 1) as F * m.spacing, depth })
        .collect();
    let widest = placed.iter().map(|p| 2.0 * p.half).fold(0.0, F::max).max(size);
    let mut index = Index { cell: widest * std::f64::consts::SQRT_2, cubes: HashMap::new() };
    for (t, p) in placed.iter().enumerate() {
        let key = index.key(p.maplet.centre);
        index.cubes.entry(key).or_default().push(t);
    }
    // The mesh's own field: inside by the crossings of three rays, the
    // distance that of its nearest vertex, no more than `depth`.
    let bvh = super::bvh::Bvh::new(&base, indices);
    let cell = depth.max(size) / 4.0;
    let mut grid: HashMap<(i64, i64, i64), Vec<u32>> = HashMap::new();
    let key = |p: V3| ((p.x / cell).floor() as i64, (p.y / cell).floor() as i64, (p.z / cell).floor() as i64);
    for (k, p) in base.iter().enumerate() {
        grid.entry(key(*p)).or_default().push(k as u32);
    }
    let rays = [V3::new(0.123, 0.456, 0.881).normalize(), V3::new(-0.702, 0.311, -0.640).normalize(), V3::new(0.533, -0.811, 0.241).normalize()];
    let own = |p: V3| -> F {
        let votes = rays.iter().filter(|d| bvh.crossings(p, **d) % 2 == 1).count();
        let (a, b, c) = key(p);
        // Ring by ring out from the point's cell, until what is found is
        // nearer than the next ring can be.
        let mut nearest = F::INFINITY;
        for r in 0..=4i64 {
            for x in a - r..=a + r {
                for y in b - r..=b + r {
                    for z in c - r..=c + r {
                        if (x - a).abs() < r && (y - b).abs() < r && (z - c).abs() < r {
                            continue;
                        }
                        for &k in grid.get(&(x, y, z)).into_iter().flatten() {
                            nearest = nearest.min((base[k as usize] - p).length_squared());
                        }
                    }
                }
            }
            if nearest.sqrt() <= r as F * cell {
                break;
            }
        }
        let d = nearest.sqrt().min(depth);
        if votes >= 2 {
            d
        } else {
            -d
        }
    };
    let taper = |s: F| (1.0 - s * s).max(0.0).powi(2);
    let field = |points: &[V3]| -> Vec<F> {
        points
            .iter()
            .map(|&p| {
                let (a, b, c) = index.key(p);
                let (mut sum, mut weight) = (0.0, 0.0);
                for x in a - 1..=a + 1 {
                    for y in b - 1..=b + 1 {
                        for z in c - 1..=c + 1 {
                            for &t in index.cubes.get(&(x, y, z)).into_iter().flatten() {
                                let pl = &placed[t];
                                let m = pl.maplet;
                                let [lx, ly, lz] = m.local(p - m.centre);
                                if lx.abs() > pl.half || ly.abs() > pl.half {
                                    continue;
                                }
                                let mid = (m.n - 1) as F / 2.0;
                                let (fi, fj) = (lx / m.spacing + mid, ly / m.spacing + mid);
                                let (i, j) = ((fi.floor().max(0.0) as usize).min(m.n - 2), (fj.floor().max(0.0) as usize).min(m.n - 2));
                                let (tx, ty) = (fi - i as F, fj - j as F);
                                let known = |a: usize, b: usize| m.reference[b * m.n + a].is_finite();
                                if !(known(i, j) && known(i + 1, j) && known(i, j + 1) && known(i + 1, j + 1)) {
                                    continue;
                                }
                                let h = |a: usize, b: usize| m.heights[b * m.n + a];
                                let height = (1.0 - ty) * ((1.0 - tx) * h(i, j) + tx * h(i + 1, j)) + ty * ((1.0 - tx) * h(i, j + 1) + tx * h(i + 1, j + 1));
                                let below = height - lz;
                                let share = taper(lx / pl.half) * taper(ly / pl.half) * taper(below / depth);
                                sum += share * below;
                                weight += share;
                            }
                        }
                    }
                }
                let rest = (0.5 - weight).max(0.0);
                if rest > 0.0 {
                    sum += rest * own(p);
                    weight += rest;
                }
                sum / weight
            })
            .collect()
    };
    let radius = base.iter().map(|p| p.length()).fold(0.0, F::max) + depth + 2.0 * size;
    super::march(radius, size, relax, smooth, &field)
}

/// Each vertex's normal: its facets', weighed by their areas.
fn vertex_normals(vertices: &[V3], indices: &[u32]) -> Vec<V3> {
    let mut normals = vec![V3::ZERO; vertices.len()];
    for t in indices.chunks_exact(3) {
        let (a, b, c) = (t[0] as usize, t[1] as usize, t[2] as usize);
        let n = (vertices[b] - vertices[a]).cross(vertices[c] - vertices[a]);
        normals[a] += n;
        normals[b] += n;
        normals[c] += n;
    }
    normals.into_iter().map(|n| n.normalize_or_zero()).collect()
}

/// Each vertex moved halfway toward its neighbours' middle, across the
/// surface only: the triangles evened, the shape kept.
fn relaxed(vertices: &[V3], indices: &[u32]) -> Vec<V3> {
    let normals = vertex_normals(vertices, indices);
    let mut sum = vec![V3::ZERO; vertices.len()];
    let mut count = vec![0u32; vertices.len()];
    for t in indices.chunks_exact(3) {
        for (x, y) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            sum[x as usize] += vertices[y as usize];
            count[x as usize] += 1;
        }
    }
    (0..vertices.len())
        .map(|k| {
            if count[k] == 0 {
                return vertices[k];
            }
            let d = sum[k] / count[k] as F - vertices[k];
            vertices[k] + 0.5 * (d - d.dot(normals[k]) * normals[k])
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A flat square of the shape model, `k x k` cells of `s`, at `z`.
    fn sheet(k: usize, s: F) -> (Vec<V3>, Vec<u32>) {
        let c = k as F / 2.0;
        let p: Vec<V3> = (0..(k + 1) * (k + 1)).map(|v| V3::new(((v % (k + 1)) as F - c) * s, ((v / (k + 1)) as F - c) * s, 0.0)).collect();
        let mut idx = vec![];
        for j in 0..k {
            for i in 0..k {
                let a = (j * (k + 1) + i) as u32;
                let (b, cc, d) = (a + 1, a + k as u32 + 1, a + k as u32 + 2);
                idx.extend_from_slice(&[a, b, d, a, d, cc]);
            }
        }
        (p, idx)
    }

    fn area(v: &[V3], idx: &[u32]) -> F {
        idx.chunks_exact(3).map(|t| 0.5 * (v[t[1] as usize] - v[t[0] as usize]).cross(v[t[2] as usize] - v[t[0] as usize]).length()).sum()
    }

    #[test]
    fn two_maplets_replace_the_ground_they_cover_and_no_more() {
        let (v, idx) = sheet(60, 1.0);
        let lift = |m: &mut Maplet, dz: F| {
            for h in m.heights.iter_mut() {
                *h += dz;
            }
        };
        // Two maplets of 21 x 21 at 1, overlapping by 6, a few millimetres
        // apart in height.
        let mut a = Maplet::from_surface(&v, &idx, V3::new(-7.0, 0.0, 0.0), Some(V3::Z), 21, 1.0).unwrap();
        let mut b = Maplet::from_surface(&v, &idx, V3::new(7.0, 0.0, 0.0), Some(V3::Z), 21, 1.0).unwrap();
        lift(&mut a, 0.003);
        lift(&mut b, -0.002);
        let (out, oidx, albedo) = compose(&v, &idx, &[a, b], 0.0);
        assert_eq!(out.len(), albedo.len());
        // The ground is all there, a little more where the pieces overlap.
        let (before, after) = (area(&v, &idx), area(&out, &oidx));
        assert!(after >= before && after < 1.05 * before, "{before} -> {after}");
        // Every point of the sheet under some facet.
        for y in -29..29 {
            for x in -29..29 {
                let p = V3::new(x as F + 0.37, y as F + 0.61, 0.0);
                let under = oidx.chunks_exact(3).any(|t| {
                    let [q0, q1, q2] = [out[t[0] as usize], out[t[1] as usize], out[t[2] as usize]];
                    let d = |a: V3, b: V3| (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
                    let (d0, d1, d2) = (d(q0, q1), d(q1, q2), d(q2, q0));
                    (d0 >= -1e-9 && d1 >= -1e-9 && d2 >= -1e-9) || (d0 <= 1e-9 && d1 <= 1e-9 && d2 <= 1e-9)
                });
                assert!(under, "a hole at {p}");
            }
        }
        // The maplets' ground is theirs: no facet of the sheet left at the
        // middle of either.
        for centre in [V3::new(-7.0, 0.0, 0.0), V3::new(7.0, 0.0, 0.0)] {
            let flat = oidx.chunks_exact(3).filter(|t| t.iter().all(|&k| out[k as usize].z == 0.0)).any(|t| {
                let m = (out[t[0] as usize] + out[t[1] as usize] + out[t[2] as usize]) / 3.0;
                (m - centre).length() < 5.0
            });
            assert!(!flat, "the shape model is left under the maplet at {centre}");
        }
    }

    #[test]
    fn fuse_moves_a_closed_mesh_onto_its_maplets() {
        // A sphere of radius 1, an octahedron refined; a maplet on its top,
        // its heights 0.05 above the sphere: the vertices under its middle
        // rise by that, those beyond it stay, and no facet is lost.
        let (mut v, mut idx) = (
            vec![V3::X, -V3::X, V3::Y, -V3::Y, V3::Z, -V3::Z],
            vec![0u32, 2, 4, 2, 1, 4, 1, 3, 4, 3, 0, 4, 2, 0, 5, 1, 2, 5, 3, 1, 5, 0, 3, 5],
        );
        for _ in 0..5 {
            let mut middle: HashMap<(u32, u32), u32> = HashMap::new();
            let mut out = vec![];
            for t in idx.chunks_exact(3) {
                let mut mid = |a: u32, b: u32| {
                    *middle.entry((a.min(b), a.max(b))).or_insert_with(|| {
                        v.push(((v[a as usize] + v[b as usize]) / 2.0).normalize());
                        (v.len() - 1) as u32
                    })
                };
                let (ab, bc, ca) = (mid(t[0], t[1]), mid(t[1], t[2]), mid(t[2], t[0]));
                out.extend_from_slice(&[t[0], ab, ca, ab, t[1], bc, ca, bc, t[2], ab, bc, ca]);
            }
            idx = out;
        }
        let mut m = Maplet::from_surface(&v, &idx, V3::Z, Some(V3::Z), 21, 0.04).unwrap();
        for h in m.heights.iter_mut() {
            *h += 0.05;
        }
        let (out, moved, cover) = fuse(&v, &idx, &[m], 0, 0, 0.0);
        assert_eq!(out.len(), v.len());
        let top = (0..v.len()).max_by(|&a, &b| v[a].z.total_cmp(&v[b].z)).unwrap();
        assert!((moved[top] - 0.05).abs() < 0.005, "the top rose {}", moved[top]);
        assert!(cover[top] > 0.9);
        for k in 0..v.len() {
            if v[k].z < 0.8 {
                assert_eq!(moved[k], 0.0, "vertex {k} at z {} moved", v[k].z);
            }
        }
    }

    #[test]
    fn merge_makes_a_closed_mesh_with_a_maplet_sunk_in_it() {
        // The sphere of fuse's test; a maplet over its top sunk 0.1 into
        // it. Meshed again: one closed piece, no handle, dented under the
        // maplet's middle by about that, as it was far from it.
        let (mut v, mut idx) = (
            vec![V3::X, -V3::X, V3::Y, -V3::Y, V3::Z, -V3::Z],
            vec![0u32, 2, 4, 2, 1, 4, 1, 3, 4, 3, 0, 4, 2, 0, 5, 1, 2, 5, 3, 1, 5, 0, 3, 5],
        );
        for _ in 0..5 {
            let mut middle: HashMap<(u32, u32), u32> = HashMap::new();
            let mut out = vec![];
            for t in idx.chunks_exact(3) {
                let mut mid = |a: u32, b: u32| {
                    *middle.entry((a.min(b), a.max(b))).or_insert_with(|| {
                        v.push(((v[a as usize] + v[b as usize]) / 2.0).normalize());
                        (v.len() - 1) as u32
                    })
                };
                let (ab, bc, ca) = (mid(t[0], t[1]), mid(t[1], t[2]), mid(t[2], t[0]));
                out.extend_from_slice(&[t[0], ab, ca, ab, t[1], bc, ca, bc, t[2], ab, bc, ca]);
            }
            idx = out;
        }
        let mut m = Maplet::from_surface(&v, &idx, V3::Z, Some(V3::Z), 21, 0.04).unwrap();
        for h in m.heights.iter_mut() {
            *h -= 0.1;
        }
        let (out, oidx, report) = merge(&v, &idx, &[m], 0.03, 0.03, 2, 0.3).unwrap();
        assert_eq!(report.genus, 0, "{report:?}");
        assert_eq!(report.dropped, 0, "{report:?}");
        let top = out.iter().map(|p| p.z).fold(F::MIN, F::max);
        assert!((top - 0.9).abs() < 0.04, "the top at {top}, the maplet's middle at 0.9");
        let equator = out.iter().filter(|p| p.z.abs() < 0.05).map(|p| p.length()).fold(0.0, F::max);
        assert!((equator - 1.0).abs() < 0.03, "the equator at {equator}");
        assert!(oidx.len() % 3 == 0 && !out.is_empty());
    }
}
