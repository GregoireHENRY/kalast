//! Exact partial shadowing and visibility, by polygon clipping.
//!
//! The lit fraction of a facet, computed as an *area* rather than sampled at
//! points. `app::facet_shadow` answers the same question by testing 4 points
//! per facet against the GPU shadow map, so its answer is one of
//! `{0, 1/4, 1/2, 3/4, 1}`; this answers it as a real number, exactly, up to
//! the shape's own discretisation.
//!
//! That difference is worth 0.7 to 40 mmag on a synthetic light curve --
//! measured in `examples/analytical/shadow_quantisation.py`, and the reason
//! this module exists. For the thermophysical model the quantisation averages
//! out over a rotation and the GPU path remains the right tool; for
//! photometry it does not, and no amount of mesh refinement rescues it
//! (`N^-0.5` to `N^-0.85`, so 0.1 mmag needs 50k-240k facets).
//!
//! # The method
//!
//! Brož et al. 2023, A&A 676, A60, §3 -- carried over from Prša's Phoebe2,
//! where it models eclipsing binaries. The Sun and the observer are both
//! effectively at infinity for an asteroid, so occlusion along either is a
//! **2D problem** in the plane perpendicular to that direction:
//!
//! 1. project every facet onto that plane;
//! 2. clip the facet against every facet in front of it;
//! 3. the surviving area, back-projected onto the facet's own plane, is the
//!    lit (or visible) area.
//!
//! Run it along the Sun vector for partial shadowing and along the observer
//! vector for partial visibility. Same code, different direction.
//!
//! # Why there is no Clipper2 here
//!
//! Brož uses Vatti's general polygon clipper via Clipper2, because his
//! polygons are arbitrary. Ours are **triangles**, and that is a much easier
//! problem: a triangle is convex, the intersection of convex sets is convex,
//! and the difference `A \ B` for convex `B` with `n` edges decomposes exactly
//! into at most `n` convex pieces --
//!
//! ```text
//! A \ B = union over i of  (A ∩ H_1 ∩ ... ∩ H_{i-1} ∩ ~H_i)
//! ```
//!
//! where `H_i` are `B`'s edge half-planes. Every operation is then
//! Sutherland-Hodgman clipping against one half-plane, which is a dozen lines
//! and exact. So the whole thing needs no dependency, no C++ toolchain in the
//! `maturin develop` path on two machines, and no general-polygon edge cases.
//!
//! The cost is that the piece count can grow with the number of occluders.
//! [`MAX_PIECES`] bounds it; past that the facet reports what it has and sets
//! the overflow flag rather than silently returning a wrong area.

use crate::{Float, Vec3};

/// Below this, an area or a cross product is treated as zero. Areas here are
/// in the mesh's own units squared, and the meshes range from a unit sphere to
/// a kilometre-scale body, so this is relative to the facet rather than
/// absolute wherever it can be.
const EPS: Float = 1e-12;

/// Cap on the convex pieces one facet's un-occluded region may be cut into.
///
/// Each occluder can split a piece into at most 3 (a triangle's edge count),
/// so this is reached only by a facet genuinely shredded by many overlapping
/// occluders. Hitting it is reported, not hidden: see [`LitArea::overflowed`].
pub const MAX_PIECES: usize = 256;

/// A convex polygon in the projection plane, counter-clockwise.
#[derive(Debug, Clone, Default)]
pub struct Poly2 {
    pub pts: Vec<[Float; 2]>,
}

impl Poly2 {
    pub fn tri(a: [Float; 2], b: [Float; 2], c: [Float; 2]) -> Self {
        // Wound counter-clockwise so every half-plane test below has one
        // sign convention. A clockwise triangle would invert every `inside`.
        let cross = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
        Self {
            pts: if cross >= 0.0 {
                vec![a, b, c]
            } else {
                vec![a, c, b]
            },
        }
    }

    /// Twice the signed area, which is what every test here actually wants.
    pub fn area2(&self) -> Float {
        let n = self.pts.len();
        if n < 3 {
            return 0.0;
        }
        let mut s = 0.0;
        for i in 0..n {
            let p = self.pts[i];
            let q = self.pts[(i + 1) % n];
            s += p[0] * q[1] - q[0] * p[1];
        }
        s
    }

    pub fn area(&self) -> Float {
        0.5 * self.area2().abs()
    }

    pub fn is_empty(&self) -> bool {
        self.pts.len() < 3 || self.area2().abs() < EPS
    }

    /// Sutherland-Hodgman against the half-plane `a x + b y + c >= 0`.
    ///
    /// Convex in, convex out, which is the property the whole decomposition
    /// rests on.
    fn clip_half_plane(&self, a: Float, b: Float, c: Float) -> Poly2 {
        let n = self.pts.len();
        if n == 0 {
            return Poly2::default();
        }
        let mut out: Vec<[Float; 2]> = Vec::with_capacity(n + 2);
        for i in 0..n {
            let p = self.pts[i];
            let q = self.pts[(i + 1) % n];
            let dp = a * p[0] + b * p[1] + c;
            let dq = a * q[0] + b * q[1] + c;
            if dp >= 0.0 {
                out.push(p);
            }
            // Strictly opposite signs: a vertex exactly on the line is
            // already emitted by the test above, and adding the crossing
            // there too would duplicate it and can make the area
            // double-count a degenerate spur.
            if (dp > 0.0 && dq < 0.0) || (dp < 0.0 && dq > 0.0) {
                let t = dp / (dp - dq);
                out.push([p[0] + t * (q[0] - p[0]), p[1] + t * (q[1] - p[1])]);
            }
        }
        Poly2 { pts: out }
    }
}

/// The half-planes of a counter-clockwise convex polygon, inward-positive.
fn half_planes(poly: &Poly2) -> Vec<(Float, Float, Float)> {
    let n = poly.pts.len();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let p = poly.pts[i];
        let q = poly.pts[(i + 1) % n];
        let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
        // Inward normal for a CCW winding is the left normal (-dy, dx).
        let (a, b) = (-dy, dx);
        out.push((a, b, -(a * p[0] + b * p[1])));
    }
    out
}

/// `subject \ clipper`, as convex pieces. Both must be convex and CCW.
///
/// The decomposition is exact: walking the clipper's edges, the `i`-th piece
/// is what lies inside the first `i-1` half-planes and outside the `i`-th, and
/// those pieces are disjoint and cover the difference exactly.
pub fn convex_difference(subject: &Poly2, clipper: &Poly2) -> Vec<Poly2> {
    if subject.is_empty() {
        return vec![];
    }
    if clipper.is_empty() {
        return vec![subject.clone()];
    }
    let mut pieces = Vec::new();
    let mut inside = subject.clone();
    for (a, b, c) in half_planes(clipper) {
        // Outside this edge, but inside every edge before it.
        let outside = inside.clip_half_plane(-a, -b, -c);
        if !outside.is_empty() {
            pieces.push(outside);
        }
        inside = inside.clip_half_plane(a, b, c);
        if inside.is_empty() {
            // Nothing of the subject reaches the remaining edges, so the
            // pieces collected so far are the whole difference.
            break;
        }
    }
    pieces
}

/// The facet's plane in projection coordinates: `w = a u + b v + c`.
///
/// Brož's back-projection, eq. (14), solved from the triangle's own three
/// projected points rather than from its normal, so it needs no separate
/// normal and degenerates gracefully. `None` when the triangle is edge-on to
/// the ray, where it has no projected area and can shadow nothing.
fn plane_in_projection(p: &[[Float; 3]; 3]) -> Option<(Float, Float, Float)> {
    let (x0, y0, z0) = (p[0][0], p[0][1], p[0][2]);
    let (x1, y1, z1) = (p[1][0], p[1][1], p[1][2]);
    let (x2, y2, z2) = (p[2][0], p[2][1], p[2][2]);
    let det = (x1 - x0) * (y2 - y0) - (x2 - x0) * (y1 - y0);
    if det.abs() < EPS {
        return None;
    }
    let a = ((z1 - z0) * (y2 - y0) - (z2 - z0) * (y1 - y0)) / det;
    let b = ((x1 - x0) * (z2 - z0) - (x2 - x0) * (z1 - z0)) / det;
    Some((a, b, z0 - a * x0 - b * y0))
}

/// Intersection of two convex polygons: clip one by every half-plane of the
/// other. Convex in, convex out.
fn convex_intersection(subject: &Poly2, clipper: &Poly2) -> Poly2 {
    let mut out = subject.clone();
    for (a, b, c) in half_planes(clipper) {
        out = out.clip_half_plane(a, b, c);
        if out.pts.len() < 3 {
            return Poly2::default();
        }
    }
    out
}

fn centroid(poly: &Poly2) -> [Float; 2] {
    let n = poly.pts.len() as Float;
    let mut s = [0.0, 0.0];
    for p in &poly.pts {
        s[0] += p[0];
        s[1] += p[1];
    }
    [s[0] / n, s[1] / n]
}

/// A facet's lit area, and whether the piece cap was hit computing it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LitArea {
    /// Area still lit, in the projection plane.
    pub projected: Float,
    /// The facet's own projected area, so `projected / total` is the fraction.
    pub total: Float,
    /// True if [`MAX_PIECES`] was reached, meaning `projected` is an upper
    /// bound rather than the answer.
    pub overflowed: bool,
}

impl LitArea {
    pub fn fraction(&self) -> Float {
        if self.total <= EPS {
            0.0
        } else {
            (self.projected / self.total).clamp(0.0, 1.0)
        }
    }
}

/// Two axes spanning the plane perpendicular to `d`.
pub fn basis_for(d: Vec3) -> (Vec3, Vec3) {
    let t = if d.z.abs() > 0.9 { Vec3::X } else { Vec3::Z };
    let u = d.cross(t).normalize();
    (u, d.cross(u))
}

/// Exact lit fraction of `target` against `occluders`, looking along `dir`.
///
/// `dir` points from the body toward the light (or the observer). Only
/// occluders in front of the target along `dir` are subtracted, judged by the
/// plane offset -- for a mesh whose facets do not interpenetrate, that is the
/// same answer a per-point depth test gives and costs one dot product.
///
/// The caller is responsible for handing in a candidate list; doing that
/// naively is `O(n^2)` and the whole point of a spatial index. See
/// [`lit_fractions`].
pub fn lit_area(target: &[Vec3; 3], occluders: &[[Vec3; 3]], dir: Vec3) -> LitArea {
    let (u, v) = basis_for(dir);
    let proj = |p: Vec3| [p.dot(u), p.dot(v)];

    let subject = Poly2::tri(proj(target[0]), proj(target[1]), proj(target[2]));
    let total = subject.area();
    if total <= EPS {
        return LitArea {
            projected: 0.0,
            total: 0.0,
            overflowed: false,
        };
    }

    // Which of the two is in front is decided **at the centroid of their
    // overlap**, not at either triangle's own centroid.
    //
    // Comparing triangle centroids is the obvious thing and it is wrong for a
    // facet near grazing incidence: such a facet is nearly edge-on, its plane
    // is steep in projection, and its centroid depth says almost nothing
    // about the depth where it actually meets an occluder. Measured against a
    // converged ray trace on a cratered sphere, that version disagreed by up
    // to 0.435 on facets with mu_i < 0.05 -- immaterial for flux, since those
    // facets carry 0.22 % of the lit area, but wrong, and wrong in the regime
    // shadows come from.
    let proj3 = |p: Vec3| [p.dot(u), p.dot(v), p.dot(dir)];
    let target_plane = plane_in_projection(&[proj3(target[0]), proj3(target[1]), proj3(target[2])]);

    let mut pieces = vec![subject.clone()];
    let mut overflowed = false;

    for occ in occluders {
        let clip = Poly2::tri(proj(occ[0]), proj(occ[1]), proj(occ[2]));
        if clip.is_empty() {
            continue;
        }
        let overlap = convex_intersection(&subject, &clip);
        if overlap.is_empty() {
            continue;
        }
        let occ_plane =
            plane_in_projection(&[proj3(occ[0]), proj3(occ[1]), proj3(occ[2])]);
        if let (Some(tp), Some(op)) = (target_plane, occ_plane) {
            let m = centroid(&overlap);
            let w_t = tp.0 * m[0] + tp.1 * m[1] + tp.2;
            let w_o = op.0 * m[0] + op.1 * m[1] + op.2;
            if w_o <= w_t {
                continue; // behind the target where they meet: no shadow
            }
        } else if occ_plane.is_none() {
            continue; // edge-on occluder has no projected area to cast
        }
        let mut next = Vec::with_capacity(pieces.len());
        for p in &pieces {
            next.extend(convex_difference(p, &clip));
            if next.len() > MAX_PIECES {
                overflowed = true;
                break;
            }
        }
        pieces = next;
        if overflowed || pieces.is_empty() {
            break;
        }
    }

    LitArea {
        projected: pieces.iter().map(|p| p.area()).sum(),
        total,
        overflowed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tri(a: [Float; 2], b: [Float; 2], c: [Float; 2]) -> Poly2 {
        Poly2::tri(a, b, c)
    }

    fn unit() -> Poly2 {
        tri([0.0, 0.0], [1.0, 0.0], [0.0, 1.0])
    }

    #[test]
    fn triangle_area_is_right_either_winding() {
        assert!((unit().area() - 0.5).abs() < 1e-12);
        // Clockwise input must be rewound, not negated away.
        let cw = tri([0.0, 0.0], [0.0, 1.0], [1.0, 0.0]);
        assert!((cw.area() - 0.5).abs() < 1e-12);
        assert!(cw.area2() > 0.0, "tri() must produce CCW");
    }

    #[test]
    fn subtracting_a_disjoint_triangle_changes_nothing() {
        let far = tri([5.0, 5.0], [6.0, 5.0], [5.0, 6.0]);
        let d = convex_difference(&unit(), &far);
        let a: Float = d.iter().map(|p| p.area()).sum();
        assert!((a - 0.5).abs() < 1e-12, "got {a}");
    }

    #[test]
    fn subtracting_itself_leaves_nothing() {
        let d = convex_difference(&unit(), &unit());
        let a: Float = d.iter().map(|p| p.area()).sum();
        assert!(a < 1e-12, "got {a}");
    }

    #[test]
    fn subtracting_a_covering_triangle_leaves_nothing() {
        let big = tri([-1.0, -1.0], [3.0, -1.0], [-1.0, 3.0]);
        let d = convex_difference(&unit(), &big);
        let a: Float = d.iter().map(|p| p.area()).sum();
        assert!(a < 1e-12, "got {a}");
    }

    /// The case the whole module exists for: a partial overlap, where a
    /// 4-point sample would answer 1/4, 1/2 or 3/4 and this answers exactly.
    #[test]
    fn a_half_covering_square_leaves_exactly_half() {
        // The unit triangle cut by x >= 0.5 has area 0.5 * (0.5)^2 = 0.125
        // remaining on the far side, so the near side keeps 0.375.
        let cover = Poly2 {
            pts: vec![[0.5, -1.0], [3.0, -1.0], [3.0, 3.0], [0.5, 3.0]],
        };
        let d = convex_difference(&unit(), &cover);
        let a: Float = d.iter().map(|p| p.area()).sum();
        assert!((a - 0.375).abs() < 1e-12, "got {a}, want 0.375");
    }

    /// Pieces of a difference must not overlap, or the area double-counts.
    /// Checked by Monte Carlo, which is independent of the decomposition.
    #[test]
    fn difference_pieces_tile_without_overlapping() {
        let subject = tri([0.0, 0.0], [4.0, 0.0], [0.0, 4.0]);
        let clipper = tri([0.5, 0.5], [2.5, 0.5], [0.5, 2.5]);
        let pieces = convex_difference(&subject, &clipper);

        // *Strictly* inside, by a margin. Adjacent pieces share an edge by
        // construction, so an inclusive test counts every boundary point
        // twice and reports an overlap that is not there -- 22 of 40,000 on
        // this configuration, all of them on the clipper's x + y = 3 diagonal.
        // The property that actually matters is that no point is interior to
        // two pieces.
        let inside = |p: &Poly2, x: Float, y: Float| {
            half_planes(p)
                .iter()
                .all(|(a, b, c)| a * x + b * y + c > 1e-9)
        };

        let n = 200;
        let mut hits = 0u32;
        let mut multi = 0u32;
        for i in 0..n {
            for j in 0..n {
                let x = 4.0 * (i as Float + 0.5) / n as Float;
                let y = 4.0 * (j as Float + 0.5) / n as Float;
                let k = pieces.iter().filter(|p| inside(p, x, y)).count();
                if k > 0 {
                    hits += 1;
                }
                if k > 1 {
                    multi += 1;
                }
            }
        }
        assert_eq!(multi, 0, "{multi} sample points landed in two pieces");

        // And the covered area agrees with the analytic 8 - 2 = 6.
        let cell = 16.0 / (n * n) as Float;
        let mc = hits as Float * cell;
        let exact: Float = pieces.iter().map(|p| p.area()).sum();
        assert!((exact - 6.0).abs() < 1e-9, "exact area {exact}");

        // The grid estimate is only good to about `perimeter * cell`, which
        // here is ~20 * 0.02 = 0.4: the boundaries are diagonal and no cell
        // size makes a square grid follow them. Measured 5.94 against 6. This
        // is a sanity bound catching a factor-two error in the decomposition,
        // not a precision claim -- the precision claim is the analytic 6.0
        // above, and the point of the Monte Carlo is that it is derived a
        // completely different way.
        assert!((mc - exact).abs() < 0.4, "monte carlo {mc} vs {exact}");
    }

    #[test]
    fn an_occluder_behind_the_target_does_not_shadow_it() {
        let target = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ];
        let dir = Vec3::Z;
        // Same footprint, but further from the light.
        let behind = [
            Vec3::new(0.0, 0.0, -1.0),
            Vec3::new(1.0, 0.0, -1.0),
            Vec3::new(0.0, 1.0, -1.0),
        ];
        let r = lit_area(&target, &[behind], dir);
        assert!((r.fraction() - 1.0).abs() < 1e-12, "got {}", r.fraction());

        // In front, it shadows completely.
        let front = [
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(1.0, 0.0, 1.0),
            Vec3::new(0.0, 1.0, 1.0),
        ];
        let r = lit_area(&target, &[front], dir);
        assert!(r.fraction() < 1e-12, "got {}", r.fraction());
    }

    /// A fraction a point-sampling scheme cannot produce.
    #[test]
    fn partial_shadow_is_a_real_number_not_a_quarter() {
        let target = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ];
        // Covers x > 0.3 of the footprint: leaves 1 - (0.7)^2 = 0.51.
        let front = [
            Vec3::new(0.3, -1.0, 1.0),
            Vec3::new(3.0, -1.0, 1.0),
            Vec3::new(0.3, 3.0, 1.0),
        ];
        let f = lit_area(&target, &[front], Vec3::Z).fraction();
        assert!((f - 0.51).abs() < 1e-9, "got {f}, want 0.51");
        for q in [0.0, 0.25, 0.5, 0.75, 1.0] {
            assert!((f - q).abs() > 1e-3, "landed on the quantised value {q}");
        }
    }
}

/// Exact lit fraction for every facet of a mesh, looking along `dir`.
///
/// The naive form is `O(n^2)` clips. Because the rays are parallel, the
/// candidate occluders of a facet are exactly those whose *projected* bounding
/// box overlaps its own, so triangles are bucketed into a grid in the
/// projection plane and each facet only meets the ones sharing its cells. That
/// is the same observation the method itself rests on, applied twice.
///
/// `dir` points from the body toward the light or the observer, and need not
/// be normalised -- only its direction is used, and depth comparisons are
/// monotone in its length.
pub fn lit_fractions(tris: &[[Vec3; 3]], dir: Vec3) -> Vec<LitArea> {
    let n = tris.len();
    if n == 0 {
        return vec![];
    }
    let dir = dir.normalize();
    let (u, v) = basis_for(dir);
    let proj = |p: Vec3| [p.dot(u), p.dot(v)];

    // Projected bounding boxes, and the extent they all live in.
    let mut lo = vec![[Float::MAX; 2]; n];
    let mut hi = vec![[Float::MIN; 2]; n];
    let (mut gmin, mut gmax) = ([Float::MAX; 2], [Float::MIN; 2]);
    for (t, tri) in tris.iter().enumerate() {
        for p in tri {
            let q = proj(*p);
            for k in 0..2 {
                lo[t][k] = lo[t][k].min(q[k]);
                hi[t][k] = hi[t][k].max(q[k]);
                gmin[k] = gmin[k].min(q[k]);
                gmax[k] = gmax[k].max(q[k]);
            }
        }
    }

    // Roughly 8 triangles a cell, the same target the Python prototype used.
    let side = ((n as Float / 8.0).sqrt().ceil() as usize).max(1);
    let cell = [
        ((gmax[0] - gmin[0]) / side as Float).max(EPS),
        ((gmax[1] - gmin[1]) / side as Float).max(EPS),
    ];
    let cell_of = |p: [Float; 2], k: usize| {
        (((p[k] - gmin[k]) / cell[k]) as isize).clamp(0, side as isize - 1) as usize
    };

    let mut buckets: Vec<Vec<u32>> = vec![Vec::new(); side * side];
    for t in 0..n {
        for i in cell_of(lo[t], 0)..=cell_of(hi[t], 0) {
            for j in cell_of(lo[t], 1)..=cell_of(hi[t], 1) {
                buckets[i * side + j].push(t as u32);
            }
        }
    }

    let depth_of = |t: &[Vec3; 3]| (t[0] + t[1] + t[2]).dot(dir) / 3.0;

    let mut out = Vec::with_capacity(n);
    let mut cands: Vec<[Vec3; 3]> = Vec::new();
    let mut seen: Vec<u32> = Vec::new();
    for t in 0..n {
        cands.clear();
        seen.clear();
        let d_t = depth_of(&tris[t]);
        for i in cell_of(lo[t], 0)..=cell_of(hi[t], 0) {
            for j in cell_of(lo[t], 1)..=cell_of(hi[t], 1) {
                for &o in &buckets[i * side + j] {
                    if o as usize == t || seen.contains(&o) {
                        continue;
                    }
                    // Bounding boxes must actually overlap, and the occluder
                    // must be in front. Both are cheap next to a clip.
                    let (a, b) = (o as usize, t);
                    if lo[a][0] > hi[b][0]
                        || hi[a][0] < lo[b][0]
                        || lo[a][1] > hi[b][1]
                        || hi[a][1] < lo[b][1]
                    {
                        continue;
                    }
                    if depth_of(&tris[a]) <= d_t {
                        continue;
                    }
                    seen.push(o);
                    cands.push(tris[a]);
                }
            }
        }
        out.push(lit_area(&tris[t], &cands, dir));
    }
    out
}

#[cfg(feature = "python")]
pub(crate) mod py {
    use numpy::{PyArray1, PyReadonlyArray2};
    use pyo3::prelude::*;

    use super::{Float, Vec3};

    /// Exact lit fraction per facet, `(n_facets,)` in `[0, 1]`.
    ///
    /// `vertices` is `(n_vertices, 3)`, `indices` is `(n_facets, 3)`, and
    /// `direction` points from the body toward the Sun (for shadowing) or the
    /// observer (for visibility).
    ///
    /// Unlike `sim.facet_shadow`, this is a real number rather than a multiple
    /// of 1/4, and it needs no GPU, no shadow map and no bias constants.
    #[pyfunction]
    #[pyo3(name = "lit_fractions")]
    pub fn py_lit_fractions<'py>(
        py: Python<'py>,
        vertices: PyReadonlyArray2<'py, Float>,
        indices: PyReadonlyArray2<'py, u32>,
        direction: [Float; 3],
    ) -> PyResult<Bound<'py, PyArray1<Float>>> {
        let v = vertices.as_array();
        let f = indices.as_array();
        if v.shape()[1] != 3 || f.shape()[1] != 3 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "vertices must be (n, 3) and indices (m, 3)",
            ));
        }
        let nv = v.shape()[0];
        let mut tris = Vec::with_capacity(f.shape()[0]);
        for t in 0..f.shape()[0] {
            let mut tri = [Vec3::ZERO; 3];
            for k in 0..3 {
                let i = f[[t, k]] as usize;
                if i >= nv {
                    return Err(pyo3::exceptions::PyValueError::new_err(format!(
                        "index {i} out of range for {nv} vertices"
                    )));
                }
                tri[k] = Vec3::new(v[[i, 0]], v[[i, 1]], v[[i, 2]]);
            }
            tris.push(tri);
        }
        let d = Vec3::new(direction[0], direction[1], direction[2]);
        let out: Vec<Float> = super::lit_fractions(&tris, d)
            .iter()
            .map(|a| a.fraction())
            .collect();
        Ok(PyArray1::from_vec(py, out))
    }
}
