//! A maplet's grid: slopes from heights, heights from slopes, which points a
//! light or a camera sees past the rest, and one grid sampled on another.

use super::F;

/// The heights' slopes `(p, q) = (dh/dx, dh/dy)`, central differences
/// inside and one-sided on the edges.
pub(crate) fn slopes(h: &[F], n: usize, s: F) -> (Vec<F>, Vec<F>) {
    let mut p = vec![0.0; n * n];
    let mut q = vec![0.0; n * n];
    for j in 0..n {
        for i in 0..n {
            let k = j * n + i;
            let (a, b) = match i {
                0 => (k, k + 1),
                _ if i == n - 1 => (k - 1, k),
                _ => (k - 1, k + 1),
            };
            p[k] = (h[b] - h[a]) / (s * (b - a) as F);
            let (a, b) = match j {
                0 => (k, k + n),
                _ if j == n - 1 => (k - n, k),
                _ => (k - n, k + n),
            };
            q[k] = (h[b] - h[a]) / (s * ((b - a) / n) as F);
        }
    }
    (p, q)
}

/// A box of `width` points (odd) along the rows, then the columns, by
/// running sums, zero beyond the edges: symmetric, as the normal equations
/// need it, and as cheap at any width.
fn boxed(values: &[F], n: usize, width: usize) -> Vec<F> {
    let r = width / 2;
    let w = width as F;
    let mut rows = vec![0.0; n * n];
    for j in 0..n {
        let line = &values[j * n..(j + 1) * n];
        let mut sum: F = line[..r.min(n)].iter().sum();
        for i in 0..n {
            if i + r < n {
                sum += line[i + r];
            }
            rows[j * n + i] = sum / w;
            if i >= r {
                sum -= line[i - r];
            }
        }
    }
    let mut out = vec![0.0; n * n];
    for i in 0..n {
        let mut sum: F = (0..r.min(n)).map(|j| rows[j * n + i]).sum();
        for j in 0..n {
            if j + r < n {
                sum += rows[(j + r) * n + i];
            }
            out[j * n + i] = sum / w;
            if j >= r {
                sum -= rows[(j - r) * n + i];
            }
        }
    }
    out
}

/// A Gaussian of `sigma` points as three boxes, normalised by what they pass
/// of a constant: `G = D^-1 B^3`, each value the weighted mean of the
/// grid's within reach, up to the edges -- and a kernel wider than the grid
/// its mean. `G^T G = B^3 D^-2 B^3`, the box being symmetric.
struct Lowpass {
    width: usize,
    /// `B^3 1`.
    norm: Vec<F>,
}

impl Lowpass {
    fn new(sigma: F, n: usize) -> Lowpass {
        // Three boxes of width w: variance 3 (w^2 - 1) / 12.
        let w = (4.0 * sigma * sigma + 1.0).sqrt().round() as usize;
        let width = (w | 1).max(1);
        let mut norm = vec![1.0; n * n];
        for _ in 0..3 {
            norm = boxed(&norm, n, width);
        }
        Lowpass { width, norm }
    }

    fn three(&self, values: &[F], n: usize) -> Vec<F> {
        let mut v = values.to_vec();
        for _ in 0..3 {
            v = boxed(&v, n, self.width);
        }
        v
    }

    /// `G^T G values`.
    fn twice(&self, values: &[F], n: usize) -> Vec<F> {
        let mut v = self.three(values, n);
        for (x, d) in v.iter_mut().zip(&self.norm) {
            *x /= d * d;
        }
        self.three(&v, n)
    }

    /// The diagonal of `G^T G` inside the grid: the squared weights of the
    /// 2-D kernel, summed.
    fn diagonal(&self) -> F {
        let w = self.width;
        let mut k = vec![1.0 / w as F; w];
        for _ in 0..2 {
            let mut next = vec![0.0; k.len() + w - 1];
            for (a, x) in k.iter().enumerate() {
                for b in 0..w {
                    next[a + b] += x / w as F;
                }
            }
            k = next;
        }
        k.iter().map(|x| x * x).sum::<F>().powi(2)
    }
}

/// The heights' mean and tilt made the reference's, rigidly: what a tilt
/// changes -- every point's brightness alike -- each view's scale takes up,
/// so shading says little of it, and the low-pass holds no tilt once wider
/// than the grid. Held inside the least squares instead, a tilt the slopes
/// disagree on by a hair bends the whole surface into a cubic, the
/// Laplacian's answer to a ramp: 0.14 cm of error became 0.30 on synthetic
/// terrain (`notes/2026-10-14_shape_from_images/`).
pub(crate) fn hold_plane(h: &mut [F], reference: &[F], n: usize) {
    let plane = Plane::new(n);
    let reference = fill(reference, n);
    let difference: Vec<F> = reference.iter().zip(h.iter()).map(|(r, h)| r - h).collect();
    plane.add(&difference, h, 1.0);
}

/// The projection onto planes over the grid, `Q Q^T`, `Q` an orthonormal
/// basis of the constant and the two coordinates.
struct Plane {
    q: [Vec<F>; 3],
}

impl Plane {
    fn new(n: usize) -> Plane {
        let len = n * n;
        let c = (n - 1) as F / 2.0;
        let one = vec![1.0 / (len as F).sqrt(); len];
        let unit = |v: Vec<F>| {
            let norm = v.iter().map(|x| x * x).sum::<F>().sqrt().max(1e-300);
            v.iter().map(|x| x / norm).collect::<Vec<F>>()
        };
        // The coordinates about the centre are orthogonal to the constant
        // and to each other on a square grid.
        let x = unit((0..len).map(|k| (k % n) as F - c).collect());
        let y = unit((0..len).map(|k| (k / n) as F - c).collect());
        Plane { q: [one, x, y] }
    }

    /// `out += weight Q Q^T values`.
    fn add(&self, values: &[F], out: &mut [F], weight: F) {
        for q in &self.q {
            let dot: F = q.iter().zip(values).map(|(a, b)| a * b).sum();
            for (o, a) in out.iter_mut().zip(q) {
                *o += weight * dot * a;
            }
        }
    }

}

/// Heights from slopes: the difference between neighbours as the mean of
/// their two slopes along the edge between them, in least squares, the
/// heights' long wavelengths held to `reference`'s --
///
/// ```text
/// (L + mu G^T G) h = div g + mu G^T G h_ref
/// ```
///
/// `L` the grid's Laplacian, its edges free, `G` a Gaussian -- three boxes,
/// normalised -- that passes half of a wave of `wavelength` (in the
/// surface's unit) and less of shorter ones, `mu` 1: below about the wavelength the heights are their slopes',
/// above it the reference's. A spring on the heights themselves, `mu h`,
/// would pull every wavelength toward the reference -- a shape model's
/// smoothed boulders and all -- in the share its weight bears to the
/// Laplacian's there. By conjugate gradients from `h`, preconditioned by the
/// diagonal; the reference's unknown points the nearest known one's.
///
/// A mean of slopes along an edge rather than either end's: a slope that
/// alternates point to point, which central differences cannot see, adds
/// nothing to the heights. Returns the iterations taken.
pub(crate) fn integrate(h: &mut [F], p: &[F], q: &[F], reference: &[F], n: usize, s: F, wavelength: F) -> usize {
    let len = n * n;
    let mu = 1.0;
    // Half of a wave of `wavelength` through: exp(-2 pi^2 sigma^2 / L^2) = 1/2.
    let sigma = wavelength * (std::f64::consts::LN_2 / (2.0 * std::f64::consts::PI.powi(2))).sqrt() / s;
    let lowpass = Lowpass::new(sigma, n);
    let reference = fill(reference, n);
    let anchored = lowpass.twice(&reference, n);
    let mut b = vec![0.0; len];
    let mut diag = vec![0.0; len];
    let squares = lowpass.diagonal();
    for j in 0..n {
        for i in 0..n {
            let at = j * n + i;
            if i + 1 < n {
                let g = 0.5 * s * (p[at] + p[at + 1]);
                b[at] -= g;
                b[at + 1] += g;
                diag[at] += 1.0;
                diag[at + 1] += 1.0;
            }
            if j + 1 < n {
                let g = 0.5 * s * (q[at] + q[at + n]);
                b[at] -= g;
                b[at + n] += g;
                diag[at] += 1.0;
                diag[at + n] += 1.0;
            }
            b[at] += mu * anchored[at];
            diag[at] += mu * squares;
        }
    }
    let apply = |x: &[F], y: &mut [F]| {
        let held = lowpass.twice(x, n);
        for j in 0..n {
            for i in 0..n {
                let at = j * n + i;
                let mut v = 0.0;
                if i > 0 {
                    v += x[at] - x[at - 1];
                }
                if i + 1 < n {
                    v += x[at] - x[at + 1];
                }
                if j > 0 {
                    v += x[at] - x[at - n];
                }
                if j + 1 < n {
                    v += x[at] - x[at + n];
                }
                y[at] = v + mu * held[at];
            }
        }
    };
    let dot = |a: &[F], b: &[F]| a.iter().zip(b).map(|(x, y)| x * y).sum::<F>();
    let mut ax = vec![0.0; len];
    apply(h, &mut ax);
    let mut r: Vec<F> = b.iter().zip(&ax).map(|(b, a)| b - a).collect();
    let mut z: Vec<F> = r.iter().zip(&diag).map(|(r, d)| r / d.max(1e-12)).collect();
    let mut d = z.clone();
    let mut rz = dot(&r, &z);
    let goal = 1e-12 * dot(&b, &b).max(1e-30);
    let mut ad = vec![0.0; len];
    let max = 10 * n + 500;
    for it in 0..max {
        if dot(&r, &r) <= goal {
            return it;
        }
        apply(&d, &mut ad);
        let curvature = dot(&d, &ad);
        if curvature <= 0.0 {
            return it;
        }
        let alpha = rz / curvature;
        for at in 0..len {
            h[at] += alpha * d[at];
            r[at] -= alpha * ad[at];
        }
        for at in 0..len {
            z[at] = r[at] / diag[at].max(1e-12);
        }
        let next = dot(&r, &z);
        let beta = next / rz;
        rz = next;
        for at in 0..len {
            d[at] = z[at] + beta * d[at];
        }
    }
    max
}

/// The known values where they are, each unknown one the nearest known
/// one's, grown a ring at a time.
pub(crate) fn fill(values: &[F], n: usize) -> Vec<F> {
    let mut out = values.to_vec();
    loop {
        let missing: Vec<usize> = (0..n * n).filter(|&k| !out[k].is_finite()).collect();
        if missing.is_empty() || missing.len() == n * n {
            return out;
        }
        let before = out.clone();
        for k in missing {
            let (i, j) = (k % n, k / n);
            let mut sum = 0.0;
            let mut count = 0;
            for (di, dj) in [(-1isize, 0isize), (1, 0), (0, -1), (0, 1)] {
                let (a, b) = (i as isize + di, j as isize + dj);
                if a >= 0 && b >= 0 && (a as usize) < n && (b as usize) < n {
                    let v = before[b as usize * n + a as usize];
                    if v.is_finite() {
                        sum += v;
                        count += 1;
                    }
                }
            }
            if count > 0 {
                out[k] = sum / count as F;
            }
        }
    }
}

/// Which points see along `d` past the rest of the grid, true for those:
/// `d` a unit direction toward a light or a camera in the maplet's frame
/// `(u, v, w)`.
///
/// The height the terrain toward `d` reaches over each point, carried one
/// column (or row) at a time from the grid's edge facing `d`: the ray from
/// a point toward `d` crosses the next column up between two of its
/// points, and what blocks it there -- that column's terrain, or what
/// blocked it in turn -- less the ray's rise over the step, blocks it here.
/// `O(n^2)`, against `O(n^3)` for a ray per point. Beyond the grid nothing
/// blocks: the shape model's own cast shadows and what hides a point beyond
/// the maplet are [`super::View::valid`]'s. A light below the maplet's plane
/// sees none of it.
pub(crate) fn unblocked(h: &[F], n: usize, s: F, d: [F; 3]) -> Vec<bool> {
    let (du, dv, dw) = (d[0], d[1], d[2]);
    if dw <= 0.0 {
        return vec![false; n * n];
    }
    let mut out = vec![true; n * n];
    if (du * du + dv * dv).sqrt() < 1e-9 {
        return out;
    }
    // Along the axis the direction leans on more: `m` along it, `o` across.
    let along_u = du.abs() >= dv.abs();
    let (a, b) = if along_u { (du, dv) } else { (dv, du) };
    let at = |m: usize, o: usize| if along_u { o * n + m } else { m * n + o };
    let slide = b / a.abs();
    let rise = s * dw / a.abs();
    let eps = 1e-3 * s;
    // What blocks each point's ray or its own terrain, whichever is higher.
    let mut block = vec![0.0; n * n];
    let toward_high = a > 0.0;
    let first = if toward_high { n - 1 } else { 0 };
    for o in 0..n {
        block[at(first, o)] = h[at(first, o)];
    }
    for step in 1..n {
        let m = if toward_high { n - 1 - step } else { step };
        let up = if toward_high { m + 1 } else { m - 1 };
        for o in 0..n {
            let k = at(m, o);
            let x = o as F + slide;
            if x < 0.0 || x > (n - 1) as F {
                block[k] = h[k];
                continue;
            }
            let o0 = (x.floor() as usize).min(n - 2);
            let t = x - o0 as F;
            let over = (1.0 - t) * block[at(up, o0)] + t * block[at(up, o0 + 1)] - rise;
            if h[k] < over - eps {
                out[k] = false;
            }
            block[k] = h[k].max(over);
        }
    }
    out
}

/// A grid of `n` points `spacing` apart sampled at `m` points `step` apart
/// over the same centre, bilinear; held at the edge beyond it.
pub(crate) fn resample(values: &[F], n: usize, spacing: F, m: usize, step: F) -> Vec<F> {
    let c = (n - 1) as F / 2.0;
    let d = (m - 1) as F / 2.0;
    let mut out = vec![0.0; m * m];
    for j in 0..m {
        for i in 0..m {
            let x = (((i as F - d) * step / spacing) + c).clamp(0.0, (n - 1) as F);
            let y = (((j as F - d) * step / spacing) + c).clamp(0.0, (n - 1) as F);
            let (i0, j0) = ((x as usize).min(n - 2), (y as usize).min(n - 2));
            let (tx, ty) = (x - i0 as F, y - j0 as F);
            let v = |a: usize, b: usize| values[b * n + a];
            out[j * m + i] = (1.0 - ty) * ((1.0 - tx) * v(i0, j0) + tx * v(i0 + 1, j0))
                + ty * ((1.0 - tx) * v(i0, j0 + 1) + tx * v(i0 + 1, j0 + 1));
        }
    }
    out
}

/// `values` at each point moved by `(di, dj)` points, bilinear, held at the
/// edge; not finite where an unknown value is near.
pub(crate) fn shifted(values: &[F], n: usize, di: F, dj: F) -> Vec<F> {
    let filled = fill(values, n);
    let known: Vec<F> = values.iter().map(|v| if v.is_finite() { 1.0 } else { 0.0 }).collect();
    let last = (n - 1) as F;
    let at = |a: &[F], x: F, y: F| {
        let (x, y) = (x.clamp(0.0, last), y.clamp(0.0, last));
        let (i, j) = ((x as usize).min(n - 2), (y as usize).min(n - 2));
        let (tx, ty) = (x - i as F, y - j as F);
        let v = |a2: usize, b2: usize| a[b2 * n + a2];
        (1.0 - ty) * ((1.0 - tx) * v(i, j) + tx * v(i + 1, j)) + ty * ((1.0 - tx) * v(i, j + 1) + tx * v(i + 1, j + 1))
    };
    (0..n * n)
        .map(|k| {
            let (x, y) = ((k % n) as F + di, (k / n) as F + dj);
            if at(&known, x, y) > 0.99 {
                at(&filled, x, y)
            } else {
                F::NAN
            }
        })
        .collect()
}

/// The mean of `values` over each block of a coarser grid: `m` points `step`
/// apart over the same centre, each the mean of the fine points nearer it
/// than half a step, the non-finite left out; not finite where all are.
pub(crate) fn coarsen(values: &[F], n: usize, spacing: F, m: usize, step: F) -> Vec<F> {
    let c = (n - 1) as F / 2.0;
    let d = (m - 1) as F / 2.0;
    let mut sum = vec![0.0; m * m];
    let mut count = vec![0usize; m * m];
    for j in 0..n {
        for i in 0..n {
            let v = values[j * n + i];
            if !v.is_finite() {
                continue;
            }
            let a = ((i as F - c) * spacing / step + d).round();
            let b = ((j as F - c) * spacing / step + d).round();
            if a < 0.0 || b < 0.0 || a > (m - 1) as F || b > (m - 1) as F {
                continue;
            }
            let k = b as usize * m + a as usize;
            sum[k] += v;
            count[k] += 1;
        }
    }
    sum.iter()
        .zip(&count)
        .map(|(s, &c)| if c > 0 { s / c as F } else { F::NAN })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heights_come_back_from_their_slopes() {
        let (n, s) = (41, 0.5);
        let c = (n - 1) as F / 2.0;
        let truth: Vec<F> = (0..n * n)
            .map(|k| {
                let (x, y) = (((k % n) as F - c) * s, ((k / n) as F - c) * s);
                0.3 * (x / 3.0).sin() * (y / 2.0).cos() + 0.02 * x
            })
            .collect();
        let (p, q) = slopes(&truth, n, s);
        // The reference a metre off: the anchor keeps only its long waves.
        let reference: Vec<F> = truth.iter().map(|t| t + 1.0).collect();
        let mut h = vec![0.0; n * n];
        integrate(&mut h, &p, &q, &reference, n, s, 1e3);
        let mean: F = h.iter().zip(&truth).map(|(a, b)| a - b).sum::<F>() / (n * n) as F;
        let worst = h.iter().zip(&truth).map(|(a, b)| (a - b - mean).abs()).fold(0.0, F::max);
        assert!((mean - 1.0).abs() < 1e-3, "offset {mean}");
        assert!(worst < 0.01, "worst {worst}");
    }

    #[test]
    fn a_wall_casts_its_shadow() {
        let (n, s) = (21, 1.0);
        let mut h = vec![0.0; n * n];
        for j in 0..n {
            h[j * n + 15] = 5.0; // a wall along v at i = 15
        }
        // The Sun toward +u, 45 deg up: the wall's shadow runs back from
        // it to i = 11, i = 10 grazing its top.
        let d = [std::f64::consts::FRAC_1_SQRT_2, 0.0, std::f64::consts::FRAC_1_SQRT_2];
        let lit = unblocked(&h, n, s, d);
        let row: Vec<bool> = (0..n).map(|i| lit[10 * n + i]).collect();
        assert!(row[..10].iter().all(|&l| l), "{row:?}");
        assert!(row[11..15].iter().all(|&l| !l), "{row:?}");
        assert!(row[15..].iter().all(|&l| l), "{row:?}");
        // From the other side, the floor before the wall is lit and the
        // shadow falls behind it, to i = 20 where the ray grazes its top.
        let lit = unblocked(&h, n, s, [-d[0], 0.0, d[2]]);
        assert!((0..16).all(|i| lit[10 * n + i]));
        assert!((16..20).all(|i| !lit[10 * n + i]));
        assert!(lit[10 * n + 20]);
    }

    #[test]
    fn resampling_keeps_a_plane() {
        let (n, s) = (11, 1.0);
        let plane: Vec<F> = (0..n * n).map(|k| (k % n) as F * 0.5 + (k / n) as F * 0.25).collect();
        let fine = resample(&plane, n, s, 21, 0.5);
        for j in 0..21 {
            for i in 0..21 {
                let expect = i as F * 0.25 + j as F * 0.125;
                assert!((fine[j * 21 + i] - expect).abs() < 1e-12);
            }
        }
        let back = coarsen(&fine, 21, 0.5, n, s);
        for k in 0..n * n {
            assert!((back[k] - plane[k]).abs() < 0.2, "{} {}", back[k], plane[k]);
        }
    }
}
