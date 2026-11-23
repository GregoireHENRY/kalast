//! Each image's pointing, from its registrations in many maplets.
//!
//! A maplet's solve registers every image to the maplet's shading: the
//! shift that puts the ground where the image shows it. That shift is not
//! the image's pointing alone. The maplet's own place is off too -- the
//! shape model's position error there, and whatever `follow` took for it --
//! and that shows in each image as its projection of a move on the ground:
//!
//! ```text
//! s[k, t] = P[k] + J[k, t] o[t]
//! ```
//!
//! `P[k]` the image's pointing, in pixels; `o[t]` the maplet's offset on the
//! ground along its `u` and `v` -- and along its normal `w`, its height,
//! when asked: what the images' parallax tells, each seeing a move along `w`
//! as its own move in the image, and the stereo of SPC's landmarks when
//! the surface under the maplets is a first guess; `J[k, t]` the image's
//! pixels per unit of ground there. An image is in dozens of maplets and a maplet has dozens of
//! images, so both are fitted to every shift at once -- what SPC, ASP and
//! MPCD do outside the shape fit: an image's pointing from all of the
//! surface, not from one patch of it. Solved again with each image's
//! pointing held at the fit and a few pixels searched, a maplet is no longer
//! at the mercy of a wrong lock on its own: on comet 67P's 6,489 tiles the
//! held-out correlation went from 0.504 to 0.581
//! (`notes/2026-10-16_67p_second_pass/`).
//!
//! Alternating: each maplet's offset by weighted least squares, held toward
//! zero where few images tie it (`offset_sigma`); each image's pointing the
//! median of what its maplets leave, each component. Each shift weighs as
//! the caller says and as a Huber loss allows -- a wrong lock is tens of
//! pixels, and a squared loss would let one move an image's pointing.
//!
//! An image whose attitude is only the plan's is off about its boresight
//! too, a degree or so: its roll turns every maplet about the image's
//! middle, a shift growing with the distance from it --
//!
//! ```text
//! s[k, t] = P[k] + R[k] (q[k, t].y, -q[k, t].x) + J[k, t] o[t]
//! ```
//!
//! `q` the maplet's pixel from the middle, `R[k]` the roll, radians, what
//! turning the camera's axes about their `z` by it does (`Rz(R) axes`, rows
//! `x`, `y`). Freed by the caller image by image, `R[k]` is fitted with
//! `P[k]` by weighted least squares, the weights the Huber loss leaves; the
//! images whose attitude is measured keep `R` at zero and the median.
//!
//! A body whose rotation model is uncertain is turned in each image from
//! where its model puts it: a small turn `d[k]` about the body's origin (a rotation vector, the
//! surface's frame) moves the maplet at `c[t]` by `d[k] x c[t]`, and its
//! pixel by the projection of that move --
//!
//! ```text
//! s[k, t] = P[k] + D[k, t] d[k] + J[k, t] o[t]
//! ```
//!
//! fitted with `P[k]` the same way, for the images the caller frees. The
//! maplets' offsets, held by `offset_sigma`, keep the body's frame: every
//! image turned one way and every maplet the other would explain the shifts
//! alike.
//!
//! An image registered in a handful of maplets cannot tell three angles of
//! turn from its shift. A body turns smoothly, though: the images of one
//! session (a group, the caller's) share a turn and its rate, `d[k] = a[g] +
//! (t[k] - t[g]) b[g]` (`t[g]` the session's mean time), fitted with each
//! image's shift from all the session's registrations at once.

use super::maplet::frame;
use super::{F, M3, V3};

/// A camera's geometry, the part of a [`View`](super::View) the pointing
/// needs: where, turned how (rows `x`, `y`, `z`), its focal length in
/// pixels.
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub position: V3,
    pub axes: M3,
    pub focal: F,
}

/// Where image `image`'s registration put it in maplet `tile`: the shift,
/// pixels, as [`ViewReport::shift`](super::ViewReport), and its weight.
#[derive(Clone, Copy, Debug)]
pub struct Registration {
    pub image: usize,
    pub tile: usize,
    pub shift: [F; 2],
    pub weight: F,
}

/// What the shifts say.
#[derive(Clone, Debug)]
pub struct Pointing {
    /// Each image's pointing, pixels: added to where its boresight falls,
    /// its projection is where its maplets put it.
    pub images: Vec<[F; 2]>,
    /// How many registrations each image's pointing rests on.
    pub behind: Vec<usize>,
    /// Each maplet's offset along its `u`, `v` and `w`, in the surface's
    /// unit; `w` 0 unless `height_sigma` is given.
    pub offsets: Vec<[F; 3]>,
    /// Each image's roll, radians: its axes turned about their `z` by it
    /// (`Rz(roll) axes`) put its maplets where it sees them, after
    /// `images`' shift; 0 for the images whose roll was not freed.
    pub rolls: Vec<F>,
    /// Each image's turn of the body, a rotation vector (radians, the
    /// surface's frame) about its origin: the body turned by it puts the
    /// maplets where the image sees them, after `images`' shift -- for the
    /// caller, the camera turned the other way about the origin (position
    /// `Rᵀ p`, axes `A R`, `R` the rotation of the vector); 0 for the images
    /// whose turn was not freed.
    pub turns: Vec<[F; 3]>,
    /// Each registration's residual, pixels.
    pub residuals: Vec<F>,
}

/// The pointing of `cameras` and the offsets of the maplets at `tiles`
/// (centre, normal -- the maplet's frame from it as
/// [`Maplet::from_surface`](super::Maplet::from_surface) lays it) that
/// explain `registrations`. `offset_sigma`, in the surface's unit: how far a
/// maplet may move for one pixel of residual -- a maplet few images tie
/// stays near where it was. `huber`, pixels: past it a residual weighs as
/// its square root. `height_sigma`, if positive, frees each maplet's height
/// as well, held so to zero. `fixed`, if not empty, a flag a camera: those
/// set keep their pointing, 0 -- images whose pointing is known, from their
/// limbs, say -- and hold the maplets where they are on the whole: without
/// them, every maplet moved one way and every image's pointing the other
/// explain the shifts alike, and only `offset_sigma` holds them. `roll`, if
/// not empty, a flag a camera: those set get their roll about the boresight
/// fitted too -- the images whose attitude is the plan's. `turn`, if not
/// empty, a flag a camera: those set get the body's turn about its origin
/// fitted too (a body whose rotation is not known); not with `roll` for the
/// same camera. `group` and `time`, if not empty, a number a camera each: the
/// freed cameras of one group (0 or more; -1, none) share a turn and its
/// rate, `time` in hours.
#[allow(clippy::too_many_arguments)]
pub fn pointing(
    cameras: &[Camera],
    tiles: &[(V3, V3)],
    registrations: &[Registration],
    offset_sigma: F,
    huber: F,
    iterations: usize,
    height_sigma: F,
    fixed: &[bool],
    roll: &[bool],
    turn: &[bool],
    group: &[i64],
    time: &[F],
) -> Result<Pointing, String> {
    if !fixed.is_empty() && fixed.len() != cameras.len() {
        return Err(format!("{} flags for {} cameras", fixed.len(), cameras.len()));
    }
    if !roll.is_empty() && roll.len() != cameras.len() {
        return Err(format!("{} roll flags for {} cameras", roll.len(), cameras.len()));
    }
    if !turn.is_empty() && turn.len() != cameras.len() {
        return Err(format!("{} turn flags for {} cameras", turn.len(), cameras.len()));
    }
    if let Some(k) = (0..turn.len()).find(|&k| turn[k] && roll.get(k).copied().unwrap_or(false)) {
        return Err(format!("camera {k}: its roll and the body's turn both freed"));
    }
    if !group.is_empty() && (group.len() != cameras.len() || time.len() != cameras.len()) {
        return Err(format!("{} groups and {} times for {} cameras: one each", group.len(), time.len(), cameras.len()));
    }
    if let Some(k) = (0..time.len()).find(|&k| !time[k].is_finite()) {
        return Err(format!("camera {k}: time {}", time[k]));
    }
    // The freed cameras tied in sessions: group -> its cameras (not fixed).
    let mut sessions: std::collections::BTreeMap<i64, Vec<usize>> = Default::default();
    for (k, &g) in group.iter().enumerate() {
        if g >= 0 && turn.get(k).copied().unwrap_or(false) && !fixed.get(k).copied().unwrap_or(false) {
            sessions.entry(g).or_default().push(k);
        }
    }
    let tied = |k: usize| group.get(k).is_some_and(|&g| g >= 0) && sessions.values().any(|c| c.contains(&k));
    if !(offset_sigma > 0.0 && offset_sigma.is_finite()) || !(huber > 0.0 && huber.is_finite()) {
        return Err(format!("offset_sigma {offset_sigma}, huber {huber}: both must be positive"));
    }
    if !(height_sigma >= 0.0 && height_sigma.is_finite()) {
        return Err(format!("height_sigma {height_sigma}: zero, or how far a height may move"));
    }
    let heights = height_sigma > 0.0;
    let frames: Vec<[V3; 3]> = tiles.iter().map(|&(_, n)| frame(n)).collect::<Result<_, _>>()?;
    // Each registration's pixels per unit of ground along the maplet's u and
    // v, at its centre: the projection's derivative.
    let mut jac = Vec::with_capacity(registrations.len());
    // And what a roll of one radian moves it by: its pixel from the
    // middle, turned a quarter.
    let mut lever = Vec::with_capacity(registrations.len());
    // And what a turn of the body by one radian about each axis moves it by:
    // its centre moved by the axis cross the centre, projected.
    let mut twist = Vec::with_capacity(registrations.len());
    for (i, r) in registrations.iter().enumerate() {
        let (Some(cam), Some(&(centre, _))) = (cameras.get(r.image), tiles.get(r.tile)) else {
            return Err(format!("registration {i}: image {} or tile {} out of range", r.image, r.tile));
        };
        if !(r.shift[0].is_finite() && r.shift[1].is_finite() && r.weight.is_finite()) {
            return Err(format!("registration {i}: shift {:?}, weight {}", r.shift, r.weight));
        }
        let c = cam.axes * (centre - cam.position);
        if c.z <= 0.0 {
            return Err(format!("registration {i}: tile {} is behind image {}", r.tile, r.image));
        }
        let d = |e: V3| {
            let de = cam.axes * e;
            [
                cam.focal * (de.x * c.z - c.x * de.z) / (c.z * c.z),
                cam.focal * (de.y * c.z - c.y * de.z) / (c.z * c.z),
            ]
        };
        let [u, v, w] = frames[r.tile];
        let (du, dv, dw) = (d(u), d(v), if heights { d(w) } else { [0.0; 2] });
        jac.push([[du[0], dv[0], dw[0]], [du[1], dv[1], dw[1]]]);
        lever.push([cam.focal * c.y / c.z, -cam.focal * c.x / c.z]);
        let (tx, ty, tz) = (d(V3::X.cross(centre)), d(V3::Y.cross(centre)), d(V3::Z.cross(centre)));
        twist.push([[tx[0], ty[0], tz[0]], [tx[1], ty[1], tz[1]]]);
    }
    let mut by_tile = vec![vec![]; tiles.len()];
    let mut by_image = vec![vec![]; cameras.len()];
    for (i, r) in registrations.iter().enumerate() {
        by_tile[r.tile].push(i);
        by_image[r.image].push(i);
    }
    let apply = |j: &[[F; 3]; 2], o: [F; 3]| {
        [
            j[0][0] * o[0] + j[0][1] * o[1] + j[0][2] * o[2],
            j[1][0] * o[0] + j[1][1] * o[1] + j[1][2] * o[2],
        ]
    };
    let lambda = 1.0 / (offset_sigma * offset_sigma);
    // Without heights, the third unknown held at zero by a weight that
    // swamps any data: its column of the Jacobian is zero anyway.
    let lambda_w = if heights { 1.0 / (height_sigma * height_sigma) } else { 1.0 };
    let mut images = vec![[0.0; 2]; cameras.len()];
    let mut rolls: Vec<F> = vec![0.0; cameras.len()];
    let mut turns = vec![[0.0; 3]; cameras.len()];
    let mut offsets = vec![[0.0; 3]; tiles.len()];
    // What image `k`'s pointing, roll and turn put registration `i` at.
    let seen = |images: &[[F; 2]], rolls: &[F], turns: &[[F; 3]], i: usize, k: usize| {
        let (t, d) = (&twist[i], turns[k]);
        [
            images[k][0] + rolls[k] * lever[i][0] + t[0][0] * d[0] + t[0][1] * d[1] + t[0][2] * d[2],
            images[k][1] + rolls[k] * lever[i][1] + t[1][0] * d[0] + t[1][1] * d[1] + t[1][2] * d[2],
        ]
    };
    let mut weights = vec![0.0; registrations.len()];
    let mut errors = vec![0.0; registrations.len()];
    let mut left: Vec<F> = vec![];
    for _ in 0..iterations {
        for (i, r) in registrations.iter().enumerate() {
            let p = seen(&images, &rolls, &turns, i, r.image);
            let jo = apply(&jac[i], offsets[r.tile]);
            let e = ((r.shift[0] - p[0] - jo[0]).powi(2) + (r.shift[1] - p[1] - jo[1]).powi(2)).sqrt();
            weights[i] = r.weight * if e < huber { 1.0 } else { huber / e.max(1e-300) };
            errors[i] = e;
        }
        for (t, members) in by_tile.iter().enumerate() {
            let (mut a, mut b) = ([[lambda, 0.0, 0.0], [0.0, lambda, 0.0], [0.0, 0.0, lambda_w]], [0.0; 3]);
            for &i in members {
                let (j, h, r) = (&jac[i], weights[i], &registrations[i]);
                let p = seen(&images, &rolls, &turns, i, r.image);
                let rest = [r.shift[0] - p[0], r.shift[1] - p[1]];
                for x in 0..3 {
                    for y in 0..3 {
                        a[x][y] += h * (j[0][x] * j[0][y] + j[1][x] * j[1][y]);
                    }
                    b[x] += h * (j[0][x] * rest[0] + j[1][x] * rest[1]);
                }
            }
            let m = M3::from_cols(V3::from(a[0]), V3::from(a[1]), V3::from(a[2]));
            let o = m.inverse() * V3::from(b);
            offsets[t] = [o.x, o.y, o.z];
        }
        for (k, members) in by_image.iter().enumerate() {
            if members.is_empty() || fixed.get(k).copied().unwrap_or(false) {
                continue;
            }
            if tied(k) {
                continue;  // with its session, below
            }
            if turn.get(k).copied().unwrap_or(false) {
                // Shift and turn at once, as the roll below: the wrong locks
                // out, the turn held weakly at zero (TURN_SIGMA) where the
                // maplets cannot tell it from a shift. A wrong lock is far
                // off what the others leave: a turn of degrees not yet fitted
                // moves every maplet by tens of pixels, none of them wrong.
                let cut = wrong_lock(members, &errors, huber);
                let (mut a, mut b) = ([[0.0; 5]; 5], [0.0; 5]);
                for &i in members.iter().filter(|&&i| errors[i] <= cut) {
                    let (h, t, r) = (weights[i], &twist[i], &registrations[i]);
                    let jo = apply(&jac[i], offsets[r.tile]);
                    let rest = [r.shift[0] - jo[0], r.shift[1] - jo[1]];
                    let rows = [[1.0, 0.0, t[0][0], t[0][1], t[0][2]], [0.0, 1.0, t[1][0], t[1][1], t[1][2]]];
                    for (row, value) in rows.iter().zip(rest) {
                        for x in 0..5 {
                            for y in 0..5 {
                                a[x][y] += h * row[x] * row[y];
                            }
                            b[x] += h * row[x] * value;
                        }
                    }
                }
                for (x, row) in a.iter_mut().enumerate().skip(2) {
                    row[x] += 1.0 / (TURN_SIGMA * TURN_SIGMA);
                }
                if let Some(x) = solve(a.iter().flatten().copied().collect(), b.to_vec()) {
                    images[k] = [x[0], x[1]];
                    turns[k] = [x[2], x[3], x[4]];
                }
                continue;
            }
            if roll.get(k).copied().unwrap_or(false) {
                // Shift and roll at once, by the weights the Huber loss
                // left, the wrong locks out: past REJECT Huber lengths a
                // shift weighs nothing here, as the median ignores it -- a
                // wrong lock far from the middle would turn the image by
                // hundredths of a degree. A roll held weakly at zero
                // (ROLL_SIGMA, for one pixel of residual) where the maplets
                // are all near the middle and cannot tell it.
                let (mut a, mut b) = ([[0.0; 3]; 3], [0.0; 3]);
                for &i in members.iter().filter(|&&i| errors[i] <= REJECT * huber) {
                    let (h, l, r) = (weights[i], lever[i], &registrations[i]);
                    let jo = apply(&jac[i], offsets[r.tile]);
                    let rest = [r.shift[0] - jo[0], r.shift[1] - jo[1]];
                    let rows = [[1.0, 0.0, l[0]], [0.0, 1.0, l[1]]];
                    for (row, value) in rows.iter().zip(rest) {
                        for x in 0..3 {
                            for y in 0..3 {
                                a[x][y] += h * row[x] * row[y];
                            }
                            b[x] += h * row[x] * value;
                        }
                    }
                }
                a[2][2] += 1.0 / (ROLL_SIGMA * ROLL_SIGMA);
                let m = M3::from_cols(V3::from(a[0]), V3::from(a[1]), V3::from(a[2]));
                if m.determinant().abs() > 1e-300 {
                    let x = m.inverse() * V3::from(b);
                    images[k] = [x.x, x.y];
                    rolls[k] = x.z;
                }
                continue;
            }
            for (c, value) in images[k].iter_mut().enumerate() {
                left.clear();
                left.extend(members.iter().map(|&i| {
                    let r = &registrations[i];
                    r.shift[c] - apply(&jac[i], offsets[r.tile])[c]
                }));
                *value = median(&mut left);
            }
        }
        for cams in sessions.values() {
            // One session's shifts (two a camera) and its turn and rate (six)
            // from all its registrations, the wrong locks out, the turn held
            // weakly at zero (TURN_SIGMA) and the rate too (RATE_SIGMA).
            let (n, mean) = (cams.len(), cams.iter().map(|&k| time[k]).sum::<F>() / cams.len() as F);
            let size = 2 * n + 6;
            let (mut a, mut b) = (vec![0.0; size * size], vec![0.0; size]);
            let all: Vec<usize> = cams.iter().flat_map(|&k| by_image[k].iter().copied()).collect();
            let cut = wrong_lock(&all, &errors, huber);
            for (j, &k) in cams.iter().enumerate() {
                let tau = time[k] - mean;
                for &i in by_image[k].iter().filter(|&&i| errors[i] <= cut) {
                    let (h, t, r) = (weights[i], &twist[i], &registrations[i]);
                    let jo = apply(&jac[i], offsets[r.tile]);
                    for (row, value) in [r.shift[0] - jo[0], r.shift[1] - jo[1]].into_iter().enumerate() {
                        let terms = [
                            (2 * j + row, 1.0),
                            (2 * n, t[row][0]),
                            (2 * n + 1, t[row][1]),
                            (2 * n + 2, t[row][2]),
                            (2 * n + 3, tau * t[row][0]),
                            (2 * n + 4, tau * t[row][1]),
                            (2 * n + 5, tau * t[row][2]),
                        ];
                        for &(x, vx) in &terms {
                            for &(y, vy) in &terms {
                                a[x * size + y] += h * vx * vy;
                            }
                            b[x] += h * vx * value;
                        }
                    }
                }
                // A camera of the session with no registration keeps its
                // shift, its turn the session's.
                for c in 2 * j..2 * j + 2 {
                    a[c * size + c] += 1e-9;
                    b[c] += 1e-9 * images[k][c - 2 * j];
                }
            }
            for c in 0..3 {
                a[(2 * n + c) * size + 2 * n + c] += 1.0 / (TURN_SIGMA * TURN_SIGMA);
                a[(2 * n + 3 + c) * size + 2 * n + 3 + c] += 1.0 / (RATE_SIGMA * RATE_SIGMA);
            }
            if let Some(x) = solve(a, b) {
                for (j, &k) in cams.iter().enumerate() {
                    let tau = time[k] - mean;
                    images[k] = [x[2 * j], x[2 * j + 1]];
                    turns[k] = [0, 1, 2].map(|c| x[2 * n + c] + tau * x[2 * n + 3 + c]);
                }
            }
        }
    }
    let residuals = registrations
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let (p, jo) = (seen(&images, &rolls, &turns, i, r.image), apply(&jac[i], offsets[r.tile]));
            ((r.shift[0] - p[0] - jo[0]).powi(2) + (r.shift[1] - p[1] - jo[1]).powi(2)).sqrt()
        })
        .collect();
    Ok(Pointing {
        images,
        behind: by_image.iter().map(Vec::len).collect(),
        offsets,
        rolls,
        turns,
        residuals,
    })
}

/// Past what error a registration is a wrong lock, to a freed turn's fit:
/// REJECT Huber lengths, or REJECT times the median error of the
/// registrations it is fitted with when that is more (a turn not yet fitted).
fn wrong_lock(members: &[usize], errors: &[F], huber: F) -> F {
    let mut e: Vec<F> = members.iter().map(|&i| errors[i]).collect();
    if e.is_empty() {
        return REJECT * huber;
    }
    REJECT * huber.max(median(&mut e))
}

/// `a x = b` (`a` n x n by rows) by Gaussian elimination with partial
/// pivoting; None when singular.
fn solve(mut a: Vec<F>, mut b: Vec<F>) -> Option<Vec<F>> {
    let n = b.len();
    for c in 0..n {
        let p = (c..n).max_by(|&i, &j| a[i * n + c].abs().total_cmp(&a[j * n + c].abs()))?;
        if a[p * n + c].abs() < 1e-300 {
            return None;
        }
        if p != c {
            for x in 0..n {
                a.swap(c * n + x, p * n + x);
            }
            b.swap(c, p);
        }
        let (pivot, at) = (a[c * n..(c + 1) * n].to_vec(), b[c]);
        for r in c + 1..n {
            let f = a[r * n + c] / pivot[c];
            if f != 0.0 {
                for (v, &q) in a[r * n + c..(r + 1) * n].iter_mut().zip(&pivot[c..]) {
                    *v -= f * q;
                }
                b[r] -= f * at;
            }
        }
    }
    let mut x = vec![0.0; n];
    for r in (0..n).rev() {
        let s: F = (r + 1..n).map(|c| a[r * n + c] * x[c]).sum();
        x[r] = (b[r] - s) / a[r * n + r];
    }
    Some(x)
}

/// How far a freed roll may go, radians, for one pixel of residual: a weak
/// hold, for the images whose maplets lie near their middle.
const ROLL_SIGMA: F = 0.1;
/// Past this many Huber lengths, a shift is a wrong lock to an image whose
/// roll or turn is freed: out of its fit.
const REJECT: F = 3.0;
/// How far a freed turn may go, radians, for one pixel of residual: a weak
/// hold, for the images whose maplets cannot tell it from a shift.
const TURN_SIGMA: F = 0.3;
/// How fast a session's turn may change, radians an hour, for one pixel of
/// residual: as weak.
const RATE_SIGMA: F = 0.3;

/// The middle value, the two middle ones' mean for an even count.
fn median(values: &mut [F]) -> F {
    let n = values.len();
    values.sort_unstable_by(F::total_cmp);
    if n % 2 == 1 {
        values[n / 2]
    } else {
        0.5 * (values[n / 2 - 1] + values[n / 2])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fixed scramble in [-1, 1).
    fn noise(state: &mut u64) -> F {
        *state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((*state >> 11) as F / (1u64 << 53) as F) * 2.0 - 1.0
    }

    /// Cameras around a sphere of tiles, each image a known pointing, each
    /// tile a known offset, every shift their sum through the projection
    /// with a little noise -- and one in twenty a wrong lock, tens of
    /// pixels off, any way. The fit finds the pointings -- within 0.15 px
    /// here -- and leaves the wrong locks as the residuals. (Wrong locks all
    /// off the same way would move every tile's offset alike, and the
    /// pointings with them: not what registration does.)
    #[test]
    fn the_shifts_give_back_each_images_pointing() {
        let mut state = 7u64;
        let (radius, distance, focal) = (1.0, 20.0, 5000.0);
        let tiles: Vec<(V3, V3)> = (0..300)
            .map(|_| {
                let p = V3::new(noise(&mut state), noise(&mut state), noise(&mut state)).normalize() * radius;
                (p, p.normalize())
            })
            .collect();
        let cameras: Vec<Camera> = (0..60)
            .map(|_| {
                let d = V3::new(noise(&mut state), noise(&mut state), noise(&mut state)).normalize();
                let position = d * distance;
                let z = -d;
                let x = V3::Z.cross(z).normalize();
                let y = z.cross(x);
                Camera { position, axes: M3::from_cols(x, y, z).transpose(), focal }
            })
            .collect();
        let truth_p: Vec<[F; 2]> = (0..cameras.len()).map(|_| [5.0 * noise(&mut state), 5.0 * noise(&mut state)]).collect();
        let truth_o: Vec<[F; 2]> = (0..tiles.len()).map(|_| [0.002 * noise(&mut state), 0.002 * noise(&mut state)]).collect();
        let mut registrations = vec![];
        let mut wrong = vec![];
        for (k, cam) in cameras.iter().enumerate() {
            for (t, &(centre, normal)) in tiles.iter().enumerate() {
                let to_camera = (cam.position - centre).normalize();
                if normal.dot(to_camera) < 0.5 {
                    continue;
                }
                let [u, v, _] = frame(normal).unwrap();
                let c = cam.axes * (centre - cam.position);
                let at = |p: V3| {
                    let q = cam.axes * (p - cam.position);
                    [focal * q.x / q.z, focal * q.y / q.z]
                };
                let (a, b) = (at(centre), at(centre + u * truth_o[t][0] + v * truth_o[t][1]));
                assert!(c.z > 0.0);
                let mut shift = [truth_p[k][0] + b[0] - a[0] + 0.2 * noise(&mut state), truth_p[k][1] + b[1] - a[1] + 0.2 * noise(&mut state)];
                let lock = registrations.len() % 20 == 0;
                if lock {
                    // A wrong lock: 25 to 40 pixels off, any way.
                    let (r, a) = (32.5 + 7.5 * noise(&mut state), std::f64::consts::PI * noise(&mut state));
                    shift[0] += r * a.cos();
                    shift[1] += r * a.sin();
                }
                wrong.push(lock);
                registrations.push(Registration { image: k, tile: t, shift, weight: 1.0 });
            }
        }
        let fit = pointing(&cameras, &tiles, &registrations, 0.01, 3.0, 25, 0.0, &[], &[], &[], &[], &[]).unwrap();
        let err: Vec<F> = (0..cameras.len())
            .filter(|&k| fit.behind[k] >= 3)
            .map(|k| ((fit.images[k][0] - truth_p[k][0]).powi(2) + (fit.images[k][1] - truth_p[k][1]).powi(2)).sqrt())
            .collect();
        let worst = err.iter().cloned().fold(0.0, F::max);
        assert!(worst < 0.5, "an image's pointing {worst:.2} px off; {} images", err.len());
        let (mut good, mut bad): (Vec<F>, Vec<F>) = (vec![], vec![]);
        for (r, &w) in fit.residuals.iter().zip(&wrong) {
            if w { bad.push(*r) } else { good.push(*r) }
        }
        let good_max = good.iter().cloned().fold(0.0, F::max);
        let bad_min = bad.iter().cloned().fold(F::INFINITY, F::min);
        assert!(good_max < 2.0 && bad_min > 20.0, "residuals: right locks up to {good_max:.2}, wrong ones from {bad_min:.2}");
    }

    /// The same with each tile also off its height -- the surface under it
    /// a first guess -- and the heights freed: each image's parallax gives
    /// them back.
    #[test]
    fn the_shifts_give_back_each_tiles_height() {
        let mut state = 11u64;
        let (radius, distance, focal) = (1.0, 20.0, 5000.0);
        let tiles: Vec<(V3, V3)> = (0..200)
            .map(|_| {
                let p = V3::new(noise(&mut state), noise(&mut state), noise(&mut state)).normalize() * radius;
                (p, p.normalize())
            })
            .collect();
        let cameras: Vec<Camera> = (0..80)
            .map(|_| {
                let d = V3::new(noise(&mut state), noise(&mut state), noise(&mut state)).normalize();
                let z = -d;
                let x = V3::Z.cross(z).normalize();
                Camera { position: d * distance, axes: M3::from_cols(x, z.cross(x), z).transpose(), focal }
            })
            .collect();
        let truth_p: Vec<[F; 2]> = (0..cameras.len()).map(|_| [5.0 * noise(&mut state), 5.0 * noise(&mut state)]).collect();
        let truth_o: Vec<[F; 3]> = (0..tiles.len()).map(|_| [0.002 * noise(&mut state), 0.002 * noise(&mut state), 0.02 * noise(&mut state)]).collect();
        let mut registrations = vec![];
        for (k, cam) in cameras.iter().enumerate() {
            for (t, &(centre, normal)) in tiles.iter().enumerate() {
                if normal.dot((cam.position - centre).normalize()) < 0.3 {
                    continue;
                }
                let [u, v, w] = frame(normal).unwrap();
                let at = |p: V3| {
                    let q = cam.axes * (p - cam.position);
                    [focal * q.x / q.z, focal * q.y / q.z]
                };
                let o = truth_o[t];
                let (a, b) = (at(centre), at(centre + u * o[0] + v * o[1] + w * o[2]));
                let shift = [truth_p[k][0] + b[0] - a[0] + 0.2 * noise(&mut state), truth_p[k][1] + b[1] - a[1] + 0.2 * noise(&mut state)];
                registrations.push(Registration { image: k, tile: t, shift, weight: 1.0 });
            }
        }
        let fit = pointing(&cameras, &tiles, &registrations, 0.01, 3.0, 40, 0.1, &[], &[], &[], &[], &[]).unwrap();
        let err: Vec<F> = (0..tiles.len()).map(|t| (fit.offsets[t][2] - truth_o[t][2]).abs()).collect();
        let worst = err.iter().cloned().fold(0.0, F::max);
        // A 0.2-pixel noise is 0.0008 on the ground here; a height is seen
        // through the parallax, a few times weaker.
        assert!(worst < 0.004, "a tile's height {worst:.4} off, of up to 0.02");
        let p_err = (0..cameras.len()).map(|k| ((fit.images[k][0] - truth_p[k][0]).powi(2) + (fit.images[k][1] - truth_p[k][1]).powi(2)).sqrt()).fold(0.0, F::max);
        assert!(p_err < 0.5, "an image's pointing {p_err:.2} px off");
    }

    /// The roll's lever is what turning a camera's axes about their `z`
    /// (rows `x`, `y`: `Rz(roll) axes`) does to a point's pixel, to first
    /// order: what the caller applies the fitted roll by.
    #[test]
    fn a_roll_is_the_axes_turned_about_their_z() {
        let (focal, roll): (F, F) = (5000.0, 1e-4);
        let axes = M3::from_cols(V3::new(0.6, 0.8, 0.0), V3::new(-0.8, 0.6, 0.0), V3::Z).transpose();
        let (c, s) = (roll.cos(), roll.sin());
        let turn = M3::from_cols(V3::new(c, -s, 0.0), V3::new(s, c, 0.0), V3::Z);
        for p in [V3::new(0.3, -0.2, 20.0), V3::new(-0.5, 0.4, 18.0), V3::new(0.05, 0.6, 22.0)] {
            let (q, t) = (axes * p, turn * axes * p);
            let moved = [focal * (t.x / t.z - q.x / q.z), focal * (t.y / t.z - q.y / q.z)];
            let lever = [focal * q.y / q.z, -focal * q.x / q.z];
            assert!((moved[0] - roll * lever[0]).abs() < 1e-6 && (moved[1] - roll * lever[1]).abs() < 1e-6, "{moved:?} vs {lever:?}");
        }
    }

    /// Half the cameras turned about their boresight by up to two degrees,
    /// as images under the plan's attitude are, every shift made with the
    /// axes turned so (not the linear lever) and one in twenty a wrong lock:
    /// the rolls freed come back within 0.06 degrees (0.013 at the median),
    /// a quarter of a pixel at the edge of the bodies' discs here, every
    /// pointing within half a pixel, and the cameras not freed keep a roll
    /// of zero.
    #[test]
    fn the_shifts_give_back_each_images_roll() {
        let mut state = 23u64;
        let (radius, distance, focal) = (1.0, 20.0, 5000.0);
        let tiles: Vec<(V3, V3)> = (0..300)
            .map(|_| {
                let p = V3::new(noise(&mut state), noise(&mut state), noise(&mut state)).normalize() * radius;
                (p, p.normalize())
            })
            .collect();
        let cameras: Vec<Camera> = (0..60)
            .map(|_| {
                let d = V3::new(noise(&mut state), noise(&mut state), noise(&mut state)).normalize();
                let z = -d;
                let x = V3::Z.cross(z).normalize();
                Camera { position: d * distance, axes: M3::from_cols(x, z.cross(x), z).transpose(), focal }
            })
            .collect();
        let freed: Vec<bool> = (0..cameras.len()).map(|k| k % 2 == 0).collect();
        let truth_r: Vec<F> = (0..cameras.len()).map(|k| if freed[k] { 0.035 * noise(&mut state) } else { 0.0 }).collect();
        let truth_p: Vec<[F; 2]> = (0..cameras.len()).map(|_| [5.0 * noise(&mut state), 5.0 * noise(&mut state)]).collect();
        let truth_o: Vec<[F; 2]> = (0..tiles.len()).map(|_| [0.002 * noise(&mut state), 0.002 * noise(&mut state)]).collect();
        let mut registrations = vec![];
        for (k, cam) in cameras.iter().enumerate() {
            let (c, s) = (truth_r[k].cos(), truth_r[k].sin());
            let turned = M3::from_cols(V3::new(c, -s, 0.0), V3::new(s, c, 0.0), V3::Z) * cam.axes;
            for (t, &(centre, normal)) in tiles.iter().enumerate() {
                if normal.dot((cam.position - centre).normalize()) < 0.5 {
                    continue;
                }
                let [u, v, _] = frame(normal).unwrap();
                let at = |axes: M3, p: V3| {
                    let q = axes * (p - cam.position);
                    [focal * q.x / q.z, focal * q.y / q.z]
                };
                let (a, b) = (at(cam.axes, centre), at(turned, centre + u * truth_o[t][0] + v * truth_o[t][1]));
                let mut shift = [truth_p[k][0] + b[0] - a[0] + 0.2 * noise(&mut state), truth_p[k][1] + b[1] - a[1] + 0.2 * noise(&mut state)];
                if registrations.len() % 20 == 0 {
                    let (r, a) = (32.5 + 7.5 * noise(&mut state), std::f64::consts::PI * noise(&mut state));
                    shift[0] += r * a.cos();
                    shift[1] += r * a.sin();
                }
                registrations.push(Registration { image: k, tile: t, shift, weight: 1.0 });
            }
        }
        let fit = pointing(&cameras, &tiles, &registrations, 0.01, 3.0, 25, 0.0, &[], &freed, &[], &[], &[]).unwrap();
        let held: Vec<usize> = (0..cameras.len()).filter(|&k| fit.behind[k] >= 3).collect();
        let roll_err = held.iter().map(|&k| (fit.rolls[k] - truth_r[k]).abs()).fold(0.0, F::max);
        let p_err = held.iter().map(|&k| ((fit.images[k][0] - truth_p[k][0]).powi(2) + (fit.images[k][1] - truth_p[k][1]).powi(2)).sqrt()).fold(0.0, F::max);
        assert!(roll_err < 0.06_f64.to_radians(), "a roll {:.3} degrees off", roll_err.to_degrees());
        assert!(p_err < 0.5, "an image's pointing {p_err:.2} px off");
        assert!((0..cameras.len()).filter(|&k| !freed[k]).all(|k| fit.rolls[k] == 0.0), "a roll fitted where not freed");
    }

    #[test]
    fn a_tile_behind_a_camera_is_refused() {
        let cameras = [Camera { position: V3::ZERO, axes: M3::IDENTITY, focal: 100.0 }];
        let tiles = [(V3::new(0.0, 0.0, -1.0), V3::Z)];
        let r = [Registration { image: 0, tile: 0, shift: [0.0, 0.0], weight: 1.0 }];
        assert!(pointing(&cameras, &tiles, &r, 0.01, 3.0, 5, 0.0, &[], &[], &[], &[], &[]).is_err());
    }

    /// A turn's lever is what turning the body about its origin by a small
    /// rotation does to its maplets' pixels, to first order (the maplet at
    /// `c` moved by the rotation vector cross `c`): one camera, thirty
    /// maplets over the face it sees, their offsets held, the body turned by
    /// a known rotation of a few hundredths of a degree -- the fit gives it
    /// back, and no shift.
    #[test]
    fn a_turn_is_the_body_turned_about_its_origin() {
        let mut state = 5u64;
        let focal: F = 5000.0;
        let cam = Camera { position: V3::new(0.0, 0.0, -20.0), axes: M3::IDENTITY, focal };
        let tiles: Vec<(V3, V3)> = (0..30)
            .map(|_| {
                let p = V3::new(noise(&mut state), noise(&mut state), -1.0 - noise(&mut state).abs()).normalize();
                (p, p)
            })
            .collect();
        let d = V3::new(2e-4, -1e-4, 3e-4);
        let q = glam::DQuat::from_scaled_axis(d);
        let at = |p: V3| {
            let w = cam.axes * (p - cam.position);
            [focal * w.x / w.z, focal * w.y / w.z]
        };
        let registrations: Vec<Registration> = (0..tiles.len())
            .map(|t| {
                let (a, b) = (at(tiles[t].0), at(q * tiles[t].0));
                Registration { image: 0, tile: t, shift: [b[0] - a[0], b[1] - a[1]], weight: 1.0 }
            })
            .collect();
        let fit = pointing(&[cam], &tiles, &registrations, 1e-9, 3.0, 5, 0.0, &[], &[], &[true], &[], &[]).unwrap();
        let err = (V3::from(fit.turns[0]) - d).length();
        assert!(err < 1e-6, "turn {:?} for {d:?}", fit.turns[0]);
        assert!(fit.images[0][0].abs() < 1e-3 && fit.images[0][1].abs() < 1e-3, "shift {:?}", fit.images[0]);
    }

    /// A body whose rotation is off: half the images see it turned by up to
    /// three degrees about any axis, every shift made with the tiles turned
    /// so (not the linear lever), a 0.2-pixel noise and one in twenty a wrong
    /// lock. The turns freed come back within 0.05 degrees about the line of
    /// sight and 0.2 across it -- there a turn moves the face seen almost as
    /// a shift does, by each maplet's depth, and only the maplets towards
    /// the limb tell the two apart -- every pointing within a pixel (what
    /// the turn across leaves to the shift); the images not freed keep a
    /// turn of zero.
    #[test]
    fn the_shifts_give_back_each_images_turn() {
        let mut state = 31u64;
        let (radius, distance, focal) = (1.0, 20.0, 5000.0);
        let tiles: Vec<(V3, V3)> = (0..300)
            .map(|_| {
                let p = V3::new(noise(&mut state), noise(&mut state), noise(&mut state)).normalize() * radius;
                (p, p.normalize())
            })
            .collect();
        let cameras: Vec<Camera> = (0..60)
            .map(|_| {
                let d = V3::new(noise(&mut state), noise(&mut state), noise(&mut state)).normalize();
                let z = -d;
                let x = V3::Z.cross(z).normalize();
                Camera { position: d * distance, axes: M3::from_cols(x, z.cross(x), z).transpose(), focal }
            })
            .collect();
                        let freed: Vec<bool> = (0..cameras.len()).map(|k| k % 2 == 0).collect();
        let truth_d: Vec<V3> = (0..cameras.len())
            .map(|k| if freed[k] { V3::new(noise(&mut state), noise(&mut state), noise(&mut state)) * 0.052 } else { V3::ZERO })
            .collect();
        let truth_p: Vec<[F; 2]> = (0..cameras.len()).map(|_| [5.0 * noise(&mut state), 5.0 * noise(&mut state)]).collect();
        let truth_o: Vec<[F; 2]> = (0..tiles.len()).map(|_| [0.002 * noise(&mut state), 0.002 * noise(&mut state)]).collect();
        let mut registrations = vec![];
        for (k, cam) in cameras.iter().enumerate() {
            let q = glam::DQuat::from_scaled_axis(truth_d[k]);
            for (t, &(centre, normal)) in tiles.iter().enumerate() {
                if normal.dot((cam.position - centre).normalize()) < 0.5 {
                    continue;
                }
                let [u, v, _] = frame(normal).unwrap();
                let at = |p: V3| {
                    let w = cam.axes * (p - cam.position);
                    [focal * w.x / w.z, focal * w.y / w.z]
                };
                let (a, b) = (at(centre), at(q * (centre + u * truth_o[t][0] + v * truth_o[t][1])));
                let mut shift = [truth_p[k][0] + b[0] - a[0] + 0.2 * noise(&mut state), truth_p[k][1] + b[1] - a[1] + 0.2 * noise(&mut state)];
                if registrations.len() % 20 == 0 {
                    let (r, a) = (32.5 + 7.5 * noise(&mut state), std::f64::consts::PI * noise(&mut state));
                    shift[0] += r * a.cos();
                    shift[1] += r * a.sin();
                }
                registrations.push(Registration { image: k, tile: t, shift, weight: 1.0 });
            }
        }
        let fit = pointing(&cameras, &tiles, &registrations, 0.01, 3.0, 40, 0.0, &[], &[], &freed, &[], &[]).unwrap();
        let held: Vec<usize> = (0..cameras.len()).filter(|&k| fit.behind[k] >= 3).collect();
        // Each error in the camera's frame: across the line of sight (x, y)
        // and along it (z).
        let (mut across, mut along) = (0.0_f64, 0.0_f64);
        for &k in &held {
            let e = cameras[k].axes * (V3::from(fit.turns[k]) - truth_d[k]);
            across = across.max(e.x.hypot(e.y));
            along = along.max(e.z.abs());
        }
        let p_err = held.iter().map(|&k| ((fit.images[k][0] - truth_p[k][0]).powi(2) + (fit.images[k][1] - truth_p[k][1]).powi(2)).sqrt()).fold(0.0, F::max);
        assert!(across < 0.3_f64.to_radians(), "a turn {:.3} degrees off across the line of sight", across.to_degrees());
        assert!(along < 0.1_f64.to_radians(), "a turn {:.3} degrees off about the line of sight", along.to_degrees());
        // The shift and the turn across the line of sight trade off: a pixel.
        assert!(p_err < 1.0, "an image's pointing {p_err:.2} px off");
        assert!((0..cameras.len()).filter(|&k| !freed[k]).all(|k| fit.turns[k] == [0.0; 3]), "a turn fitted where not freed");
    }

    /// Sessions of images that see little: six of ten images over two hours,
    /// the view sweeping 60 degrees about the pole in each, each session's
    /// body turned by up to three degrees about each axis and turning at up
    /// to three degrees an hour, each image registered in only eight
    /// maplets, 0.2 px of noise and one in twenty a wrong lock; three rounds,
    /// as SPC applies each and measures again. Each image's own turn takes
    /// the noise and the wrong locks (2.1 degrees off at worst across the
    /// line of sight, 0.6 about it: eight registrations cannot tell three
    /// angles from a shift, and a turn and a shift take part of a wrong lock
    /// in); one turn and rate a session, three times closer (0.7 and 0.4).
    /// Without the wrong locks 0.2 and 0.05 against 0.3 and 0.08; without the
    /// noise either, 0.01 both: the rounds find the truth.
    #[test]
    fn a_session_shares_one_turn_and_its_rate() {
        let mut state = 41u64;
        let (radius, distance, focal) = (1.0, 20.0, 5000.0);
        let tiles: Vec<(V3, V3)> = (0..300)
            .map(|_| {
                let p = V3::new(noise(&mut state), noise(&mut state), noise(&mut state)).normalize() * radius;
                (p, p.normalize())
            })
            .collect();
        let (sessions, each) = (6, 10);
        let mut cameras = vec![];
        let (mut group, mut time) = (vec![], vec![]);
        for g in 0..sessions {
            // a session: the view sweeping 60 degrees about the pole in two
            // hours, at one latitude (a body seen from an orbit around it)
            let (lon0, lat) = (std::f64::consts::PI * noise(&mut state), 0.35 * noise(&mut state));
            for k in 0..each {
                let lon = lon0 + 60_f64.to_radians() * k as F / (each - 1) as F;
                let d = V3::new(lat.cos() * lon.cos(), lat.cos() * lon.sin(), lat.sin());
                let z = -d;
                let x = V3::Z.cross(z).normalize();
                cameras.push(Camera { position: d * distance, axes: M3::from_cols(x, z.cross(x), z).transpose(), focal });
                group.push(g as i64);
                time.push(10.0 * g as F + 0.2 * k as F);
            }
        }
        let turn_of: Vec<(V3, V3)> = (0..sessions)
            .map(|_| (V3::new(noise(&mut state), noise(&mut state), noise(&mut state)) * 0.052,
                      V3::new(noise(&mut state), noise(&mut state), noise(&mut state)) * 0.052))
            .collect();
        let truth: Vec<V3> = (0..cameras.len())
            .map(|k| {
                let g = group[k] as usize;
                let mean = 10.0 * g as F + 0.2 * (each - 1) as F / 2.0;
                turn_of[g].0 + turn_of[g].1 * (time[k] - mean)
            })
            .collect();
        // Which maplets each image registers (eight of those it sees) and
        // which registrations are wrong locks: fixed, as the image's.
        let picks: Vec<Vec<usize>> = cameras
            .iter()
            .map(|cam| {
                let seen: Vec<usize> = (0..tiles.len()).filter(|&t| tiles[t].1.dot((cam.position - tiles[t].0).normalize()) >= 0.5).collect();
                seen.iter().step_by((seen.len() / 8).max(1)).take(8).copied().collect()
            })
            .collect();
        let shake: Vec<Vec<[F; 2]>> = picks
            .iter()
            .enumerate()
            .map(|(k, p)| {
                p.iter()
                    .enumerate()
                    .map(|(j, _)| {
                        let mut e = [0.2 * noise(&mut state), 0.2 * noise(&mut state)];
                        if (k * 8 + j) % 20 == 0 {
                            let (r, a) = (32.5 + 7.5 * noise(&mut state), std::f64::consts::PI * noise(&mut state));
                            e[0] += r * a.cos();
                            e[1] += r * a.sin();
                        }
                        e
                    })
                    .collect()
            })
            .collect();
        // Three rounds as SPC does: each image's camera turned by the turn
        // found so far (position Tᵀ p, axes A T), what is left of it measured
        // again (the image against the body turned by T), its fit composed
        // onto T. Each turn's error at the end: across the line of sight and
        // about it.
        let rounds = |shared: bool| {
            let mut found = vec![glam::DQuat::IDENTITY; cameras.len()];
            for _ in 0..3 {
                let turned: Vec<Camera> = cameras
                    .iter()
                    .zip(&found)
                    .map(|(c, t)| {
                        let m = M3::from_quat(*t);
                        Camera { position: m.transpose() * c.position, axes: c.axes * m, focal }
                    })
                    .collect();
                let mut registrations = vec![];
                for (k, cam) in cameras.iter().enumerate() {
                    let (q, t) = (glam::DQuat::from_scaled_axis(truth[k]), found[k]);
                    let at = |p: V3| {
                        let w = cam.axes * (p - cam.position);
                        [focal * w.x / w.z, focal * w.y / w.z]
                    };
                    for (j, &tile) in picks[k].iter().enumerate() {
                        let centre = tiles[tile].0;
                        let (seen, drawn) = (at(q * centre), at(t * centre));
                        let shift = [seen[0] - drawn[0] + shake[k][j][0], seen[1] - drawn[1] + shake[k][j][1]];
                        registrations.push(Registration { image: k, tile, shift, weight: 1.0 });
                    }
                }
                let (g, tm): (&[i64], &[F]) = if shared { (&group, &time) } else { (&[], &[]) };
                let fit = pointing(&turned, &tiles, &registrations, 0.01, 3.0, 40, 0.0, &[], &[], &vec![true; cameras.len()], g, tm).unwrap();
                for k in 0..cameras.len() {
                    found[k] = (found[k] * glam::DQuat::from_scaled_axis(V3::from(fit.turns[k]))).normalize();
                }
            }
            let (mut across, mut along) = (0.0_f64, 0.0_f64);
            for k in 0..cameras.len() {
                let e = cameras[k].axes * (found[k].inverse() * glam::DQuat::from_scaled_axis(truth[k])).to_scaled_axis();
                across = across.max(e.x.hypot(e.y));
                along = along.max(e.z.abs());
            }
            (across.to_degrees(), along.to_degrees())
        };
        let (alone, shared) = (rounds(false), rounds(true));
        assert!(shared.0 < 1.0 && shared.1 < 0.6, "a session's image {:.3} degrees off across the line of sight, {:.3} about it", shared.0, shared.1);
        assert!(shared.0 < 0.5 * alone.0, "sessions {:.3} degrees off at worst across, each image alone {:.3}", shared.0, alone.0);
    }

    #[test]
    fn groups_without_times_are_refused() {
        let cameras = [Camera { position: V3::new(0.0, 0.0, -10.0), axes: M3::IDENTITY, focal: 100.0 }];
        let tiles = [(V3::ZERO, V3::Z)];
        let r = [Registration { image: 0, tile: 0, shift: [0.0, 0.0], weight: 1.0 }];
        assert!(pointing(&cameras, &tiles, &r, 0.01, 3.0, 5, 0.0, &[], &[], &[true], &[0], &[]).is_err());
    }

    #[test]
    fn a_turn_and_a_roll_together_are_refused() {
        let cameras = [Camera { position: V3::new(0.0, 0.0, -10.0), axes: M3::IDENTITY, focal: 100.0 }];
        let tiles = [(V3::ZERO, V3::Z)];
        let r = [Registration { image: 0, tile: 0, shift: [0.0, 0.0], weight: 1.0 }];
        assert!(pointing(&cameras, &tiles, &r, 0.01, 3.0, 5, 0.0, &[], &[true], &[true], &[], &[]).is_err());
    }

    #[test]
    fn the_median_is_numpys() {
        assert_eq!(median(&mut [3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(&mut [4.0, 1.0, 3.0, 2.0]), 2.5);
    }
}
