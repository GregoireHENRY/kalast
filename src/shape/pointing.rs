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
/// as well, held so to zero.
#[allow(clippy::too_many_arguments)]
pub fn pointing(
    cameras: &[Camera],
    tiles: &[(V3, V3)],
    registrations: &[Registration],
    offset_sigma: F,
    huber: F,
    iterations: usize,
    height_sigma: F,
) -> Result<Pointing, String> {
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
    let mut offsets = vec![[0.0; 3]; tiles.len()];
    let mut weights = vec![0.0; registrations.len()];
    let mut left: Vec<F> = vec![];
    for _ in 0..iterations {
        for (i, r) in registrations.iter().enumerate() {
            let p = images[r.image];
            let jo = apply(&jac[i], offsets[r.tile]);
            let e = ((r.shift[0] - p[0] - jo[0]).powi(2) + (r.shift[1] - p[1] - jo[1]).powi(2)).sqrt();
            weights[i] = r.weight * if e < huber { 1.0 } else { huber / e.max(1e-300) };
        }
        for (t, members) in by_tile.iter().enumerate() {
            let (mut a, mut b) = ([[lambda, 0.0, 0.0], [0.0, lambda, 0.0], [0.0, 0.0, lambda_w]], [0.0; 3]);
            for &i in members {
                let (j, h, r) = (&jac[i], weights[i], &registrations[i]);
                let p = images[r.image];
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
            if members.is_empty() {
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
    }
    let residuals = registrations
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let (p, jo) = (images[r.image], apply(&jac[i], offsets[r.tile]));
            ((r.shift[0] - p[0] - jo[0]).powi(2) + (r.shift[1] - p[1] - jo[1]).powi(2)).sqrt()
        })
        .collect();
    Ok(Pointing {
        images,
        behind: by_image.iter().map(Vec::len).collect(),
        offsets,
        residuals,
    })
}

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
        let fit = pointing(&cameras, &tiles, &registrations, 0.01, 3.0, 25, 0.0).unwrap();
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
        let fit = pointing(&cameras, &tiles, &registrations, 0.01, 3.0, 40, 0.1).unwrap();
        let err: Vec<F> = (0..tiles.len()).map(|t| (fit.offsets[t][2] - truth_o[t][2]).abs()).collect();
        let worst = err.iter().cloned().fold(0.0, F::max);
        // A 0.2-pixel noise is 0.0008 on the ground here; a height is seen
        // through the parallax, a few times weaker.
        assert!(worst < 0.004, "a tile's height {worst:.4} off, of up to 0.02");
        let p_err = (0..cameras.len()).map(|k| ((fit.images[k][0] - truth_p[k][0]).powi(2) + (fit.images[k][1] - truth_p[k][1]).powi(2)).sqrt()).fold(0.0, F::max);
        assert!(p_err < 0.5, "an image's pointing {p_err:.2} px off");
    }

    #[test]
    fn a_tile_behind_a_camera_is_refused() {
        let cameras = [Camera { position: V3::ZERO, axes: M3::IDENTITY, focal: 100.0 }];
        let tiles = [(V3::new(0.0, 0.0, -1.0), V3::Z)];
        let r = [Registration { image: 0, tile: 0, shift: [0.0, 0.0], weight: 1.0 }];
        assert!(pointing(&cameras, &tiles, &r, 0.01, 3.0, 5, 0.0).is_err());
    }

    #[test]
    fn the_median_is_numpys() {
        assert_eq!(median(&mut [3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(&mut [4.0, 1.0, 3.0, 2.0]), 2.5);
    }
}
