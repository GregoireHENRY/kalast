//! Aperture photometry, as a night's reduction measures its frames: a
//! source's light inside a circle, the sky from a ring about it taken off,
//! then its magnitude against comparison stars of known magnitude in the
//! same frame -- which takes out the atmosphere's extinction and the
//! instrument's zero point together.

/// The circle the light is summed in and the ring the sky is read from,
/// radii in pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "python", pyo3::pyclass(get_all, set_all, from_py_object, module = "kalast.telescope"))]
pub struct Aperture {
    pub radius: f64,
    pub inner: f64,
    pub outer: f64,
}

/// One source measured in one frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measurement {
    /// Where its light is centred, zero-based pixels.
    pub x: f64,
    pub y: f64,
    /// Counts in the aperture above the sky, ADU.
    pub flux: f64,
    /// Its standard error, ADU: photons, the sky and the read noise.
    pub error: f64,
    /// The sky, ADU a pixel, and the scatter of the ring's pixels about it.
    pub sky: f64,
    pub sky_sigma: f64,
    /// Pixels in the aperture, fractions at its edge counted as such.
    pub area: f64,
    /// The brightest pixel in the aperture, sky included: against
    /// saturation.
    pub peak: f64,
}

impl Measurement {
    /// Its instrumental magnitude, counts a second, and that magnitude's
    /// error; `None` for a flux that is not positive.
    pub fn magnitude(&self, gain: f64, exposure: f64) -> Option<(f64, f64)> {
        (self.flux > 0.0).then(|| {
            let m = -2.5 * (self.flux * gain / exposure).log10();
            (m, 2.5 / std::f64::consts::LN_10 * self.error / self.flux)
        })
    }
}

/// The area of the disc of `r` about the origin inside the box
/// [`x0`, `x1`] x [`y0`, `y1`], exactly: the disc's height over the box,
/// integrated in closed form between the abscissae where the circle crosses
/// the box's edges.
fn disc_in_box(r: f64, x0: f64, x1: f64, y0: f64, y1: f64) -> f64 {
    let r2 = r * r;
    // The integral of the circle's upper half, sqrt(r^2 - x^2).
    let half = |x: f64| {
        let x = x.clamp(-r, r);
        0.5 * (x * (r2 - x * x).max(0.0).sqrt() + r2 * (x / r).asin())
    };
    let (lo, hi) = (x0.max(-r), x1.min(r));
    if lo >= hi {
        return 0.0;
    }
    let mut cuts = vec![lo, hi];
    for y in [y0, y1] {
        if y.abs() < r {
            let s = (r2 - y * y).sqrt();
            cuts.extend([s, -s]);
        }
    }
    cuts.retain(|&c| (lo..=hi).contains(&c));
    cuts.sort_by(f64::total_cmp);
    cuts.dedup();
    let mut area = 0.0;
    for w in cuts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let m = 0.5 * (a + b);
        let h = (r2 - m * m).max(0.0).sqrt();
        // Between two cuts the disc's top is either the box's or the
        // circle's throughout, and so is its bottom.
        let (top_circle, bottom_circle) = (h < y1, -h > y0);
        let (top, bottom) = (if top_circle { h } else { y1 }, if bottom_circle { -h } else { y0 });
        if top <= bottom {
            continue;
        }
        let arc = half(b) - half(a);
        area += if top_circle { arc } else { y1 * (b - a) } - if bottom_circle { -arc } else { y0 * (b - a) };
    }
    area
}

/// How much of the pixel centred at (`i`, `j`) lies within `radius` of
/// (`x`, `y`), exactly.
fn overlap(i: usize, j: usize, x: f64, y: f64, radius: f64) -> f64 {
    let (dx, dy) = (i as f64 - x, j as f64 - y);
    let d2 = dx * dx + dy * dy;
    if d2.sqrt() <= radius - std::f64::consts::FRAC_1_SQRT_2 {
        return 1.0;
    }
    if d2.sqrt() >= radius + std::f64::consts::FRAC_1_SQRT_2 {
        return 0.0;
    }
    disc_in_box(radius, dx - 0.5, dx + 0.5, dy - 0.5, dy + 0.5)
}

/// The sky in a ring: the mean of its pixels, those more than three
/// standard deviations from it set aside until none are -- stars in the
/// ring go, and a mean of whole counts has no rounding bias, which a
/// median of them would.
fn sky(data: &[f32], width: usize, height: usize, x: f64, y: f64, inner: f64, outer: f64) -> Option<(f64, f64, usize)> {
    let mut values = Vec::new();
    let (lo_x, hi_x) = ((x - outer).floor().max(0.0) as usize, ((x + outer).ceil() as usize).min(width.saturating_sub(1)));
    let (lo_y, hi_y) = ((y - outer).floor().max(0.0) as usize, ((y + outer).ceil() as usize).min(height.saturating_sub(1)));
    for j in lo_y..=hi_y {
        for i in lo_x..=hi_x {
            let d2 = (i as f64 - x).powi(2) + (j as f64 - y).powi(2);
            if d2 >= inner * inner && d2 <= outer * outer {
                let v = data[j * width + i] as f64;
                if v.is_finite() {
                    values.push(v);
                }
            }
        }
    }
    let mut kept = values;
    for _ in 0..10 {
        if kept.len() < 5 {
            return None;
        }
        let n = kept.len() as f64;
        let mean = kept.iter().sum::<f64>() / n;
        let sigma = (kept.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1.0)).sqrt();
        let before = kept.len();
        kept.retain(|v| (v - mean).abs() <= 3.0 * sigma);
        if kept.len() == before {
            return Some((mean, sigma, kept.len()));
        }
    }
    let n = kept.len() as f64;
    let mean = kept.iter().sum::<f64>() / n;
    let sigma = (kept.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0)).sqrt();
    Some((mean, sigma, kept.len()))
}

/// The centre of the light above `sky` within `radius` of (`x`, `y`),
/// moved there up to five times; the start if it has no light.
fn centroid(data: &[f32], width: usize, height: usize, mut x: f64, mut y: f64, radius: f64, sky: f64) -> (f64, f64) {
    for _ in 0..5 {
        let (mut s, mut sx, mut sy) = (0.0, 0.0, 0.0);
        let (lo_x, hi_x) = ((x - radius).floor().max(0.0) as usize, ((x + radius).ceil() as usize).min(width - 1));
        let (lo_y, hi_y) = ((y - radius).floor().max(0.0) as usize, ((y + radius).ceil() as usize).min(height - 1));
        for j in lo_y..=hi_y {
            for i in lo_x..=hi_x {
                if (i as f64 - x).powi(2) + (j as f64 - y).powi(2) > radius * radius {
                    continue;
                }
                let v = data[j * width + i] as f64 - sky;
                if v > 0.0 {
                    s += v;
                    sx += v * i as f64;
                    sy += v * j as f64;
                }
            }
        }
        if s <= 0.0 {
            break;
        }
        let (nx, ny) = (sx / s, sy / s);
        let moved = ((nx - x).powi(2) + (ny - y).powi(2)).sqrt();
        x = nx;
        y = ny;
        if moved < 1e-3 {
            break;
        }
    }
    (x, y)
}

/// Measure the source near (`x`, `y`) in a frame of counts: its centre found
/// first when `recentre`, then its light in `aperture` with the sky taken
/// off. `gain` in electrons an ADU. `None` where the ring has too few
/// pixels or the aperture leaves the frame.
pub fn measure(
    data: &[f32],
    width: usize,
    height: usize,
    x: f64,
    y: f64,
    aperture: &Aperture,
    gain: f64,
    recentre: bool,
) -> Option<Measurement> {
    let (sky0, _, _) = sky(data, width, height, x, y, aperture.inner, aperture.outer)?;
    let (x, y) = if recentre { centroid(data, width, height, x, y, aperture.radius, sky0) } else { (x, y) };
    let r = aperture.radius;
    if x - r < 0.0 || y - r < 0.0 || x + r > width as f64 - 1.0 || y + r > height as f64 - 1.0 {
        return None;
    }
    let (sky, sky_sigma, n_sky) = sky(data, width, height, x, y, aperture.inner, aperture.outer)?;
    let (mut sum, mut area, mut peak) = (0.0, 0.0, f64::MIN);
    for j in (y - r).floor() as usize..=(y + r).ceil() as usize {
        for i in (x - r).floor() as usize..=(x + r).ceil() as usize {
            let w = overlap(i, j, x, y, r);
            if w > 0.0 {
                let v = data[j * width + i] as f64;
                sum += w * v;
                area += w;
                peak = peak.max(v);
            }
        }
    }
    let flux = sum - area * sky;
    // The CCD equation in electrons (Merline & Howell 1995): the source's
    // photons, the sky's scatter over the aperture, and the sky's own error.
    let sky_e = sky_sigma * gain;
    let variance_e = flux.max(0.0) * gain + area * sky_e * sky_e + area * area * sky_e * sky_e / n_sky as f64;
    Some(Measurement { x, y, flux, error: variance_e.sqrt() / gain, sky, sky_sigma, area, peak })
}

/// A frame's zero point: what its comparison stars' catalogue magnitudes
/// say its instrumental ones lack.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZeroPoint {
    /// Catalogue less instrumental magnitude, the stars' mean weighted by
    /// their errors.
    pub value: f64,
    /// Its error from the stars' photometry alone: how much it moves from
    /// frame to frame, the comparison stars being the same.
    pub error: f64,
    /// The stars' scatter about it, the catalogue's errors in it: how far
    /// the frame's magnitudes may be off the catalogue's system, the same
    /// way in every frame.
    pub scatter: f64,
    pub stars: usize,
}

fn median(values: &mut [f64]) -> f64 {
    values.sort_by(|a, b| a.total_cmp(b));
    let n = values.len();
    if n % 2 == 1 { values[n / 2] } else { 0.5 * (values[n / 2 - 1] + values[n / 2]) }
}

/// A frame's zero point from its comparison stars, those set aside that sit
/// more than three robust standard deviations (the median absolute
/// deviation's) from the stars' median, or three times their typical error
/// where the stars agree better than that: a variable or a blend goes.
/// `None` without two stars left.
pub fn zero_point(catalogue: &[f64], instrumental: &[(f64, f64)]) -> Option<ZeroPoint> {
    let mut stars: Vec<(f64, f64)> = catalogue
        .iter()
        .zip(instrumental)
        .filter(|(c, (m, e))| c.is_finite() && m.is_finite() && *e > 0.0)
        .map(|(c, (m, e))| (c - m, *e))
        .collect();
    loop {
        if stars.len() < 2 {
            return None;
        }
        let mid = median(&mut stars.iter().map(|s| s.0).collect::<Vec<_>>());
        let mad = median(&mut stars.iter().map(|s| (s.0 - mid).abs()).collect::<Vec<_>>());
        let typical = median(&mut stars.iter().map(|s| s.1).collect::<Vec<_>>());
        let limit = (3.0 * 1.4826 * mad).max(3.0 * typical);
        let before = stars.len();
        stars.retain(|(z, _)| (z - mid).abs() <= limit);
        if stars.len() == before {
            break;
        }
    }
    let (mut sw, mut swz) = (0.0, 0.0);
    for (z, e) in &stars {
        let w = 1.0 / (e * e);
        sw += w;
        swz += w * z;
    }
    let value = swz / sw;
    let n = stars.len() as f64;
    let scatter = (stars.iter().map(|(z, _)| (z - value).powi(2)).sum::<f64>() / (n - 1.0)).sqrt();
    Some(ZeroPoint { value, error: (1.0 / sw).sqrt(), scatter, stars: stars.len() })
}

#[cfg(test)]
mod tests {
    use super::super::noise::Rng;
    use super::super::psf::{self, Moffat};
    use super::*;

    /// A frame of `sky` counts with a star of `total` at (`x`, `y`), noised.
    fn frame(w: usize, h: usize, sky: f64, stars: &[(f64, f64, f64)], seed: u64) -> Vec<f32> {
        let mut image = vec![sky; w * h];
        let p = Moffat::from_fwhm(3.3, 3.0);
        for &(x, y, total) in stars {
            psf::add(&mut image, w, h, x, y, total, Some(&p));
        }
        let mut rng = Rng::new(seed);
        image.iter().map(|&e| rng.poisson(e) as f32).collect()
    }

    #[test]
    fn the_aperture_is_the_circles_area() {
        let data = vec![1.0f32; 64 * 64];
        let ap = Aperture { radius: 7.3, inner: 12.0, outer: 20.0 };
        for (x, y) in [(31.7, 30.2), (32.0, 32.0), (31.5, 30.5), (30.13, 33.91)] {
            let m = measure(&data, 64, 64, x, y, &ap, 1.0, false).unwrap();
            assert!((m.area - std::f64::consts::PI * 7.3 * 7.3).abs() < 1e-9, "{}", m.area);
            assert!(m.flux.abs() < 1e-9 && m.sky == 1.0);
        }
    }

    #[test]
    fn a_disc_in_a_box_is_its_area() {
        let pi = std::f64::consts::PI;
        // Whole, halved, quartered, and a corner of the box only.
        assert!((disc_in_box(2.0, -5.0, 5.0, -5.0, 5.0) - 4.0 * pi).abs() < 1e-12);
        assert!((disc_in_box(2.0, 0.0, 5.0, -5.0, 5.0) - 2.0 * pi).abs() < 1e-12);
        assert!((disc_in_box(2.0, 0.0, 5.0, 0.0, 5.0) - pi).abs() < 1e-12);
        assert!((disc_in_box(2.0, 0.0, 1.0, 0.0, 1.0) - 1.0).abs() < 1e-12);
        assert_eq!(disc_in_box(1.0, 1.0, 2.0, 1.0, 2.0), 0.0);
        // Against a fine grid, off-centre.
        let (n, (x0, x1, y0, y1)) = (2000, (0.3, 1.3, -0.4, 0.6));
        let mut inside = 0usize;
        for j in 0..n {
            for i in 0..n {
                let x = x0 + (i as f64 + 0.5) / n as f64 * (x1 - x0);
                let y = y0 + (j as f64 + 0.5) / n as f64 * (y1 - y0);
                inside += (x * x + y * y <= 1.0) as usize;
            }
        }
        let grid = inside as f64 / (n * n) as f64;
        assert!((disc_in_box(1.0, x0, x1, y0, y1) - grid).abs() < 1e-5);
    }

    #[test]
    fn a_star_is_found_and_measured_within_its_error() {
        let (w, h) = (101, 101);
        let ap = Aperture { radius: 10.0, inner: 15.0, outer: 25.0 };
        let total = 2.0e5;
        let p = Moffat::from_fwhm(3.3, 3.0);
        let mut pulls = Vec::new();
        for seed in 0..40 {
            let data = frame(w, h, 400.0, &[(50.4, 49.7, total)], seed);
            let m = measure(&data, w, h, 52.0, 48.0, &ap, 1.0, true).unwrap();
            assert!((m.x - 50.4).abs() < 0.05 && (m.y - 49.7).abs() < 0.05, "{} {}", m.x, m.y);
            // What the aperture holds of the star.
            let expected = total * (1.0 - p.beyond(10.0));
            pulls.push((m.flux - expected) / m.error);
        }
        let n = pulls.len() as f64;
        let mean = pulls.iter().sum::<f64>() / n;
        let sd = (pulls.iter().map(|p| (p - mean).powi(2)).sum::<f64>() / (n - 1.0)).sqrt();
        // Unbiased, and its error what the scatter is.
        assert!(mean.abs() < 0.5 && (0.6..1.5).contains(&sd), "pulls: mean {mean}, sd {sd}");
    }

    #[test]
    fn a_star_in_the_ring_moves_the_sky_little() {
        // Five times the target, its core in the ring: the clipping sets
        // the core aside; its faint wings, under the noise, stay, as they
        // do in any reduction that does not mask its neighbours.
        let (w, h) = (101, 101);
        let ap = Aperture { radius: 8.0, inner: 15.0, outer: 25.0 };
        let data = frame(w, h, 400.0, &[(50.0, 50.0, 1.0e5), (70.0, 50.0, 5.0e5)], 9);
        let m = measure(&data, w, h, 50.0, 50.0, &ap, 1.0, true).unwrap();
        assert!((m.sky - 400.0).abs() < 4.0, "{}", m.sky);
        let ring_mean = {
            let (mut s, mut n) = (0.0, 0.0);
            for j in 0..h {
                for i in 0..w {
                    let d2 = (i as f64 - m.x).powi(2) + (j as f64 - m.y).powi(2);
                    if (225.0..=625.0).contains(&d2) {
                        s += data[j * w + i] as f64;
                        n += 1.0;
                    }
                }
            }
            s / n
        };
        assert!(ring_mean - 400.0 > 10.0 * (m.sky - 400.0).abs().max(1.0), "unclipped {ring_mean}");
    }

    #[test]
    fn the_zero_point_sets_aside_a_variable_star() {
        let cat = [12.0, 13.0, 14.0, 12.5, 13.5, 11.8];
        let mut inst: Vec<(f64, f64)> = cat.iter().map(|c| (c - 23.0, 0.002)).collect();
        inst[3].0 += 0.3; // a variable, or a blend
        let zp = zero_point(&cat, &inst).unwrap();
        assert_eq!(zp.stars, 5);
        assert!((zp.value - 23.0).abs() < 1e-9 && zp.error < 0.002 && zp.scatter < 1e-9, "{zp:?}");
        // The catalogue's scatter, a few hundredths, is kept, not clipped.
        let spread = [0.03, -0.02, 0.01, -0.04, 0.02, 0.0];
        let inst: Vec<(f64, f64)> = cat.iter().zip(spread).map(|(c, s)| (c - 23.0 + s, 0.002)).collect();
        let zp = zero_point(&cat, &inst).unwrap();
        assert_eq!(zp.stars, 6);
        assert!((zp.value - 23.0).abs() < 0.01 && (0.02..0.04).contains(&zp.scatter), "{zp:?}");
    }
}
