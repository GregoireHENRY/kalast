//! Rays against a shape model: a bounding volume hierarchy over its
//! triangles, binned by the surface area heuristic, traversed nearest child
//! first. What hides a point from a camera, or the Sun from it.

use super::{F, V3};

/// A node: its box, and either two children -- `first`, `first + 1` -- or,
/// a leaf, `count` triangles from `first`.
#[derive(Clone, Copy)]
struct Node {
    lo: [f32; 3],
    hi: [f32; 3],
    first: u32,
    count: u32,
}

/// The hierarchy, its triangles in its own order, `f32`: a millimetre on a
/// body of kilometres.
pub struct Bvh {
    nodes: Vec<Node>,
    tris: Vec<[[f32; 3]; 3]>,
    /// The facet each of `tris` is, in the mesh's order.
    ids: Vec<u32>,
}

const BINS: usize = 12;
const LEAF: usize = 4;

impl Bvh {
    /// Over `indices`' triangles of `positions`. Three seconds or so for 4M.
    pub fn new<P: Copy + Into<V3>>(positions: &[P], indices: &[u32]) -> Bvh {
        let count = indices.len() / 3;
        let corner = |f: usize, k: usize| -> [f32; 3] {
            let p: V3 = positions[indices[3 * f + k] as usize].into();
            [p.x as f32, p.y as f32, p.z as f32]
        };
        let tris: Vec<[[f32; 3]; 3]> = (0..count).map(|f| [corner(f, 0), corner(f, 1), corner(f, 2)]).collect();
        let centres: Vec<[f32; 3]> = tris
            .iter()
            .map(|t| std::array::from_fn(|a| (t[0][a] + t[1][a] + t[2][a]) / 3.0))
            .collect();
        let mut ids: Vec<u32> = (0..count as u32).collect();
        let mut nodes = Vec::with_capacity(2 * count / LEAF + 1);
        nodes.push(Node { lo: [0.0; 3], hi: [0.0; 3], first: 0, count: count as u32 });
        // Split nodes until each leaf holds a few triangles or splitting
        // would cost more than it saves.
        let mut stack = vec![0usize];
        while let Some(at) = stack.pop() {
            let (first, count) = (nodes[at].first as usize, nodes[at].count as usize);
            let span = &ids[first..first + count];
            let (lo, hi) = bounds(span.iter().map(|&i| &tris[i as usize]));
            nodes[at].lo = lo;
            nodes[at].hi = hi;
            if count <= LEAF {
                continue;
            }
            let Some((axis, split)) = best_split(span, &tris, &centres) else {
                continue;
            };
            // Partition by the centre's bin.
            let span = &mut ids[first..first + count];
            let mut left = 0;
            for k in 0..count {
                if centres[span[k] as usize][axis] < split {
                    span.swap(k, left);
                    left += 1;
                }
            }
            if left == 0 || left == count {
                continue;
            }
            let child = nodes.len();
            nodes.push(Node { lo: [0.0; 3], hi: [0.0; 3], first: first as u32, count: left as u32 });
            nodes.push(Node { lo: [0.0; 3], hi: [0.0; 3], first: (first + left) as u32, count: (count - left) as u32 });
            nodes[at].first = child as u32;
            nodes[at].count = 0;
            stack.push(child);
            stack.push(child + 1);
        }
        let tris = ids.iter().map(|&i| tris[i as usize]).collect();
        Bvh { nodes, tris, ids }
    }

    /// Whether a triangle meets the ray from `origin` along unit `dir`
    /// before `far`, facets for which `skip` holds aside.
    pub fn blocked(&self, origin: V3, dir: V3, far: F, skip: &dyn Fn(u32) -> bool) -> bool {
        self.trace(origin, dir, far, true, skip).is_some()
    }

    /// The first triangle the ray meets before `far`: its distance and its
    /// facet in the mesh's order.
    pub fn first(&self, origin: V3, dir: V3, far: F, skip: &dyn Fn(u32) -> bool) -> Option<(F, u32)> {
        self.trace(origin, dir, far, false, skip)
    }

    fn trace(&self, origin: V3, dir: V3, far: F, any: bool, skip: &dyn Fn(u32) -> bool) -> Option<(F, u32)> {
        if self.tris.is_empty() {
            return None;
        }
        let o = [origin.x as f32, origin.y as f32, origin.z as f32];
        let d = [dir.x as f32, dir.y as f32, dir.z as f32];
        let inv = d.map(|x| 1.0 / x);
        let mut best = far as f32;
        let mut hit = None;
        let mut stack = [0u32; 128];
        let mut top = 1;
        while top > 0 {
            top -= 1;
            let node = &self.nodes[stack[top] as usize];
            if node.count > 0 {
                let first = node.first as usize;
                for k in first..first + node.count as usize {
                    let id = self.ids[k];
                    if let Some(t) = triangle(&self.tris[k], o, d) {
                        if t < best && !skip(id) {
                            best = t;
                            hit = Some((t as F, id));
                            if any {
                                return hit;
                            }
                        }
                    }
                }
                continue;
            }
            let (a, b) = (node.first as usize, node.first as usize + 1);
            let ta = enter(&self.nodes[a], o, inv, best);
            let tb = enter(&self.nodes[b], o, inv, best);
            // The nearer child last, popped first.
            match (ta, tb) {
                (Some(ta), Some(tb)) => {
                    let (near, far) = if ta <= tb { (a, b) } else { (b, a) };
                    stack[top] = far as u32;
                    stack[top + 1] = near as u32;
                    top += 2;
                }
                (Some(_), None) => {
                    stack[top] = a as u32;
                    top += 1;
                }
                (None, Some(_)) => {
                    stack[top] = b as u32;
                    top += 1;
                }
                (None, None) => {}
            }
        }
        hit
    }
}

fn bounds<'a>(tris: impl Iterator<Item = &'a [[f32; 3]; 3]>) -> ([f32; 3], [f32; 3]) {
    let (mut lo, mut hi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    for t in tris {
        for p in t {
            for a in 0..3 {
                lo[a] = lo[a].min(p[a]);
                hi[a] = hi[a].max(p[a]);
            }
        }
    }
    (lo, hi)
}

fn area(lo: [f32; 3], hi: [f32; 3]) -> f32 {
    let e = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
    if e[0] < 0.0 {
        return 0.0;
    }
    e[0] * e[1] + e[1] * e[2] + e[2] * e[0]
}

/// The axis and the plane across it that split `span` cheapest, by the
/// surface area heuristic over `BINS` bins of the centres; `None` when no
/// split costs less than the leaf.
fn best_split(span: &[u32], tris: &[[[f32; 3]; 3]], centres: &[[f32; 3]]) -> Option<(usize, f32)> {
    let (mut lo, mut hi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    for &i in span {
        let c = centres[i as usize];
        for a in 0..3 {
            lo[a] = lo[a].min(c[a]);
            hi[a] = hi[a].max(c[a]);
        }
    }
    let mut best: Option<(f32, usize, f32)> = None;
    for axis in 0..3 {
        let extent = hi[axis] - lo[axis];
        if !(extent > 0.0) {
            continue;
        }
        let scale = BINS as f32 / extent;
        let mut counts = [0usize; BINS];
        let mut boxes = [([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]); BINS];
        for &i in span {
            let b = (((centres[i as usize][axis] - lo[axis]) * scale) as usize).min(BINS - 1);
            counts[b] += 1;
            let (bl, bh) = &mut boxes[b];
            for p in &tris[i as usize] {
                for a in 0..3 {
                    bl[a] = bl[a].min(p[a]);
                    bh[a] = bh[a].max(p[a]);
                }
            }
        }
        // Left of each plane, then right of it.
        let mut left = [(0usize, 0f32); BINS - 1];
        let (mut n, mut bl, mut bh) = (0, [f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
        for b in 0..BINS - 1 {
            n += counts[b];
            for a in 0..3 {
                bl[a] = bl[a].min(boxes[b].0[a]);
                bh[a] = bh[a].max(boxes[b].1[a]);
            }
            left[b] = (n, area(bl, bh));
        }
        let (mut n, mut bl, mut bh) = (0, [f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
        for b in (1..BINS).rev() {
            n += counts[b];
            for a in 0..3 {
                bl[a] = bl[a].min(boxes[b].0[a]);
                bh[a] = bh[a].max(boxes[b].1[a]);
            }
            let (nl, al) = left[b - 1];
            if nl == 0 || n == 0 {
                continue;
            }
            let cost = nl as f32 * al + n as f32 * area(bl, bh);
            if best.is_none_or(|(c, _, _)| cost < c) {
                best = Some((cost, axis, lo[axis] + b as f32 / scale));
            }
        }
    }
    let (cost, axis, split) = best?;
    let (blo, bhi) = bounds(span.iter().map(|&i| &tris[i as usize]));
    // A leaf costs its triangles; a split, a traversal step more.
    (cost < (span.len() as f32 - 1.0) * area(blo, bhi) || span.len() > 4 * LEAF).then_some((axis, split))
}

/// Where the ray enters the box, if before `far`.
#[inline]
fn enter(node: &Node, o: [f32; 3], inv: [f32; 3], far: f32) -> Option<f32> {
    let (mut t0, mut t1) = (0f32, far);
    for a in 0..3 {
        let ta = (node.lo[a] - o[a]) * inv[a];
        let tb = (node.hi[a] - o[a]) * inv[a];
        let (ta, tb) = if ta <= tb { (ta, tb) } else { (tb, ta) };
        // NaN, from a zero direction on a box's face, leaves the bound.
        t0 = if ta > t0 { ta } else { t0 };
        t1 = if tb < t1 { tb } else { t1 };
    }
    (t0 <= t1).then_some(t0)
}

/// Moller-Trumbore: the distance along the ray to the triangle, either
/// side of it.
#[inline]
fn triangle(t: &[[f32; 3]; 3], o: [f32; 3], d: [f32; 3]) -> Option<f32> {
    let sub = |a: [f32; 3], b: [f32; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let cross = |a: [f32; 3], b: [f32; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let e1 = sub(t[1], t[0]);
    let e2 = sub(t[2], t[0]);
    let p = cross(d, e2);
    let det = dot(e1, p);
    if det.abs() < 1e-20 {
        return None;
    }
    let inv = 1.0 / det;
    let s = sub(o, t[0]);
    let u = dot(s, p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = cross(s, e1);
    let v = dot(d, q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = dot(e2, q) * inv;
    (t > 0.0).then_some(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unit cube's twelve triangles.
    fn cube() -> (Vec<V3>, Vec<u32>) {
        let p: Vec<V3> = (0..8).map(|k| V3::new((k & 1) as F, ((k >> 1) & 1) as F, ((k >> 2) & 1) as F)).collect();
        let quads = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
        let mut idx = vec![];
        for q in quads {
            idx.extend_from_slice(&[q[0], q[1], q[2], q[0], q[2], q[3]]);
        }
        (p, idx)
    }

    #[test]
    fn rays_meet_the_cube_where_they_should() {
        let (p, idx) = cube();
        let bvh = Bvh::new(&p, &idx);
        let none = |_: u32| false;
        let (t, _) = bvh.first(V3::new(0.5, 0.5, -2.0), V3::Z, 10.0, &none).unwrap();
        assert!((t - 2.0).abs() < 1e-6, "{t}");
        assert!(bvh.first(V3::new(2.0, 0.5, -2.0), V3::Z, 10.0, &none).is_none());
        assert!(!bvh.blocked(V3::new(0.5, 0.5, -2.0), V3::Z, 1.5, &none));
        // From inside, out through the top.
        let (t, f) = bvh.first(V3::new(0.3, 0.6, 0.5), V3::Z, 10.0, &none).unwrap();
        assert!((t - 0.5).abs() < 1e-6);
        // Skipping that facet, the ray leaves through its twin or not at all.
        let skip = move |g: u32| g == f;
        assert!(bvh.first(V3::new(0.3, 0.6, 0.5), V3::Z, 10.0, &skip).is_none_or(|(_, g)| g != f));
    }

    #[test]
    fn many_triangles_agree_with_brute_force() {
        // A bumpy sheet of 20k triangles, rays from above at random.
        let n = 101;
        let mut p = vec![];
        for j in 0..n {
            for i in 0..n {
                let (x, y) = (i as F / 10.0, j as F / 10.0);
                p.push(V3::new(x, y, (x * 1.3).sin() * (y * 0.7).cos()));
            }
        }
        let mut idx = vec![];
        for j in 0..n - 1 {
            for i in 0..n - 1 {
                let a = (j * n + i) as u32;
                idx.extend_from_slice(&[a, a + 1, a + n as u32 + 1, a, a + n as u32 + 1, a + n as u32]);
            }
        }
        let bvh = Bvh::new(&p, &idx);
        let none = |_: u32| false;
        let mut seed = 12345u64;
        let mut rand = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 11) as F / (1u64 << 53) as F
        };
        for _ in 0..200 {
            let o = V3::new(rand() * 10.0, rand() * 10.0, 3.0);
            let d = V3::new(rand() - 0.5, rand() - 0.5, -1.0).normalize();
            let fast = bvh.first(o, d, 100.0, &none);
            let mut slow: Option<(F, u32)> = None;
            for f in 0..idx.len() / 3 {
                let t: [[f32; 3]; 3] = std::array::from_fn(|k| {
                    let q = p[idx[3 * f + k] as usize];
                    [q.x as f32, q.y as f32, q.z as f32]
                });
                let o32 = [o.x as f32, o.y as f32, o.z as f32];
                let d32 = [d.x as f32, d.y as f32, d.z as f32];
                if let Some(t) = triangle(&t, o32, d32) {
                    if slow.is_none_or(|(s, _)| (t as F) < s) {
                        slow = Some((t as F, f as u32));
                    }
                }
            }
            match (fast, slow) {
                (Some((a, _)), Some((b, _))) => assert!((a - b).abs() < 1e-5, "{a} {b}"),
                (None, None) => {}
                other => panic!("{other:?}"),
            }
        }
    }
}
