//! Where a star falls on the detector: the gnomonic projection, FITS's
//! `RA---TAN`/`DEC--TAN`.
//!
//! North up, east to the left, as a sky image is shown: `CD1_1 = -scale`,
//! `CD2_2 = +scale`, the reference pixel at the frame's centre. Pixel
//! coordinates here are zero-based, the first pixel's centre at (0, 0); FITS's
//! `CRPIX` is one-based, and [`Wcs::header`] writes it so.

use super::fits::Header;

/// A frame's sky coordinates: its centre on the sky and its pixel scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Wcs {
    /// The sky at the reference pixel, degrees.
    pub ra: f64,
    pub dec: f64,
    /// The reference pixel, zero-based.
    pub x0: f64,
    pub y0: f64,
    /// Arcseconds a pixel.
    pub scale: f64,
}

impl Wcs {
    /// Centred on (`ra`, `dec`) over a `width` x `height` frame.
    pub fn centred(ra: f64, dec: f64, width: usize, height: usize, scale: f64) -> Self {
        Self { ra, dec, x0: (width as f64 - 1.0) / 2.0, y0: (height as f64 - 1.0) / 2.0, scale }
    }

    /// The pixel a sky position falls on, or `None` behind the tangent plane.
    pub fn pixel(&self, ra: f64, dec: f64) -> Option<(f64, f64)> {
        let (a, d) = (ra.to_radians(), dec.to_radians());
        let (a0, d0) = (self.ra.to_radians(), self.dec.to_radians());
        let cos_c = d0.sin() * d.sin() + d0.cos() * d.cos() * (a - a0).cos();
        if cos_c <= 0.0 {
            return None;
        }
        let xi = d.cos() * (a - a0).sin() / cos_c;
        let eta = (d0.cos() * d.sin() - d0.sin() * d.cos() * (a - a0).cos()) / cos_c;
        let per_pixel = (self.scale / 3600.0).to_radians();
        Some((self.x0 - xi / per_pixel, self.y0 + eta / per_pixel))
    }

    /// The sky position of a pixel, degrees.
    pub fn sky(&self, x: f64, y: f64) -> (f64, f64) {
        let per_pixel = (self.scale / 3600.0).to_radians();
        let xi = -(x - self.x0) * per_pixel;
        let eta = (y - self.y0) * per_pixel;
        let d0 = self.dec.to_radians();
        let rho = (xi * xi + eta * eta).sqrt();
        if rho == 0.0 {
            return (self.ra, self.dec);
        }
        let c = rho.atan();
        let dec = (c.cos() * d0.sin() + eta * c.sin() * d0.cos() / rho).asin();
        let ra = self.ra.to_radians() + (xi * c.sin()).atan2(rho * d0.cos() * c.cos() - eta * d0.sin() * c.sin());
        (ra.to_degrees().rem_euclid(360.0), dec.to_degrees())
    }

    /// The FITS cards that say this.
    pub fn header(&self, header: &mut Header) {
        let deg = self.scale / 3600.0;
        header.set("CTYPE1", "RA---TAN", "gnomonic projection");
        header.set("CTYPE2", "DEC--TAN", "");
        header.set("CRVAL1", self.ra, "[deg] right ascension at CRPIX");
        header.set("CRVAL2", self.dec, "[deg] declination at CRPIX");
        header.set("CRPIX1", self.x0 + 1.0, "");
        header.set("CRPIX2", self.y0 + 1.0, "");
        header.set("CD1_1", -deg, "[deg/pixel] east to the left");
        header.set("CD1_2", 0.0, "");
        header.set("CD2_1", 0.0, "");
        header.set("CD2_2", deg, "[deg/pixel] north up");
        header.set("RADESYS", "ICRS", "");
        header.set("EQUINOX", 2000.0, "");
    }

    /// Read back from a header [`Wcs::header`] wrote, or any `TAN` header
    /// without rotation or skew.
    pub fn from_header(header: &Header) -> Option<Self> {
        let scale = header.f64("CD2_2").or_else(|| header.f64("CDELT2"))? * 3600.0;
        Some(Self {
            ra: header.f64("CRVAL1")?,
            dec: header.f64("CRVAL2")?,
            x0: header.f64("CRPIX1")? - 1.0,
            y0: header.f64("CRPIX2")? - 1.0,
            scale,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn east_is_left_north_is_up() {
        let w = Wcs::centred(124.6, 15.2, 2048, 2048, 0.6);
        let (x, y) = w.pixel(124.6, 15.2).unwrap();
        assert!((x - 1023.5).abs() < 1e-9 && (y - 1023.5).abs() < 1e-9);
        // 60" north: 100 pixels up.
        let (x, y) = w.pixel(124.6, 15.2 + 60.0 / 3600.0).unwrap();
        assert!((x - 1023.5).abs() < 1e-6 && (y - 1123.5).abs() < 1e-3, "{x} {y}");
        // 60" east (RA up by 60"/cos dec): 100 pixels left -- and 0.004 up,
        // the parallel of declination curving away from the tangent plane's
        // straight line.
        let (x, y) = w.pixel(124.6 + 60.0 / 3600.0 / 15.2f64.to_radians().cos(), 15.2).unwrap();
        assert!((x - 923.5).abs() < 1e-3 && (y - 1023.504).abs() < 1e-3, "{x} {y}");
    }

    #[test]
    fn pixel_and_sky_are_inverses() {
        let w = Wcs::centred(359.99, -62.0, 2048, 2048, 0.6);
        for (x, y) in [(0.0, 0.0), (2047.0, 13.0), (500.25, 1900.75)] {
            let (ra, dec) = w.sky(x, y);
            let (x2, y2) = w.pixel(ra, dec).unwrap();
            assert!((x - x2).abs() < 1e-7 && (y - y2).abs() < 1e-7, "{x} {y} -> {x2} {y2}");
        }
    }

    #[test]
    fn the_header_round_trips() {
        let w = Wcs::centred(124.6, 15.2, 512, 256, 0.6);
        let mut h = Header::default();
        w.header(&mut h);
        let back = Wcs::from_header(&h).unwrap();
        assert!((back.scale - 0.6).abs() < 1e-12 && back.x0 == w.x0 && back.y0 == w.y0);
    }
}
