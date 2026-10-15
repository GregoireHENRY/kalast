//! An image and how it was taken; the window of it a maplet falls in.

use super::{F, M3, V3};

/// One image of the surface, and its camera and Sun.
///
/// A pinhole camera: what the image's archive calls geometrically
/// corrected, or a distortion taken out beforehand.
#[derive(Clone, Debug)]
pub struct View {
    /// For the report.
    pub name: String,
    /// `height` rows of `width`, the first row first: I/F, or anything
    /// proportional to it -- each view's scale is fitted.
    pub image: Vec<f32>,
    pub width: usize,
    pub height: usize,
    /// Which pixels to use, nonzero for those; every pixel when empty. What
    /// the maplet cannot know from its own grid: the shape model's cast
    /// shadows from beyond it, and terrain beyond it between it and the
    /// camera -- a render of the shape model in the image's geometry has
    /// both. Stars, hot pixels and saturation too.
    pub valid: Vec<u8>,
    /// The camera's centre in the body's frame, in the surface's unit.
    pub position: V3,
    /// The camera's axes in the body's frame, by rows: `x` along the image's
    /// rows (to the next column), `y` down its columns (to the next row),
    /// `z` the boresight. Orthonormal; mirrored is allowed, as an archive
    /// may store an image.
    pub axes: M3,
    /// The focal length in pixels: one over a pixel's angle.
    pub focal: F,
    /// Where the boresight falls, `(x, y)` in pixels from the image's outer
    /// corner, pixel `(c, r)` spanning `[c, c + 1) x [r, r + 1)`:
    /// `(1024, 1024)` in a 2048-pixel frame centred on it.
    pub centre: [F; 2],
    /// Toward the Sun from the surface, a unit vector in the body's frame.
    pub sun: V3,
    /// The view's weight against the others, 1 by default: a share of it
    /// for each of several images taken under one geometry, so that seconds
    /// apart they count as one.
    pub weight: F,
}

impl View {
    /// Where `p`, in the body's frame, falls in the image: `(x, y, depth)`,
    /// in the pixels of [`View::centre`], `depth` along the boresight.
    /// `None` behind the camera.
    pub fn project(&self, p: V3) -> Option<(F, F, F)> {
        let c = self.axes * (p - self.position);
        if c.z <= 0.0 {
            return None;
        }
        Some((
            self.centre[0] + self.focal * c.x / c.z,
            self.centre[1] + self.focal * c.y / c.z,
            c.z,
        ))
    }

    pub fn check(&self) -> Result<(), String> {
        let name = &self.name;
        if self.width < 2 || self.height < 2 || self.image.len() != self.width * self.height {
            return Err(format!(
                "view {name}: the image has {} values for {} x {}",
                self.image.len(),
                self.width,
                self.height
            ));
        }
        if !self.valid.is_empty() && self.valid.len() != self.image.len() {
            return Err(format!(
                "view {name}: the valid mask has {} values for {} pixels",
                self.valid.len(),
                self.image.len()
            ));
        }
        if !(self.focal > 0.0 && self.focal.is_finite()) {
            return Err(format!("view {name}: the focal length is {}", self.focal));
        }
        let unit = |v: V3| (v.length() - 1.0).abs() < 1e-6;
        let a = self.axes.transpose();
        if !(unit(a.x_axis) && unit(a.y_axis) && unit(a.z_axis))
            || a.x_axis.dot(a.y_axis).abs() > 1e-6
            || a.x_axis.dot(a.z_axis).abs() > 1e-6
            || a.y_axis.dot(a.z_axis).abs() > 1e-6
        {
            return Err(format!("view {name}: the axes are not orthonormal"));
        }
        if !unit(self.sun) {
            return Err(format!("view {name}: the Sun's direction is not a unit vector"));
        }
        if !self.position.is_finite() {
            return Err(format!("view {name}: the camera's position is not finite"));
        }
        if !(self.weight >= 0.0 && self.weight.is_finite()) {
            return Err(format!("view {name}: the weight is {}", self.weight));
        }
        Ok(())
    }
}

/// The window of a view's image a maplet falls in, smoothed to the maplet's
/// spacing.
///
/// A grid point stands for a patch `spacing` on a side, and an image finer
/// than that resolves what the point cannot: a Gaussian of 0.35 spacing
/// (in pixels) takes it out, the box's width to a few percent. Weighted by
/// the valid mask -- the mean of the valid pixels under the kernel -- so
/// neither a shadow nor the sky bleeds into the pixels beside it; a pixel is
/// kept where nine tenths of the kernel's weight is valid.
pub(crate) struct Crop {
    x0: usize,
    y0: usize,
    w: usize,
    h: usize,
    data: Vec<f32>,
    ok: Vec<bool>,
}

impl Crop {
    /// The pixels `[x0, x1) x [y0, y1)` of `view`, clamped to the image,
    /// smoothed by `sigma` pixels.
    pub fn new(view: &View, x0: F, y0: F, x1: F, y1: F, sigma: F) -> Option<Crop> {
        let clamp = |v: F, max: usize| (v.floor().max(0.0) as usize).min(max);
        let (cx0, cy0) = (clamp(x0, view.width), clamp(y0, view.height));
        let (cx1, cy1) = (clamp(x1.ceil(), view.width), clamp(y1.ceil(), view.height));
        if cx1 < cx0 + 2 || cy1 < cy0 + 2 {
            return None;
        }
        let (w, h) = (cx1 - cx0, cy1 - cy0);
        let mut data = vec![0f32; w * h];
        let mut weight = vec![0f32; w * h];
        for r in 0..h {
            for c in 0..w {
                let at = (cy0 + r) * view.width + cx0 + c;
                let v = view.image[at];
                if v.is_finite() && (view.valid.is_empty() || view.valid[at] != 0) {
                    data[r * w + c] = v;
                    weight[r * w + c] = 1.0;
                }
            }
        }
        let ok;
        if sigma >= 0.3 {
            let kernel = gaussian(sigma);
            let num = blur(&data, w, h, &kernel);
            let den = blur(&weight, w, h, &kernel);
            data = num
                .iter()
                .zip(&den)
                .map(|(&n, &d)| if d > 1e-6 { n / d } else { 0.0 })
                .collect();
            ok = den.iter().map(|&d| d > 0.9).collect();
        } else {
            ok = weight.iter().map(|&d| d > 0.5).collect();
        }
        Some(Crop {
            x0: cx0,
            y0: cy0,
            w,
            h,
            data,
            ok,
        })
    }

    /// The smoothed image at `(x, y)` in the image's pixels, bilinear between
    /// the pixels' centres, and its gradient: `(value, d/dx, d/dy)`. `None`
    /// off the window or where a pixel it needs is not valid.
    #[inline]
    pub fn sample(&self, x: F, y: F) -> Option<(F, F, F)> {
        let fx = x - 0.5 - self.x0 as F;
        let fy = y - 0.5 - self.y0 as F;
        if !(fx >= 0.0 && fy >= 0.0) {
            return None;
        }
        let (i, j) = (fx as usize, fy as usize);
        if i + 1 >= self.w || j + 1 >= self.h {
            return None;
        }
        let k = j * self.w + i;
        if !(self.ok[k] && self.ok[k + 1] && self.ok[k + self.w] && self.ok[k + self.w + 1]) {
            return None;
        }
        let (tx, ty) = (fx - i as F, fy - j as F);
        let a = self.data[k] as F;
        let b = self.data[k + 1] as F;
        let c = self.data[k + self.w] as F;
        let d = self.data[k + self.w + 1] as F;
        let value = (1.0 - ty) * ((1.0 - tx) * a + tx * b) + ty * ((1.0 - tx) * c + tx * d);
        let dx = (1.0 - ty) * (b - a) + ty * (d - c);
        let dy = (1.0 - tx) * (c - a) + tx * (d - b);
        Some((value, dx, dy))
    }
}

fn gaussian(sigma: F) -> Vec<f32> {
    let radius = (3.0 * sigma).ceil() as isize;
    let k: Vec<F> = (-radius..=radius)
        .map(|x| (-0.5 * (x as F / sigma).powi(2)).exp())
        .collect();
    let sum: F = k.iter().sum();
    k.iter().map(|v| (v / sum) as f32).collect()
}

/// Separable convolution, zero beyond the edges -- which the weights,
/// convolved alike, account for.
fn blur(data: &[f32], w: usize, h: usize, kernel: &[f32]) -> Vec<f32> {
    let r = (kernel.len() / 2) as isize;
    let mut rows = vec![0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut s = 0f32;
            for (t, &k) in kernel.iter().enumerate() {
                let xx = x as isize + t as isize - r;
                if xx >= 0 && (xx as usize) < w {
                    s += k * data[y * w + xx as usize];
                }
            }
            rows[y * w + x] = s;
        }
    }
    let mut out = vec![0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut s = 0f32;
            for (t, &k) in kernel.iter().enumerate() {
                let yy = y as isize + t as isize - r;
                if yy >= 0 && (yy as usize) < h {
                    s += k * rows[yy as usize * w + x];
                }
            }
            out[y * w + x] = s;
        }
    }
    out
}

/// A law's I/F at unit albedo, `pi mu0 r(mu0, mu, alpha)`, and its two
/// derivatives in the cosines, tabulated over `mu0` and `mu` at a few
/// phase angles: the range one view's phase spans over a maplet, a few
/// tenths of a degree from afar. Trilinear between the nodes, which holds
/// Hapke's law to 1e-5 of itself -- and costs a fraction of evaluating it,
/// which the photoclinometry would do a hundred million times a pass.
pub(crate) struct Table {
    alpha0: F,
    dalpha: F,
    slices: usize,
    f: Vec<f32>,
    d0: Vec<f32>,
    d1: Vec<f32>,
}

const NODES: usize = 129;

impl Table {
    pub fn new(law: &crate::lightcurve::Law, alpha_min: F, alpha_max: F) -> Table {
        let pi = std::f64::consts::PI;
        let step = 0.25 * pi / 180.0;
        let alpha0 = (alpha_min - step).max(0.0);
        let alpha1 = (alpha_max + step).min(pi);
        let slices = (((alpha1 - alpha0) / step).ceil() as usize).max(1) + 1;
        let dalpha = if slices > 1 { (alpha1 - alpha0) / (slices - 1) as F } else { 1.0 };
        let value = |mu0: F, mu: F, alpha: F| -> F {
            if mu0 <= 0.0 || mu <= 0.0 {
                0.0
            } else {
                pi * mu0 * super::reflectance(law, mu0, mu, alpha)
            }
        };
        let n = NODES;
        let h = 1.0 / (n - 1) as F;
        // Kalast's laws are evaluated at its own precision, f32 by default:
        // a step the f32 rounding is small against.
        let eps = 1e-3;
        let mut f = vec![0f32; slices * n * n];
        let mut d0 = vec![0f32; slices * n * n];
        let mut d1 = vec![0f32; slices * n * n];
        for s in 0..slices {
            let alpha = alpha0 + s as F * dalpha;
            for a in 0..n {
                for b in 0..n {
                    let (mu0, mu) = (a as F * h, b as F * h);
                    let k = (s * n + a) * n + b;
                    f[k] = value(mu0, mu, alpha) as f32;
                    // Central differences inside, one-sided on the edges.
                    let (lo, hi) = ((mu0 - eps).max(1e-6), (mu0 + eps).min(1.0));
                    d0[k] = ((value(hi, mu, alpha) - value(lo, mu, alpha)) / (hi - lo)) as f32;
                    let (lo, hi) = ((mu - eps).max(1e-6), (mu + eps).min(1.0));
                    d1[k] = ((value(mu0, hi, alpha) - value(mu0, lo, alpha)) / (hi - lo)) as f32;
                }
            }
        }
        Table {
            alpha0,
            dalpha,
            slices,
            f,
            d0,
            d1,
        }
    }

    /// `(F, dF/dmu0, dF/dmu)` at the cosines and phase angle.
    #[inline]
    pub fn eval(&self, mu0: F, mu: F, alpha: F) -> (F, F, F) {
        let n = NODES;
        let last = (n - 1) as F;
        let x = (mu0.clamp(0.0, 1.0) * last).min(last - 1e-9);
        let y = (mu.clamp(0.0, 1.0) * last).min(last - 1e-9);
        let (a, b) = (x as usize, y as usize);
        let (tx, ty) = (x - a as F, y - b as F);
        let z = if self.slices > 1 {
            ((alpha - self.alpha0) / self.dalpha).clamp(0.0, (self.slices - 1) as F - 1e-9)
        } else {
            0.0
        };
        let s = z as usize;
        let tz = z - s as F;
        let mut out = [0.0; 3];
        for (o, table) in out.iter_mut().zip([&self.f, &self.d0, &self.d1]) {
            let at = |s: usize, a: usize, b: usize| table[(s * n + a) * n + b] as F;
            let plane = |s: usize| {
                (1.0 - tx) * ((1.0 - ty) * at(s, a, b) + ty * at(s, a, b + 1))
                    + tx * ((1.0 - ty) * at(s, a + 1, b) + ty * at(s, a + 1, b + 1))
            };
            *o = if self.slices > 1 {
                (1.0 - tz) * plane(s) + tz * plane(s + 1)
            } else {
                plane(0)
            };
        }
        (out[0], out[1], out[2])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lightcurve::Law;
    use crate::scattering::Hapke;

    #[test]
    fn the_table_holds_the_law() {
        let law = Law::Hapke(Hapke {
            w: 0.0385,
            b: 0.276,
            c: 1.0,
            b0: 4.73,
            h: 0.061,
            theta_bar: (24.4 * crate::shape::RPD) as crate::Float,
            k: 1.2,
        });
        let rad = crate::shape::RPD;
        let r = |mu0: F, mu: F, alpha: F| crate::shape::reflectance(&law, mu0, mu, alpha);
        let table = Table::new(&law, 40.0 * rad, 41.0 * rad);
        let mut worst: F = 0.0;
        for &(mu0, mu, alpha) in &[(0.8, 0.7, 40.3), (0.3, 0.9, 40.9), (0.55, 0.45, 40.0), (0.95, 0.2, 40.6)] {
            let alpha = alpha * rad;
            let exact = std::f64::consts::PI * mu0 * r(mu0, mu, alpha);
            let (f, d0, d1) = table.eval(mu0, mu, alpha);
            worst = worst.max(((f - exact) / exact).abs());
            let e = 1e-3;
            let dd0 = std::f64::consts::PI
                * ((mu0 + e) * r(mu0 + e, mu, alpha) - (mu0 - e) * r(mu0 - e, mu, alpha))
                / (2.0 * e);
            let dd1 = std::f64::consts::PI * mu0 * (r(mu0, mu + e, alpha) - r(mu0, mu - e, alpha)) / (2.0 * e);
            assert!((d0 - dd0).abs() < 2e-3 * dd0.abs().max(exact), "dF/dmu0 {d0} against {dd0}");
            assert!((d1 - dd1).abs() < 2e-3 * dd1.abs().max(exact), "dF/dmu {d1} against {dd1}");
        }
        assert!(worst < 1e-3, "the table is {worst} off the law");
    }

    #[test]
    fn a_point_projects_where_its_pixel_is() {
        let view = View {
            name: "t".into(),
            image: vec![0.0; 100 * 80],
            width: 100,
            height: 80,
            valid: vec![],
            position: V3::new(0.0, 0.0, 10.0),
            // Looking down -z, rows along +x, columns down +y... mirrored.
            axes: M3::from_cols(V3::X, V3::Y, -V3::Z).transpose(),
            focal: 100.0,
            centre: [50.0, 40.0],
            sun: V3::Z,
            weight: 1.0,
        };
        view.check().unwrap();
        let (x, y, d) = view.project(V3::new(1.0, -0.5, 0.0)).unwrap();
        assert!((x - 60.0).abs() < 1e-9 && (y - 35.0).abs() < 1e-9 && (d - 10.0).abs() < 1e-9);
        assert!(view.project(V3::new(0.0, 0.0, 20.0)).is_none());
    }
}
