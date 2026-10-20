//! A body's shape from its silhouettes: the visual hull, the start of a shape
//! model made from images alone.
//!
//! Before there are maplets there has to be a surface to lay them on. The
//! images that show the body against the sky say where it is not: a point of
//! space that falls on the sky in an image is not the body. Every image's sky
//! carved away, what is left is the visual hull -- the body, its concavities
//! filled where no limb crosses them: a neck, a crater. Gaskell's
//! stereophotoclinometry starts from such a shape, the limbs', and digs the
//! rest out with maplets.
//!
//! The sky is not every dark pixel: the night side is dark, and ground in a
//! shadow. Both lie behind lit ground, seen from the Sun, so a dark pixel is
//! sky only if nothing lit lies toward the Sun from it in the image -- and
//! only within the body's bounding sphere is anything the body, which keeps
//! stars and a coma's glow out ([`classify`]).
//!
//! Each image's silhouette is kept as a signed distance in pixels: to the
//! nearest sky from the ground, to the nearest ground from the sky. A point
//! of space takes, from every image it falls in, that distance at its
//! projection times a pixel's size there -- the surface's unit -- and of
//! those the `rank`-th least: the second, so that one image wrong, by its
//! pointing or a jet, carves nothing alone. The surface is where that
//! crosses zero, found on a grid of cubes by marching tetrahedra: six to a
//! cube, split alike in every cube, so that neighbouring cubes' triangles
//! meet edge to edge -- a closed, two-manifold mesh. Only the cubes near the
//! surface are visited, by the distance a coarser grid finds there; the
//! distance changes by no more than the cube's size across it.

use std::collections::HashMap;

use super::par_map;
use super::view::View;
use super::{F, M3, V3};

/// A pixel [`classify`] cannot tell: not valid in its view.
pub const UNKNOWN: i8 = 0;
/// The body, or what may be: lit, or behind lit ground from the Sun.
pub const BODY: i8 = 1;
/// The sky: dark, and nothing lit toward the Sun from it.
pub const SKY: i8 = -1;

/// Sixteenths of a pixel, the unit a silhouette's distances are kept in.
const STEP: F = 16.0;

/// The silhouette of the body in one image: its camera, and each pixel's
/// signed distance to the silhouette's edge.
#[derive(Clone, Debug)]
pub struct Silhouette {
    pub name: String,
    pub position: V3,
    pub axes: M3,
    pub focal: F,
    pub centre: [F; 2],
    pub width: usize,
    pub height: usize,
    /// Positive on the body, negative on the sky, in sixteenths of a pixel.
    distance: Vec<i16>,
}

/// The images' silhouettes, carved into a shape.
#[derive(Clone, Debug, Default)]
pub struct Hull {
    /// The body's bounding sphere about the frame's origin: nothing beyond
    /// it is the body.
    pub radius: F,
    pub silhouettes: Vec<Silhouette>,
}

/// What [`Hull::mesh`] made.
#[derive(Clone, Debug, Default)]
pub struct HullReport {
    /// Cubes whose corners were all evaluated, of the grid's.
    pub visited: usize,
    pub cubes: usize,
    /// Triangles dropped with the pieces other than the largest.
    pub dropped: usize,
    /// `V - E + F = 2 - 2 genus`: 0 for a body without a hole through it.
    pub genus: i64,
    /// Inside the mesh, the surface's unit cubed.
    pub volume: F,
}

/// Where the body may fall in a view, on the frame and beyond it: within
/// its bounding sphere's disc, or within an earlier hull's outline widened
/// by a margin.
pub enum Footprint {
    /// The disc's centre and radius, pixels; all the image from inside the
    /// sphere.
    Disc { x: F, y: F, r: F },
    /// Cells `cell` pixels on a side from `(x0, y0)`, `w` by `h`, row by row:
    /// true where the outline is.
    Cells { x0: F, y0: F, cell: F, w: usize, h: usize, on: Vec<bool> },
}

impl Footprint {
    /// The disc of the sphere of `radius` about the origin in `view`.
    pub fn sphere(view: &View, radius: F) -> Footprint {
        match view.project(V3::ZERO) {
            Some((x, y, depth)) if depth > radius => Footprint::Disc { x, y, r: view.focal * radius / (depth * depth - radius * radius).sqrt() },
            _ => Footprint::Disc { x: 0.0, y: 0.0, r: F::INFINITY },
        }
    }

    /// The outline of the closed mesh `vertices`, `facets` in `view`,
    /// widened by `margin` (the surface's unit) -- an earlier hull the body
    /// is inside, its error allowed for.
    pub fn outline(view: &View, vertices: &[V3], facets: &[u32], margin: F) -> Footprint {
        let projected: Vec<Option<(F, F)>> = vertices.iter().map(|p| view.project(*p).map(|(x, y, _)| (x, y))).collect();
        let (mut x0, mut y0, mut x1, mut y1) = (F::INFINITY, F::INFINITY, F::NEG_INFINITY, F::NEG_INFINITY);
        for &(x, y) in projected.iter().flatten() {
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
        }
        if !x0.is_finite() {
            return Footprint::Cells { x0: 0.0, y0: 0.0, cell: 1.0, w: 0, h: 0, on: vec![] };
        }
        let depth = vertices.iter().filter_map(|p| view.project(*p).map(|q| q.2)).fold(F::INFINITY, F::min);
        let widen = margin * view.focal / depth.max(1e-9);
        let cell = ((x1 - x0).max(y1 - y0) / 1024.0).max(2.0);
        let grow = (widen / cell).ceil() as usize + 1;
        let (x0, y0) = (x0 - (grow + 1) as F * cell, y0 - (grow + 1) as F * cell);
        let (w, h) = (((x1 - x0) / cell).ceil() as usize + grow + 2, ((y1 - y0) / cell).ceil() as usize + grow + 2);
        let mut on = vec![false; w * h];
        let at = |x: F, y: F| (((x - x0) / cell).floor() as usize, ((y - y0) / cell).floor() as usize);
        for t in facets.chunks_exact(3) {
            let (Some(a), Some(b), Some(c)) = (projected[t[0] as usize], projected[t[1] as usize], projected[t[2] as usize]) else {
                continue;
            };
            for (x, y) in [a, b, c] {
                let (i, j) = at(x, y);
                on[j * w + i] = true;
            }
            let (ia, ja) = at(a.0.min(b.0).min(c.0), a.1.min(b.1).min(c.1));
            let (ib, jb) = at(a.0.max(b.0).max(c.0), a.1.max(b.1).max(c.1));
            let area = (b.0 - a.0) * (c.1 - a.1) - (c.0 - a.0) * (b.1 - a.1);
            if area == 0.0 {
                continue;
            }
            for j in ja..=jb {
                for i in ia..=ib {
                    let (x, y) = (x0 + (i as F + 0.5) * cell, y0 + (j as F + 0.5) * cell);
                    let u = ((b.0 - x) * (c.1 - y) - (c.0 - x) * (b.1 - y)) / area;
                    let v = ((c.0 - x) * (a.1 - y) - (a.0 - x) * (c.1 - y)) / area;
                    if u >= 0.0 && v >= 0.0 && u + v <= 1.0 {
                        on[j * w + i] = true;
                    }
                }
            }
        }
        // Widened: a square `grow` cells about each, along the rows then the
        // columns.
        let mut rows = vec![false; w * h];
        for j in 0..h {
            for i in 0..w {
                if on[j * w + i] {
                    for k in i.saturating_sub(grow)..(i + grow + 1).min(w) {
                        rows[j * w + k] = true;
                    }
                }
            }
        }
        let mut wide = vec![false; w * h];
        for j in 0..h {
            for i in 0..w {
                if rows[j * w + i] {
                    for k in j.saturating_sub(grow)..(j + grow + 1).min(h) {
                        wide[k * w + i] = true;
                    }
                }
            }
        }
        Footprint::Cells { x0, y0, cell, w, h, on: wide }
    }

    /// Whether the body may fall at `(x, y)`, pixels.
    pub fn may(&self, x: F, y: F) -> bool {
        match self {
            Footprint::Disc { x: cx, y: cy, r } => (x - cx).powi(2) + (y - cy).powi(2) <= r * r,
            Footprint::Cells { x0, y0, cell, w, h, on } => {
                let (i, j) = ((x - x0) / cell, (y - y0) / cell);
                i >= 0.0 && j >= 0.0 && (i as usize) < *w && (j as usize) < *h && on[j as usize * w + i as usize]
            }
        }
    }

    /// Whether the body may fall past the edge of a frame `width` by
    /// `height`, from `(x, y)` on it along `(sx, sy)`, a unit vector.
    fn beyond(&self, x: F, y: F, sx: F, sy: F, width: usize, height: usize) -> bool {
        let off = |px: F, py: F| px < 0.0 || py < 0.0 || px >= width as F || py >= height as F;
        match self {
            Footprint::Disc { x: cx, y: cy, r } => {
                // Where the ray leaves the disc, against where it leaves the
                // frame.
                let (px, py) = (x - cx, y - cy);
                let b = px * sx + py * sy;
                let exit = -b + (b * b - (px * px + py * py - r * r)).max(0.0).sqrt();
                let to_edge = |p: F, d: F, size: usize| {
                    if d > 0.0 {
                        (size as F - p) / d
                    } else if d < 0.0 {
                        -p / d
                    } else {
                        F::INFINITY
                    }
                };
                exit > to_edge(x, sx, width).min(to_edge(y, sy, height))
            }
            Footprint::Cells { x0, y0, cell, w, h, .. } => {
                // Along the ray to the cells' far side, half a cell at a step.
                let span = (*w as F + *h as F) * cell;
                let step = 0.5 * cell;
                let mut t = 0.0;
                while t < span {
                    let (px, py) = (x + sx * t, y + sy * t);
                    if px < *x0 || py < *y0 || px > x0 + *w as F * cell || py > y0 + *h as F * cell {
                        if t > 0.0 {
                            return false;
                        }
                    } else if off(px, py) && self.may(px, py) {
                        return true;
                    }
                    t += step;
                }
                false
            }
        }
    }
}

/// Each pixel of `view` as the hull takes it: [`BODY`], [`SKY`] or
/// [`UNKNOWN`]. Lit is brighter than `threshold` of the image's lit ground
/// (its 99.5th percentile); a dark pixel is [`BODY`] if a lit one lies
/// toward the Sun from it, or `footprint` runs past the frame's edge that
/// way, where lit ground may be; nothing outside `footprint` is the body.
pub fn classify(view: &View, threshold: F, footprint: &Footprint) -> Vec<i8> {
    let (w, h) = (view.width, view.height);
    let n = w * h;
    let valid = |i: usize| view.image[i].is_finite() && (view.valid.is_empty() || view.valid[i] != 0);
    let mut values: Vec<f32> = (0..n).filter(|&i| valid(i)).map(|i| view.image[i]).collect();
    if values.is_empty() {
        return vec![UNKNOWN; n];
    }
    let at = ((values.len() - 1) as F * 0.995) as usize;
    let level = *values.select_nth_unstable_by(at, f32::total_cmp).1;
    let cut = (threshold * level as F) as f32;

    let centre_of = |i: usize| ((i % w) as F + 0.5, (i / w) as F + 0.5);
    let inside: Vec<bool> = (0..n)
        .map(|i| {
            let (x, y) = centre_of(i);
            footprint.may(x, y)
        })
        .collect();
    let lit: Vec<bool> = (0..n).map(|i| valid(i) && view.image[i] > cut && inside[i]).collect();

    // Toward the Sun in the image: its direction's projection, in the
    // camera's x and y.
    let s = view.axes * view.sun;
    let across = (s.x * s.x + s.y * s.y).sqrt();
    let mut maybe = vec![false; n];
    if across < 1e-3 {
        // The Sun on the line of sight: behind the camera, no night side is
        // in view; ahead of it, all is night.
        for (i, m) in maybe.iter_mut().enumerate() {
            *m = inside[i] && (lit[i] || !valid(i) || s.z > 0.0);
        }
    } else {
        let (sx, sy) = (s.x / across, s.y / across);
        // Sunward first: a pixel's neighbour toward the Sun is decided
        // before it, a whole pixel further along.
        let mut order: Vec<u32> = (0..n as u32).collect();
        order.sort_unstable_by(|&a, &b| {
            let (ax, ay) = centre_of(a as usize);
            let (bx, by) = centre_of(b as usize);
            (bx * sx + by * sy).total_cmp(&(ax * sx + ay * sy))
        });
        for &i in &order {
            let i = i as usize;
            if !inside[i] {
                continue;
            }
            if lit[i] || !valid(i) {
                maybe[i] = true;
                continue;
            }
            let (x, y) = centre_of(i);
            let (qx, qy) = ((x + sx).floor(), (y + sy).floor());
            maybe[i] = if qx >= 0.0 && qy >= 0.0 && (qx as usize) < w && (qy as usize) < h {
                maybe[qy as usize * w + qx as usize]
            } else {
                // Off the frame toward the Sun: lit ground may be there.
                footprint.beyond(x, y, sx, sy, w, h)
            };
        }
    }
    (0..n)
        .map(|i| {
            if !valid(i) {
                UNKNOWN
            } else if maybe[i] {
                BODY
            } else {
                SKY
            }
        })
        .collect()
}

/// The squared distance in pixels from each pixel to the nearest where
/// `seed` holds -- Felzenszwalb and Huttenlocher's (2012), along the rows
/// then the columns. With `border`, the frame's edge counts as one, a pixel
/// beyond it.
fn squared_distance(seed: &[bool], w: usize, h: usize, border: bool) -> Vec<f64> {
    const FAR: f64 = 1e30;
    // Padded by one all round, for the border.
    let (pw, ph) = (w + 2, h + 2);
    let mut grid = vec![FAR; pw * ph];
    for r in 0..ph {
        for c in 0..pw {
            let edge = r == 0 || c == 0 || r == ph - 1 || c == pw - 1;
            if edge {
                if border {
                    grid[r * pw + c] = 0.0;
                }
            } else if seed[(r - 1) * w + c - 1] {
                grid[r * pw + c] = 0.0;
            }
        }
    }
    let longest = pw.max(ph);
    let mut f = vec![0.0; longest];
    let mut d = vec![0.0; longest];
    let mut v = vec![0usize; longest];
    let mut z = vec![0.0; longest + 1];
    for c in 0..pw {
        for r in 0..ph {
            f[r] = grid[r * pw + c];
        }
        transform(&f[..ph], &mut d[..ph], &mut v, &mut z);
        for r in 0..ph {
            grid[r * pw + c] = d[r];
        }
    }
    for r in 0..ph {
        f[..pw].copy_from_slice(&grid[r * pw..(r + 1) * pw]);
        transform(&f[..pw], &mut d[..pw], &mut v, &mut z);
        grid[r * pw..(r + 1) * pw].copy_from_slice(&d[..pw]);
    }
    let mut out = vec![0.0; w * h];
    for r in 0..h {
        out[r * w..(r + 1) * w].copy_from_slice(&grid[(r + 1) * pw + 1..(r + 1) * pw + 1 + w]);
    }
    out
}

/// One line of [`squared_distance`]: the lower envelope of the parabolas
/// rooted at `f`.
fn transform(f: &[f64], d: &mut [f64], v: &mut [usize], z: &mut [f64]) {
    let n = f.len();
    let cross = |q: usize, p: usize| ((f[q] + (q * q) as f64) - (f[p] + (p * p) as f64)) / (2.0 * (q as f64 - p as f64));
    let mut k = 0;
    v[0] = 0;
    z[0] = f64::NEG_INFINITY;
    z[1] = f64::INFINITY;
    for q in 1..n {
        let mut s = cross(q, v[k]);
        while s <= z[k] {
            k -= 1;
            s = cross(q, v[k]);
        }
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = f64::INFINITY;
    }
    k = 0;
    for (q, out) in d.iter_mut().enumerate().take(n) {
        while z[k + 1] < q as f64 {
            k += 1;
        }
        *out = (q as f64 - v[k] as f64).powi(2) + f[v[k]];
    }
}

impl Silhouette {
    /// `view`'s silhouette, [`classify`]'s classes made a signed distance.
    pub fn new(view: &View, threshold: F, footprint: &Footprint) -> (Silhouette, Vec<i8>) {
        let classes = classify(view, threshold, footprint);
        let (w, h) = (view.width, view.height);
        let sky: Vec<bool> = classes.iter().map(|&c| c == SKY).collect();
        let ground: Vec<bool> = sky.iter().map(|s| !s).collect();
        // To the nearest sky; to the nearest ground or the frame's edge,
        // beyond which the body may be.
        let to_sky = squared_distance(&sky, w, h, false);
        let to_ground = squared_distance(&ground, w, h, true);
        let distance = (0..w * h)
            .map(|i| {
                let d = if sky[i] { -(to_ground[i].sqrt() - 0.5) } else { to_sky[i].sqrt() - 0.5 };
                (d * STEP).round().clamp(-32767.0, 32767.0) as i16
            })
            .collect();
        let s = Silhouette {
            name: view.name.clone(),
            position: view.position,
            axes: view.axes,
            focal: view.focal,
            centre: view.centre,
            width: w,
            height: h,
            distance,
        };
        (s, classes)
    }

    /// The signed distance at `p`'s projection, in the surface's unit at
    /// `p`'s depth: positive on the body. Off the frame, at least the
    /// distance to it -- the image knows nothing there, and the distance
    /// stays continuous across the edge. `None` behind the camera.
    pub fn at(&self, p: V3) -> Option<F> {
        let c = self.axes * (p - self.position);
        if c.z <= 0.0 {
            return None;
        }
        // Pixel centres at whole numbers.
        let x = self.centre[0] + self.focal * c.x / c.z - 0.5;
        let y = self.centre[1] + self.focal * c.y / c.z - 0.5;
        let (w, h) = (self.width as F, self.height as F);
        // How far outside the frame, from the edge pixels' outer side.
        let ox = (-0.5 - x).max(x - (w - 0.5)).max(0.0);
        let oy = (-0.5 - y).max(y - (h - 0.5)).max(0.0);
        let outside = (ox * ox + oy * oy).sqrt();
        let (x, y) = (x.clamp(0.0, w - 1.0), y.clamp(0.0, h - 1.0));
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(self.width - 1), (y0 + 1).min(self.height - 1));
        let (fx, fy) = (x - x0 as F, y - y0 as F);
        let at = |c: usize, r: usize| self.distance[r * self.width + c] as F;
        let d = (at(x0, y0) * (1.0 - fx) + at(x1, y0) * fx) * (1.0 - fy) + (at(x0, y1) * (1.0 - fx) + at(x1, y1) * fx) * fy;
        let d = if outside > 0.0 { (d / STEP).max(outside) } else { d / STEP };
        Some(d * c.z / self.focal)
    }
}

/// The six tetrahedra of a cube along its diagonal from corner 0 to corner
/// 7, corners numbered by their offsets' bits, x 1, y 2, z 4. The same in
/// every cube, so that neighbours' faces are split alike.
const TETRAHEDRA: [[usize; 4]; 6] = [[0, 1, 3, 7], [0, 1, 5, 7], [0, 2, 3, 7], [0, 2, 6, 7], [0, 4, 5, 7], [0, 4, 6, 7]];

impl Hull {
    pub fn new(radius: F) -> Hull {
        Hull { radius, silhouettes: vec![] }
    }

    /// `view`'s silhouette added, [`classify`]'s by `threshold`; its classes
    /// returned. The body within the bounding sphere, or with `within`
    /// within that closed mesh -- an earlier hull -- widened by `margin`:
    /// what lets a view that does not hold the whole body carve, where the
    /// sphere runs past its frame.
    pub fn add(&mut self, view: &View, threshold: F, within: Option<(&[V3], &[u32], F)>) -> Result<Vec<i8>, String> {
        view.check()?;
        let footprint = match within {
            Some((vertices, facets, margin)) => Footprint::outline(view, vertices, facets, margin),
            None => Footprint::sphere(view, self.radius),
        };
        let (s, classes) = Silhouette::new(view, threshold, &footprint);
        self.silhouettes.push(s);
        Ok(classes)
    }

    /// [`Hull::add`] for many views at once, over the machine's cores.
    pub fn add_many(&mut self, views: &[&View], threshold: F, within: Option<(&[V3], &[u32], F)>) -> Result<Vec<Vec<i8>>, String> {
        for v in views {
            v.check()?;
        }
        let radius = self.radius;
        let made = par_map(views.len(), |k| {
            let footprint = match within {
                Some((vertices, facets, margin)) => Footprint::outline(views[k], vertices, facets, margin),
                None => Footprint::sphere(views[k], radius),
            };
            Silhouette::new(views[k], threshold, &footprint)
        });
        let mut out = Vec::with_capacity(made.len());
        for (s, classes) in made {
            self.silhouettes.push(s);
            out.push(classes);
        }
        Ok(out)
    }

    /// The hull's signed distance at `p`: the `rank`-th least of the
    /// silhouettes' (`rank` 1 to 8), and no more than its depth inside the
    /// bounding sphere. Positive inside.
    pub fn value(&self, p: V3, rank: usize) -> F {
        self.values(&[p], rank)[0]
    }

    /// [`Hull::value`] at each of `points`, a silhouette at a time: points
    /// near one another fall near one another in each image.
    pub fn values(&self, points: &[V3], rank: usize) -> Vec<F> {
        const MOST: usize = 8;
        let rank = rank.clamp(1, MOST);
        let mut least = vec![[F::INFINITY; MOST]; points.len()];
        for s in &self.silhouettes {
            for (p, l) in points.iter().zip(least.iter_mut()) {
                if let Some(d) = s.at(*p) {
                    // Kept sorted, the `rank` least.
                    if d < l[rank - 1] {
                        let mut k = rank - 1;
                        while k > 0 && l[k - 1] > d {
                            l[k] = l[k - 1];
                            k -= 1;
                        }
                        l[k] = d;
                    }
                }
            }
        }
        points.iter().zip(least).map(|(p, l)| l[rank - 1].min(self.radius - p.length())).collect()
    }

    /// The hull as a mesh: `(vertices, facets)` and what it took. Cubes of
    /// `size` over the bounding sphere; the surface where [`Hull::value`]
    /// crosses zero, by marching tetrahedra, its largest piece kept. Then
    /// `relax` rounds of each vertex moved toward its neighbours' middle
    /// across the surface and back onto it: the tetrahedra's slivers
    /// evened out.
    pub fn mesh(&self, size: F, rank: usize, relax: usize) -> Result<(Vec<V3>, Vec<u32>, HullReport), String> {
        if !(size > 0.0 && self.radius > 0.0) {
            return Err(format!("a cube of {size} in a sphere of {}", self.radius));
        }
        // Nodes along each axis, the sphere inside with a cube to spare.
        let n = (2.0 * self.radius / size).ceil() as usize + 3;
        if n > (1 << 20) {
            return Err(format!("{n} cubes a side: a larger size"));
        }
        let origin = V3::splat(-0.5 * (n - 1) as F * size);
        let node = |i: usize, j: usize, k: usize| origin + V3::new(i as F, j as F, k as F) * size;
        const B: usize = 8;
        let blocks = (n - 1).div_ceil(B);
        let half = 0.5 * (B as F) * size * 3f64.sqrt();
        // A block's middle tells whether the surface may cross it.
        let near: Vec<usize> = {
            let total = blocks * blocks * blocks;
            let middle = |b: usize| {
                let (bi, bj, bk) = (b % blocks, (b / blocks) % blocks, b / (blocks * blocks));
                node(bi * B, bj * B, bk * B) + V3::splat(0.5 * B as F * size)
            };
            let chunk = 4096;
            let middles: Vec<F> = par_map(total.div_ceil(chunk), |c| {
                let points: Vec<V3> = (c * chunk..((c + 1) * chunk).min(total)).map(middle).collect();
                self.values(&points, rank)
            })
            .concat();
            (0..total).filter(|&b| middles[b].abs() <= 1.5 * half).collect()
        };
        let key = |i: usize, j: usize, k: usize, dir: usize| ((((k * n + j) * n + i) as u64) << 3) | dir as u64;
        // Each near block's triangles: their corners' edges, by key, and
        // where on them.
        let parts = par_map(near.len(), |m| {
            let b = near[m];
            let (bi, bj, bk) = (b % blocks * B, (b / blocks) % blocks * B, b / (blocks * blocks) * B);
            let (ni, nj, nk) = ((n - 1 - bi).min(B) + 1, (n - 1 - bj).min(B) + 1, (n - 1 - bk).min(B) + 1);
            let mut nodes = Vec::with_capacity(ni * nj * nk);
            for k in 0..nk {
                for j in 0..nj {
                    for i in 0..ni {
                        nodes.push(node(bi + i, bj + j, bk + k));
                    }
                }
            }
            // Never on a node: a crossing is strictly between two.
            let values: Vec<F> = self.values(&nodes, rank).into_iter().map(|v| if v == 0.0 { 1e-12 } else { v }).collect();
            let mut points: Vec<(u64, V3)> = vec![];
            let mut triangles: Vec<[u64; 3]> = vec![];
            for k in 0..nk - 1 {
                for j in 0..nj - 1 {
                    for i in 0..ni - 1 {
                        let corner = |c: usize| (i + (c & 1), j + ((c >> 1) & 1), k + ((c >> 2) & 1));
                        let value = |c: usize| {
                            let (a, b, d) = corner(c);
                            values[(d * nj + b) * ni + a]
                        };
                        let v: [F; 8] = std::array::from_fn(value);
                        if v.iter().all(|&x| x > 0.0) || v.iter().all(|&x| x < 0.0) {
                            continue;
                        }
                        for t in TETRAHEDRA {
                            let (inner, outer): (Vec<usize>, Vec<usize>) = t.iter().partition(|&&c| v[c] > 0.0);
                            if inner.is_empty() || outer.is_empty() {
                                continue;
                            }
                            // The crossing on the edge between corners a and b.
                            let mut cross = |a: usize, b: usize| -> (u64, V3) {
                                let (lo, hi) = if a < b { (a, b) } else { (b, a) };
                                let (li, lj, lk) = corner(lo);
                                let id = key(bi + li, bj + lj, bk + lk, hi - lo);
                                let s = (v[lo] / (v[lo] - v[hi])).clamp(0.02, 0.98);
                                let (hi_i, hi_j, hi_k) = corner(hi);
                                let p0 = node(bi + li, bj + lj, bk + lk);
                                let p1 = node(bi + hi_i, bj + hi_j, bk + hi_k);
                                let p = p0 + (p1 - p0) * s;
                                points.push((id, p));
                                (id, p)
                            };
                            let mid = |cs: &[usize]| cs.iter().map(|&c| V3::new((c & 1) as F, ((c >> 1) & 1) as F, ((c >> 2) & 1) as F)).sum::<V3>() / cs.len() as F;
                            // Out of the body: from the inner corners to the
                            // outer, across the plane the crossings lie in.
                            let out = mid(&outer) - mid(&inner);
                            let made = if inner.len() == 1 || outer.len() == 1 {
                                let (lone, rest) = if inner.len() == 1 { (inner[0], &outer) } else { (outer[0], &inner) };
                                vec![[cross(lone, rest[0]), cross(lone, rest[1]), cross(lone, rest[2])]]
                            } else {
                                let (a, b) = (inner[0], inner[1]);
                                let (c, d) = (outer[0], outer[1]);
                                let (ac, ad, bd, bc) = (cross(a, c), cross(a, d), cross(b, d), cross(b, c));
                                vec![[ac, ad, bd], [ac, bd, bc]]
                            };
                            for [p0, p1, p2] in made {
                                if (p1.1 - p0.1).cross(p2.1 - p0.1).dot(out) < 0.0 {
                                    triangles.push([p0.0, p2.0, p1.0]);
                                } else {
                                    triangles.push([p0.0, p1.0, p2.0]);
                                }
                            }
                        }
                    }
                }
            }
            (points, triangles)
        });

        // Welded by key: a crossing is the same point in every cube that
        // shares its edge.
        let mut index: HashMap<u64, u32> = HashMap::new();
        let mut vertices: Vec<V3> = vec![];
        let mut facets: Vec<u32> = vec![];
        for (points, triangles) in &parts {
            for &(id, p) in points {
                index.entry(id).or_insert_with(|| {
                    vertices.push(p);
                    (vertices.len() - 1) as u32
                });
            }
            for t in triangles {
                let f = [index[&t[0]], index[&t[1]], index[&t[2]]];
                if f[0] != f[1] && f[1] != f[2] && f[0] != f[2] {
                    facets.extend_from_slice(&f);
                }
            }
        }
        let mut report = HullReport { visited: near.len() * B * B * B, cubes: (n - 1).pow(3), ..Default::default() };
        let (vertices, facets, dropped) = largest_piece(vertices, facets);
        report.dropped = dropped;
        let mut vertices = vertices;
        for _ in 0..relax {
            vertices = self.relaxed(&vertices, &facets, rank);
        }
        let edges = {
            let mut e: Vec<(u32, u32)> = facets.chunks_exact(3).flat_map(|t| [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])]).map(|(a, b)| (a.min(b), a.max(b))).collect();
            e.sort_unstable();
            e.dedup();
            e.len() as i64
        };
        report.genus = (2 - (vertices.len() as i64 - edges + (facets.len() / 3) as i64)) / 2;
        report.volume = facets
            .chunks_exact(3)
            .map(|t| vertices[t[0] as usize].dot(vertices[t[1] as usize].cross(vertices[t[2] as usize])))
            .sum::<F>()
            / 6.0;
        Ok((vertices, facets, report))
    }

    /// Each vertex moved halfway toward its neighbours' middle across the
    /// surface, then onto the surface along its normal by the hull's
    /// distance there.
    fn relaxed(&self, vertices: &[V3], facets: &[u32], rank: usize) -> Vec<V3> {
        let mut sum = vec![V3::ZERO; vertices.len()];
        let mut count = vec![0u32; vertices.len()];
        let mut normal = vec![V3::ZERO; vertices.len()];
        for t in facets.chunks_exact(3) {
            let (a, b, c) = (t[0] as usize, t[1] as usize, t[2] as usize);
            let n = (vertices[b] - vertices[a]).cross(vertices[c] - vertices[a]);
            for (x, y) in [(a, b), (b, c), (c, a)] {
                sum[x] += vertices[y];
                count[x] += 1;
                normal[x] += n;
            }
        }
        par_map(vertices.len(), |i| {
            let p = vertices[i];
            let n = normal[i].normalize_or_zero();
            if count[i] == 0 || n == V3::ZERO {
                return p;
            }
            let towards = sum[i] / count[i] as F - p;
            let q = p + 0.5 * (towards - towards.dot(n) * n);
            // Positive inside: the surface is outward by the distance.
            let d = self.value(q, rank);
            if d.is_finite() {
                q + d.clamp(-0.5 * towards.length(), 0.5 * towards.length()) * n
            } else {
                q
            }
        })
    }
}

/// The shift that best lays mask `b` on the signs `a`, both `w` by `h` row
/// by row: `(dx, dy)` such that `b` at `(x - dx, y - dy)` holds most where
/// `a` is 1 and least where it is -1 -- 0 for no matter -- and that score
/// as a share of `a`'s nonzero pixels. For a surface's outline drawn in an
/// image's geometry against the image: 1 where it shows the body lit, -1
/// on the sky; what to add to the camera's centre. Whole pixels within
/// `search`, a quarter of the resolution first, then to half a pixel by a
/// parabola through the best and its neighbours.
pub fn align(a: &[i8], b: &[bool], w: usize, h: usize, search: usize) -> ([F; 2], F) {
    // A quarter: `a` summed over each cell, `b` held where most of it does.
    let (sw, sh) = (w.div_ceil(4), h.div_ceil(4));
    let mut qa = vec![0i32; sw * sh];
    let mut count = vec![0u8; sw * sh];
    for r in 0..h {
        for c in 0..w {
            qa[(r / 4) * sw + c / 4] += a[r * w + c] as i32;
            count[(r / 4) * sw + c / 4] += b[r * w + c] as u8;
        }
    }
    let qb: Vec<bool> = count.into_iter().map(|n| n >= 8).collect();
    fn score<T: Copy + Into<i32> + Sync>(a: &[T], b: &[bool], w: usize, h: usize, dx: i64, dy: i64) -> F {
        let mut sum = 0i64;
        for r in 0..h as i64 {
            let rb = r - dy;
            if rb < 0 || rb >= h as i64 {
                continue;
            }
            for c in 0..w as i64 {
                let cb = c - dx;
                if cb >= 0 && cb < w as i64 && b[(rb * w as i64 + cb) as usize] {
                    sum += a[(r * w as i64 + c) as usize].into() as i64;
                }
            }
        }
        sum as F
    }
    fn best<T: Copy + Into<i32> + Sync>(a: &[T], b: &[bool], w: usize, h: usize, around: (i64, i64), reach: i64) -> ((i64, i64), F) {
        let shifts: Vec<(i64, i64)> = (-reach..=reach).flat_map(|y| (-reach..=reach).map(move |x| (around.0 + x, around.1 + y))).collect();
        let scores = par_map(shifts.len(), |k| score(a, b, w, h, shifts[k].0, shifts[k].1));
        let k = (0..shifts.len()).max_by(|&i, &j| scores[i].total_cmp(&scores[j])).unwrap_or(0);
        (shifts[k], scores[k])
    }
    let ((cx, cy), _) = best(&qa, &qb, sw, sh, (0, 0), (search / 4 + 1) as i64);
    let ((x, y), top) = best(a, b, w, h, (4 * cx, 4 * cy), 4);
    // Half a pixel either way, by a parabola through three.
    let vertex = |m: F, z: F, p: F| {
        let d = m - 2.0 * z + p;
        if d < 0.0 {
            (0.5 * (m - p) / d).clamp(-0.5, 0.5)
        } else {
            0.0
        }
    };
    let fx = vertex(score(a, b, w, h, x - 1, y), top, score(a, b, w, h, x + 1, y));
    let fy = vertex(score(a, b, w, h, x, y - 1), top, score(a, b, w, h, x, y + 1));
    let total = a.iter().filter(|&&v| v != 0).count().max(1);
    ([x as F + fx, y as F + fy], top / total as F)
}

/// The largest piece of a mesh, by its facets, and how many facets the
/// others had.
fn largest_piece(vertices: Vec<V3>, facets: Vec<u32>) -> (Vec<V3>, Vec<u32>, usize) {
    let mut parent: Vec<u32> = (0..vertices.len() as u32).collect();
    fn root(parent: &mut [u32], mut x: u32) -> u32 {
        while parent[x as usize] != x {
            parent[x as usize] = parent[parent[x as usize] as usize];
            x = parent[x as usize];
        }
        x
    }
    for t in facets.chunks_exact(3) {
        for (a, b) in [(t[0], t[1]), (t[1], t[2])] {
            let (ra, rb) = (root(&mut parent, a), root(&mut parent, b));
            if ra != rb {
                parent[ra as usize] = rb;
            }
        }
    }
    let mut size: HashMap<u32, usize> = HashMap::new();
    for t in facets.chunks_exact(3) {
        *size.entry(root(&mut parent, t[0])).or_default() += 1;
    }
    let Some((&keep, _)) = size.iter().max_by_key(|&(_, &s)| s) else {
        return (vertices, facets, 0);
    };
    let mut renumber = vec![u32::MAX; vertices.len()];
    let mut kept = vec![];
    let mut out = vec![];
    let mut dropped = 0;
    for t in facets.chunks_exact(3) {
        if root(&mut parent, t[0]) != keep {
            dropped += 1;
            continue;
        }
        for &v in t {
            if renumber[v as usize] == u32::MAX {
                renumber[v as usize] = kept.len() as u32;
                kept.push(vertices[v as usize]);
            }
            out.push(renumber[v as usize]);
        }
    }
    (kept, out, dropped)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An ellipsoid's semi-axes.
    const AXES: V3 = V3::new(1.0, 0.7, 0.5);

    /// A view of the ellipsoid from `from` (a unit vector) at distance 12,
    /// lit from `sun`: Lambert's law, 160 x 160 pixels, the body about 100
    /// across.
    fn view(from: V3, sun: V3) -> View {
        view_at(from, sun, 12.0)
    }

    /// The same from `distance`.
    fn view_at(from: V3, sun: V3, distance: F) -> View {
        let (w, h) = (160usize, 160usize);
        let position = from * distance;
        let z = -from;
        let up = if z.z.abs() < 0.9 { V3::Z } else { V3::X };
        let x = up.cross(z).normalize();
        let y = z.cross(x);
        let axes = M3::from_cols(x, y, z).transpose();
        let focal = 600.0;
        let mut image = vec![0f32; w * h];
        for r in 0..h {
            for c in 0..w {
                let d = (x * ((c as F + 0.5 - 80.0) / focal) + y * ((r as F + 0.5 - 80.0) / focal) + z).normalize();
                // The ray in the ellipsoid's unit sphere.
                let (o, e) = (position / AXES, d / AXES);
                let (a, b, cc) = (e.dot(e), 2.0 * o.dot(e), o.dot(o) - 1.0);
                let disc = b * b - 4.0 * a * cc;
                if disc < 0.0 {
                    continue;
                }
                let t = (-b - disc.sqrt()) / (2.0 * a);
                let p = position + d * t;
                let n = (p / (AXES * AXES)).normalize();
                image[r * w + c] = n.dot(sun).max(0.0) as f32;
            }
        }
        View {
            name: String::new(),
            image,
            width: w,
            height: h,
            valid: vec![],
            position,
            axes,
            focal,
            centre: [80.0, 80.0],
            sun,
            weight: 1.0,
        }
    }

    fn directions(count: usize) -> Vec<V3> {
        (0..count)
            .map(|k| {
                let z = 1.0 - 2.0 * (k as F + 0.5) / count as F;
                let r = (1.0 - z * z).sqrt();
                let phi = std::f64::consts::PI * (1.0 + 5f64.sqrt()) * k as F;
                V3::new(r * phi.cos(), r * phi.sin(), z)
            })
            .collect()
    }

    #[test]
    fn the_night_side_is_not_sky() {
        // Lit from the camera's +x side at 60 deg of phase: the night side's
        // crescent lies at -x, dark, and must not be taken for sky.
        let from = V3::new(0.0, -1.0, 0.0);
        let sun = (from * 0.5 + V3::new(3f64.sqrt() / 2.0, 0.0, 0.0)).normalize();
        let v = view(from, sun);
        let classes = classify(&v, 0.05, &Footprint::sphere(&v, 1.2));
        let s = v.axes * sun;
        assert!(s.x.abs() > 0.5, "the Sun across the image");
        let mut night = 0;
        for r in 0..v.height {
            for c in 0..v.width {
                let i = r * v.width + c;
                // On the body by geometry, dark: the night side.
                let d = (v.axes.transpose() * V3::new((c as F + 0.5 - 80.0) / 600.0, (r as F + 0.5 - 80.0) / 600.0, 1.0)).normalize();
                let (o, e) = (v.position / AXES, d / AXES);
                let disc = (2.0 * o.dot(e)).powi(2) - 4.0 * e.dot(e) * (o.dot(o) - 1.0);
                if disc > 0.0 && v.image[i] == 0.0 {
                    night += 1;
                    assert_eq!(classes[i], BODY, "pixel ({c}, {r}) on the night side taken for sky");
                }
            }
        }
        assert!(night > 300, "{night} pixels of night side");
    }

    #[test]
    fn close_views_carve_within_an_earlier_hull() {
        // Views from 6: the body past the frame. Within the sphere they
        // cannot tell the night side beyond the frame from the sky; within
        // a hull of distant views, widened a little, they carve.
        let suns = directions(7);
        let far: Vec<View> = directions(24).into_iter().enumerate().map(|(k, from)| view(from, (from + 0.6 * suns[k % 7]).normalize())).collect();
        let close: Vec<View> = directions(40).into_iter().enumerate().map(|(k, from)| view_at(from, (from + 0.6 * suns[(k + 3) % 7]).normalize(), 6.0)).collect();
        let mut first = Hull::new(1.3);
        for v in &far {
            first.add(v, 0.05, None).unwrap();
        }
        let (v1, f1, _) = first.mesh(0.04, 2, 2).unwrap();
        let mut sphere = Hull::new(1.3);
        let mut within = Hull::new(1.3);
        let (mut sky_sphere, mut sky_within) = (0, 0);
        for v in &close {
            sky_sphere += sphere.add(v, 0.05, None).unwrap().iter().filter(|&&c| c == SKY).count();
            sky_within += within.add(v, 0.05, Some((&v1, &f1, 0.05))).unwrap().iter().filter(|&&c| c == SKY).count();
        }
        assert!(sky_within > 3 * sky_sphere.max(1), "sky {sky_within} within the hull, {sky_sphere} within the sphere");
        for v in &far {
            within.add(v, 0.05, Some((&v1, &f1, 0.05))).unwrap();
        }
        let (vertices, _, report) = within.mesh(0.04, 2, 2).unwrap();
        assert_eq!(report.genus, 0, "{report:?}");
        let worst = vertices.iter().map(|p| (p.length() - p.length() / (*p / AXES).length()).abs()).fold(0.0, F::max);
        assert!(worst < 0.03, "a vertex {worst} off the ellipsoid");
    }

    #[test]
    fn an_ellipsoid_from_its_silhouettes() {
        let mut hull = Hull::new(1.3);
        let suns = directions(7);
        for (k, from) in directions(60).into_iter().enumerate() {
            // Lit from within 50 deg of the camera.
            let sun = (from + 0.6 * suns[k % suns.len()]).normalize();
            hull.add(&view(from, sun), 0.05, None).unwrap();
        }
        let size = 0.04;
        let (vertices, facets, report) = hull.mesh(size, 2, 3).unwrap();
        assert_eq!(report.genus, 0, "{report:?}");
        assert_eq!(report.dropped, 0, "{report:?}");
        // Every vertex near the ellipsoid: a visual hull of a convex body is
        // the body, to the pixels' and the cubes' size.
        let mut worst: F = 0.0;
        for p in &vertices {
            // The distance along the radius to the ellipsoid.
            let r = p.length();
            let surface = 1.0 / (*p / AXES).length() * r;
            worst = worst.max((r - surface).abs());
        }
        assert!(worst < 0.03, "a vertex {worst} off the ellipsoid");
        let truth = 4.0 / 3.0 * std::f64::consts::PI * AXES.x * AXES.y * AXES.z;
        assert!((report.volume / truth - 1.0).abs() < 0.02, "volume {} against {truth}", report.volume);
        // Closed: every edge in two facets, once each way.
        let mut edges: HashMap<(u32, u32), i32> = HashMap::new();
        for t in facets.chunks_exact(3) {
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                *edges.entry((a, b)).or_default() += 1;
            }
        }
        for (&(a, b), &count) in &edges {
            assert_eq!(count, 1, "edge {a}-{b} twice the same way");
            assert_eq!(edges.get(&(b, a)), Some(&1), "edge {a}-{b} open");
        }
    }
}
