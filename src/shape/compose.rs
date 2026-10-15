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
}
