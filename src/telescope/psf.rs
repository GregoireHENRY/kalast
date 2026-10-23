//! A point source's light on the detector: Moffat's profile, the shape
//! seeing gives a star (Moffat 1969; Trujillo et al. 2001), integrated over
//! each pixel.
//!
//! ```text
//! P(r) = (beta - 1) / (pi alpha^2) * (1 + r^2 / alpha^2)^-beta,
//! FWHM = 2 alpha sqrt(2^(1/beta) - 1)
//! ```
//!
//! normalised to one over the plane. `beta` near 3 fits most seeing-limited
//! images; a large `beta` tends to a Gaussian.

/// Moffat's profile, in pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Moffat {
    pub alpha: f64,
    pub beta: f64,
}

/// Each pixel is integrated on a grid this fine.
const SUB: usize = 5;

impl Moffat {
    /// From the full width at half maximum, in pixels.
    pub fn from_fwhm(fwhm: f64, beta: f64) -> Self {
        Self { alpha: fwhm / (2.0 * (2f64.powf(1.0 / beta) - 1.0).sqrt()), beta }
    }

    /// The light a unit pixel area receives at `r2`, the squared distance in
    /// pixels.
    pub fn density(&self, r2: f64) -> f64 {
        let a2 = self.alpha * self.alpha;
        (self.beta - 1.0) / (std::f64::consts::PI * a2) * (1.0 + r2 / a2).powf(-self.beta)
    }

    /// The share of the light beyond radius `r`.
    pub fn beyond(&self, r: f64) -> f64 {
        (1.0 + r * r / (self.alpha * self.alpha)).powf(1.0 - self.beta)
    }

    /// The radius past which a pixel of a source of `total` gets less than
    /// `floor`: where its stamp can stop.
    pub fn reach(&self, total: f64, floor: f64) -> f64 {
        let peak = total * self.density(0.0);
        if peak <= floor {
            return 1.0;
        }
        self.alpha * ((peak / floor).powf(1.0 / self.beta) - 1.0).max(0.0).sqrt()
    }
}

/// Add `total` to `image` (`width` x `height`, row-major) spread by `psf`
/// about (`x`, `y`), zero-based pixel centres; light falling off the frame
/// is lost, as a real frame's is. `None` puts it all in the pixel the point
/// falls in -- no seeing, no optics.
pub fn add(image: &mut [f64], width: usize, height: usize, x: f64, y: f64, total: f64, psf: Option<&Moffat>) {
    let Some(psf) = psf else {
        let (i, j) = (x.round(), y.round());
        if i >= 0.0 && j >= 0.0 && (i as usize) < width && (j as usize) < height {
            image[j as usize * width + i as usize] += total;
        }
        return;
    };
    let reach = psf.reach(total, 0.01).clamp(3.0, 400.0);
    let (x_lo, x_hi) = ((x - reach).floor().max(0.0), (x + reach).ceil().min(width as f64 - 1.0));
    let (y_lo, y_hi) = ((y - reach).floor().max(0.0), (y + reach).ceil().min(height as f64 - 1.0));
    if x_lo > x_hi || y_lo > y_hi {
        return;
    }
    let step = 1.0 / SUB as f64;
    let weight = total / (SUB * SUB) as f64;
    for j in y_lo as usize..=y_hi as usize {
        for i in x_lo as usize..=x_hi as usize {
            let mut sum = 0.0;
            for sj in 0..SUB {
                let dy = j as f64 - 0.5 + (sj as f64 + 0.5) * step - y;
                for si in 0..SUB {
                    let dx = i as f64 - 0.5 + (si as f64 + 0.5) * step - x;
                    sum += psf.density(dx * dx + dy * dy);
                }
            }
            image[j * width + i] += sum * weight;
        }
    }
}

/// [`add`] along a trail, from (`x0`, `y0`) to (`x1`, `y1`): a source that
/// moves against the tracking during the exposure. Points a fifth of a
/// pixel apart or closer.
#[allow(clippy::too_many_arguments)]
pub fn add_trail(
    image: &mut [f64],
    width: usize,
    height: usize,
    (x0, y0): (f64, f64),
    (x1, y1): (f64, f64),
    total: f64,
    psf: Option<&Moffat>,
) {
    let length = ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt();
    let n = ((length / 0.2).ceil() as usize).max(1);
    for k in 0..n {
        let t = (k as f64 + 0.5) / n as f64;
        add(image, width, height, x0 + t * (x1 - x0), y0 + t * (y1 - y0), total / n as f64, psf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_profile_has_its_fwhm_and_its_light() {
        let p = Moffat::from_fwhm(3.3, 3.0);
        let half = p.density(0.0) / 2.0;
        assert!((p.density((3.3f64 / 2.0).powi(2)) - half).abs() < 1e-12 * half.max(1.0));
        // Numerically over the plane, out to where little is left.
        let (mut sum, h) = (0.0, 0.002);
        let r_max = 400.0;
        let mut r = h / 2.0;
        while r < r_max {
            sum += p.density(r * r) * 2.0 * std::f64::consts::PI * r * h;
            r += h;
        }
        assert!((sum + p.beyond(r_max) - 1.0).abs() < 1e-6, "{sum}");
    }

    #[test]
    fn a_stamp_keeps_the_light_but_what_falls_off() {
        let (w, h) = (101, 101);
        let p = Moffat::from_fwhm(3.3, 3.0);
        let mut image = vec![0.0; w * h];
        add(&mut image, w, h, 50.3, 49.8, 1.0e6, Some(&p));
        let total: f64 = image.iter().sum();
        // What a 50-pixel half-width leaves out: the wings beyond it.
        let lost = 1.0e6 * p.beyond(50.0);
        assert!((total - 1.0e6).abs() < 2.0 * lost + 1.0, "{total}, about {lost} beyond the frame");
        // The centroid is where the source is.
        let (mut sx, mut sy) = (0.0, 0.0);
        for j in 0..h {
            for i in 0..w {
                sx += image[j * w + i] * i as f64;
                sy += image[j * w + i] * j as f64;
            }
        }
        assert!((sx / total - 50.3).abs() < 2e-3 && (sy / total - 49.8).abs() < 2e-3);
    }

    #[test]
    fn without_seeing_the_light_lands_in_one_pixel() {
        let mut image = vec![0.0; 9];
        add(&mut image, 3, 3, 1.2, 0.9, 5.0, None);
        assert_eq!(image[4], 5.0);
        add(&mut image, 3, 3, -3.0, 1.0, 5.0, None);
        assert_eq!(image.iter().sum::<f64>(), 5.0, "off the frame, lost");
    }

    #[test]
    fn a_trail_spreads_the_light_along_its_length() {
        let (w, h) = (121, 61);
        let p = Moffat::from_fwhm(3.3, 3.0);
        let mut image = vec![0.0; w * h];
        add_trail(&mut image, w, h, (40.0, 30.0), (80.0, 30.0), 1.0e6, Some(&p));
        let total: f64 = image.iter().sum();
        let sx: f64 = (0..w * h).map(|k| image[k] * (k % w) as f64).sum();
        assert!((sx / total - 60.0).abs() < 1e-2);
        // Flat along the middle of the trail.
        let row = |i: usize| image[30 * w + i];
        assert!((row(55) - row(65)).abs() < 1e-3 * row(60));
    }
}
