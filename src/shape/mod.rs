//! Shape from images: a surface's relief finer than its shape model, from the
//! images that see it.
//!
//! Stereophotoclinometry, the method Gaskell et al. (2008) built for Eros
//! and that has modelled every small body imaged up close since -- Itokawa,
//! Vesta, Ceres, Bennu, Ryugu. The surface is cut into *maplets*: square
//! grids of heights over a patch of the shape model, along the patch's mean
//! normal. Each grid point takes the slopes and the albedo that make it as
//! bright as it is in every image that sees it lit, the images' Sun on it
//! from as many sides as they were taken under -- photometric stereo, with
//! the body's own scattering law ([`crate::lightcurve::Law`]). The heights
//! are then integrated from the slopes, held to the shape model at long
//! wavelengths, where shading says least and stereophotogrammetry most. Each
//! image is registered to the maplet first, its scale fitted: what pointing
//! and calibration leave, at the scale of a pixel. And again, from the new
//! heights: their shadows, their facing, where their points fall in each
//! image.
//!
//! By default the heights are fitted to the images directly instead
//! ([`Options::direct`], `direct.rs`): every point each image shows, lit or
//! in shadow, is a residual of all the heights at once -- the shading, the
//! cast shadows' edges, which move with the heights that cast them, and the
//! parallax, which moves where each point falls in each image: photometric
//! stereo, shape from shadows and stereo in one least squares. A slope the
//! Suns barely constrain then weighs as little as it tells, and a shadow
//! tells the height of what casts it.
//!
//! What it takes:
//!
//! - the shape model, the maplet's start and the long wavelengths it keeps
//!   ([`Maplet::from_surface`]);
//! - the images, each as its camera saw it: where, turned how, its focal
//!   length, and where the Sun was ([`View`]) -- I/F, or anything
//!   proportional to it;
//! - which pixels to use, optionally: the shape model's own cast shadows and
//!   what hides the maplet from a camera beyond it, which the maplet's grid
//!   cannot see -- a render of the shape model in each image's geometry
//!   draws both ([`View::valid`]). Within the maplet, its own cast shadows
//!   and what it hides of itself are found from its heights, every
//!   iteration.
//!
//! What it gives: the heights and the albedo on the grid, the images' shifts
//! and scales, and a mesh of the maplet to draw. `notes/2026-10-14_shape_from_images/`
//! has the method, its checks on synthetic terrain and comet 67P.
//!
//! The grid is `n` points on a side, `spacing` apart, point `(i, j)` at
//!
//! ```text
//! P(i, j) = centre + (i - c) spacing u + (j - c) spacing v + h(i, j) w,   c = (n - 1) / 2
//! ```
//!
//! `w` the outward normal, `u` and `v` across it (`u x v = w`), stored row
//! by row, `j` the row: index `j n + i`.

/// The module's own precision, f64 whatever kalast's `Float` is: a hundred
/// thousand heights integrated from their slopes, and as many residuals
/// summed, want it.
pub type F = f64;
pub type V3 = glam::DVec3;
pub type M3 = glam::DMat3;

/// Radians in a degree.
pub(crate) const RPD: F = std::f64::consts::PI / 180.0;

/// `law`'s bidirectional reflectance, in this module's precision.
#[inline]
pub(crate) fn reflectance(law: &crate::lightcurve::Law, mu0: F, mu: F, alpha: F) -> F {
    law.reflectance(mu0 as crate::Float, mu as crate::Float, alpha as crate::Float) as F
}

mod bvh;
mod compose;
mod direct;
mod grid;
mod hull;
mod maplet;
mod pointing;
#[cfg(feature = "python")]
pub(crate) mod py;
mod solve;
mod surface;
mod view;

pub use compose::{compose, fuse, merge};
pub use hull::{align, classify, march, Footprint, Hull, HullReport, Silhouette, BODY, SKY, UNKNOWN};
pub use maplet::Maplet;
pub use pointing::{pointing, Camera, Pointing, Registration};
pub use solve::{Options, Report, ViewReport};
pub use surface::{choose, plan, Samples, Seen, Surface, Tile, FACING, IN_FRAME, LIT, SEEN};
pub use view::View;

/// `f(0), ..., f(count - 1)` over the machine's cores, in order.
///
/// Contiguous chunks, one a thread: the work here -- a view's points, or a
/// run of grid points -- costs about the same everywhere.
pub(crate) fn par_map<T: Send, F: Fn(usize) -> T + Sync>(count: usize, f: F) -> Vec<T> {
    let threads = std::thread::available_parallelism()
        .map_or(1, |t| t.get())
        .min(count.max(1));
    if threads <= 1 {
        return (0..count).map(f).collect();
    }
    let chunk = count.div_ceil(threads);
    std::thread::scope(|scope| {
        let f = &f;
        let handles: Vec<_> = (0..threads)
            .map(|t| {
                scope.spawn(move || {
                    (t * chunk..((t + 1) * chunk).min(count))
                        .map(f)
                        .collect::<Vec<T>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().expect("a shape thread panicked"))
            .collect()
    })
}
