//! The exact view factor between two triangles.
//!
//! The view factor from a surface 1 to a surface 2 is the share of the radiation leaving 1, diffusely, that reaches 2:
//!
//! ```text
//! F12 = 1/A1  int_A1 int_A2  cos(theta1) cos(theta2) / (pi d^2)  dA2 dA1
//! ```
//!
//! Applying Stokes' theorem twice turns the two surface integrals into two integrals along the outlines (Sparrow
//! 1963; Lagerros 1998, appendix A.1, whose notation this follows):
//!
//! ```text
//! F12 = 1/(2 pi A1)  oint_G1 oint_G2  ln(d)  dr2 . dr1
//! ```
//!
//! each outline taken in the positive sense about its surface's normal. For two polygons this is a sum over pairs of
//! sides, `v1 . v2  int_0^1 int_0^1 ln|r1 - r2| dt1 dt2` with `r = p + v t`. The inner integral has a closed form;
//! the outer one is done numerically, except for two sides on one line (a side two triangles share), where the whole
//! is closed: `(ln a - 3)/2` for a side of squared length `a` shared in full.
//!
//! The formula holds where each surface is in front of the other's plane throughout. A triangle that crosses the
//! other's plane is cut by it first, and only the part in front counts. Nothing here knows of a third surface
//! standing between the two: visibility is another routine's question.
//!
//! The arithmetic is in double precision whatever `Float` is: the logarithms of nearly equal lengths cancel.

use glam::DVec3;

use crate::{Float, Vec3};

/// A triangle's corners, in any frame, counter-clockwise seen from the side its normal points to.
pub type Triangle = [Vec3; 3];

/// The outer integral is refined until two estimates agree to this, relative to the outline's scale.
const TOLERANCE: f64 = 1.0e-11;

/// And no further than this many halvings: a side crossing the other's line leaves a logarithm's kink, which each
/// halving shrinks by about a half times its own logarithm.
const DEPTH: u32 = 40;

/// Nor past this many values of the inner integral for one pair of sides. A kink costs a few hundred; this is
/// what stops the halving when round-off, not the integrand, is what two estimates disagree by.
const BUDGET: usize = 20_000;

/// `F(a -> b)`: the share of the radiation leaving triangle `a` that reaches triangle `b`, each seen from its
/// front, with nothing between them.
///
/// Zero when either is degenerate, when one is wholly behind the other, and for two triangles in one plane.
pub fn exact(a: &Triangle, b: &Triangle) -> Float {
    let pa: Vec<DVec3> = a.iter().map(|&p| double(p)).collect();
    let pb: Vec<DVec3> = b.iter().map(|&p| double(p)).collect();
    exact_polygons(&pa, &pb) as Float
}

/// `exact` for two convex plane polygons given in double precision.
pub fn exact_polygons(a: &[DVec3], b: &[DVec3]) -> f64 {
    let (Some((na, area_a)), Some((nb, _))) = (plane(a), plane(b)) else {
        return 0.0;
    };
    // what is behind the other's plane neither sends to it nor receives from it
    let scale = a.iter().chain(b).fold(0.0_f64, |m, p| m.max((*p - a[0]).length()));
    let eps = 1.0e-12 * scale.max(f64::MIN_POSITIVE);
    let ca = clip(a, b[0], nb, eps);
    let cb = clip(b, a[0], na, eps);
    if plane(&ca).is_none() || plane(&cb).is_none() {
        return 0.0;
    }
    let mut sum = 0.0;
    for k in 0..ca.len() {
        let (p1, v1) = (ca[k], ca[(k + 1) % ca.len()] - ca[k]);
        if v1.length_squared() == 0.0 {
            continue;
        }
        for l in 0..cb.len() {
            let (p2, v2) = (cb[l], cb[(l + 1) % cb.len()] - cb[l]);
            let dot = v1.dot(v2);
            // sides at a right angle add nothing, whatever the integral
            if v2.length_squared() == 0.0 || dot.abs() <= 1.0e-15 * v1.length() * v2.length() {
                continue;
            }
            sum += dot * sides(p1, v1, p2, v2, scale);
        }
    }
    // the outlines run in the positive sense about normals that face each other: the sides nearest each other
    // then run opposite ways, and the far ones, where ln(d) is larger, the same way, which makes the sum positive
    (sum / (2.0 * std::f64::consts::PI * area_a)).max(0.0)
}

fn double(p: Vec3) -> DVec3 {
    DVec3::new(p.x as f64, p.y as f64, p.z as f64)
}

/// A polygon's unit normal and area, or nothing for one without area.
fn plane(p: &[DVec3]) -> Option<(DVec3, f64)> {
    if p.len() < 3 {
        return None;
    }
    let mut n = DVec3::ZERO;
    for k in 1..p.len() - 1 {
        n += (p[k] - p[0]).cross(p[k + 1] - p[0]);
    }
    let len = n.length();
    let scale = p.iter().fold(0.0_f64, |m, q| m.max((*q - p[0]).length_squared()));
    (len > 1.0e-14 * scale && len > 0.0).then(|| (n / len, 0.5 * len))
}

/// The part of a convex polygon in front of the plane through `origin` with `normal` (Sutherland and Hodgman's
/// cut), points within `eps` of the plane counting as on it.
fn clip(p: &[DVec3], origin: DVec3, normal: DVec3, eps: f64) -> Vec<DVec3> {
    let height = |q: DVec3| {
        let h = (q - origin).dot(normal);
        if h.abs() <= eps { 0.0 } else { h }
    };
    // a polygon lying in the plane is seen edge on: it has no part in front
    if !p.iter().any(|&q| height(q) > 0.0) {
        return Vec::new();
    }
    if p.iter().all(|&q| height(q) >= 0.0) {
        return p.to_vec();
    }
    let mut out = Vec::with_capacity(p.len() + 1);
    for k in 0..p.len() {
        let (q, r) = (p[k], p[(k + 1) % p.len()]);
        let (hq, hr) = (height(q), height(r));
        if hq >= 0.0 {
            out.push(q);
        }
        if (hq > 0.0 && hr < 0.0) || (hq < 0.0 && hr > 0.0) {
            out.push(q + (r - q) * (hq / (hq - hr)));
        }
    }
    out
}

/// `int_0^1 int_0^1 ln|r1 - r2| dt1 dt2` for the sides `r1 = p1 + v1 t1` and `r2 = p2 + v2 t2`.
fn sides(p1: DVec3, v1: DVec3, p2: DVec3, v2: DVec3, scale: f64) -> f64 {
    let (l1, l2) = (v1.length(), v2.length());
    let u = v1 / l1;
    // two sides on one line: closed, with F'' = ln|x|
    let off = |q: DVec3| ((q - p1) - u * (q - p1).dot(u)).length();
    if off(p2) <= 1.0e-12 * scale && off(p2 + v2) <= 1.0e-12 * scale {
        let (c, d) = ((p2 - p1).dot(u), (p2 + v2 - p1).dot(u));
        let f = |x: f64| if x == 0.0 { 0.0 } else { 0.5 * x * x * x.abs().ln() - 0.75 * x * x };
        // int_0^l1 int_c^d ln|x - y| dy dx, with y running from c to d as t2 runs from 0 to 1
        let whole = f(l1 - c) - f(-c) - f(l1 - d) + f(-d);
        let sign = if d >= c { 1.0 } else { -1.0 };
        return sign * whole / (l1 * l2);
    }
    let inner = |t2: f64| along(p1, v1, p2 + v2 * t2);
    let left = std::cell::Cell::new(BUDGET);
    adaptive(&inner, 0.0, 1.0, TOLERANCE * (1.0 + scale.ln().abs()), DEPTH, &left)
}

/// `int_0^1 ln|p + v t - q| dt`: Lagerros's inner integral, written about the foot of `q` on the side's line so that
/// it holds on the line too (his equation A1), where `x ln x` goes to zero.
fn along(p: DVec3, v: DVec3, q: DVec3) -> f64 {
    let a = v.length_squared();
    let d = p - q;
    let s0 = d.dot(v) / a; // the side runs from s0 to s0 + 1 in units of its length, the foot at 0
    // the distance to the line from what is left of d beside it: as a difference of squares it is noise for a
    // point almost on the line, and that noise, times the arc tangent, is what the integrator then chases
    let h2 = (d - v * s0).length_squared() / a;
    let h = h2.sqrt();
    let term = |s: f64| {
        let r2 = s * s + h2;
        let log = if r2 > 0.0 { s * r2.ln() } else { 0.0 };
        let turn = if h > 0.0 { 2.0 * h * (s / h).atan() } else { 0.0 };
        log - 2.0 * s + turn
    };
    0.5 * a.ln() + 0.5 * (term(s0 + 1.0) - term(s0))
}

/// Gauss and Kronrod's 7 and 15 points on `[lo, hi]`, halved where the two disagree, while `left` values remain.
fn adaptive(f: &dyn Fn(f64) -> f64, lo: f64, hi: f64, tolerance: f64, depth: u32, left: &std::cell::Cell<usize>) -> f64 {
    const XK: [f64; 8] = [
        0.991_455_371_120_812_6,
        0.949_107_912_342_758_5,
        0.864_864_423_359_769_1,
        0.741_531_185_599_394_4,
        0.586_087_235_467_691_1,
        0.405_845_151_377_397_2,
        0.207_784_955_007_898_47,
        0.0,
    ];
    const WK: [f64; 8] = [
        0.022_935_322_010_529_224,
        0.063_092_092_629_978_55,
        0.104_790_010_322_250_18,
        0.140_653_259_715_525_92,
        0.169_004_726_639_267_9,
        0.190_350_578_064_785_4,
        0.204_432_940_075_298_9,
        0.209_482_141_084_727_82,
    ];
    const WG: [f64; 4] = [
        0.129_484_966_168_869_7,
        0.279_705_391_489_276_7,
        0.381_830_050_505_118_93,
        0.417_959_183_673_469_4,
    ];
    let (mid, half) = (0.5 * (lo + hi), 0.5 * (hi - lo));
    let centre = f(mid);
    let (mut kronrod, mut gauss) = (WK[7] * centre, WG[3] * centre);
    for i in 0..7 {
        let pair = f(mid - half * XK[i]) + f(mid + half * XK[i]);
        kronrod += WK[i] * pair;
        if i % 2 == 1 {
            gauss += WG[i / 2] * pair;
        }
    }
    let (kronrod, gauss) = (kronrod * half, gauss * half);
    left.set(left.get().saturating_sub(15));
    if depth == 0 || left.get() == 0 || (kronrod - gauss).abs() <= tolerance * (hi - lo) {
        return kronrod;
    }
    adaptive(f, lo, mid, tolerance, depth - 1, left) + adaptive(f, mid, hi, tolerance, depth - 1, left)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn measured() -> bool {
        std::env::var_os("KALAST_MEASURE").is_some()
    }

    fn tri(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> Vec<DVec3> {
        vec![DVec3::from_array(a), DVec3::from_array(b), DVec3::from_array(c)]
    }

    /// A rectangle `origin, origin + e1, origin + e1 + e2, origin + e2` as two triangles, normal `e1 x e2`.
    fn rectangle(origin: DVec3, e1: DVec3, e2: DVec3) -> [Vec<DVec3>; 2] {
        let (p0, p1, p2, p3) = (origin, origin + e1, origin + e1 + e2, origin + e2);
        [vec![p0, p1, p2], vec![p0, p2, p3]]
    }

    fn area(p: &[DVec3]) -> f64 {
        plane(p).map_or(0.0, |(_, a)| a)
    }

    /// From a set of triangles to another, by the factors of their triangles.
    fn between(from: &[Vec<DVec3>], to: &[Vec<DVec3>]) -> f64 {
        let total: f64 = from.iter().map(|t| area(t)).sum();
        from.iter().map(|s| area(s) * to.iter().map(|t| exact_polygons(s, t)).sum::<f64>()).sum::<f64>() / total
    }

    /// Two equal rectangles `x` by `y`, directly opposed, `c` apart (the closed form in the heat-transfer texts).
    fn opposed(x: f64, y: f64, c: f64) -> f64 {
        let (x, y) = (x / c, y / c);
        let (x1, y1) = ((1.0 + x * x).sqrt(), (1.0 + y * y).sqrt());
        2.0 / (std::f64::consts::PI * x * y)
            * (((1.0 + x * x) * (1.0 + y * y) / (1.0 + x * x + y * y)).sqrt().ln()
                + x * y1 * (x / y1).atan()
                + y * x1 * (y / x1).atan()
                - x * x.atan()
                - y * y.atan())
    }

    /// Two rectangles at a right angle sharing a side of length `l`: from the one `w` wide to the one `h` high.
    fn hinged(w: f64, h: f64, l: f64) -> f64 {
        let (w, h) = (w / l, h / l);
        let (w2, h2) = (w * w, h * h);
        let s = w2 + h2;
        (w * (1.0 / w).atan() + h * (1.0 / h).atan() - s.sqrt() * (1.0 / s.sqrt()).atan()
            + 0.25
                * (((1.0 + w2) * (1.0 + h2) / (1.0 + s)).ln()
                    + w2 * (w2 * (1.0 + s) / ((1.0 + w2) * s)).ln()
                    + h2 * (h2 * (1.0 + s) / ((1.0 + h2) * s)).ln()))
            / (std::f64::consts::PI * w)
    }

    #[test]
    fn the_inner_integral_is_the_integral() {
        // against a plain sum, off the line and on it
        let (p, v) = (DVec3::new(0.2, -0.1, 0.3), DVec3::new(1.5, 0.4, -0.2));
        for q in [DVec3::new(0.7, 0.9, -0.4), p + v * 0.3, p - v * 0.5, p, p + v] {
            let n = 100_000;
            let sum: f64 = (0..n).map(|i| (p + v * ((i as f64 + 0.5) / n as f64) - q).length().ln()).sum::<f64>() / n as f64;
            let got = along(p, v, q);
            assert!((got - sum).abs() < 2.0e-5, "{got} against {sum} at {q}");
        }
    }

    #[test]
    fn opposed_squares_have_their_closed_form() {
        for (x, y, c) in [(1.0, 1.0, 1.0), (1.0, 1.0, 0.1), (2.0, 0.5, 3.0), (1.0, 1.0, 20.0)] {
            let low = rectangle(DVec3::ZERO, DVec3::X * x, DVec3::Y * y); // normal +z
            let high = rectangle(DVec3::new(0.0, 0.0, c), DVec3::Y * y, DVec3::X * x); // normal -z
            let (got, want) = (between(&low, &high), opposed(x, y, c));
            if measured() {
                println!("opposed {x} x {y} at {c}: {got:.12} closed form {want:.12} difference {:.1e}", got - want);
            }
            assert!((got - want).abs() < 1.0e-9, "{got} against {want}");
        }
        assert!((opposed(1.0, 1.0, 1.0) - 0.199_825).abs() < 1.0e-6);
    }

    #[test]
    fn hinged_squares_have_their_closed_form() {
        // the side they share, and the corners on it, are where the centroid formula fails
        for (w, h, l) in [(1.0, 1.0, 1.0), (0.3, 2.0, 1.0), (1.0, 1.0, 5.0)] {
            let floor = rectangle(DVec3::ZERO, DVec3::X * l, DVec3::Y * w); // normal +z, hinge along x at y = 0
            let wall = rectangle(DVec3::ZERO, DVec3::Z * h, DVec3::X * l); // normal +y
            let (got, want) = (between(&floor, &wall), hinged(w, h, l));
            if measured() {
                println!("hinged {w} wide, {h} high, {l} long: {got:.12} closed form {want:.12} difference {:.1e}", got - want);
            }
            assert!((got - want).abs() < 1.0e-8, "{got} against {want}");
        }
        assert!((hinged(1.0, 1.0, 1.0) - 0.200_044).abs() < 1.0e-6);
    }

    #[test]
    fn a_closed_box_sends_everything_to_itself() {
        // six faces looking inward: every triangle's factors to the eleven others add up to one
        let (x, y, z) = (DVec3::X * 1.3, DVec3::Y * 0.8, DVec3::Z * 2.1);
        let o = DVec3::new(-0.4, 0.2, 0.1);
        let faces = [
            rectangle(o, x, y),
            rectangle(o + z, y, x),
            rectangle(o, z, x),
            rectangle(o + y, x, z),
            rectangle(o, y, z),
            rectangle(o + x, z, y),
        ];
        let all: Vec<&Vec<DVec3>> = faces.iter().flatten().collect();
        let mut worst = 0.0_f64;
        for (i, a) in all.iter().enumerate() {
            let sum: f64 = all.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, b)| exact_polygons(a, b)).sum();
            worst = worst.max((sum - 1.0).abs());
            // and what a sends to b is what b sends to a, by their areas
            for b in &all {
                let (ab, ba) = (area(a) * exact_polygons(a, b), area(b) * exact_polygons(b, a));
                assert!((ab - ba).abs() < 1.0e-9, "reciprocity: {ab} against {ba}");
            }
        }
        if measured() {
            println!("closed box: the sums are within {worst:.1e} of one");
        }
        assert!(worst < 1.0e-8, "the sums miss one by {worst}");
    }

    #[test]
    fn far_apart_it_is_the_formula_of_two_points() {
        let a = tri([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        let shift = DVec3::new(30.0, -12.0, 80.0);
        let b = vec![a[0] + shift, a[2] + shift, a[1] + shift]; // normal -z, facing down to a
        let (ca, cb) = ((a[0] + a[1] + a[2]) / 3.0, (b[0] + b[1] + b[2]) / 3.0);
        let d = cb - ca;
        let cos_a = d.normalize().dot(DVec3::Z);
        let cos_b = (-d).normalize().dot(-DVec3::Z);
        let points = cos_a * cos_b / (std::f64::consts::PI * d.length_squared()) * area(&b);
        let got = exact_polygons(&a, &b);
        if measured() {
            println!("far apart: {got:.6e} two points {points:.6e} ratio {:.6}", got / points);
        }
        assert!((got / points - 1.0).abs() < 1.0e-3);
    }

    #[test]
    fn behind_in_one_plane_or_without_area_is_zero_and_a_cut_triangle_counts_in_part() {
        let a = tri([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]); // normal +z
        // behind a
        let below = tri([0.0, 0.0, -1.0], [1.0, 0.0, -1.0], [0.0, 1.0, -1.0]);
        assert_eq!(exact_polygons(&a, &below), 0.0);
        // above a but turned away
        let away = tri([0.0, 0.0, 1.0], [1.0, 0.0, 1.0], [0.0, 1.0, 1.0]);
        assert_eq!(exact_polygons(&a, &away), 0.0);
        // in a's plane
        let beside = tri([2.0, 0.0, 0.0], [3.0, 0.0, 0.0], [2.0, 1.0, 0.0]);
        assert_eq!(exact_polygons(&a, &beside), 0.0);
        // no area
        let line = tri([0.0, 0.0, 1.0], [1.0, 0.0, 1.0], [2.0, 0.0, 1.0]);
        assert_eq!(exact_polygons(&a, &line), 0.0);
        assert_eq!(exact_polygons(&line, &a), 0.0);
        // a wall through a's plane: only its part above counts, which is the wall cut by hand
        let wall = vec![DVec3::new(0.2, 2.0, -1.0), DVec3::new(0.8, 2.0, -1.0), DVec3::new(0.8, 2.0, 1.5), DVec3::new(0.2, 2.0, 1.5)];
        let above = vec![DVec3::new(0.2, 2.0, 0.0), DVec3::new(0.8, 2.0, 0.0), DVec3::new(0.8, 2.0, 1.5), DVec3::new(0.2, 2.0, 1.5)];
        let (cut, by_hand) = (exact_polygons(&a, &wall), exact_polygons(&a, &above));
        assert!(by_hand > 1.0e-3 && (cut - by_hand).abs() < 1.0e-12, "{cut} against {by_hand}");
        // and it is what a fine subdivision of the two gives
        let n = 40;
        let mut sum = 0.0;
        for i in 0..n {
            for j in 0..n - i {
                let p = DVec3::new((i as f64 + 1.0 / 3.0) / n as f64, (j as f64 + 1.0 / 3.0) / n as f64, 0.0);
                for (k, l) in (0..30).flat_map(|k| (0..30).map(move |l| (k, l))) {
                    let q = DVec3::new(0.2 + 0.6 * (k as f64 + 0.5) / 30.0, 2.0, 1.5 * (l as f64 + 0.5) / 30.0);
                    let d = q - p;
                    sum += (d.z / d.length()) * (d.y / d.length()) / (std::f64::consts::PI * d.length_squared())
                        * (0.6 * 1.5 / 900.0)
                        * (0.5 / (n * n) as f64);
                }
            }
        }
        // the lower triangles of the subdivision only: half of a's area is sampled, the factor is the same to 1e-3
        let sampled = sum / (0.5 * (n * (n + 1) / 2) as f64 / (n * n) as f64);
        assert!((sampled / by_hand - 1.0).abs() < 1.0e-2, "{sampled} against {by_hand}");
    }

    #[test]
    fn corners_that_miss_each_other_by_a_rounding_do_not_stall_it() {
        // a box whose top was built from other sums than its walls, in single precision: the sides that should be
        // shared lie 2e-7 apart, on lines that are almost one. It must end, and still close to one part in 1e5.
        let f = |v: f32| v as f64;
        let (ox, oy, oz) = (-0.4_f32, 0.2_f32, 0.1_f32);
        let (x, y, z) = (1.3_f32, 0.8_f32, 2.1_f32);
        let top = (0.1_f64 + 2.1_f64) as f32; // the sum rounded once, where the walls add two rounded numbers
        assert!(top != oz + z, "the two sums are the same on this machine: the test tests nothing");
        let c = |i: f32, j: f32, k: f32| DVec3::new(f(i), f(j), f(k));
        let rect = |p0: DVec3, p1: DVec3, p2: DVec3, p3: DVec3| [vec![p0, p1, p2], vec![p0, p2, p3]];
        let (x1, y1, z1) = (ox + x, oy + y, oz + z);
        let faces = [
            rect(c(ox, oy, oz), c(x1, oy, oz), c(x1, y1, oz), c(ox, y1, oz)),
            rect(c(ox, oy, top), c(ox, y1, top), c(x1, y1, top), c(x1, oy, top)),
            rect(c(ox, oy, oz), c(ox, oy, z1), c(x1, oy, z1), c(x1, oy, oz)),
            rect(c(ox, y1, oz), c(x1, y1, oz), c(x1, y1, z1), c(ox, y1, z1)),
            rect(c(ox, oy, oz), c(ox, y1, oz), c(ox, y1, z1), c(ox, oy, z1)),
            rect(c(x1, oy, oz), c(x1, oy, z1), c(x1, y1, z1), c(x1, y1, oz)),
        ];
        let all: Vec<&Vec<DVec3>> = faces.iter().flatten().collect();
        let started = std::time::Instant::now();
        let mut worst = 0.0_f64;
        for (i, a) in all.iter().enumerate() {
            let sum: f64 = all.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, b)| exact_polygons(a, b)).sum();
            worst = worst.max((sum - 1.0).abs());
        }
        if measured() {
            println!("a box closed to a rounding: the sums within {worst:.1e} of one, in {:.1} s", started.elapsed().as_secs_f64());
        }
        assert!(worst < 1.0e-5, "the sums miss one by {worst}");
        assert!(started.elapsed().as_secs() < 120, "{} s", started.elapsed().as_secs());
    }

    #[test]
    fn single_precision_triangles_give_the_same() {
        let a: Triangle = [Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0), Vec3::new(1.0, 1.0, 0.0)];
        let b: Triangle = [Vec3::new(0.0, 0.0, 1.0), Vec3::new(0.0, 1.0, 1.0), Vec3::new(1.0, 1.0, 1.0)];
        let da: Vec<DVec3> = a.iter().map(|&p| double(p)).collect();
        let db: Vec<DVec3> = b.iter().map(|&p| double(p)).collect();
        assert_eq!(exact(&a, &b), exact_polygons(&da, &db) as Float);
        assert!(exact(&a, &b) > 0.05);
    }
}
