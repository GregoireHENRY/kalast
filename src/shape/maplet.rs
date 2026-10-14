//! A maplet: a square grid of heights and albedo over a patch of a surface.

use super::grid;
use super::view::View;
use crate::lightcurve::Law;
use super::{F, V3};

/// A square grid of heights and albedo over a patch of a body's surface,
/// along the patch's outward normal. See the [module](super) for the grid.
#[derive(Clone, Debug)]
pub struct Maplet {
    /// The grid's centre at height 0, in the body's frame.
    pub centre: V3,
    /// `u`, `v` across the grid -- `i` along `u`, `j` along `v` -- and `w`
    /// the outward normal, `u x v = w`.
    pub axes: [V3; 3],
    /// Points on a side.
    pub n: usize,
    /// Between neighbouring points, in the surface's unit.
    pub spacing: F,
    /// Along `w`, `n x n`, row `j` after row `j - 1`.
    pub heights: Vec<F>,
    /// Relative albedo, the I/F's factor on the law's, `n x n`: about 1.
    pub albedo: Vec<F>,
    /// The shape model's heights the solution is held to at long
    /// wavelengths; not finite where it was not found.
    pub reference: Vec<F>,
    /// The shape model's facets the grid covers: to leave out of a mesh the
    /// maplet joins, or to tell in a render of the model which pixels see
    /// this patch rather than terrain in front of it.
    pub facets: Vec<u32>,
}

impl Maplet {
    /// A maplet of given heights and albedo, its reference its heights.
    pub fn new(
        centre: V3,
        normal: V3,
        n: usize,
        spacing: F,
        heights: Vec<F>,
        albedo: Vec<F>,
    ) -> Result<Maplet, String> {
        if n < 3 || heights.len() != n * n || albedo.len() != n * n {
            return Err(format!(
                "a maplet of {n} x {n} points takes {} heights and albedos, got {} and {}",
                n * n,
                heights.len(),
                albedo.len()
            ));
        }
        if !(spacing > 0.0) {
            return Err(format!("the spacing is {spacing}"));
        }
        Ok(Maplet {
            centre,
            axes: frame(normal)?,
            n,
            spacing,
            reference: heights.clone(),
            heights,
            albedo,
            facets: vec![],
        })
    }

    /// The maplet over a surface around `centre`, `n` points on a side and
    /// `spacing` apart, along `normal` -- the mean normal of the surface
    /// under it when `None`. Each point's height is where a ray down `-w`
    /// first meets a facet facing up, within half the maplet's width of the
    /// plane through `centre`: the top of an overhang, as a height field
    /// must. Where none is met, the nearest point's, and no reference.
    ///
    /// One pass over the facets, then the grid points under each facet near
    /// the patch: a few tenths of a second for 44M.
    pub fn from_surface<P: Copy + Into<V3> + Sync>(
        positions: &[P],
        indices: &[u32],
        centre: V3,
        normal: Option<V3>,
        n: usize,
        spacing: F,
    ) -> Result<Maplet, String> {
        if n < 3 {
            return Err(format!("a maplet takes 3 points on a side or more, got {n}"));
        }
        if !(spacing > 0.0) {
            return Err(format!("the spacing is {spacing}"));
        }
        if indices.len() % 3 != 0 {
            return Err("the indices come in threes".into());
        }
        let half = 0.5 * (n - 1) as F * spacing;
        let reach = half * std::f64::consts::SQRT_2;
        // The facets near the patch: within reach of the centre, counting
        // each facet's size.
        let near: Vec<usize> = super::par_map(indices.len() / 3, |f| {
            let t = facet(positions, indices, f)?;
            let c = (t[0] + t[1] + t[2]) / 3.0;
            let size = t.iter().map(|p| (*p - c).length()).fold(0.0, F::max);
            ((c - centre).length() < reach + size).then_some(f)
        })
        .into_iter()
        .flatten()
        .collect();
        if near.is_empty() {
            return Err("no facet of the surface near the maplet's centre".into());
        }
        let normal = match normal {
            Some(v) => v,
            None => {
                let mut sum = V3::ZERO;
                for &f in &near {
                    let t = facet(positions, indices, f).unwrap();
                    let c = (t[0] + t[1] + t[2]) / 3.0;
                    if (c - centre).length() < half {
                        sum += (t[1] - t[0]).cross(t[2] - t[0]);
                    }
                }
                sum
            }
        };
        let axes = frame(normal)?;
        let [u, v, w] = axes;
        let c = (n - 1) as F / 2.0;
        let window = half.max(10.0 * spacing);
        let mut reference = vec![F::NEG_INFINITY; n * n];
        let mut facets = vec![];
        for &f in &near {
            let t = facet(positions, indices, f).unwrap();
            let local = t.map(|p| {
                let d = p - centre;
                [d.dot(u), d.dot(v), d.dot(w)]
            });
            let zs = local.map(|l| l[2]);
            if zs.iter().all(|&z| z > window) || zs.iter().all(|&z| z < -window) {
                continue;
            }
            let [a, b, cc] = local;
            let mid = [(a[0] + b[0] + cc[0]) / 3.0, (a[1] + b[1] + cc[1]) / 3.0, (a[2] + b[2] + cc[2]) / 3.0];
            if mid[0].abs() <= half + 0.5 * spacing && mid[1].abs() <= half + 0.5 * spacing && mid[2].abs() < window {
                facets.push(f as u32);
            }
            // Facing up: counterclockwise in (u, v).
            let area = (b[0] - a[0]) * (cc[1] - a[1]) - (b[1] - a[1]) * (cc[0] - a[0]);
            if area <= 0.0 {
                continue;
            }
            let lo = |k: usize| local.iter().map(|l| l[k]).fold(F::INFINITY, F::min);
            let hi = |k: usize| local.iter().map(|l| l[k]).fold(F::NEG_INFINITY, F::max);
            let first = |x: F| ((x / spacing + c).ceil().max(0.0)) as usize;
            let last = |x: F| (x / spacing + c).floor();
            let (i0, j0) = (first(lo(0)), first(lo(1)));
            let (i1, j1) = (last(hi(0)), last(hi(1)));
            if i1 < 0.0 || j1 < 0.0 {
                continue;
            }
            let (i1, j1) = ((i1 as usize).min(n - 1), (j1 as usize).min(n - 1));
            for j in j0..=j1 {
                for i in i0..=i1 {
                    let (x, y) = ((i as F - c) * spacing, (j as F - c) * spacing);
                    // Barycentric in (u, v).
                    let l1 = ((b[1] - cc[1]) * (x - cc[0]) + (cc[0] - b[0]) * (y - cc[1])) / area;
                    let l2 = ((cc[1] - a[1]) * (x - cc[0]) + (a[0] - cc[0]) * (y - cc[1])) / area;
                    let l3 = 1.0 - l1 - l2;
                    let e = -1e-9;
                    if l1 < e || l2 < e || l3 < e {
                        continue;
                    }
                    let z = l1 * a[2] + l2 * b[2] + l3 * cc[2];
                    if z.abs() < window && z > reference[j * n + i] {
                        reference[j * n + i] = z;
                    }
                }
            }
        }
        for r in reference.iter_mut() {
            if !r.is_finite() {
                *r = F::NAN;
            }
        }
        if reference.iter().all(|r| !r.is_finite()) {
            return Err("the surface was found under none of the maplet's points".into());
        }
        let heights = grid::fill(&reference, n);
        Ok(Maplet {
            centre,
            axes,
            n,
            spacing,
            heights,
            albedo: vec![1.0; n * n],
            reference,
            facets,
        })
    }

    /// Point `(i, j)` in the body's frame.
    #[inline]
    pub fn point(&self, i: usize, j: usize) -> V3 {
        let c = (self.n - 1) as F / 2.0;
        let [u, v, w] = self.axes;
        self.centre
            + u * ((i as F - c) * self.spacing)
            + v * ((j as F - c) * self.spacing)
            + w * self.heights[j * self.n + i]
    }

    /// Every point, row after row.
    pub fn points(&self) -> Vec<V3> {
        (0..self.n * self.n).map(|k| self.point(k % self.n, k / self.n)).collect()
    }

    /// The outward normal at each point, of the heights' central slopes.
    pub fn normals(&self) -> Vec<V3> {
        let (p, q) = grid::slopes(&self.heights, self.n, self.spacing);
        let [u, v, w] = self.axes;
        p.iter().zip(&q).map(|(p, q)| (w - u * *p - v * *q).normalize()).collect()
    }

    /// `body`, in the body's frame, as the maplet's `(u, v, w)`.
    #[inline]
    pub fn local(&self, body: V3) -> [F; 3] {
        let [u, v, w] = self.axes;
        [body.dot(u), body.dot(v), body.dot(w)]
    }

    /// The grid as triangles, two a cell facing out: the points row after
    /// row, and three indices a facet.
    pub fn mesh(&self) -> (Vec<V3>, Vec<u32>) {
        let n = self.n;
        let mut indices = Vec::with_capacity(6 * (n - 1) * (n - 1));
        for j in 0..n - 1 {
            for i in 0..n - 1 {
                let a = (j * n + i) as u32;
                let (b, c, d) = (a + 1, a + n as u32, a + n as u32 + 1);
                indices.extend_from_slice(&[a, b, d, a, d, c]);
            }
        }
        (self.points(), indices)
    }

    /// The same patch at another spacing: `n` points over it now, the
    /// heights and albedo bilinear between the old points, the reference
    /// averaged over them when coarser and bilinear when finer.
    pub fn resampled(&self, n: usize, spacing: F) -> Maplet {
        let finer = spacing < self.spacing;
        let reference = if finer {
            let filled = grid::fill(&self.reference, self.n);
            let known: Vec<F> = self.reference.iter().map(|r| if r.is_finite() { 1.0 } else { 0.0 }).collect();
            let r = grid::resample(&filled, self.n, self.spacing, n, spacing);
            let k = grid::resample(&known, self.n, self.spacing, n, spacing);
            r.iter().zip(&k).map(|(r, k)| if *k > 0.99 { *r } else { F::NAN }).collect()
        } else {
            grid::coarsen(&self.reference, self.n, self.spacing, n, spacing)
        };
        Maplet {
            centre: self.centre,
            axes: self.axes,
            n,
            spacing,
            heights: grid::resample(&self.heights, self.n, self.spacing, n, spacing),
            albedo: grid::resample(&self.albedo, self.n, self.spacing, n, spacing),
            reference,
            facets: self.facets.clone(),
        }
    }

    /// The maplet as `view` sees it under `law`: each pixel the mean of
    /// `oversample x oversample` rays, each the I/F of the facet it meets
    /// first -- two facets a cell, flat, at its corners' mean albedo, lit as
    /// its corners are (the maplet's own cast shadows). Not finite where a
    /// ray meets none. For synthetic images, and for drawing what a solution
    /// predicts beside what an image shows; [`View::valid`] is not used.
    pub fn render(&self, view: &View, law: &Law, oversample: usize) -> Vec<f32> {
        let os = oversample.max(1);
        let n = self.n;
        let (w_img, h_img) = (view.width, view.height);
        let points = self.points();
        let projected: Vec<Option<(F, F, F)>> = points.iter().map(|p| view.project(*p)).collect();
        let sun = self.local(view.sun);
        let lit = grid::unblocked(&self.heights, n, self.spacing, sun);
        // The window the maplet falls in.
        let (mut x0, mut y0, mut x1, mut y1) = (F::INFINITY, F::INFINITY, F::NEG_INFINITY, F::NEG_INFINITY);
        for (x, y, _) in projected.iter().flatten() {
            x0 = x0.min(*x);
            y0 = y0.min(*y);
            x1 = x1.max(*x);
            y1 = y1.max(*y);
        }
        let mut out = vec![F::NAN as f32; w_img * h_img];
        if !(x0.is_finite()) {
            return out;
        }
        let bx0 = (x0.floor().max(0.0) as usize).min(w_img);
        let by0 = (y0.floor().max(0.0) as usize).min(h_img);
        let bx1 = (x1.ceil().max(0.0) as usize).min(w_img);
        let by1 = (y1.ceil().max(0.0) as usize).min(h_img);
        if bx1 <= bx0 || by1 <= by0 {
            return out;
        }
        let (bw, bh) = ((bx1 - bx0) * os, (by1 - by0) * os);
        let mut depth = vec![F::INFINITY; bw * bh];
        let mut shade = vec![0f32; bw * bh];
        let mut hit = vec![false; bw * bh];
        let (_, indices) = self.mesh();
        let pi = std::f64::consts::PI;
        for t in indices.chunks_exact(3) {
            let k = [t[0] as usize, t[1] as usize, t[2] as usize];
            let (Some(a), Some(b), Some(c)) = (projected[k[0]], projected[k[1]], projected[k[2]]) else {
                continue;
            };
            let (pa, pb, pc) = (points[k[0]], points[k[1]], points[k[2]]);
            let normal = (pb - pa).cross(pc - pa).normalize();
            let mid = (pa + pb + pc) / 3.0;
            let to_camera = (view.position - mid).normalize();
            let mu = normal.dot(to_camera);
            if mu <= 0.0 {
                continue;
            }
            let mu0 = normal.dot(view.sun);
            let alpha = view.sun.dot(to_camera).clamp(-1.0, 1.0).acos();
            let fraction = k.iter().filter(|&&p| lit[p]).count() as F / 3.0;
            let albedo = k.iter().map(|&p| self.albedo[p]).sum::<F>() / 3.0;
            let value = if mu0 > 0.0 { albedo * fraction * pi * mu0 * super::reflectance(law, mu0, mu, alpha) } else { 0.0 };
            // Rasterise in the oversampled window.
            let to = |p: (F, F, F)| ((p.0 - bx0 as F) * os as F, (p.1 - by0 as F) * os as F, p.2);
            let (a, b, c) = (to(a), to(b), to(c));
            let area = (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0);
            if area.abs() < 1e-12 {
                continue;
            }
            let sx0 = (a.0.min(b.0).min(c.0).floor().max(0.0)) as usize;
            let sy0 = (a.1.min(b.1).min(c.1).floor().max(0.0)) as usize;
            let sx1 = ((a.0.max(b.0).max(c.0).ceil().max(0.0)) as usize).min(bw);
            let sy1 = ((a.1.max(b.1).max(c.1).ceil().max(0.0)) as usize).min(bh);
            for sy in sy0..sy1 {
                for sx in sx0..sx1 {
                    let (x, y) = (sx as F + 0.5, sy as F + 0.5);
                    let l1 = ((b.0 - x) * (c.1 - y) - (b.1 - y) * (c.0 - x)) / area;
                    let l2 = ((c.0 - x) * (a.1 - y) - (c.1 - y) * (a.0 - x)) / area;
                    let l3 = 1.0 - l1 - l2;
                    if l1 < 0.0 || l2 < 0.0 || l3 < 0.0 {
                        continue;
                    }
                    let z = l1 * a.2 + l2 * b.2 + l3 * c.2;
                    let s = sy * bw + sx;
                    if z < depth[s] {
                        depth[s] = z;
                        shade[s] = value as f32;
                        hit[s] = true;
                    }
                }
            }
        }
        for py in 0..by1 - by0 {
            for px in 0..bx1 - bx0 {
                let mut sum = 0f32;
                let mut all = true;
                for sy in 0..os {
                    for sx in 0..os {
                        let s = (py * os + sy) * bw + px * os + sx;
                        all &= hit[s];
                        sum += shade[s];
                    }
                }
                if all {
                    out[(by0 + py) * w_img + bx0 + px] = sum / (os * os) as f32;
                }
            }
        }
        out
    }
}

fn facet<P: Copy + Into<V3>>(positions: &[P], indices: &[u32], f: usize) -> Option<[V3; 3]> {
    let t = &indices[3 * f..3 * f + 3];
    Some([
        (*positions.get(t[0] as usize)?).into(),
        (*positions.get(t[1] as usize)?).into(),
        (*positions.get(t[2] as usize)?).into(),
    ])
}

/// `u`, `v` across `normal` and `w` along it, `u x v = w`: `u` toward
/// increasing longitude about the body's `z` -- east -- and `v` north,
/// unless `w` is within 25 degrees of a pole, where `u` is about `x`.
fn frame(normal: V3) -> Result<[V3; 3], String> {
    let w = normal.normalize_or_zero();
    if w == V3::ZERO || !w.is_finite() {
        return Err("the maplet's normal is not a direction".into());
    }
    let axis = if w.z.abs() < 0.9 { V3::Z } else { V3::X };
    let u = axis.cross(w).normalize();
    let v = w.cross(u);
    Ok([u, v, w])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plane tilted about `v`, as a mesh of two facets per cell.
    fn plane(n: usize, s: F) -> (Vec<V3>, Vec<u32>) {
        let c = (n - 1) as F / 2.0;
        let positions = (0..n * n)
            .map(|k| {
                let (x, y) = (((k % n) as F - c) * s, ((k / n) as F - c) * s);
                V3::new(x, y, 10.0 + 0.1 * x)
            })
            .collect();
        let mut indices = vec![];
        for j in 0..n - 1 {
            for i in 0..n - 1 {
                let a = (j * n + i) as u32;
                indices.extend_from_slice(&[a, a + 1, a + n as u32 + 1, a, a + n as u32 + 1, a + n as u32]);
            }
        }
        (positions, indices)
    }

    #[test]
    fn a_maplet_lies_on_its_surface() {
        let (positions, indices) = plane(31, 1.0);
        let m = Maplet::from_surface(&positions, &indices, V3::new(0.0, 0.0, 10.0), Some(V3::Z), 21, 0.5).unwrap();
        // At the pole u is about x: x cross z, -y.
        assert!((m.axes[2] - V3::Z).length() < 1e-12);
        assert!((m.axes[0] + V3::Y).length() < 1e-12);
        for j in 0..m.n {
            for i in 0..m.n {
                let p = m.point(i, j);
                assert!((p.z - (10.0 + 0.1 * p.x)).abs() < 1e-9, "{p}");
                assert!(m.reference[j * m.n + i].is_finite());
            }
        }
        // The mean normal when none is given: the plane's.
        let m = Maplet::from_surface(&positions, &indices, V3::new(0.0, 0.0, 10.0), None, 21, 0.5).unwrap();
        let expect = V3::new(-0.1, 0.0, 1.0).normalize();
        assert!((m.axes[2] - expect).length() < 1e-9, "{}", m.axes[2]);
        assert!(m.heights.iter().all(|h| h.abs() < 1e-9));
        assert!(!m.facets.is_empty());
    }

    #[test]
    fn the_mesh_faces_out() {
        let m = Maplet::new(V3::ZERO, V3::new(1.0, 2.0, 3.0), 5, 1.0, vec![0.0; 25], vec![1.0; 25]).unwrap();
        let (p, idx) = m.mesh();
        for t in idx.chunks_exact(3) {
            let n = (p[t[1] as usize] - p[t[0] as usize]).cross(p[t[2] as usize] - p[t[0] as usize]);
            assert!(n.normalize().dot(m.axes[2]) > 0.999);
        }
    }
}
