//! Stereophotoclinometry on a maplet: register, shade, integrate.

use super::grid;
use super::maplet::Maplet;
use super::par_map;
use super::view::{Crop, Table, View};
use crate::lightcurve::Law;
use super::{F, V3};

/// How [`Maplet::solve`] goes about it.
#[derive(Clone, Debug)]
pub struct Options {
    /// Passes at each level: register, the slopes and albedo point by point,
    /// the heights from the slopes.
    pub iterations: usize,
    /// Levels of resolution: the first at `2^(levels - 1)` times the
    /// maplet's spacing, each next twice as fine, the last its own. The
    /// coarse ones see more images -- more pixels are under their spacing --
    /// and carry what they find to the fine ones. A coarse level leaves out
    /// the views more than twice finer than its spacing, and is skipped
    /// when none is left; the last uses every view within `resolution`, so
    /// its spacing is best near the finest views' pixel.
    pub levels: usize,
    /// The wavelength, in the surface's unit, above which the shape model
    /// keeps the heights: shading tells slopes, and its errors -- in the law,
    /// in the calibration -- add up over distance to a tilt, where
    /// stereophotogrammetry is at its best. 0 for 25 spacings.
    pub anchor: F,
    /// Register each view: shift its pixels to where the maplet's shading
    /// falls in them, to a fraction of a pixel -- what pointing and the
    /// shape model's position leave. Off, the projection is taken as it is.
    pub register: bool,
    /// How far, in pixels, the first registration looks.
    pub search: F,
    /// The coarsest a view's pixel may be at the maplet, in spacings. A
    /// view coarser than the spacing tells the slopes at its own scale, and
    /// weighs as the square of the ratio less.
    pub resolution: F,
    /// The largest incidence used, radians.
    pub incidence: F,
    /// The largest emission used, radians.
    pub emission: F,
    /// How far a point's slopes may stray from the integrated surface's in
    /// one pass, as a prior's standard deviation: a point no view sees keeps
    /// its slopes.
    pub slope_sigma: F,
    /// The prior's standard deviation on the log of the albedo, toward its
    /// neighbours' mean.
    pub albedo_sigma: F,
    /// An offset per view as well as a scale: a background, a coma.
    pub offset: bool,
    /// What is darker than this share of its view's median is taken for a
    /// shadow the maplet does not cast -- from terrain beyond it, or relief
    /// it does not have yet -- and left out: read as slopes, an unmodelled
    /// shadow would flatten what casts it.
    pub dark: F,
    /// A line a pass.
    pub verbose: bool,
}

impl Default for Options {
    fn default() -> Self {
        let rad = super::RPD;
        Options {
            iterations: 6,
            levels: 1,
            anchor: 0.0,
            register: true,
            search: 4.0,
            resolution: 2.0,
            incidence: 80.0 * rad,
            emission: 75.0 * rad,
            slope_sigma: 0.5,
            albedo_sigma: 0.3,
            offset: false,
            dark: 0.1,
            verbose: false,
        }
    }
}

/// What a view gave, in the last pass.
#[derive(Clone, Debug, Default)]
pub struct ViewReport {
    pub name: String,
    /// Whether it saw enough of the maplet, lit and fine enough, to be used.
    pub used: bool,
    /// A pixel's size at the maplet's centre, in the surface's unit.
    pub pixel: F,
    /// Where the maplet's points fall in the image against where its camera
    /// puts them, in pixels.
    pub shift: [F; 2],
    /// The image over the maplet's I/F.
    pub scale: F,
    pub offset: F,
    /// The maplet's points it saw lit.
    pub points: usize,
    /// The RMS of the image less the maplet's, over the image's mean.
    pub residual: F,
    /// The correlation of the image with the maplet's shading on the points
    /// it saw: as the shape model has it, and after the last pass.
    pub correlation: [F; 2],
}

/// What [`Maplet::solve`] did.
#[derive(Clone, Debug, Default)]
pub struct Report {
    pub views: Vec<ViewReport>,
    /// Per pass, the views' residual: the RMS of the images less the
    /// maplet's, over their mean -- at the pass's start, the last one after
    /// the last pass.
    pub residual: Vec<F>,
    /// Per pass, the RMS change of the heights.
    pub change: Vec<F>,
    /// Per pass, the spacing it ran at.
    pub spacing: Vec<F>,
}

impl Maplet {
    /// The heights and albedo from `views` under `law`, from the heights as
    /// they are. See the [module](super).
    pub fn solve(&mut self, views: &[View], law: &Law, options: &Options) -> Result<Report, String> {
        law.check()?;
        for v in views {
            v.check()?;
        }
        let wavelength = if options.anchor > 0.0 { options.anchor } else { 25.0 * self.spacing };
        let extent = (self.n - 1) as F * self.spacing;
        let mut report = Report {
            views: views
                .iter()
                .map(|v| ViewReport {
                    name: v.name.clone(),
                    scale: 1.0,
                    correlation: [F::NAN; 2],
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        let levels = options.levels.max(1);
        let mut registered = vec![false; views.len()];
        let mut previous: Option<(Vec<F>, Vec<F>, usize, F)> = None;
        // How far the views have moved the maplet (`follow`), carried from
        // level to level: its centre, and the reference with it.
        let mut moved = V3::ZERO;
        let [u, v, _] = self.axes;
        for level in (0..levels).rev() {
            let mut work = if level == 0 {
                self.clone()
            } else {
                let m = ((self.n - 1) >> level).max(2) + 1;
                self.resampled(m, extent / (m - 1) as F)
            };
            if moved != V3::ZERO {
                work.centre += moved;
                let s = work.spacing;
                work.reference = grid::shifted(&work.reference, work.n, moved.dot(u) / s, moved.dot(v) / s);
            }
            if let Some((h, a, n, s)) = &previous {
                work.heights = grid::resample(h, *n, *s, work.n, work.spacing);
                work.albedo = grid::resample(a, *n, *s, work.n, work.spacing);
            }
            let before = work.centre;
            passes(&mut work, views, law, options, wavelength, level > 0, &mut registered, &mut report);
            moved += work.centre - before;
            previous = Some((work.heights.clone(), work.albedo.clone(), work.n, work.spacing));
            if level == 0 {
                self.heights = work.heights;
                self.albedo = work.albedo;
                self.centre = work.centre;
                self.reference = work.reference;
            }
        }
        Ok(report)
    }
}

/// A view as it looks at a level: its window, its law's table, its pixel.
struct Setup {
    crop: Crop,
    table: Table,
    pixel: F,
    weight: F,
}

/// A view as it sees the maplet in a pass.
struct Look {
    /// Where each point falls, before the registration's shift.
    xy: Vec<[F; 2]>,
    /// Lit, seen, within the angles.
    usable: Vec<bool>,
    /// The law's I/F at each point's normal, at unit albedo.
    law: Vec<F>,
    /// Seen within the emission, lit or not: what the registration compares
    /// -- the shadows' pattern with the rest, the strongest thing a
    /// low-Sun image shows.
    shown: Vec<bool>,
    /// Lit: facing the Sun and in no shadow the maplet casts.
    lit: Vec<bool>,
}

fn setup(view: &View, work: &Maplet, law: &Law, options: &Options, coarse: bool) -> Option<Setup> {
    let n = work.n;
    let s = work.spacing;
    let middle = work.point(n / 2, n / 2);
    let (_, _, depth) = view.project(middle)?;
    let pixel = depth / view.focal;
    // A coarse level leaves the views more than twice finer than its
    // spacing to the levels after it: what they resolve and the grid cannot
    // -- a boulder's lit flank, a few pixels across -- the photoclinometry
    // would place where the grid's slopes can have it, and each view's
    // registration follow it there, along its Sun.
    if pixel > options.resolution * s || (coarse && pixel < 0.5 * s) {
        return None;
    }
    let w = work.axes[2];
    let to_camera = (view.position - middle).normalize();
    let margin = 10.0 * super::RPD;
    if w.dot(to_camera) < (options.emission + margin).min(1.5).cos() || w.dot(view.sun) < (options.incidence + margin).min(1.5).cos() {
        return None;
    }
    let (mut x0, mut y0, mut x1, mut y1) = (F::INFINITY, F::INFINITY, F::NEG_INFINITY, F::NEG_INFINITY);
    let (mut a0, mut a1) = (F::INFINITY, F::NEG_INFINITY);
    for j in [0, n / 2, n - 1] {
        for i in [0, n / 2, n - 1] {
            let p = work.point(i, j);
            let alpha = view.sun.dot((view.position - p).normalize()).clamp(-1.0, 1.0).acos();
            a0 = a0.min(alpha);
            a1 = a1.max(alpha);
        }
    }
    for k in 0..n * n {
        let (x, y, _) = view.project(work.point(k % n, k / n))?;
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    let sigma = if s > pixel { 0.35 * s / pixel } else { 0.0 };
    let pad = options.search.max(0.0) + 3.0 * sigma + 3.0;
    if x1 + pad < 0.0 || y1 + pad < 0.0 || x0 - pad > view.width as F || y0 - pad > view.height as F {
        return None;
    }
    let crop = Crop::new(view, x0 - pad, y0 - pad, x1 + pad, y1 + pad, sigma)?;
    Some(Setup {
        crop,
        table: Table::new(law, a0, a1),
        pixel,
        weight: (s / pixel).powi(2).min(1.0),
    })
}

fn look(view: &View, setup: &Setup, work: &Maplet, points: &[V3], normals: &[V3], options: &Options) -> Look {
    let n = work.n;
    let s = work.spacing;
    let lit = grid::unblocked(&work.heights, n, s, work.local(view.sun));
    let middle = work.point(n / 2, n / 2);
    let seen = grid::unblocked(&work.heights, n, s, work.local((view.position - middle).normalize()));
    let (cos_i, cos_e) = (options.incidence.cos(), options.emission.cos());
    let mut out = Look {
        xy: vec![[F::NAN; 2]; n * n],
        usable: vec![false; n * n],
        law: vec![0.0; n * n],
        shown: vec![false; n * n],
        lit: vec![false; n * n],
    };
    for k in 0..n * n {
        let Some((x, y, _)) = view.project(points[k]) else {
            continue;
        };
        out.xy[k] = [x, y];
        let to_camera = (view.position - points[k]).normalize();
        let (mu0, mu) = (normals[k].dot(view.sun), normals[k].dot(to_camera));
        if mu0 > 0.0 && mu > 0.0 {
            let alpha = view.sun.dot(to_camera).clamp(-1.0, 1.0).acos();
            out.law[k] = setup.table.eval(mu0, mu, alpha).0;
        }
        out.usable[k] = lit[k] && seen[k] && mu0 >= cos_i && mu >= cos_e && out.law[k] > 0.0;
        out.shown[k] = seen[k] && mu >= cos_e;
        out.lit[k] = lit[k] && mu0 > 0.0;
    }
    out
}

/// The image at each usable point, shifted: not finite elsewhere.
fn observe(setup: &Setup, look: &Look, shift: [F; 2]) -> Vec<F> {
    look.xy
        .iter()
        .zip(&look.usable)
        .map(|(xy, &ok)| {
            if !ok {
                return F::NAN;
            }
            setup
                .crop
                .sample(xy[0] + shift[0], xy[1] + shift[1])
                .map_or(F::NAN, |s| s.0)
        })
        .collect()
}

fn correlation(a: &[F], b: &[F]) -> F {
    let n = a.len() as F;
    if a.len() < 3 {
        return F::NAN;
    }
    let (ma, mb) = (a.iter().sum::<F>() / n, b.iter().sum::<F>() / n);
    let (mut sab, mut saa, mut sbb) = (0.0, 0.0, 0.0);
    for (x, y) in a.iter().zip(b) {
        sab += (x - ma) * (y - mb);
        saa += (x - ma) * (x - ma);
        sbb += (y - mb) * (y - mb);
    }
    sab / (saa * sbb).sqrt().max(1e-300)
}

/// The shift, scale and offset that bring the image onto the maplet's
/// shading `model` (albedo times the law): the whole-pixel shift of best
/// correlation within `search` first when asked, then Gauss-Newton on all
/// together. `None` where too few points are seen.
#[allow(clippy::too_many_arguments)]
fn register(
    setup: &Setup,
    look: &Look,
    model: &[F],
    start: [F; 2],
    search: Option<F>,
    shift_too: bool,
    offset_too: bool,
    dark: F,
) -> Option<([F; 2], F, F)> {
    // The points seen, lit or in the maplet's shadow, the model nothing
    // there: the shadows' pattern registers as well as the shading.
    let points: Vec<usize> = (0..model.len()).filter(|&k| look.shown[k] && look.xy[k][0].is_finite()).collect();
    if points.iter().filter(|&&k| model[k] > 0.0).count() < 30 {
        return None;
    }
    let _ = dark;
    let cut = F::NEG_INFINITY;
    let pairs = |shift: [F; 2]| -> (Vec<F>, Vec<F>) {
        let mut o = Vec::with_capacity(points.len());
        let mut m = Vec::with_capacity(points.len());
        for &k in &points {
            if let Some((v, _, _)) = setup.crop.sample(look.xy[k][0] + shift[0], look.xy[k][1] + shift[1]) {
                if v >= cut {
                    o.push(v);
                    m.push(model[k]);
                }
            }
        }
        (o, m)
    };
    let mut shift = start;
    if let Some(r) = search {
        let r = r.round() as i64;
        let mut best = F::NEG_INFINITY;
        for dy in -r..=r {
            for dx in -r..=r {
                let s = [start[0] + dx as F, start[1] + dy as F];
                let (o, m) = pairs(s);
                if o.len() * 2 < points.len() {
                    continue;
                }
                let c = correlation(&o, &m);
                if c > best {
                    best = c;
                    shift = s;
                }
            }
        }
    }
    // Gauss-Newton on (dx, dy, scale, offset): residual image - scale model - offset.
    let mut scale = 1.0;
    let mut offset = 0.0;
    {
        let (o, m) = pairs(shift);
        if o.len() < 30 {
            return None;
        }
        let (so, sm) = (o.iter().sum::<F>(), m.iter().sum::<F>());
        if offset_too {
            let nn = o.len() as F;
            let (smm, som) = (m.iter().map(|x| x * x).sum::<F>(), o.iter().zip(&m).map(|(a, b)| a * b).sum::<F>());
            let det = nn * smm - sm * sm;
            if det.abs() > 1e-300 {
                scale = (nn * som - sm * so) / det;
                offset = (so - scale * sm) / nn;
            }
        } else if sm > 0.0 {
            scale = so / sm;
        }
    }
    let unknowns = if shift_too { 2 } else { 0 } + 1 + if offset_too { 1 } else { 0 };
    for _ in 0..8 {
        let mut h = [[0.0; 4]; 4];
        let mut g = [0.0; 4];
        let mut count = 0;
        for &k in &points {
            let Some((v, dx, dy)) = setup.crop.sample(look.xy[k][0] + shift[0], look.xy[k][1] + shift[1]) else {
                continue;
            };
            if v < cut {
                continue;
            }
            let r = v - scale * model[k] - offset;
            // The residual's derivatives in the unknowns.
            let mut j = [0.0; 4];
            let mut t = 0;
            if shift_too {
                j[0] = dx;
                j[1] = dy;
                t = 2;
            }
            j[t] = -model[k];
            if offset_too {
                j[t + 1] = -1.0;
            }
            for a in 0..unknowns {
                g[a] -= j[a] * r;
                for b in 0..unknowns {
                    h[a][b] += j[a] * j[b];
                }
            }
            count += 1;
        }
        if count < 30 {
            return None;
        }
        for a in 0..unknowns {
            h[a][a] *= 1.0 + 1e-6;
        }
        let step = solve_small(&h, &g, unknowns)?;
        let mut t = 0;
        if shift_too {
            // A pixel at most a step: the bilinear gradient holds near.
            let len = (step[0] * step[0] + step[1] * step[1]).sqrt();
            let f = if len > 1.0 { 1.0 / len } else { 1.0 };
            shift[0] += f * step[0];
            shift[1] += f * step[1];
            t = 2;
        }
        scale += step[t];
        if offset_too {
            offset += step[t + 1];
        }
        let moved = if shift_too { step[0].abs() + step[1].abs() } else { 0.0 };
        if moved < 1e-3 && step[t].abs() < 1e-4 * scale.abs() {
            break;
        }
    }
    (scale > 0.0 && scale.is_finite()).then_some((shift, scale, offset))
}

/// Where the maplet is: where the views' pointing puts it, on average.
///
/// A sideways move of the whole relief, every view's shift following it,
/// changes nothing the images can tell -- and the reference holds nothing
/// sideways at wavelengths beyond the maplet, where the anchor often is. So
/// a whole-pixel slip of the first registration, on a smooth start, stayed:
/// kalast's own renders of a known terrain came back a centimetre aside,
/// every view's shift 1.7 pixels off in the same direction on the ground
/// (`notes/2026-10-14_shape_from_images/`). What the views' shifts share as
/// one translation of the ground, in least squares, moves the maplet
/// instead -- its centre, and the reference with it -- and leaves each view
/// its own correction: the views' given pointing fixes where the maplet is,
/// as a spacecraft's does where its images were taken. After a pass's
/// heights, so that the pass sampled each image where its registration said.
fn follow(work: &mut Maplet, views: &[View], reports: &mut [ViewReport]) {
    let n = work.n;
    let s = work.spacing;
    let middle = work.point(n / 2, n / 2);
    let [u, v, _] = work.axes;
    // Each view's pixels per unit of ground along u and v at the centre.
    let jacobian = |view: &View| -> Option<[[F; 2]; 2]> {
        let (x0, y0, _) = view.project(middle)?;
        let (xu, yu, _) = view.project(middle + u * s)?;
        let (xv, yv, _) = view.project(middle + v * s)?;
        Some([[(xu - x0) / s, (xv - x0) / s], [(yu - y0) / s, (yv - y0) / s]])
    };
    let mut normal = [[0.0; 2]; 2];
    let mut right = [0.0; 2];
    let mut each = vec![None; views.len()];
    for (k, r) in reports.iter().enumerate() {
        let (true, Some(j)) = (r.used, jacobian(&views[k])) else {
            continue;
        };
        let d = r.shift;
        for a in 0..2 {
            for b in 0..2 {
                normal[a][b] += j[0][a] * j[0][b] + j[1][a] * j[1][b];
            }
            right[a] += j[0][a] * d[0] + j[1][a] * d[1];
        }
        each[k] = Some(j);
    }
    let det = normal[0][0] * normal[1][1] - normal[0][1] * normal[1][0];
    if !(det.abs() > 1e-30) {
        return;
    }
    let du = (normal[1][1] * right[0] - normal[0][1] * right[1]) / det;
    let dv = (normal[0][0] * right[1] - normal[1][0] * right[0]) / det;
    if (du * du + dv * dv).sqrt() < 0.01 * s {
        return;
    }
    work.centre += u * du + v * dv;
    work.reference = grid::shifted(&work.reference, n, du / s, dv / s);
    for (r, j) in reports.iter_mut().zip(&each) {
        if let Some(j) = j {
            r.shift[0] -= j[0][0] * du + j[0][1] * dv;
            r.shift[1] -= j[1][0] * du + j[1][1] * dv;
        }
    }
}

/// `h x = g` for the first `n` unknowns, by Gaussian elimination with
/// partial pivoting.
fn solve_small(h: &[[F; 4]; 4], g: &[F; 4], n: usize) -> Option<[F; 4]> {
    let mut a = *h;
    let mut b = *g;
    for c in 0..n {
        let p = (c..n).max_by(|&x, &y| a[x][c].abs().total_cmp(&a[y][c].abs()))?;
        if a[p][c].abs() < 1e-300 {
            return None;
        }
        a.swap(c, p);
        b.swap(c, p);
        for r in c + 1..n {
            let f = a[r][c] / a[c][c];
            for k in c..n {
                a[r][k] -= f * a[c][k];
            }
            b[r] -= f * b[c];
        }
    }
    let mut x = [0.0; 4];
    for c in (0..n).rev() {
        let mut s = b[c];
        for k in c + 1..n {
            s -= a[c][k] * x[k];
        }
        x[c] = s / a[c][c];
    }
    Some(x)
}

/// A view in a pass: where it looks, what it shows, and how it is weighed.
struct Seen<'a> {
    view: &'a View,
    table: &'a Table,
    observed: Vec<F>,
    scale: F,
    offset: F,
    sigma: F,
    weight: F,
}

fn median(mut v: Vec<F>) -> F {
    if v.is_empty() {
        return F::NAN;
    }
    let mid = v.len() / 2;
    v.select_nth_unstable_by(mid, |a, b| a.total_cmp(b));
    v[mid]
}

#[allow(clippy::too_many_arguments)]
fn passes(
    work: &mut Maplet,
    views: &[View],
    law: &Law,
    options: &Options,
    wavelength: F,
    coarse: bool,
    registered: &mut [bool],
    report: &mut Report,
) {
    let n = work.n;
    let s = work.spacing;
    let len = n * n;
    let start: &Maplet = work;
    let setups: Vec<Option<Setup>> = par_map(views.len(), |k| setup(&views[k], start, law, options, coarse));
    if setups.iter().all(Option::is_none) {
        return;
    }
    let [u, v, w] = work.axes;
    for pass in 0..=options.iterations {
        let last = pass == options.iterations;
        let points = work.points();
        let normals = work.normals();
        let now: &Maplet = work;
        let looks: Vec<Option<Look>> =
            par_map(views.len(), |k| setups[k].as_ref().map(|st| look(&views[k], st, now, &points, &normals, options)));
        let model: Vec<Vec<F>> = looks
            .iter()
            .map(|l| match l {
                Some(l) => l
                    .law
                    .iter()
                    .zip(&work.albedo)
                    .zip(&l.lit)
                    .map(|((f, a), &lit)| if lit { f * a } else { 0.0 })
                    .collect(),
                None => vec![],
            })
            .collect();
        // Register each view on the maplet's shading as it now is: the
        // first time it is used, a search by correlation first.
        let shifts: Vec<Option<([F; 2], F, F)>> = par_map(views.len(), |k| {
            let (Some(st), Some(l)) = (setups[k].as_ref(), looks[k].as_ref()) else {
                return None;
            };
            let search = (options.register && !registered[k]).then_some(options.search);
            register(st, l, &model[k], report.views[k].shift, search, options.register, options.offset, options.dark)
        });
        for (k, shift) in shifts.iter().enumerate() {
            registered[k] |= shift.is_some();
        }
        let mut seen: Vec<Seen> = vec![];
        let (mut sum_r, mut sum_o, mut count) = (0.0, 0.0, 0usize);
        for k in 0..views.len() {
            let r = &mut report.views[k];
            r.used = false;
            let (Some(st), Some(l), Some((shift, scale, offset))) = (setups[k].as_ref(), looks[k].as_ref(), shifts[k]) else {
                continue;
            };
            r.pixel = st.pixel;
            let mut observed = observe(st, l, shift);
            let cut = options.dark * median(observed.iter().copied().filter(|o| o.is_finite()).collect());
            if cut.is_finite() {
                for o in observed.iter_mut() {
                    if *o < cut {
                        *o = F::NAN;
                    }
                }
            }
            let (mut o, mut m, mut res) = (vec![], vec![], vec![]);
            for p in 0..len {
                if observed[p].is_finite() {
                    let predicted = scale * model[k][p] + offset;
                    o.push(observed[p]);
                    m.push(predicted);
                    res.push(observed[p] - predicted);
                }
            }
            if o.len() < 30 || o.len() * 20 < len {
                continue;
            }
            let mean_o = o.iter().sum::<F>() / o.len() as F;
            let rms = (res.iter().map(|x| x * x).sum::<F>() / res.len() as F).sqrt();
            let mid = median(res.clone());
            let sigma = (1.4826 * median(res.iter().map(|x| (x - mid).abs()).collect())).max(1e-3 * mean_o.abs()).max(1e-30);
            let c = correlation(&o, &m);
            if r.correlation[0].is_nan() {
                r.correlation[0] = c;
            }
            r.correlation[1] = c;
            r.used = true;
            r.shift = shift;
            r.scale = scale;
            r.offset = offset;
            r.points = o.len();
            r.residual = rms / mean_o.abs().max(1e-30);
            sum_r += res.iter().map(|x| x * x).sum::<F>();
            sum_o += o.iter().map(|x| x * x).sum::<F>();
            count += o.len();
            seen.push(Seen {
                view: &views[k],
                table: &st.table,
                observed,
                scale,
                offset,
                sigma,
                weight: st.weight,
            });
        }
        let residual = if count > 0 { (sum_r / sum_o.max(1e-300)).sqrt() } else { F::NAN };
        report.residual.push(residual);
        report.spacing.push(s);
        if last || seen.is_empty() {
            report.change.push(0.0);
            if options.verbose {
                println!("shape: spacing {s:.4}, {} views, residual {residual:.4} (final)", seen.len());
            }
            break;
        }

        // The slopes and albedo point by point.
        let (p_now, q_now) = grid::slopes(&work.heights, n, s);
        let log_albedo: Vec<F> = work.albedo.iter().map(|a| a.max(1e-6).ln()).collect();
        let around: Vec<F> = (0..len)
            .map(|k| {
                let (i, j) = (k % n, k / n);
                let mut sum = 0.0;
                let mut c = 0.0;
                if i > 0 {
                    sum += log_albedo[k - 1];
                    c += 1.0;
                }
                if i + 1 < n {
                    sum += log_albedo[k + 1];
                    c += 1.0;
                }
                if j > 0 {
                    sum += log_albedo[k - n];
                    c += 1.0;
                }
                if j + 1 < n {
                    sum += log_albedo[k + n];
                    c += 1.0;
                }
                sum / c
            })
            .collect();
        let (wp, wa) = (1.0 / options.slope_sigma.powi(2), 1.0 / options.albedo_sigma.powi(2));
        let solved: Vec<(F, F, F, bool)> = par_map(len, |k| {
            let mut x = [p_now[k], q_now[k], log_albedo[k]];
            let mut any = false;
            for _ in 0..5 {
                let mut h = [[0.0; 4]; 4];
                let mut g = [0.0; 4];
                h[0][0] = wp;
                h[1][1] = wp;
                h[2][2] = wa;
                g[0] = wp * (p_now[k] - x[0]);
                g[1] = wp * (q_now[k] - x[1]);
                g[2] = wa * (around[k] - x[2]);
                let t = w - u * x[0] - v * x[1];
                let tl = t.length();
                let normal = t / tl;
                let dn_dp = (-u + normal * normal.dot(u)) / tl;
                let dn_dq = (-v + normal * normal.dot(v)) / tl;
                let albedo = x[2].exp();
                let mut used = 0;
                for sv in &seen {
                    let o = sv.observed[k];
                    if !o.is_finite() {
                        continue;
                    }
                    let to_camera = (sv.view.position - points[k]).normalize();
                    let (mu0, mu) = (normal.dot(sv.view.sun), normal.dot(to_camera));
                    if mu0 <= 0.02 || mu <= 0.02 {
                        continue;
                    }
                    let alpha = sv.view.sun.dot(to_camera).clamp(-1.0, 1.0).acos();
                    let (f, f0, f1) = sv.table.eval(mu0, mu, alpha);
                    let gain = sv.scale * albedo;
                    let r = o - (gain * f + sv.offset);
                    let z = (r / sv.sigma).abs();
                    let huber = if z <= 2.0 { 1.0 } else { 2.0 / z };
                    let wt = sv.weight * huber / (sv.sigma * sv.sigma);
                    let j = [
                        gain * (f0 * sv.view.sun.dot(dn_dp) + f1 * to_camera.dot(dn_dp)),
                        gain * (f0 * sv.view.sun.dot(dn_dq) + f1 * to_camera.dot(dn_dq)),
                        gain * f,
                    ];
                    for a in 0..3 {
                        g[a] += wt * j[a] * r;
                        for b in 0..3 {
                            h[a][b] += wt * j[a] * j[b];
                        }
                    }
                    used += 1;
                }
                if used == 0 {
                    break;
                }
                any = true;
                let Some(step) = solve_small(&h, &g, 3) else {
                    break;
                };
                x[0] = (x[0] + step[0]).clamp(-4.0, 4.0);
                x[1] = (x[1] + step[1]).clamp(-4.0, 4.0);
                x[2] = (x[2] + step[2]).clamp(-1.5, 1.5);
                if step[0].abs() + step[1].abs() < 1e-6 && step[2].abs() < 1e-6 {
                    break;
                }
            }
            (x[0], x[1], x[2], any)
        });
        let p: Vec<F> = solved.iter().map(|x| x.0).collect();
        let q: Vec<F> = solved.iter().map(|x| x.1).collect();
        // The albedo's median 1 where it was solved for: the views' scales
        // carry the rest.
        let shift = median(solved.iter().filter(|x| x.3).map(|x| x.2).collect());
        let shift = if shift.is_finite() { shift } else { 0.0 };
        for (a, x) in work.albedo.iter_mut().zip(&solved) {
            *a = (x.2 - shift).exp();
        }
        let before = work.heights.clone();
        grid::integrate(&mut work.heights, &p, &q, &work.reference, n, s, wavelength);
        grid::hold_plane(&mut work.heights, &work.reference, n);
        if options.register {
            follow(work, views, &mut report.views);
        }
        let change = (work.heights.iter().zip(&before).map(|(a, b)| (a - b).powi(2)).sum::<F>() / len as F).sqrt();
        report.change.push(change);
        if options.verbose {
            println!(
                "shape: spacing {s:.4}, pass {}, {} views, residual {residual:.4}, heights moved {change:.3e}",
                pass + 1,
                seen.len()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scattering::Hapke;
    use super::super::M3;

    /// Rolling ground with boulders and a crater, `n` points `s` apart.
    fn terrain(n: usize, s: F) -> Vec<F> {
        let c = (n - 1) as F / 2.0;
        let boulders = [(-1.2, 0.8, 0.35, 0.25), (1.5, -1.0, 0.25, 0.2), (0.4, 1.7, 0.3, 0.15), (-0.9, -1.6, 0.2, 0.18)];
        (0..n * n)
            .map(|k| {
                let (x, y) = (((k % n) as F - c) * s, ((k / n) as F - c) * s);
                let mut h = 0.15 * (1.3 * x).sin() * (0.9 * y).cos() + 0.08 * (2.9 * x + 1.7 * y).sin();
                for (bx, by, r, top) in boulders {
                    let d2 = ((x - bx) * (x - bx) + (y - by) * (y - by)) / (r * r);
                    h += top * (-d2).exp();
                }
                let r = ((x - 0.6) * (x - 0.6) + (y + 0.3) * (y + 0.3)).sqrt();
                if r < 0.8 {
                    h += crate::mesh::z_in_crater(r as crate::Float, 0.0, 0.8, 0.15) as F;
                }
                h
            })
            .collect()
    }

    fn smooth(h: &[F], n: usize, sigma: F) -> Vec<F> {
        let r = (3.0 * sigma).ceil() as isize;
        let k: Vec<F> = (-r..=r).map(|x| (-0.5 * (x as F / sigma).powi(2)).exp()).collect();
        let pass = |v: &[F], along_i: bool| -> Vec<F> {
            (0..n * n)
                .map(|p| {
                    let (i, j) = ((p % n) as isize, (p / n) as isize);
                    let (mut s, mut w) = (0.0, 0.0);
                    for (t, kk) in k.iter().enumerate() {
                        let d = t as isize - r;
                        let (a, b) = if along_i { (i + d, j) } else { (i, j + d) };
                        if a >= 0 && b >= 0 && (a as usize) < n && (b as usize) < n {
                            s += kk * v[b as usize * n + a as usize];
                            w += kk;
                        }
                    }
                    s / w
                })
                .collect()
        };
        pass(&pass(h, true), false)
    }

    /// A camera `distance` away along `toward`, looking at the origin, its
    /// image `size` pixels a side of `pixel` at the origin.
    fn camera(toward: V3, distance: F, size: usize, pixel: F, sun: V3, name: &str) -> View {
        let z = -toward.normalize();
        let x = if z.cross(V3::Z).length() > 1e-6 { z.cross(V3::Z).normalize() } else { V3::X };
        let y = z.cross(x);
        View {
            name: name.into(),
            image: vec![0.0; size * size],
            width: size,
            height: size,
            valid: vec![],
            position: toward.normalize() * distance,
            axes: M3::from_cols(x, y, z).transpose(),
            focal: distance / pixel,
            centre: [size as F / 2.0, size as F / 2.0],
            sun: sun.normalize(),
        }
    }

    #[test]
    fn relief_comes_back_from_its_images() {
        let law = Law::Hapke(Hapke {
            w: 0.0385,
            b: 0.276,
            c: 1.0,
            b0: 4.73,
            h: 0.061,
            theta_bar: (24.4 * crate::shape::RPD) as crate::Float,
            k: 1.2,
        });
        let (nt, st) = (161, 0.04);
        let mut truth = Maplet::new(V3::ZERO, V3::Z, nt, st, terrain(nt, st), vec![1.0; nt * nt]).unwrap();
        let c = (nt - 1) as F / 2.0;
        truth.albedo = (0..nt * nt)
            .map(|k| 1.0 + 0.08 * (((k % nt) as F - c) * st * 1.1).sin() * (((k / nt) as F - c) * st * 0.7).cos())
            .collect();
        let deg = crate::shape::RPD;
        let dir = |az: F, el: F| V3::new((az * deg).cos() * (el * deg).cos(), (az * deg).sin() * (el * deg).cos(), (el * deg).sin());
        // Eight views: the camera from above to 30 deg off, the Sun from
        // all round at 35 to 60 deg incidence; each pointed a pixel or two
        // off, as pointing would leave it.
        let mut views = vec![];
        for k in 0..8 {
            let cam = dir(45.0 * k as F + 20.0, 90.0 - 30.0 * ((k % 3) as F / 2.0));
            let sun = dir(45.0 * ((3 * k) % 8) as F, 55.0 - 25.0 * ((k % 2) as F));
            let mut v = camera(cam, 300.0, 200, 0.045, sun, &format!("v{k}"));
            let image = truth.render(&v, &law, 3);
            v.image = image.iter().map(|x| if x.is_finite() { *x } else { 0.0 }).collect();
            v.valid = image.iter().map(|x| x.is_finite() as u8).collect();
            views.push(v);
        }
        // Each pointed a pixel or two off, nothing of it shared as one
        // move of the ground: what is, moves the maplet (`follow`).
        let mut errors = [[1.3, -0.7], [-1.6, 0.4], [0.5, 1.8], [-0.8, -1.2], [1.9, 0.9], [-0.3, -1.9], [1.1, 1.4], [-1.4, 0.2]];
        let jac: Vec<[[F; 2]; 2]> = views
            .iter()
            .map(|v| {
                let (x0, y0, _) = v.project(V3::ZERO).unwrap();
                let (xa, ya, _) = v.project(V3::X).unwrap();
                let (xb, yb, _) = v.project(V3::Y).unwrap();
                [[xa - x0, xb - x0], [ya - y0, yb - y0]]
            })
            .collect();
        let (mut nn, mut rr) = ([[0.0; 2]; 2], [0.0; 2]);
        for (j, e) in jac.iter().zip(&errors) {
            for a in 0..2 {
                for b in 0..2 {
                    nn[a][b] += j[0][a] * j[0][b] + j[1][a] * j[1][b];
                }
                rr[a] += j[0][a] * e[0] + j[1][a] * e[1];
            }
        }
        let det = nn[0][0] * nn[1][1] - nn[0][1] * nn[1][0];
        let common = [(nn[1][1] * rr[0] - nn[0][1] * rr[1]) / det, (nn[0][0] * rr[1] - nn[1][0] * rr[0]) / det];
        for ((v, j), e) in views.iter_mut().zip(&jac).zip(errors.iter_mut()) {
            e[0] -= j[0][0] * common[0] + j[0][1] * common[1];
            e[1] -= j[1][0] * common[0] + j[1][1] * common[1];
            v.centre[0] += e[0];
            v.centre[1] += e[1];
        }
        // From the terrain without its boulders and crater, as a shape
        // model a few times coarser would have it.
        let (n, s) = (81, 0.08);
        let fine_truth = grid::resample(&truth.heights, nt, st, n, s);
        // Smoothed, its plane kept: what a coarse model gets right.
        let c = (n - 1) as F / 2.0;
        let (mut sx, mut sy, mut sxx, mut syy, mut sh, mut sxh, mut syh) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        for (k, h) in fine_truth.iter().enumerate() {
            let (x, y) = ((k % n) as F - c, (k / n) as F - c);
            sx += x;
            sy += y;
            sxx += x * x;
            syy += y * y;
            sh += h;
            sxh += x * h;
            syh += y * h;
        }
        let count = (n * n) as F;
        let (mean, ax, ay) = (sh / count, (sxh - sx * sh / count) / sxx, (syh - sy * sh / count) / syy);
        let plane: Vec<F> = (0..n * n).map(|k| mean + ax * ((k % n) as F - c) + ay * ((k / n) as F - c)).collect();
        let rest: Vec<F> = fine_truth.iter().zip(&plane).map(|(h, p)| h - p).collect();
        let start: Vec<F> = smooth(&rest, n, 4.0).iter().zip(&plane).map(|(r, p)| r + p).collect();
        let mut m = Maplet::new(V3::ZERO, V3::Z, n, s, start.clone(), vec![1.0; n * n]).unwrap();
        let rms = |h: &[F]| {
            let d: Vec<F> = h.iter().zip(&fine_truth).map(|(a, b)| a - b).collect();
            let mean = d.iter().sum::<F>() / d.len() as F;
            (d.iter().map(|x| (x - mean).powi(2)).sum::<F>() / d.len() as F).sqrt()
        };
        let before = rms(&m.heights);
        // The anchor where the start is right: smoothed by 30 cm, a wave of
        // 10 m keeps 98 % of itself.
        let options = Options { iterations: 8, anchor: 10.0, ..Default::default() };
        let report = m.solve(&views, &law, &options).unwrap();
        let after = rms(&m.heights);
        let used = report.views.iter().filter(|v| v.used).count();
        // The registration: each view's error found back -- the camera
        // moved by it, the image by minus it -- to a fraction of a pixel.
        let offs: Vec<F> = report
            .views
            .iter()
            .zip(&errors)
            .map(|(v, e)| ((v.shift[0] + e[0]).powi(2) + (v.shift[1] + e[1]).powi(2)).sqrt())
            .collect();
        eprintln!("heights' rms {before:.4} -> {after:.4}; residual {:?}; shifts off by {offs:.2?}", report.residual);
        assert_eq!(used, views.len(), "{report:?}");
        assert!(offs.iter().all(|&o| o < 0.2), "shifts off by {offs:?}");
        assert!(after < 0.15 * before, "heights' rms {before:.4} -> {after:.4}; {:?}", report.residual);
        let last = *report.residual.last().unwrap();
        assert!(last < 0.2 * report.residual[0], "residual {:?}", report.residual);
    }
}
