//! Heights and albedo fitted to the images directly, cast shadows included.
//!
//! The per-point photoclinometry of [`super::solve`] reads each point's
//! slopes from the views that see it lit, then integrates them: a slope no
//! Sun constrains -- across the Suns' azimuths when they come from one side
//! -- is integrated as firmly as the best constrained, and what a shadow
//! says of the terrain that casts it is lost. Here every view's every point
//! it shows, lit or not, is a residual of the heights themselves:
//!
//! ```text
//! image = scale albedo law(n(h)) lit(h) + offset
//! ```
//!
//! `n` the normal of the heights' central slopes, `lit` a soft step at the
//! shadow's edge: the point's height against what the terrain toward the
//! Sun reaches over it ([`grid::horizon`]), so that raising a point lights
//! it and raising what casts its shadow darkens it. Gauss-Newton on all
//! heights and every point's log albedo at once -- the albedo eliminated
//! point by point, so that what a darker albedo explains no slope takes --
//! with the shape model's long wavelengths held ([`grid::Lowpass`]), a
//! small curvature penalty for what no central slope sees (heights that
//! alternate point to point), and Levenberg-Marquardt damping. By conjugate
//! gradients, the normal matrix never formed: each point's slopes' normal
//! matrix summed over the views, and the residuals at a shadow's edge kept
//! whole.

use super::grid::{self, Lowpass};
use super::maplet::Maplet;
use super::view::{Crop, Table, View};
use super::{par_map, F, V3};

/// A view in a step: the image at each point it shows, lit or in shadow,
/// and how it is weighed.
pub(crate) struct Shown<'a> {
    pub view: &'a View,
    pub table: &'a Table,
    /// The image, smoothed to the spacing, sampled where each point falls
    /// as the heights now have it -- a step is judged where its heights put
    /// the points, not where they were.
    pub crop: &'a Crop,
    /// The registration's shift, pixels.
    pub shift: [F; 2],
    /// The points the view shows -- seen within the emission, lit or not --
    /// as the pass found them.
    pub mask: Vec<bool>,
    pub scale: F,
    pub offset: F,
    /// The residuals' robust scale.
    pub sigma: F,
    pub weight: F,
    /// Darker than this, a point the maplet lights is taken to be in a
    /// shadow it does not cast -- relief beyond its grid, or that it has not
    /// grown yet: its shading tells nothing of its slopes or albedo there.
    pub dark: F,
    /// The image's lit ground, typically: what tells a shadow from light.
    pub lit: F,
    /// The image itself, unsmoothed, for a view much finer than the grid:
    /// its lit cells' two triangles each compared with the mean of the
    /// pixels the triangle covers, where a point would stand for what the
    /// view resolves within it ([`Facet`]).
    pub fine: Option<&'a Crop>,
    /// The triangles' residuals' robust scale.
    pub fine_sigma: F,
}

/// How a step goes about it.
pub(crate) struct Settings {
    /// The largest incidence whose shading is read, radians.
    pub incidence: F,
    /// The prior's standard deviation on the log albedo, toward the
    /// neighbours' mean.
    pub albedo_sigma: F,
    /// The shape model's hold above `anchor`, against the data's typical
    /// stiffness.
    pub anchor: F,
    /// The curvature penalty, against the same.
    pub curvature: F,
    /// The shadow's soft edge, in spacings across the ground: lit from an
    /// eighth to seven eighths over that much of it.
    pub softness: F,
    /// The weight of the shadows' agreement: a point an image shows in
    /// shadow below the terrain's horizon toward the Sun, one it shows lit
    /// above it.
    pub shadows: F,
    /// Below this share of the lit ground, an image shows a shadow.
    pub shadow_level: F,
    /// Above this share, light.
    pub lit_level: F,
    /// The weight of the parallax: a point's height moves where it falls
    /// in each image, and what the image shows there -- stereo.
    pub stereo: F,
}

/// One residual near a shadow's edge, kept whole.
#[derive(Clone, Copy)]
struct Edge {
    k: u32,
    from: u32,
    wt: F,
    r: F,
    jp: F,
    jq: F,
    jsh: F,
    jl: F,
    /// The point's own height, through the parallax.
    jst: F,
}

/// What the views tell a point, their residuals away from shadows' edges
/// summed: its slopes' normal matrix, their coupling to its log albedo; and
/// from every residual the log albedo's diagonal and the gradients.
#[derive(Clone, Copy, Default)]
struct Point {
    mpp: F,
    mpq: F,
    mqq: F,
    cp: F,
    cq: F,
    d: F,
    gp: F,
    gq: F,
    gl: F,
    /// The parallax's: its own square, its coupling to the slopes and the
    /// log albedo, its gradient.
    mss: F,
    msp: F,
    msq: F,
    csl: F,
    gs: F,
    seen: bool,
}

/// The two points a central slope takes, with their coefficients: along
/// `i` (`p`) and along `j` (`q`), one-sided on the edges, as
/// [`grid::slopes`].
#[inline]
fn stencil(k: usize, n: usize, s: F) -> [[(usize, F); 2]; 2] {
    let (i, j) = (k % n, k / n);
    let (a, b) = match i {
        0 => (k, k + 1),
        _ if i == n - 1 => (k - 1, k),
        _ => (k - 1, k + 1),
    };
    let cp = 1.0 / (s * (b - a) as F);
    let (c, d) = match j {
        0 => (k, k + n),
        _ if j == n - 1 => (k - n, k),
        _ => (k - n, k + n),
    };
    let cq = 1.0 / (s * ((d - c) / n) as F);
    [[(a, -cp), (b, cp)], [(c, -cq), (d, cq)]]
}

/// A cell's triangle, as [`Maplet::mesh`] cuts it: `[a, b, d]` and `[a, d,
/// c]`, `a` the cell's corner `(i, j)`, `b` along `i`, `c` along `j`. Its
/// slopes along `i` and `j`, each the difference of two corners; its
/// height for the parallax, its corners' mean; its albedo, `a`'s -- one
/// albedo a cell, so that the albedo is still eliminated point by point.
#[derive(Clone, Copy)]
struct Facet {
    corners: [usize; 3],
    p: [(usize, F); 2],
    q: [(usize, F); 2],
}

fn facets(n: usize, s: F) -> Vec<Facet> {
    let mut out = Vec::with_capacity(2 * (n - 1) * (n - 1));
    let e = 1.0 / s;
    for j in 0..n - 1 {
        for i in 0..n - 1 {
            let a = j * n + i;
            let (b, c, d) = (a + 1, a + n, a + n + 1);
            out.push(Facet { corners: [a, b, d], p: [(a, -e), (b, e)], q: [(b, -e), (d, e)] });
            out.push(Facet { corners: [a, d, c], p: [(c, -e), (d, e)], q: [(a, -e), (c, e)] });
        }
    }
    out
}

/// A fine view's residual on a triangle and its derivatives: in the
/// triangle's slopes, the log albedo of its corner `a`, its height (the
/// parallax). `None` where the view does not show it whole and lit -- a
/// corner hidden, in a shadow or at its edge, the triangle off the valid
/// pixels, too steep to the Sun or dark beyond what the maplet casts.
#[allow(clippy::too_many_arguments)]
fn facet_residual(work: &Maplet, points: &[V3], f: &Facet, sv: &Shown, light: &Light, cos_i: F, stereo: F) -> Option<(F, F, F, F, F)> {
    let fine = sv.fine?;
    let h = &work.heights;
    if !f.corners.iter().all(|&k| sv.mask[k]) {
        return None;
    }
    for &k in &f.corners {
        let (l, dl) = lit(light, k, h[k]);
        if l < 0.99 || (light.from[k] as usize != k && dl * light.tau > 0.01) {
            return None;
        }
    }
    let view = sv.view;
    let [u, v, w] = work.axes;
    let mut tri = [[0.0; 2]; 3];
    for (m, &k) in f.corners.iter().enumerate() {
        let c = view.axes * (points[k] - view.position);
        if c.z <= 0.0 {
            return None;
        }
        tri[m] = [view.centre[0] + view.focal * c.x / c.z + sv.shift[0], view.centre[1] + view.focal * c.y / c.z + sv.shift[1]];
    }
    let (o, gx, gy) = fine.mean_over(tri)?;
    if o < sv.dark {
        return None;
    }
    let p = f.p[0].1 * h[f.p[0].0] + f.p[1].1 * h[f.p[1].0];
    let q = f.q[0].1 * h[f.q[0].0] + f.q[1].1 * h[f.q[1].0];
    let t = w - u * p - v * q;
    let tl = t.length();
    let normal = t / tl;
    let dn_dp = (-u + normal * normal.dot(u)) / tl;
    let dn_dq = (-v + normal * normal.dot(v)) / tl;
    let centroid = (points[f.corners[0]] + points[f.corners[1]] + points[f.corners[2]]) / 3.0;
    let sun = view.sun;
    let to_camera = (view.position - centroid).normalize();
    let (mu0, mu) = (normal.dot(sun), normal.dot(to_camera));
    if mu0 < cos_i || mu <= 0.0 {
        return None;
    }
    let alpha = sun.dot(to_camera).clamp(-1.0, 1.0).acos();
    let (fv, f0, f1) = sv.table.eval(mu0, mu, alpha);
    let gain = sv.scale * work.albedo[f.corners[0]];
    let r = o - (gain * fv + sv.offset);
    let jp = gain * (f0 * sun.dot(dn_dp) + f1 * to_camera.dot(dn_dp));
    let jq = gain * (f0 * sun.dot(dn_dq) + f1 * to_camera.dot(dn_dq));
    // The parallax at the centroid, on the mean's own gradient: the mean
    // over a triangle that moves with its height.
    let c = view.axes * (centroid - view.position);
    let cw = view.axes * w;
    let parallax = [view.focal * (cw.x * c.z - c.x * cw.z) / (c.z * c.z), view.focal * (cw.y * c.z - c.y * cw.z) / (c.z * c.z)];
    let jst = -stereo * (gx * parallax[0] + gy * parallax[1]);
    Some((r, jp, jq, gain * fv, if jst.is_finite() { jst } else { 0.0 }))
}

/// A fine view's triangles' residuals' robust scale, the maplet as it is:
/// its `fine_sigma`. Not finite if it shows none.
pub(crate) fn facet_spread(work: &Maplet, sv: &Shown, set: &Settings) -> F {
    let points = work.points();
    let light = light(work, sv.view.sun, set.softness);
    let cos_i = set.incidence.cos();
    let all = facets(work.n, work.spacing);
    let res: Vec<F> = par_map(all.len(), |f| facet_residual(work, &points, &all[f], sv, &light, cos_i, set.stereo).map(|r| r.0))
        .into_iter()
        .flatten()
        .collect();
    if res.len() < 30 {
        return F::NAN;
    }
    let mut sorted = res.clone();
    sorted.sort_by(F::total_cmp);
    let mid = sorted[sorted.len() / 2];
    let mut dev: Vec<F> = res.iter().map(|r| (r - mid).abs()).collect();
    dev.sort_by(F::total_cmp);
    1.4826 * dev[dev.len() / 2]
}

/// Toward the Sun from each point: what is over it, what casts that, and
/// the soft edge's width in height.
struct Light {
    over: Vec<F>,
    from: Vec<u32>,
    tau: F,
    /// The ray's fall over a spacing of ground.
    rise: F,
}

fn light(work: &Maplet, sun: V3, softness: F) -> Light {
    let d = work.local(sun);
    let (over, from) = grid::horizon(&work.heights, work.n, work.spacing, d);
    let across = (d[0] * d[0] + d[1] * d[1]).sqrt();
    // The ray falls `spacing tan(elevation)` over a spacing of ground: a
    // point on lit level ground is that far over what is toward the Sun,
    // so the edge's width is a fraction of it -- lit ground lit.
    let rise = (work.spacing * d[2] / across.max(1e-9)).max(1e-6 * work.spacing);
    Light { over, from, tau: 0.25 * softness * rise, rise }
}

/// The lit share and its derivative in the point's height.
#[inline]
fn lit(light: &Light, k: usize, h: F) -> (F, F) {
    let over = light.over[k];
    if over == F::NEG_INFINITY {
        return (1.0, 0.0);
    }
    if over == F::INFINITY {
        return (0.0, 0.0);
    }
    let z = ((h - over) / light.tau).clamp(-40.0, 40.0);
    let l = 1.0 / (1.0 + (-z).exp());
    (l, l * (1.0 - l) / light.tau)
}

/// Huber's loss of `z` sigmas, and its weight.
#[inline]
fn huber(z: F) -> (F, F) {
    let a = z.abs();
    if a <= 2.0 {
        (0.5 * z * z, 1.0)
    } else {
        (2.0 * a - 2.0, 2.0 / a)
    }
}

/// The data at the heights and albedo as they are: per point, per
/// triangle of the fine views, and the residuals at shadows' edges; and
/// the data's cost. With `cost_only` the cost alone.
fn linearise(work: &Maplet, shown: &[Shown], lights: &[Light], set: &Settings, cost_only: bool) -> (Vec<Point>, Vec<Point>, Vec<Edge>, F) {
    let n = work.n;
    let len = n * n;
    let points = work.points();
    let (p, q) = grid::slopes(&work.heights, n, work.spacing);
    let [u, v, w] = work.axes;
    let cos_i = set.incidence.cos();
    let chunk = 2048;
    let parts = par_map(len.div_ceil(chunk), |c| {
        let range = c * chunk..((c + 1) * chunk).min(len);
        let mut out = Vec::with_capacity(if cost_only { 0 } else { range.len() });
        let mut edges = vec![];
        let mut cost = 0.0;
        for k in range {
            let t = w - u * p[k] - v * q[k];
            let tl = t.length();
            let normal = t / tl;
            let dn_dp = (-u + normal * normal.dot(u)) / tl;
            let dn_dq = (-v + normal * normal.dot(v)) / tl;
            let albedo = work.albedo[k];
            let h = work.heights[k];
            let mut pt = Point::default();
            for (sv, light) in shown.iter().zip(lights) {
                if !sv.mask[k] {
                    continue;
                }
                // Where the point now falls, and the parallax: how far it
                // moves in the image as it rises along the normal.
                let view = sv.view;
                let c = view.axes * (points[k] - view.position);
                let cw = view.axes * w;
                if c.z <= 0.0 {
                    continue;
                }
                let (x, y) = (view.centre[0] + view.focal * c.x / c.z, view.centre[1] + view.focal * c.y / c.z);
                let parallax = [view.focal * (cw.x * c.z - c.x * cw.z) / (c.z * c.z), view.focal * (cw.y * c.z - c.y * cw.z) / (c.z * c.z)];
                let Some((o, gx, gy)) = sv.crop.sample(x + sv.shift[0], y + sv.shift[1]) else {
                    // Off the valid pixels since the pass began: as a
                    // residual of three sigmas, so that leaving pays nothing.
                    cost += sv.weight * huber(3.0).0;
                    continue;
                };
                let sun = view.sun;
                let to_camera = (view.position - points[k]).normalize();
                let (mu0, mu) = (normal.dot(sun), normal.dot(to_camera));
                let (f, f0, f1) = if mu0 > 0.0 && mu > 0.0 {
                    let alpha = sun.dot(to_camera).clamp(-1.0, 1.0).acos();
                    sv.table.eval(mu0, mu, alpha)
                } else {
                    (0.0, 0.0, 0.0)
                };
                let (l, dl) = lit(light, k, h);
                // A fine view's lit ground is its triangles': the point keeps
                // what they do not see -- its shadows and their edges.
                let from = light.from[k];
                let edge = from as usize != k && dl * light.tau > 0.01;
                let by_facets = sv.fine.is_some() && l >= 0.99 && !edge;
                let gain = sv.scale * albedo;
                let r = o - (gain * f * l + sv.offset);
                let (loss, hub) = huber(r / sv.sigma);
                if !by_facets {
                    cost += sv.weight * loss;
                }
                // The shadows' agreement: a point the image shows in shadow
                // a quarter spacing below the horizon, one it shows lit a
                // quarter above -- what raising the caster or lowering the
                // point does, even far from the maplet's own shadows.
                let over = light.over[k];
                let mut hinge = None;
                if set.shadows > 0.0 && over.is_finite() && from as usize != k {
                    let margin = h - over;
                    let eps = 0.25 * light.rise;
                    let target = if o < set.shadow_level * sv.lit && margin > -eps {
                        Some(-eps)
                    } else if o > set.lit_level * sv.lit && margin < eps {
                        Some(eps)
                    } else {
                        None
                    };
                    if let Some(target) = target {
                        let sm = 0.5 * light.rise;
                        let rh = target - margin;
                        let (lh, hh) = huber(rh / sm);
                        cost += set.shadows * sv.weight * lh;
                        hinge = Some((rh, set.shadows * sv.weight * hh / (sm * sm)));
                    }
                }
                if cost_only {
                    continue;
                }
                if let Some((rh, wh)) = hinge {
                    edges.push(Edge { k: k as u32, from, wt: wh, r: rh, jp: 0.0, jq: 0.0, jsh: 1.0, jl: 0.0, jst: 0.0 });
                }
                if by_facets {
                    continue;
                }
                pt.seen = true;
                let wt = sv.weight * hub / (sv.sigma * sv.sigma);
                // A grazing Sun's shading, or darkness the maplet does not
                // cast, tells nothing of the slopes or the albedo.
                let shading = mu0 >= cos_i && !(o < sv.dark && l > 0.5);
                let (jp, jq, jl) = if shading {
                    (
                        gain * l * (f0 * sun.dot(dn_dp) + f1 * to_camera.dot(dn_dp)),
                        gain * l * (f0 * sun.dot(dn_dq) + f1 * to_camera.dot(dn_dq)),
                        gain * f * l,
                    )
                } else {
                    (0.0, 0.0, 0.0)
                };
                let jsh = gain * f * dl;
                // The image moves under the point as it rises: the residual
                // `image - model` changes by the image's gradient along the
                // parallax.
                let jst = -set.stereo * (gx * parallax[0] + gy * parallax[1]);
                let jst = if jst.is_finite() { jst } else { 0.0 };
                pt.d += wt * jl * jl;
                pt.gp += wt * r * jp;
                pt.gq += wt * r * jq;
                pt.gl += wt * r * jl;
                pt.gs += wt * r * jst;
                if edge {
                    edges.push(Edge { k: k as u32, from, wt, r, jp, jq, jsh, jl, jst });
                } else {
                    pt.mpp += wt * jp * jp;
                    pt.mpq += wt * jp * jq;
                    pt.mqq += wt * jq * jq;
                    pt.cp += wt * jp * jl;
                    pt.cq += wt * jq * jl;
                    pt.mss += wt * jst * jst;
                    pt.msp += wt * jst * jp;
                    pt.msq += wt * jst * jq;
                    pt.csl += wt * jst * jl;
                }
            }
            if !cost_only {
                out.push(pt);
            }
        }
        (out, edges, cost)
    });
    let mut all = Vec::with_capacity(if cost_only { 0 } else { len });
    let mut edges = vec![];
    let mut cost = 0.0;
    for (o, e, c) in parts {
        all.extend(o);
        edges.extend(e);
        cost += c;
    }
    // The fine views' triangles: half a point's weight each, two a cell.
    let fine: Vec<usize> = (0..shown.len()).filter(|&v| shown[v].fine.is_some()).collect();
    let mut tris = vec![];
    if !fine.is_empty() {
        let all_facets = facets(n, work.spacing);
        let parts = par_map(all_facets.len().div_ceil(chunk), |c| {
            let range = c * chunk..((c + 1) * chunk).min(all_facets.len());
            let mut out = Vec::with_capacity(if cost_only { 0 } else { range.len() });
            let mut cost = 0.0;
            for f in range {
                let mut pt = Point::default();
                for &v in &fine {
                    let sv = &shown[v];
                    let Some((r, jp, jq, jl, jst)) = facet_residual(work, &points, &all_facets[f], sv, &lights[v], cos_i, set.stereo) else {
                        continue;
                    };
                    let (loss, hub) = huber(r / sv.fine_sigma);
                    cost += 0.5 * sv.weight * loss;
                    if cost_only {
                        continue;
                    }
                    pt.seen = true;
                    let wt = 0.5 * sv.weight * hub / (sv.fine_sigma * sv.fine_sigma);
                    pt.mpp += wt * jp * jp;
                    pt.mpq += wt * jp * jq;
                    pt.mqq += wt * jq * jq;
                    pt.cp += wt * jp * jl;
                    pt.cq += wt * jq * jl;
                    pt.d += wt * jl * jl;
                    pt.gp += wt * r * jp;
                    pt.gq += wt * r * jq;
                    pt.gl += wt * r * jl;
                    pt.mss += wt * jst * jst;
                    pt.msp += wt * jst * jp;
                    pt.msq += wt * jst * jq;
                    pt.csl += wt * jst * jl;
                    pt.gs += wt * r * jst;
                }
                if !cost_only {
                    out.push(pt);
                }
            }
            (out, cost)
        });
        for (o, c) in parts {
            tris.extend(o);
            cost += c;
        }
    }
    (all, tris, edges, cost)
}

/// The prior's cost and the neighbours' mean of the log albedo.
fn albedo_prior(log: &[F], n: usize, sigma: F) -> (F, Vec<F>) {
    let mut cost = 0.0;
    let around: Vec<F> = (0..n * n)
        .map(|k| {
            let (i, j) = (k % n, k / n);
            let (mut sum, mut c) = (0.0, 0.0);
            for (ok, at) in [(i > 0, k.wrapping_sub(1)), (i + 1 < n, k + 1), (j > 0, k.wrapping_sub(n)), (j + 1 < n, k + n)] {
                if ok {
                    sum += log[at];
                    c += 1.0;
                }
            }
            sum / c
        })
        .collect();
    for (l, a) in log.iter().zip(&around) {
        cost += 0.5 * (l - a).powi(2) / (sigma * sigma);
    }
    (cost, around)
}

/// The regularisation's cost: the shape model's long wavelengths and the
/// curvature.
fn held(h: &[F], reference: &[F], lowpass: &Lowpass, n: usize, mu: F, lambda: F) -> F {
    let d: Vec<F> = h.iter().zip(reference).map(|(a, b)| a - b).collect();
    let g = lowpass.once(&d, n);
    let l = grid::laplacian(h, n);
    0.5 * mu * g.iter().map(|x| x * x).sum::<F>() + 0.5 * lambda * l.iter().map(|x| x * x).sum::<F>()
}

/// What a step did.
pub(crate) struct Step {
    /// The heights' RMS change.
    pub change: F,
    /// The cost before and after.
    pub cost: [F; 2],
    /// Conjugate-gradient iterations, over the tries.
    pub iterations: usize,
}

/// One Gauss-Newton step on the heights and the albedo, damped until the
/// cost falls: `damping` carried from step to step.
pub(crate) fn step(work: &mut Maplet, shown: &[Shown], wavelength: F, set: &Settings, damping: &mut F) -> Step {
    let n = work.n;
    let s = work.spacing;
    let len = n * n;
    let mut report = Step { change: 0.0, cost: [F::NAN; 2], iterations: 0 };
    if shown.is_empty() {
        return report;
    }
    let now: &Maplet = work;
    let lights: Vec<Light> = par_map(shown.len(), |v| light(now, shown[v].view.sun, set.softness));
    let (points, tris, edges, data_cost) = linearise(now, shown, &lights, set, false);
    // The fine views' triangles, if any: their stencils, and which albedo each takes.
    let tri_at = if tris.is_empty() { vec![] } else { facets(n, s) };
    // The data's typical stiffness on the heights: a point's slopes'
    // information over a spacing squared -- a cell's two triangles' together.
    let mut stiff: Vec<F> = points.iter().filter(|p| p.seen).map(|p| 0.5 * (p.mpp + p.mqq) / (s * s)).filter(|x| *x > 0.0).collect();
    stiff.extend(tris.chunks(2).filter(|c| c.iter().any(|p| p.seen)).map(|c| c.iter().map(|p| 0.5 * (p.mpp + p.mqq)).sum::<F>() / (s * s)).filter(|x| *x > 0.0));
    if stiff.is_empty() {
        return report;
    }
    let mid = stiff.len() / 2;
    stiff.select_nth_unstable_by(mid, |a, b| a.total_cmp(b));
    let typical = stiff[mid];
    let (mu, lambda) = (set.anchor * typical, set.curvature * typical);
    let lowpass = Lowpass::new(grid::half_at(wavelength, s), n);
    let reference = grid::fill(&work.reference, n);
    let wa = 1.0 / (set.albedo_sigma * set.albedo_sigma);
    let log: Vec<F> = work.albedo.iter().map(|a| a.max(1e-6).ln()).collect();
    let (prior_cost, around) = albedo_prior(&log, n, set.albedo_sigma);
    let before = data_cost + prior_cost + held(&work.heights, &reference, &lowpass, n, mu, lambda);
    report.cost[0] = before;

    let mut d: Vec<F> = points.iter().map(|p| p.d + wa).collect();
    let mut gl: Vec<F> = points.iter().zip(log.iter().zip(&around)).map(|(p, (l, a))| p.gl + wa * (a - l)).collect();
    for (t, f) in tris.iter().zip(&tri_at) {
        d[f.corners[0]] += t.d;
        gl[f.corners[0]] += t.gl;
    }
    // A triangle's slopes and height from the heights `x`.
    let tri_of = |f: &Facet, x: &[F]| -> (F, F, F) {
        (
            f.p[0].1 * x[f.p[0].0] + f.p[1].1 * x[f.p[1].0],
            f.q[0].1 * x[f.q[0].0] + f.q[1].1 * x[f.q[1].0],
            (x[f.corners[0]] + x[f.corners[1]] + x[f.corners[2]]) / 3.0,
        )
    };
    // And back: `(yp, yq, ys)` on a triangle onto the heights.
    let tri_adjoint = |f: &Facet, yp: F, yq: F, ys: F, y: &mut [F]| {
        y[f.p[0].0] += f.p[0].1 * yp;
        y[f.p[1].0] += f.p[1].1 * yp;
        y[f.q[0].0] += f.q[0].1 * yq;
        y[f.q[1].0] += f.q[1].1 * yq;
        for &c in &f.corners {
            y[c] += ys / 3.0;
        }
    };
    let stencils: Vec<[[(usize, F); 2]; 2]> = (0..len).map(|k| stencil(k, n, s)).collect();

    // `H_ld x` in the slopes' space: `(yp, yq)` and the albedo's `z`, from
    // the heights `x`; then the reduced operator.
    let slopes_of = |x: &[F]| -> (Vec<F>, Vec<F>) {
        let mut dp = vec![0.0; len];
        let mut dq = vec![0.0; len];
        for k in 0..len {
            let [[(a, ca), (b, cb)], [(c, cc), (e, ce)]] = stencils[k];
            dp[k] = ca * x[a] + cb * x[b];
            dq[k] = cc * x[c] + ce * x[e];
        }
        (dp, dq)
    };
    let adjoint = |yp: &[F], yq: &[F], y: &mut [F]| {
        for k in 0..len {
            let [[(a, ca), (b, cb)], [(c, cc), (e, ce)]] = stencils[k];
            y[a] += ca * yp[k];
            y[b] += cb * yp[k];
            y[c] += cc * yq[k];
            y[e] += ce * yq[k];
        }
    };
    // The data's part of the reduced normal matrix, times `x`; and the
    // albedo's coupling to it, `H_lh x`.
    let data = |x: &[F], y: &mut [F], z_out: Option<&mut Vec<F>>| {
        let (dp, dq) = slopes_of(x);
        let mut yp: Vec<F> = (0..len).map(|k| points[k].mpp * dp[k] + points[k].mpq * dq[k] + points[k].msp * x[k]).collect();
        let mut yq: Vec<F> = (0..len).map(|k| points[k].mpq * dp[k] + points[k].mqq * dq[k] + points[k].msq * x[k]).collect();
        let mut z: Vec<F> = (0..len).map(|k| points[k].cp * dp[k] + points[k].cq * dq[k] + points[k].csl * x[k]).collect();
        for k in 0..len {
            let pt = &points[k];
            y[k] = pt.mss * x[k] + pt.msp * dp[k] + pt.msq * dq[k];
        }
        for e in &edges {
            let (k, f) = (e.k as usize, e.from as usize);
            let t = e.jp * dp[k] + e.jq * dq[k] + e.jsh * (x[k] - x[f]) + e.jst * x[k];
            yp[k] += e.wt * e.jp * t;
            yq[k] += e.wt * e.jq * t;
            y[k] += e.wt * (e.jsh + e.jst) * t;
            y[f] -= e.wt * e.jsh * t;
            z[k] += e.wt * e.jl * t;
        }
        let tri_x: Vec<(F, F, F)> = tri_at.iter().map(|f| tri_of(f, x)).collect();
        for ((t, f), &(tp, tq, ts)) in tris.iter().zip(&tri_at).zip(&tri_x) {
            z[f.corners[0]] += t.cp * tp + t.cq * tq + t.csl * ts;
        }
        if let Some(out) = z_out {
            *out = z.clone();
        }
        // Less `H_hl D^-1 H_lh x`.
        let u: Vec<F> = z.iter().zip(&d).map(|(z, d)| z / d).collect();
        for k in 0..len {
            yp[k] -= points[k].cp * u[k];
            yq[k] -= points[k].cq * u[k];
            y[k] -= points[k].csl * u[k];
        }
        for e in &edges {
            let (k, f) = (e.k as usize, e.from as usize);
            let c = e.wt * e.jl * u[k];
            yp[k] -= c * e.jp;
            yq[k] -= c * e.jq;
            y[k] -= c * (e.jsh + e.jst);
            y[f] += c * e.jsh;
        }
        for ((t, f), &(tp, tq, ts)) in tris.iter().zip(&tri_at).zip(&tri_x) {
            let ua = u[f.corners[0]];
            let ap = t.mpp * tp + t.mpq * tq + t.msp * ts - t.cp * ua;
            let aq = t.mpq * tp + t.mqq * tq + t.msq * ts - t.cq * ua;
            let at = t.mss * ts + t.msp * tp + t.msq * tq - t.csl * ua;
            tri_adjoint(f, ap, aq, at, y);
        }
        adjoint(&yp, &yq, y);
    };

    // The right-hand side: the data's gradient less what the albedo takes of
    // it, the shape model's hold and the curvature's.
    let mut rhs = vec![0.0; len];
    {
        let gp: Vec<F> = points.iter().map(|p| p.gp).collect();
        let gq: Vec<F> = points.iter().map(|p| p.gq).collect();
        let mut yp = gp;
        let mut yq = gq;
        for k in 0..len {
            rhs[k] += points[k].gs;
        }
        // (The edges' own parallax gradient is in `gs` already: every
        // residual adds to it.)
        for e in &edges {
            rhs[e.k as usize] += e.wt * e.r * e.jsh;
            rhs[e.from as usize] -= e.wt * e.r * e.jsh;
        }
        let u: Vec<F> = gl.iter().zip(&d).map(|(g, d)| g / d).collect();
        for k in 0..len {
            yp[k] -= points[k].cp * u[k];
            yq[k] -= points[k].cq * u[k];
            rhs[k] -= points[k].csl * u[k];
        }
        for e in &edges {
            let (k, f) = (e.k as usize, e.from as usize);
            let c = e.wt * e.jl * u[k];
            yp[k] -= c * e.jp;
            yq[k] -= c * e.jq;
            rhs[k] -= c * (e.jsh + e.jst);
            rhs[f] += c * e.jsh;
        }
        adjoint(&yp, &yq, &mut rhs);
        for (t, f) in tris.iter().zip(&tri_at) {
            let ua = u[f.corners[0]];
            tri_adjoint(f, t.gp - t.cp * ua, t.gq - t.cq * ua, t.gs - t.csl * ua, &mut rhs);
        }
        let off: Vec<F> = work.heights.iter().zip(&reference).map(|(h, r)| h - r).collect();
        let anchored = lowpass.twice(&off, n);
        let curved = grid::laplacian(&grid::laplacian(&work.heights, n), n);
        for k in 0..len {
            rhs[k] -= mu * anchored[k] + lambda * curved[k];
        }
    }
    // The data's diagonal, for the damping and the preconditioner.
    let mut diag = vec![0.0; len];
    for k in 0..len {
        let pt = &points[k];
        let [sp, sq] = stencils[k];
        for (a, ca) in sp {
            diag[a] += ca * ca * pt.mpp;
        }
        for (c, cc) in sq {
            diag[c] += cc * cc * pt.mqq;
        }
        for (a, ca) in sp {
            for (c, cc) in sq {
                if a == c {
                    diag[a] += 2.0 * ca * cc * pt.mpq;
                }
            }
        }
    }
    for k in 0..len {
        diag[k] += points[k].mss;
    }
    for (t, f) in tris.iter().zip(&tri_at) {
        for (a, ca) in f.p {
            diag[a] += ca * ca * t.mpp;
        }
        for (c, cc) in f.q {
            diag[c] += cc * cc * t.mqq;
        }
        for (a, ca) in f.p {
            for (c, cc) in f.q {
                if a == c {
                    diag[a] += 2.0 * ca * cc * t.mpq;
                }
            }
        }
        for &c in &f.corners {
            diag[c] += t.mss / 9.0;
        }
    }
    for e in &edges {
        let j2 = e.wt * e.jsh * e.jsh;
        diag[e.k as usize] += j2 + e.wt * (e.jst * e.jst + 2.0 * e.jsh * e.jst);
        diag[e.from as usize] += j2;
    }
    let floor = 1e-3 * typical;
    let squares = lowpass.diagonal();

    let old_h = work.heights.clone();
    let old_a = work.albedo.clone();
    for _ in 0..4 {
        let lm = *damping;
        let apply = |x: &[F], y: &mut [F]| {
            data(x, y, None);
            let held = lowpass.twice(x, n);
            let curved = grid::laplacian(&grid::laplacian(x, n), n);
            for k in 0..len {
                y[k] += mu * held[k] + lambda * curved[k] + lm * (diag[k] + floor) * x[k];
            }
        };
        let pre: Vec<F> = (0..len).map(|k| (1.0 + lm) * (diag[k] + floor) + mu * squares + 20.0 * lambda).collect();
        let (dh, its) = cg(&apply, &rhs, &pre, 400, 1e-8);
        report.iterations += its;
        // The albedo that goes with these heights.
        let mut z = vec![];
        let mut scratch = vec![0.0; len];
        data(&dh, &mut scratch, Some(&mut z));
        let dl: Vec<F> = (0..len).map(|k| (gl[k] - z[k]) / d[k]).collect();
        for k in 0..len {
            work.heights[k] = old_h[k] + dh[k];
            work.albedo[k] = (log[k] + dl[k]).clamp(-1.5, 1.5).exp();
        }
        let now: &Maplet = work;
        let lights: Vec<Light> = par_map(shown.len(), |v| light(now, shown[v].view.sun, set.softness));
        let (_, _, _, data_after) = linearise(now, shown, &lights, set, true);
        let log_after: Vec<F> = work.albedo.iter().map(|a| a.ln()).collect();
        let (prior_after, _) = albedo_prior(&log_after, n, set.albedo_sigma);
        let after = data_after + prior_after + held(&work.heights, &reference, &lowpass, n, mu, lambda);
        if after <= before {
            *damping = (*damping / 3.0).max(1e-4);
            report.cost[1] = after;
            report.change = (dh.iter().map(|x| x * x).sum::<F>() / len as F).sqrt();
            return report;
        }
        *damping *= 4.0;
    }
    work.heights = old_h;
    work.albedo = old_a;
    report.cost[1] = before;
    report
}

/// `A x = b` by conjugate gradients from 0, preconditioned by `pre`, the
/// diagonal: at most `max` iterations, until the residual is `tol` of `b`.
fn cg(apply: &dyn Fn(&[F], &mut [F]), b: &[F], pre: &[F], max: usize, tol: F) -> (Vec<F>, usize) {
    let len = b.len();
    let dot = |a: &[F], b: &[F]| a.iter().zip(b).map(|(x, y)| x * y).sum::<F>();
    let mut x = vec![0.0; len];
    let mut r = b.to_vec();
    let mut z: Vec<F> = r.iter().zip(pre).map(|(r, p)| r / p.max(1e-300)).collect();
    let mut d = z.clone();
    let mut rz = dot(&r, &z);
    let goal = tol * dot(b, b).max(1e-300);
    let mut ad = vec![0.0; len];
    for it in 0..max {
        if dot(&r, &r) <= goal {
            return (x, it);
        }
        apply(&d, &mut ad);
        let curvature = dot(&d, &ad);
        if curvature <= 0.0 {
            return (x, it);
        }
        let alpha = rz / curvature;
        for k in 0..len {
            x[k] += alpha * d[k];
            r[k] -= alpha * ad[k];
        }
        for k in 0..len {
            z[k] = r[k] / pre[k].max(1e-300);
        }
        let next = dot(&r, &z);
        let beta = next / rz;
        rz = next;
        for k in 0..len {
            d[k] = z[k] + beta * d[k];
        }
    }
    (x, max)
}
