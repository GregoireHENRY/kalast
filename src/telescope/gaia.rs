//! The field's stars from Gaia DR3: a frame's stars, and its comparison
//! stars.
//!
//! A cone of `gaiadr3.gaia_source` from ESA's Gaia archive (TAP, synchronous,
//! CSV), kept in a file so that a night asks once. Each star is carried from
//! Gaia's epoch, 2016.0, to the night's by its proper motion, and given a
//! magnitude in the frame's band from its G and BP - RP by Riello et al.
//! (2021, A&A 649, A3), Table C.2:
//!
//! ```text
//! G - V  = -0.02704 + 0.01424 x - 0.2156 x^2 + 0.01426 x^3                     -0.5 < x < 5.0, 0.030 mag
//! G - Rc = -0.02275 + 0.3961 x - 0.1243 x^2 - 0.01396 x^3 + 0.003775 x^4       0.0 < x < 4.0, 0.032 mag
//! G - Ic =  0.01753 + 0.76 x - 0.0991 x^2                                      -0.5 < x < 4.5, 0.038 mag
//! ```
//!
//! `x = BP - RP`. A star with no colour, or one outside the polynomial's
//! range, is still drawn in a frame, at its G less a typical `G - band`, but
//! is no comparison star: its magnitude is not known well enough to
//! calibrate one.

use std::path::Path;

use super::Source;

/// Gaia's archive, its synchronous TAP endpoint.
const TAP: &str = "https://gea.esac.esa.int/tap-server/tap/sync";

/// Gaia DR3's epoch, Julian years.
const EPOCH: f64 = 2016.0;

/// A star as Gaia DR3 gives it.
#[derive(Debug, Clone, PartialEq)]
pub struct Star {
    pub source_id: u64,
    /// Degrees, ICRS, at Gaia's epoch.
    pub ra: f64,
    pub dec: f64,
    /// Proper motion, mas a year: along the right ascension (times the
    /// declination's cosine, as Gaia gives it) and the declination.
    pub pmra: Option<f64>,
    pub pmdec: Option<f64>,
    pub g: f64,
    pub bp: Option<f64>,
    pub rp: Option<f64>,
}

impl Star {
    /// Its colour, BP - RP.
    pub fn colour(&self) -> Option<f64> {
        Some(self.bp? - self.rp?)
    }

    /// Its magnitude in `band` -- V, Rc or Ic -- by Riello et al.'s
    /// polynomial, `None` without a colour, outside the polynomial's range,
    /// or in another band.
    pub fn magnitude(&self, band: &str) -> Option<f64> {
        let x = self.colour()?;
        let (lo, hi, g_less) = match band {
            "V" => (-0.5, 5.0, -0.02704 + x * (0.01424 + x * (-0.2156 + x * 0.01426))),
            "R" | "Rc" => (0.0, 4.0, -0.02275 + x * (0.3961 + x * (-0.1243 + x * (-0.01396 + x * 0.003775)))),
            "I" | "Ic" => (-0.5, 4.5, 0.01753 + x * (0.76 + x * -0.0991)),
            _ => return None,
        };
        (x > lo && x < hi).then(|| self.g - g_less)
    }

    /// Where it is at `year`, Julian years, carried by its proper motion;
    /// where Gaia saw it without one.
    pub fn at(&self, year: f64) -> (f64, f64) {
        let dt = year - EPOCH;
        let dec = self.dec + self.pmdec.unwrap_or(0.0) * dt / 3.6e6;
        let ra = self.ra + self.pmra.unwrap_or(0.0) * dt / 3.6e6 / self.dec.to_radians().cos();
        (ra, dec)
    }

    /// As a frame's source in `band` at `year`: a comparison star where its
    /// magnitude is known, drawn at its G less a typical `G - band` (that of
    /// BP - RP = 1) where it is not.
    pub fn source(&self, band: &str, year: f64) -> Source {
        let (ra, dec) = self.at(year);
        let known = self.magnitude(band);
        let typical = Star { bp: Some(1.0), rp: Some(0.0), ..self.clone() };
        let mag = known.or_else(|| typical.magnitude(band)).unwrap_or(self.g);
        Source { ra, dec, mag, rate: (0.0, 0.0), comparison: known.is_some() }
    }
}

/// Stars of the CSV the archive answers with, its first line the columns'
/// names; empty fields are `None`.
pub fn parse(csv: &str) -> Result<Vec<Star>, String> {
    let mut lines = csv.lines().filter(|l| !l.trim().is_empty());
    let names: Vec<&str> = lines.next().ok_or("no header")?.split(',').map(|s| s.trim().trim_matches('"')).collect();
    let column = |name: &str| names.iter().position(|n| *n == name).ok_or_else(|| format!("no column {name}"));
    let (id, ra, dec, pmra, pmdec, g, bp, rp) = (
        column("source_id")?,
        column("ra")?,
        column("dec")?,
        column("pmra")?,
        column("pmdec")?,
        column("phot_g_mean_mag")?,
        column("phot_bp_mean_mag")?,
        column("phot_rp_mean_mag")?,
    );
    let mut stars = Vec::new();
    for line in lines {
        let fields: Vec<&str> = line.split(',').map(|s| s.trim().trim_matches('"')).collect();
        let num = |k: usize| fields.get(k).and_then(|s| s.parse::<f64>().ok());
        let (Some(r), Some(d), Some(gm)) = (num(ra), num(dec), num(g)) else {
            continue;
        };
        stars.push(Star {
            source_id: fields.get(id).and_then(|s| s.parse().ok()).unwrap_or(0),
            ra: r,
            dec: d,
            pmra: num(pmra),
            pmdec: num(pmdec),
            g: gm,
            bp: num(bp),
            rp: num(rp),
        });
    }
    Ok(stars)
}

/// The stars within `radius` degrees of (`ra`, `dec`) brighter than `g_max`
/// in G, from the file `cache` when it holds them -- the archive's answer is
/// written there the first time -- or from the archive.
pub fn cone(ra: f64, dec: f64, radius: f64, g_max: f64, cache: Option<&Path>) -> Result<Vec<Star>, String> {
    if let Some(path) = cache.filter(|p| p.exists()) {
        let csv = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        return parse(&csv);
    }
    let query = format!(
        "SELECT source_id, ra, dec, pmra, pmdec, phot_g_mean_mag, phot_bp_mean_mag, phot_rp_mean_mag \
         FROM gaiadr3.gaia_source \
         WHERE 1 = CONTAINS(POINT('ICRS', ra, dec), CIRCLE('ICRS', {ra}, {dec}, {radius})) \
         AND phot_g_mean_mag < {g_max}"
    );
    let mut response = ureq::post(TAP)
        .header("User-Agent", &format!("kalast/{}", env!("CARGO_PKG_VERSION")))
        .send_form([("REQUEST", "doQuery"), ("LANG", "ADQL"), ("FORMAT", "csv"), ("QUERY", query.as_str())])
        .map_err(|e| format!("Gaia's archive: {e}"))?;
    let csv = response.body_mut().read_to_string().map_err(|e| format!("Gaia's archive: {e}"))?;
    let stars = parse(&csv)?;
    if let Some(path) = cache {
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::write(path, &csv).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(stars)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CSV: &str = "source_id,ra,dec,pmra,pmdec,phot_g_mean_mag,phot_bp_mean_mag,phot_rp_mean_mag
655423558539267200,124.59931680558019,15.27904073392891,-0.8161789946458278,-2.3275765992169815,15.389577,15.665113,14.9559965
652414435733230720,124.58086274157544,15.204192456736573,,,17.138256,,
";

    #[test]
    fn the_archives_csv_reads() {
        let stars = parse(CSV).unwrap();
        assert_eq!(stars.len(), 2);
        assert_eq!(stars[0].source_id, 655423558539267200);
        assert!((stars[0].colour().unwrap() - 0.709116).abs() < 1e-5);
        assert_eq!((stars[1].pmra, stars[1].bp), (None, None));
    }

    #[test]
    fn magnitudes_follow_riello_et_al() {
        let s = &parse(CSV).unwrap()[0];
        // x = 0.7091: G - Rc = 0.1916, G - V = -0.1203, by Table C.2.
        let x = s.colour().unwrap();
        let g_r = -0.02275 + 0.3961 * x - 0.1243 * x * x - 0.01396 * x.powi(3) + 0.003775 * x.powi(4);
        assert!((s.magnitude("Rc").unwrap() - (s.g - g_r)).abs() < 1e-12);
        assert!((g_r - 0.1916).abs() < 1e-4, "{g_r}");
        assert!((s.g - s.magnitude("V").unwrap() + 0.1203).abs() < 1e-4);
        assert!(s.magnitude("V").unwrap() > s.magnitude("Rc").unwrap(), "a star of this colour is fainter in V");
        // Without a colour: drawn, but no comparison star.
        let faint = parse(CSV).unwrap()[1].source("Rc", 2022.88);
        assert!(!faint.comparison && faint.mag < 17.138256);
    }

    #[test]
    fn proper_motion_carries_a_star() {
        let s = &parse(CSV).unwrap()[0];
        let (ra, dec) = s.at(2026.0);
        assert!((dec - (s.dec - 2.3275765992169815 * 10.0 / 3.6e6)).abs() < 1e-12);
        assert!(ra < s.ra);
    }
}
