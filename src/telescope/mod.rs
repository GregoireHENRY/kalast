//! A telescope's frames of a night, and their photometry.
//!
//! What a ground-based observer records of a target kalast models: each
//! exposure a FITS frame of counts -- the target and the field's stars as
//! the atmosphere and the optics spread them, the sky under them, the
//! detector's noise over it -- and then what a night's reduction makes of
//! them: each source's light in an aperture, the frame's zero point from its
//! comparison stars, the target's magnitude, a light curve. The same
//! measurement a real night's frames go through, so a model is checked the
//! way the data it is compared with were made.
//!
//! [`expose`] makes a frame; [`photometry`] measures one; [`fits`] writes and
//! reads them. Every effect is switched by [`Effects`], all on by default.
//!
//! # Brightness
//!
//! A source's magnitude in a [`Band`] above the atmosphere sets the
//! electrons it gives, through the band's photon flux of a zero-magnitude
//! star and the [`Telescope`]'s collecting area and throughput. The target's
//! magnitude comes from its disc integral (`lightcurve::flux`, or the
//! renderer's), its distances and the Sun's magnitude in the band:
//! [`Band::magnitude`].
//!
//! # What is not here
//!
//! - Flat fields, fringing, cosmic rays, a variable sky through the night.
//! - Colour terms: the target and the stars are dimmed alike by airmass.
//! - Field rotation and a frame's skew: north is up, east left.

pub mod fits;
pub mod noise;
pub mod photometry;
pub mod psf;
pub mod wcs;

use noise::Rng;
use psf::Moffat;
use wcs::Wcs;

/// The instrument: its optics and its camera. The defaults are
/// TRAPPIST-North's (Oukaïmeden, Morocco; trappist.uliege.be): a 0.60 m
/// Ritchey-Chrétien at F/8, an Andor iKon-L BEX2-DD on an e2v CCD42-40,
/// 2048 x 2048 pixels of 13.5 µm, 0.60" each, a gain of 1.1 e-/ADU, a read
/// noise of 7 e- at 1 MHz, a dark current of 0.15 e-/s, a full well of
/// 100,000 e-. The obstruction, the throughput and the bias level are not
/// published: assumed.
#[derive(Debug, Clone, PartialEq)]
pub struct Telescope {
    pub name: String,
    pub camera: String,
    pub width: usize,
    pub height: usize,
    /// Arcseconds a pixel.
    pub pixel_scale: f64,
    /// The primary mirror's diameter, metres.
    pub aperture: f64,
    /// The secondary's shadow, a share of the diameter.
    pub obstruction: f64,
    /// The optics, the filter and the detector's quantum efficiency
    /// together, in the band.
    pub throughput: f64,
    /// Electrons an ADU.
    pub gain: f64,
    /// Electrons a pixel, at each read.
    pub read_noise: f64,
    /// Electrons a pixel a second.
    pub dark: f64,
    /// ADU a pixel holds with no light.
    pub bias: f64,
    /// Electrons a pixel holds; more are lost.
    pub full_well: f64,
}

impl Default for Telescope {
    fn default() -> Self {
        Self {
            name: "TRAPPIST-North".into(),
            camera: "Andor iKon-L BEX2-DD".into(),
            width: 2048,
            height: 2048,
            pixel_scale: 0.60,
            aperture: 0.60,
            obstruction: 0.35,
            throughput: 0.60,
            gain: 1.1,
            read_noise: 7.0,
            dark: 0.15,
            bias: 1000.0,
            full_well: 100_000.0,
        }
    }
}

impl Telescope {
    /// Electrons a second from a star of magnitude zero in `band`, above the
    /// atmosphere.
    pub fn zero_rate(&self, band: &Band) -> f64 {
        let radius_cm = self.aperture * 50.0;
        let area = std::f64::consts::PI * radius_cm * radius_cm * (1.0 - self.obstruction * self.obstruction);
        band.photons * area * self.throughput
    }
}

/// A photometric band: the Sun's magnitude in it at 1 au (Willmer 2018,
/// Vega), and the photons a second a square centimetre of a magnitude-zero
/// star (Bessell, Castelli & Plez 1998's photon flux at the band's
/// effective wavelength, times its width).
#[derive(Debug, Clone, PartialEq)]
pub struct Band {
    pub name: String,
    pub sun: f64,
    pub photons: f64,
}

impl Band {
    /// Johnson B and V, Cousins Rc and Ic, by name.
    pub fn named(name: &str) -> Option<Self> {
        let (sun, per_angstrom, width) = match name {
            "B" => (-26.13, 1392.6, 890.0),
            "V" => (-26.76, 995.5, 840.0),
            "R" | "Rc" => (-27.15, 702.0, 1580.0),
            "I" | "Ic" => (-27.47, 452.0, 1540.0),
            _ => return None,
        };
        Some(Self { name: name.to_string(), sun, photons: per_angstrom * width })
    }

    /// A target's magnitude in the band from its disc integral `flux` (an
    /// area per steradian per unit irradiance, in km² for a mesh in km, as
    /// `lightcurve::flux` gives it), `r` its distance from the Sun in au and
    /// `delta` from the observer in km.
    pub fn magnitude(&self, flux: f64, r: f64, delta: f64) -> f64 {
        self.sun - 2.5 * (flux / (r * r * delta * delta)).log10()
    }
}

/// The site and the night.
#[derive(Debug, Clone, PartialEq)]
pub struct Sky {
    /// Seeing, the full width at half maximum of a star, arcseconds.
    pub seeing: f64,
    /// Moffat's beta for the stars' profile.
    pub beta: f64,
    /// The sky's brightness, magnitudes a square arcsecond.
    pub brightness: f64,
    /// Extinction, magnitudes an airmass.
    pub extinction: f64,
    /// The observatory's altitude, metres: scintillation.
    pub altitude: f64,
}

impl Default for Sky {
    /// A dark night at Oukaïmeden in Rc, assumed: 2" seeing, 20.5 mag a
    /// square arcsecond, 0.10 mag an airmass, 2751 m.
    fn default() -> Self {
        Self { seeing: 2.0, beta: 3.0, brightness: 20.5, extinction: 0.10, altitude: 2751.0 }
    }
}

/// What a frame has of what a real one would: each on by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Effects {
    /// Seeing and the optics spread a point source (Moffat); off, its light
    /// lands in one pixel.
    pub psf: bool,
    /// The target moves against the stars during the exposure.
    pub trail: bool,
    /// The sky's light.
    pub sky: bool,
    /// The field's stars.
    pub stars: bool,
    /// Extinction: the sources dimmed by the airmass.
    pub extinction: bool,
    /// Scintillation: each source's light flickers, independently.
    pub scintillation: bool,
    /// Photon noise: each pixel's electrons drawn from Poisson's law.
    pub photon_noise: bool,
    /// The dark current's electrons.
    pub dark: bool,
    /// The read noise, Gaussian, each pixel.
    pub read_noise: bool,
    /// The bias level.
    pub bias: bool,
    /// Pixels full at the full well, counts clipped at 65,535.
    pub saturation: bool,
}

impl Default for Effects {
    fn default() -> Self {
        Self {
            psf: true,
            trail: true,
            sky: true,
            stars: true,
            extinction: true,
            scintillation: true,
            photon_noise: true,
            dark: true,
            read_noise: true,
            bias: true,
            saturation: true,
        }
    }
}

impl Effects {
    /// The names of those on, for a frame's header.
    pub fn names(&self) -> Vec<&'static str> {
        let all = [
            (self.psf, "psf"),
            (self.trail, "trail"),
            (self.sky, "sky"),
            (self.stars, "stars"),
            (self.extinction, "extinction"),
            (self.scintillation, "scintillation"),
            (self.photon_noise, "photon_noise"),
            (self.dark, "dark"),
            (self.read_noise, "read_noise"),
            (self.bias, "bias"),
            (self.saturation, "saturation"),
        ];
        all.iter().filter(|(on, _)| *on).map(|(_, n)| *n).collect()
    }
}

/// A point on the sky and its magnitude above the atmosphere; `rate` its
/// motion against the stars, arcseconds a second east (along the sky, the
/// right ascension's rate times the declination's cosine) and north.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Source {
    pub ra: f64,
    pub dec: f64,
    pub mag: f64,
    pub rate: (f64, f64),
}

/// One exposure: when, how long, where the telescope points, through how
/// much air.
#[derive(Debug, Clone, PartialEq)]
pub struct Exposure {
    /// Its start, UTC, ISO 8601: `2022-11-18T00:30:00.000`.
    pub start: String,
    /// Seconds.
    pub duration: f64,
    /// The frame's centre, degrees.
    pub ra: f64,
    pub dec: f64,
    /// The target's airmass at mid-exposure.
    pub airmass: f64,
    /// The noise's seed: the same seed, the same frame.
    pub seed: u64,
}

/// The Julian date of an ISO 8601 UTC time, `YYYY-MM-DDTHH:MM:SS[.sss]`.
pub fn julian_date(iso: &str) -> Option<f64> {
    let (date, time) = iso.trim().split_once(['T', ' ']).unwrap_or((iso.trim(), "00:00:00"));
    let mut d = date.split('-').map(|s| s.parse::<i64>());
    let (y, m, day) = (d.next()?.ok()?, d.next()?.ok()?, d.next()?.ok()?);
    let mut t = time.trim_end_matches('Z').split(':');
    let hours: f64 = t.next()?.parse().ok()?;
    let minutes: f64 = t.next().unwrap_or("0").parse().ok()?;
    let seconds: f64 = t.next().unwrap_or("0").parse().ok()?;
    // Fliegel & Van Flandern's day number, Gregorian.
    let a = (14 - m) / 12;
    let (yy, mm) = (y + 4800 - a, m + 12 * a - 3);
    let jdn = day + (153 * mm + 2) / 5 + 365 * yy + yy / 4 - yy / 100 + yy / 400 - 32045;
    Some(jdn as f64 - 0.5 + (hours + minutes / 60.0 + seconds / 3600.0) / 24.0)
}

/// Airmass at an altitude above the horizon, degrees: Kasten & Young
/// (1989), right to the horizon, where `1 / sin` is not.
pub fn airmass(altitude: f64) -> f64 {
    let h = altitude.max(0.0);
    1.0 / (h.to_radians().sin() + 0.50572 * (h + 6.07995).powf(-1.6364))
}

/// Scintillation's relative scatter on a source's light (Young 1967, as
/// Osborn et al. 2015 restate it): `0.09 D^(-2/3) X^1.75 exp(-h/8000) (2t)^(-1/2)`,
/// `D` the aperture in cm, `h` the altitude in m, `t` the exposure in s.
pub fn scintillation(aperture: f64, airmass: f64, altitude: f64, exposure: f64) -> f64 {
    0.09 * (aperture * 100.0).powf(-2.0 / 3.0) * airmass.powf(1.75) * (-altitude / 8000.0).exp() / (2.0 * exposure).sqrt()
}

/// A frame of `target` and `stars` through `telescope`, in counts (ADU) as
/// its camera would store them. Its header says when, through what, under
/// which sky, where the target was predicted (`TARGRA`, `TARGDEC`), and
/// which effects it has.
#[allow(clippy::too_many_arguments)]
pub fn expose(
    telescope: &Telescope,
    band: &Band,
    sky: &Sky,
    effects: &Effects,
    exposure: &Exposure,
    target: &Source,
    stars: &[Source],
    object: &str,
) -> fits::Image {
    let (w, h) = (telescope.width, telescope.height);
    let t = exposure.duration;
    let wcs = Wcs::centred(exposure.ra, exposure.dec, w, h, telescope.pixel_scale);
    let mut rng = Rng::new(exposure.seed);
    let zero = telescope.zero_rate(band);
    let dimming = if effects.extinction { sky.extinction * exposure.airmass } else { 0.0 };
    let flicker = if effects.scintillation {
        scintillation(telescope.aperture, exposure.airmass, sky.altitude, t)
    } else {
        0.0
    };
    let psf = effects.psf.then(|| Moffat::from_fwhm(sky.seeing / telescope.pixel_scale, sky.beta));
    let electrons = |mag: f64, rng: &mut Rng| {
        let e = zero * 10f64.powf(-0.4 * (mag + dimming)) * t;
        if flicker > 0.0 { e * (1.0 + flicker * rng.normal()).max(0.0) } else { e }
    };

    // The light each pixel collects, in electrons on average.
    let mut image = vec![0.0f64; w * h];
    if effects.sky {
        let per_pixel = zero * 10f64.powf(-0.4 * sky.brightness) * telescope.pixel_scale.powi(2) * t;
        image.iter_mut().for_each(|e| *e += per_pixel);
    }
    if effects.dark {
        image.iter_mut().for_each(|e| *e += telescope.dark * t);
    }
    if effects.stars {
        for star in stars {
            if let Some((x, y)) = wcs.pixel(star.ra, star.dec) {
                let e = electrons(star.mag, &mut rng);
                psf::add(&mut image, w, h, x, y, e, psf.as_ref());
            }
        }
    }
    let e = electrons(target.mag, &mut rng);
    // The tracking follows the stars: the target runs from where it was at
    // the start to where it is at the end, about mid-exposure.
    let half = if effects.trail { 0.5 * t } else { 0.0 };
    let at = |s: f64| {
        let dec = target.dec + target.rate.1 * s / 3600.0;
        let ra = target.ra + target.rate.0 * s / 3600.0 / target.dec.to_radians().cos();
        wcs.pixel(ra, dec)
    };
    if let (Some(a), Some(b)) = (at(-half), at(half)) {
        psf::add_trail(&mut image, w, h, a, b, e, psf.as_ref());
    }

    // What the camera reads.
    let data: Vec<f32> = image
        .iter()
        .map(|&mean| {
            let mut e = if effects.photon_noise { rng.poisson(mean) } else { mean };
            if effects.saturation {
                e = e.min(telescope.full_well);
            }
            if effects.read_noise {
                e += telescope.read_noise * rng.normal();
            }
            let mut adu = e / telescope.gain + if effects.bias { telescope.bias } else { 0.0 };
            if effects.saturation {
                adu = adu.clamp(0.0, 65535.0);
            }
            adu as f32
        })
        .collect();

    let mut header = fits::Header::default();
    header.set("OBJECT", object, "");
    header.set("TELESCOP", telescope.name.as_str(), "");
    header.set("INSTRUME", telescope.camera.as_str(), "");
    header.set("FILTER", band.name.as_str(), "");
    header.set("DATE-OBS", exposure.start.as_str(), "UTC, the exposure's start");
    header.set("EXPTIME", t, "[s]");
    if let Some(jd) = julian_date(&exposure.start) {
        header.set("MJD-OBS", jd - 2_400_000.5, "UTC, the exposure's start");
        header.set("JD-MID", jd + 0.5 * t / 86_400.0, "UTC, mid-exposure");
    }
    header.set("AIRMASS", exposure.airmass, "the target's, mid-exposure");
    header.set("GAIN", telescope.gain, "[e-/ADU]");
    header.set("RDNOISE", telescope.read_noise, "[e-]");
    header.set("SATURATE", 65535.0, "[ADU]");
    header.set("PIXSCALE", telescope.pixel_scale, "[arcsec/pixel]");
    header.set("SEEING", sky.seeing, "[arcsec] FWHM");
    header.set("SKYMAG", sky.brightness, "[mag/arcsec2]");
    header.set("EXTINCT", sky.extinction, "[mag/airmass]");
    header.set("TARGRA", target.ra, "[deg] the target's predicted position");
    header.set("TARGDEC", target.dec, "[deg] mid-exposure");
    header.set("TARGMAG", target.mag, "its magnitude above the atmosphere");
    header.set("EFFECTS", effects.names().join(" "), "kalast's");
    header.set("ORIGIN", "kalast", "a simulated frame");
    wcs.header(&mut header);
    fits::Image { width: w, height: h, data, header }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quiet() -> Effects {
        Effects {
            psf: true,
            trail: false,
            sky: false,
            stars: false,
            extinction: false,
            scintillation: false,
            photon_noise: false,
            dark: false,
            read_noise: false,
            bias: false,
            saturation: false,
        }
    }

    #[test]
    fn dates_are_julian() {
        assert_eq!(julian_date("2000-01-01T12:00:00"), Some(2_451_545.0));
        let jd = julian_date("2022-11-18T00:30:00.000").unwrap();
        assert!((jd - 2_459_901.520_833_3).abs() < 1e-6, "{jd}");
    }

    #[test]
    fn airmass_is_one_at_the_zenith_and_secant_above_the_horizon() {
        assert!((airmass(90.0) - 1.0).abs() < 1e-3);
        assert!((airmass(30.0) - 2.0).abs() < 0.01);
        assert!(airmass(0.0) > 35.0 && airmass(0.0) < 40.0);
    }

    #[test]
    fn a_target_gives_the_electrons_its_magnitude_says() {
        let tel = Telescope { width: 128, height: 128, ..Default::default() };
        let band = Band::named("Rc").unwrap();
        let sky = Sky::default();
        let exposure = Exposure { start: "2022-11-18T03:00:00".into(), duration: 60.0, ra: 124.6, dec: 15.2, airmass: 1.2, seed: 1 };
        let target = Source { ra: 124.6, dec: 15.2, mag: 16.0, rate: (0.0, 0.0) };
        let frame = expose(&tel, &band, &sky, &quiet(), &exposure, &target, &[], "test");
        let adu: f64 = frame.data.iter().map(|&v| v as f64).sum();
        let expected = tel.zero_rate(&band) * 10f64.powf(-0.4 * 16.0) * 60.0 / tel.gain;
        // What the 64-pixel half-frame leaves of the Moffat wings.
        let p = Moffat::from_fwhm(sky.seeing / tel.pixel_scale, sky.beta);
        assert!((adu / (expected * (1.0 - p.beyond(60.0))) - 1.0).abs() < 2e-3, "{adu} against {expected}");
        // Extinction takes its share.
        let effects = Effects { extinction: true, ..quiet() };
        let dim = expose(&tel, &band, &sky, &effects, &exposure, &target, &[], "test");
        let adu2: f64 = dim.data.iter().map(|&v| v as f64).sum();
        assert!(((adu / adu2).log10() * 2.5 - 0.10 * 1.2).abs() < 1e-6);
    }

    #[test]
    fn a_header_says_what_the_frame_is() {
        let tel = Telescope { width: 64, height: 64, ..Default::default() };
        let exposure = Exposure { start: "2022-11-18T03:00:00".into(), duration: 120.0, ra: 124.6, dec: 15.2, airmass: 1.1, seed: 4 };
        let target = Source { ra: 124.6, dec: 15.2, mag: 16.0, rate: (0.02, 0.0) };
        let frame = expose(&tel, &Band::named("V").unwrap(), &Sky::default(), &Effects::default(), &exposure, &target, &[], "Didymos");
        let h = &frame.header;
        assert_eq!(h.str("FILTER"), Some("V"));
        assert_eq!(h.f64("EXPTIME"), Some(120.0));
        let mid = julian_date("2022-11-18T03:01:00").unwrap();
        assert!((h.f64("JD-MID").unwrap() - mid).abs() < 1e-9);
        assert!(h.str("EFFECTS").unwrap().contains("photon_noise"));
        assert!(Wcs::from_header(h).is_some());
    }

    #[test]
    fn the_noise_is_what_its_sources_say() {
        // A blank frame: sky, dark, bias and read noise; its scatter is
        // sqrt(sky + dark + read^2) electrons.
        let tel = Telescope { width: 256, height: 256, ..Default::default() };
        let band = Band::named("Rc").unwrap();
        let sky = Sky::default();
        let effects = Effects { psf: false, stars: false, scintillation: false, trail: false, ..Effects::default() };
        let exposure = Exposure { start: "2022-11-18T03:00:00".into(), duration: 120.0, ra: 0.0, dec: 0.0, airmass: 1.0, seed: 9 };
        // The target far off the frame.
        let target = Source { ra: 10.0, dec: 10.0, mag: 30.0, rate: (0.0, 0.0) };
        let frame = expose(&tel, &band, &sky, &effects, &exposure, &target, &[], "blank");
        let n = frame.data.len() as f64;
        let mean = frame.data.iter().map(|&v| v as f64).sum::<f64>() / n;
        let var = frame.data.iter().map(|&v| (v as f64 - mean).powi(2)).sum::<f64>() / (n - 1.0);
        let sky_e = tel.zero_rate(&band) * 10f64.powf(-0.4 * sky.brightness) * 0.36 * 120.0;
        let dark_e = 0.15 * 120.0;
        let expect_mean = (sky_e + dark_e) / tel.gain + tel.bias;
        let expect_var = (sky_e + dark_e + 49.0) / (tel.gain * tel.gain);
        // Five standard errors of the mean.
        assert!((mean - expect_mean).abs() < 5.0 * (expect_var / n).sqrt(), "{mean} against {expect_mean}");
        assert!((var / expect_var - 1.0).abs() < 0.02, "{var} against {expect_var}");
    }
}
