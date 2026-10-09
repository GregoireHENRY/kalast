//! Chunked level of detail for a large mesh: draw what the camera can see, as
//! finely as it can see it, and nothing else.
//!
//! A 12.9M-facet Mars drawn whole costs ~45 ms of camera pass and ~55 ms of
//! shadow pass a frame on an M1 Pro, whatever is in view: every vertex and
//! every triangle goes through the GPU before the off-screen ones are thrown
//! away, and in AFC's 5.5 deg field nearly all of them are. This builds a
//! binary tree over the facets once, at upload, and picks a cut of it each
//! frame.
//!
//! - **Leaves** hold the facets themselves, about [`LEAF_TRIANGLES`] each,
//!   split by their centres along the longest axis.
//! - **Inner nodes** hold their two children's surfaces simplified by about
//!   half, by collapsing edges no longer than `sqrt 2` times the children's
//!   size. Each node keeps two surfaces. The one it passes up holds its
//!   outline where it is, at full resolution, so two children meet edge for
//!   edge and the seam between them is simplified in the parent like the
//!   rest of it. The one it draws has the outline simplified too, along
//!   itself: held, a coarse node was mostly outline, 2,000-5,000 triangles
//!   where its inside needed 1,500.
//! - **Every node hangs a skirt** from its drawn outline: a strip down the
//!   surface's normal, [`SKIRT_CELLS`] of its cells deep and flared out under
//!   its neighbours. Neighbours drawn with simplified outlines no longer meet
//!   edge for edge, and the gap between them shows the skirt, shaded as the
//!   facet above it, rather than what is behind the body -- and lit as the
//!   outline it hangs from ([`Lod::skirt_top`]): at its own depth, under the
//!   surface, every shadow map has it dark, and the gaps showed as black
//!   dots along the patches' edges.
//! - Every drawn triangle carries the facet it stands for, `facet_of`: an
//!   original facet's own, or for a simplified one the facet it came from.
//!   Colours, values and facet ids are read through it, so they stay the
//!   original per-facet data at every level.
//!
//! [`Lod::select`] walks the tree from the root: a node outside the view is
//! skipped, one whose facets all face away from the camera too (a normal
//! cone), and one whose triangles project to no more than the target size is
//! drawn whole; otherwise its children are tried. The result is a list of
//! triangle ranges into `indices`.

use crate::Vec3;

/// Facets per leaf. A cut of about a million triangles is then a few hundred
/// draws: few enough for the CPU, small enough to cull finely.
pub const LEAF_TRIANGLES: usize = 4096;

/// Meshes below this many facets are drawn whole: the tree would buy nothing.
pub const MIN_FACETS: usize = 4 * LEAF_TRIANGLES;

const NONE: u32 = u32::MAX;

/// How deep a node's skirt hangs, in its own cells: deep enough to cover the
/// gap to a neighbour a couple of levels coarser.
pub const SKIRT_CELLS: f32 = 8.0;

/// How far out a skirt flares under its neighbours, in its own cells. A
/// simplified outline moves sideways as well as down, so the gap it leaves
/// opens sideways too, and a skirt hanging straight down is edge-on to a
/// camera above it: flared, it lies under the neighbour and fills the gap.
pub const SKIRT_FLARE_CELLS: f32 = 3.0;

#[derive(Clone, Copy, Debug)]
pub struct Node {
    /// Bounding sphere, in the mesh's own frame.
    pub center: [f32; 3],
    pub radius: f32,
    /// Every facet's normal under it lies within `acos(cone_cos)` of
    /// `cone_axis`; `cone_cos <= -1` when they face every way.
    pub cone_axis: [f32; 3],
    pub cone_cos: f32,
    /// This node's own triangles: `count` of them from triangle `first` of
    /// [`Lod::indices`] (in triangles, not indices), its surface's `surface`
    /// first and then its skirt's. The sphere holds the skirt too; the cone
    /// only the surfaces, this node's and every one below it.
    pub first: u32,
    pub count: u32,
    pub surface: u32,
    /// The size of its triangles, mesh units: a leaf's mean edge, a simplified
    /// node's longest collapsed edge.
    pub edge: f32,
    pub children: [u32; 2],
}

impl Node {
    pub fn is_leaf(&self) -> bool {
        self.children[0] == NONE
    }
}

/// The tree and the geometry of every node, ready to upload.
#[derive(Clone, Debug, Default)]
pub struct Lod {
    pub nodes: Vec<Node>,
    pub root: u32,
    /// Three per triangle, into `positions` and then `skirt`; every node's
    /// triangles contiguous.
    pub indices: Vec<u32>,
    /// One per triangle of `indices`: the facet it stands for.
    pub facet_of: Vec<u32>,
    /// The skirts' own vertices, numbered after the mesh's `n_positions`.
    pub skirt: Vec<[f32; 3]>,
    /// For each of `skirt`, the vertex of the outline it hangs from, into
    /// `positions`: the vertex stage shades a skirt as the surface there.
    pub skirt_top: Vec<u32>,
    pub n_positions: u32,
    /// The mesh's positions in the order the tree first draws them, so that
    /// a node's vertices sit together in memory. In the mesh's own order a
    /// leaf's vertices came from all over the buffer, a cache line fetched
    /// for every third vertex where packed it is one for ten.
    pub positions: Vec<[f32; 3]>,
}

/// What a selection is made for, in the mesh's own frame.
pub struct View {
    /// Planes `a x + b y + c z + d >= 0` inside, normalised.
    pub planes: Vec<[f32; 4]>,
    /// The back faces to leave out: seen from a point, a camera, or along a
    /// direction, the light's.
    pub facing: Facing,
    /// Where the size of a triangle is judged from: a node is fine when its
    /// edge is at most `edge_per_distance` times its distance from `eye` --
    /// a pixel's angle times the target -- or at most `edge_max`, whichever
    /// is larger. `None` judges by `edge_max` alone.
    pub eye: Option<[f32; 3]>,
    pub edge_per_distance: f32,
    pub edge_max: f32,
    /// A region judged by measures of its own, for a shadow cut: where the
    /// camera looks, by the camera's. There the shadow map then holds the
    /// very triangles the camera draws. A coarser caster over a finer
    /// receiver shadows it wherever the coarse surface passes above the fine
    /// one -- across every crater floor -- and no bias tells that from a
    /// real shadow.
    pub seen: Option<Seen>,
}

/// The camera's frustum and measures, in a shadow [`View`].
pub struct Seen {
    pub planes: Vec<[f32; 4]>,
    pub edge_per_distance: f32,
    pub edge_max: f32,
}

/// Which way a node must face to be drawn.
pub enum Facing {
    /// Every node in view.
    Any,
    /// Toward this point: a camera.
    Point([f32; 3]),
    /// Against this direction, which the light travels along.
    Along([f32; 3]),
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn len(a: [f32; 3]) -> f32 {
    dot(a, a).sqrt()
}
fn p3(v: Vec3) -> [f32; 3] {
    [v.x as f32, v.y as f32, v.z as f32]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn unit(a: [f32; 3]) -> Option<[f32; 3]> {
    let l = len(a);
    (l > 1e-30).then(|| [a[0] / l, a[1] / l, a[2] / l])
}

/// The smallest sphere holding two spheres.
fn merge_spheres(c0: [f32; 3], r0: f32, c1: [f32; 3], r1: f32) -> ([f32; 3], f32) {
    let d = sub(c1, c0);
    let dist = len(d);
    if dist + r1 <= r0 {
        return (c0, r0);
    }
    if dist + r0 <= r1 {
        return (c1, r1);
    }
    let r = 0.5 * (dist + r0 + r1);
    let t = (r - r0) / dist.max(1e-30);
    ([c0[0] + d[0] * t, c0[1] + d[1] * t, c0[2] + d[2] * t], r)
}

/// A cone holding two cones, conservatively.
fn merge_cones(a0: [f32; 3], c0: f32, a1: [f32; 3], c1: f32) -> ([f32; 3], f32) {
    if c0 <= -1.0 || c1 <= -1.0 {
        return ([0.0, 0.0, 1.0], -2.0);
    }
    let s = [a0[0] + a1[0], a0[1] + a1[1], a0[2] + a1[2]];
    let l = len(s);
    if l < 1e-6 {
        return ([0.0, 0.0, 1.0], -2.0);
    }
    let axis = [s[0] / l, s[1] / l, s[2] / l];
    let half = (dot(axis, a0).clamp(-1.0, 1.0).acos() + c0.clamp(-1.0, 1.0).acos())
        .max(dot(axis, a1).clamp(-1.0, 1.0).acos() + c1.clamp(-1.0, 1.0).acos());
    if half >= std::f32::consts::FRAC_PI_2 {
        // Past a hemisphere a cone culls almost nothing; call it no cone.
        return (axis, -2.0);
    }
    (axis, half.cos())
}

/// A cone widened to hold the normals of triangles `[a, b, c, _]`.
fn widen_cone(axis: [f32; 3], cos: f32, tris: &[[u32; 4]], positions: &[Vec3]) -> ([f32; 3], f32) {
    if cos <= -1.0 {
        return (axis, cos);
    }
    let mut half = cos.clamp(-1.0, 1.0).acos();
    for t in tris {
        let [a, b, c] = [0, 1, 2].map(|k| p3(positions[t[k] as usize]));
        if let Some(n) = unit(cross(sub(b, a), sub(c, a))) {
            half = half.max(dot(axis, n).clamp(-1.0, 1.0).acos());
        }
    }
    if half >= std::f32::consts::FRAC_PI_2 { (axis, -2.0) } else { (axis, half.cos()) }
}

/// Sort by the Morton code of a point, within the items' own box: 10 bits
/// an axis, ties by the original order.
fn morton_sort<T: Copy>(items: &mut [T], centre: impl Fn(&T) -> [f32; 3]) {
    if items.len() < 2 {
        return;
    }
    let c: Vec<[f32; 3]> = items.iter().map(&centre).collect();
    let (mut lo, mut hi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    for p in &c {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let spread = |x: u32| {
        let mut x = x & 0x3ff;
        x = (x | (x << 16)) & 0x030000ff;
        x = (x | (x << 8)) & 0x0300f00f;
        x = (x | (x << 4)) & 0x030c30c3;
        (x | (x << 2)) & 0x09249249
    };
    let code = |p: [f32; 3]| {
        let q = |k: usize| (((p[k] - lo[k]) / (hi[k] - lo[k]).max(1e-30)) * 1023.0) as u32;
        spread(q(0)) | (spread(q(1)) << 1) | (spread(q(2)) << 2)
    };
    let mut keyed: Vec<(u32, usize, T)> = items.iter().enumerate().map(|(i, t)| (code(c[i]), i, *t)).collect();
    keyed.sort_unstable_by_key(|k| (k.0, k.1));
    for (dst, k) in items.iter_mut().zip(keyed) {
        *dst = k.2;
    }
}

/// What the build reads: the facets' centres, and the mesh.
struct Source<'a> {
    centres: &'a [[f32; 3]],
    positions: &'a [Vec3],
    indices: &'a [u32],
    normals: &'a [Vec3],
}

/// The facets halved by their centres along the longest axis.
fn halve<'f>(facets: &'f mut [u32], centres: &[[f32; 3]]) -> (&'f mut [u32], &'f mut [u32]) {
    let (mut lo, mut hi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    for &f in facets.iter() {
        let c = centres[f as usize];
        for k in 0..3 {
            lo[k] = lo[k].min(c[k]);
            hi[k] = hi[k].max(c[k]);
        }
    }
    let axis = (0..3).max_by(|&a, &b| (hi[a] - lo[a]).total_cmp(&(hi[b] - lo[b]))).unwrap();
    let mid = facets.len() / 2;
    // Ties broken by the facet's number, so the halves are the same every time.
    facets.select_nth_unstable_by(mid, |&a, &b| {
        centres[a as usize][axis].total_cmp(&centres[b as usize][axis]).then(a.cmp(&b))
    });
    facets.split_at_mut(mid)
}

impl Lod {
    /// The tree over a mesh's facets: `indices` three per facet into
    /// `positions`, `normals` one per facet. The top levels are built on
    /// separate threads, each subtree whole, and stitched; the result is the
    /// same whatever the threads do.
    pub fn build(positions: &[Vec3], indices: &[u32], normals: &[Vec3]) -> Self {
        let n = indices.len() / 3;
        if n == 0 {
            return Lod::default();
        }
        let centres: Vec<[f32; 3]> = (0..n)
            .map(|f| {
                let [a, b, c] = [0, 1, 2].map(|k| p3(positions[indices[3 * f + k] as usize]));
                [(a[0] + b[0] + c[0]) / 3.0, (a[1] + b[1] + c[1]) / 3.0, (a[2] + b[2] + c[2]) / 3.0]
            })
            .collect();
        let mut order: Vec<u32> = (0..n as u32).collect();
        let threads = std::thread::available_parallelism().map_or(1, |t| t.get());
        // Levels split over threads: 2^depth subtrees for the cores there are.
        let depth = (usize::BITS - threads.max(1).leading_zeros()) as usize;
        let src = Source { centres: &centres, positions, indices, normals };
        let (mut lod, _) = Lod::subtree(&mut order, &src, depth);
        lod.n_positions = positions.len() as u32;
        lod.renumber(positions);
        lod.indices.shrink_to_fit();
        lod.facet_of.shrink_to_fit();
        lod
    }

    /// A tree over `facets`, its own, and its root's surface with the outline
    /// held: built here or, `depth` levels more, on two threads.
    fn subtree(facets: &mut [u32], src: &Source, depth: usize) -> (Self, Vec<[u32; 4]>) {
        let mut lod = Lod { n_positions: src.positions.len() as u32, ..Lod::default() };
        let mut scratch = Scratch::default();
        if depth == 0 || facets.len() <= 8 * LEAF_TRIANGLES {
            let (root, held) = lod.split(facets, src, &mut scratch);
            lod.root = root;
            return (lod, held);
        }
        let (left, right) = halve(facets, src.centres);
        let ((a, held_a), (b, held_b)) = std::thread::scope(|scope| {
            let a = scope.spawn(|| Lod::subtree(left, src, depth - 1));
            let b = Lod::subtree(right, src, depth - 1);
            (a.join().unwrap(), b)
        });
        let l = lod.absorb(a);
        let r = lod.absorb(b);
        let (root, held) = lod.inner((l, held_a), (r, held_b), src, &mut scratch);
        lod.root = root;
        (lod, held)
    }

    /// The mesh's vertices numbered in the order `indices` first uses them,
    /// into `positions`; any no facet uses, after.
    fn renumber(&mut self, positions: &[Vec3]) {
        let n = self.n_positions;
        let mut new_of = vec![u32::MAX; n as usize];
        let mut order: Vec<u32> = Vec::with_capacity(n as usize);
        for v in self.indices.iter_mut() {
            if *v < n {
                let slot = &mut new_of[*v as usize];
                if *slot == u32::MAX {
                    *slot = order.len() as u32;
                    order.push(*v);
                }
                *v = *slot;
            }
        }
        order.extend((0..n).filter(|&v| new_of[v as usize] == u32::MAX));
        // An outline's vertices are its triangles', so drawn and numbered.
        for t in &mut self.skirt_top {
            *t = new_of[*t as usize];
        }
        self.positions = order.iter().map(|&v| p3(positions[v as usize])).collect();
    }

    /// Another tree's nodes and geometry, after this one's; its root here.
    fn absorb(&mut self, other: Lod) -> u32 {
        let (node0, tri0) = (self.nodes.len() as u32, self.facet_of.len() as u32);
        for mut node in other.nodes {
            node.first += tri0;
            for c in &mut node.children {
                if *c != NONE {
                    *c += node0;
                }
            }
            self.nodes.push(node);
        }
        let (n, skirt0) = (self.n_positions, self.skirt.len() as u32);
        self.indices.extend(other.indices.iter().map(|&v| if v >= n { v + skirt0 } else { v }));
        self.facet_of.extend_from_slice(&other.facet_of);
        self.skirt.extend_from_slice(&other.skirt);
        self.skirt_top.extend_from_slice(&other.skirt_top);
        other.root + node0
    }

    /// A node over `facets`, and its surface with the outline held.
    fn split(&mut self, facets: &mut [u32], src: &Source, scratch: &mut Scratch) -> (u32, Vec<[u32; 4]>) {
        if facets.len() <= LEAF_TRIANGLES {
            return self.leaf(facets, src.positions, src.indices, src.normals);
        }
        let (left, right) = halve(facets, src.centres);
        let l = self.split(left, src, scratch);
        let r = self.split(right, src, scratch);
        self.inner(l, r, src, scratch)
    }

    fn leaf(&mut self, facets: &[u32], positions: &[Vec3], indices: &[u32], normals: &[Vec3]) -> (u32, Vec<[u32; 4]>) {
        // Drawn in Morton order of their centres: neighbours on the GPU's
        // vertex cache are neighbours on the surface.
        let mut facets = facets.to_vec();
        morton_sort(&mut facets, |f| {
            let t = [0, 1, 2].map(|k| p3(positions[indices[3 * *f as usize + k] as usize]));
            [(t[0][0] + t[1][0] + t[2][0]) / 3.0, (t[0][1] + t[1][1] + t[2][1]) / 3.0, (t[0][2] + t[1][2] + t[2][2]) / 3.0]
        });
        let facets = &facets[..];
        let first = self.facet_of.len() as u32;
        let (mut lo, mut hi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
        let mut sum = [0.0f32; 3];
        let mut edges = 0.0f64;
        for &f in facets {
            let t = [0, 1, 2].map(|k| indices[3 * f as usize + k]);
            self.indices.extend_from_slice(&t);
            self.facet_of.push(f);
            let p = t.map(|i| p3(positions[i as usize]));
            for q in p {
                for k in 0..3 {
                    lo[k] = lo[k].min(q[k]);
                    hi[k] = hi[k].max(q[k]);
                }
            }
            edges += (len(sub(p[0], p[1])) + len(sub(p[1], p[2])) + len(sub(p[2], p[0]))) as f64 / 3.0;
            let nn = p3(normals[f as usize]);
            for k in 0..3 {
                sum[k] += nn[k];
            }
        }
        let center = [0.5 * (lo[0] + hi[0]), 0.5 * (lo[1] + hi[1]), 0.5 * (lo[2] + hi[2])];
        let mut radius = 0.0f32;
        for &f in facets {
            for k in 0..3 {
                let q = p3(positions[indices[3 * f as usize + k] as usize]);
                radius = radius.max(len(sub(q, center)));
            }
        }
        let l = len(sum);
        let (cone_axis, cone_cos) = if l < 1e-6 {
            ([0.0, 0.0, 1.0], -2.0)
        } else {
            let axis = [sum[0] / l, sum[1] / l, sum[2] / l];
            let c = facets
                .iter()
                .map(|&f| dot(p3(normals[f as usize]), axis))
                .fold(1.0f32, f32::min);
            (axis, if c <= 0.0 { -2.0 } else { c })
        };
        let edge = (edges / facets.len().max(1) as f64) as f32;
        let surface = facets.len() as u32;
        let skirt = self.add_skirt(first, surface, positions, SKIRT_CELLS * edge);
        let radius = self.reach(center, radius, first + surface, skirt);
        self.nodes.push(Node {
            center,
            radius: radius * 1.0001 + 1e-6,
            cone_axis,
            cone_cos,
            first,
            count: surface + skirt,
            surface,
            edge,
            children: [NONE, NONE],
        });
        let held = facets.iter().map(|&f| {
            let i = 3 * f as usize;
            [indices[i], indices[i + 1], indices[i + 2], f]
        });
        ((self.nodes.len() - 1) as u32, held.collect())
    }

    /// A sphere's radius grown to hold triangles `first..first + count`.
    fn reach(&self, center: [f32; 3], mut radius: f32, first: u32, count: u32) -> f32 {
        for &v in &self.indices[3 * first as usize..3 * (first + count) as usize] {
            let p = if v < self.n_positions { continue } else { self.skirt[(v - self.n_positions) as usize] };
            radius = radius.max(len(sub(p, center)));
        }
        radius
    }

    /// A skirt under the surface triangles `first..first + surface`: for
    /// every edge of their outline, a quad down to copies of its ends moved
    /// `depth` down the surface's normal there and `flare` out of the node,
    /// facing out of it and standing for the facet above. The normal is the
    /// surface's own, not the direction from the body's centre, which on an
    /// elongated body like Deimos leans far off it. Returns how many
    /// triangles it took.
    fn add_skirt(&mut self, first: u32, surface: u32, positions: &[Vec3], depth: f32) -> u32 {
        let flare = depth * SKIRT_FLARE_CELLS / SKIRT_CELLS;
        let tri = |lod: &Lod, t: u32| {
            let i = 3 * t as usize;
            [lod.indices[i], lod.indices[i + 1], lod.indices[i + 2]]
        };
        let mut edges: Vec<(u64, u32, u32, u32)> = Vec::with_capacity(3 * surface as usize);
        for t in first..first + surface {
            let v = tri(self, t);
            for k in 0..3 {
                let (a, b) = (v[k], v[(k + 1) % 3]);
                edges.push((((a.min(b) as u64) << 32) | a.max(b) as u64, a, b, t));
            }
        }
        edges.sort_unstable_by_key(|e| e.0);
        let mut outline: Vec<(u32, u32, u32)> = Vec::new();
        let mut i = 0;
        while i < edges.len() {
            let mut j = i + 1;
            while j < edges.len() && edges[j].0 == edges[i].0 {
                j += 1;
            }
            if j - i == 1 {
                outline.push((edges[i].1, edges[i].2, edges[i].3));
            }
            i = j;
        }
        if outline.is_empty() {
            return 0;
        }
        let mut ends: Vec<u32> = outline.iter().flat_map(|e| [e.0, e.1]).collect();
        ends.sort_unstable();
        ends.dedup();
        let at = |v: u32| p3(positions[v as usize]);
        // The surface's normal at each end: its triangles' there, by area.
        let mut up = vec![[0.0f32; 3]; ends.len()];
        for t in first..first + surface {
            let v = tri(self, t);
            let n = cross(sub(at(v[1]), at(v[0])), sub(at(v[2]), at(v[0])));
            for w in v {
                if let Ok(k) = ends.binary_search(&w) {
                    for c in 0..3 {
                        up[k][c] += n[c];
                    }
                }
            }
        }
        // Out of the node: the outline runs with its triangle on the left
        // seen from above, so out is the edge crossed with that triangle's
        // normal, summed over a vertex's two edges.
        let mut out = vec![[0.0f32; 3]; ends.len()];
        for &(a, b, t) in &outline {
            let v = tri(self, t);
            let Some(n) = unit(cross(sub(at(v[1]), at(v[0])), sub(at(v[2]), at(v[0])))) else { continue };
            let o = cross(sub(at(b), at(a)), n);
            for w in [a, b] {
                let k = ends.binary_search(&w).unwrap();
                for c in 0..3 {
                    out[k][c] += o[c];
                }
            }
        }
        let base = self.n_positions + self.skirt.len() as u32;
        for (k, &v) in ends.iter().enumerate() {
            let p = at(v);
            let up = unit(up[k]).or_else(|| unit(p)).unwrap_or([0.0, 0.0, 1.0]);
            // Kept across the normal, and unit.
            let o = out[k];
            let o = unit(sub(o, [up[0] * dot(o, up), up[1] * dot(o, up), up[2] * dot(o, up)])).unwrap_or([0.0; 3]);
            self.skirt.push([
                p[0] - depth * up[0] + flare * o[0],
                p[1] - depth * up[1] + flare * o[1],
                p[2] - depth * up[2] + flare * o[2],
            ]);
            self.skirt_top.push(v);
        }
        let below = |v: u32| base + ends.binary_search(&v).unwrap() as u32;
        for &(a, b, t) in &outline {
            let (a2, b2) = (below(a), below(b));
            let f = self.facet_of[t as usize];
            self.indices.extend_from_slice(&[b, a, a2, b, a2, b2]);
            self.facet_of.extend_from_slice(&[f, f]);
        }
        2 * outline.len() as u32
    }

    /// A node over two: their surfaces, which meet edge for edge, simplified
    /// inside to about half; for drawing, its outline simplified as well,
    /// along itself, and a skirt. Returns the node, and its surface with the
    /// outline held for its parent.
    fn inner(
        &mut self,
        (l, held_l): (u32, Vec<[u32; 4]>),
        (r, held_r): (u32, Vec<[u32; 4]>),
        src: &Source,
        scratch: &mut Scratch,
    ) -> (u32, Vec<[u32; 4]>) {
        let positions = src.positions;
        let (nl, nr) = (self.nodes[l as usize], self.nodes[r as usize]);
        let child = nl.edge.max(nr.edge);
        let cell = child * std::f32::consts::SQRT_2;
        let mut held = held_l;
        held.extend_from_slice(&held_r);
        drop(held_r);
        simplify(&mut held, src, cell, 2.0 * cell, Moves::Inside, scratch);
        // Outline edges come out between one and two of `child` long: about
        // the inside's `cell` on average.
        let mut tris = held.clone();
        simplify(&mut tris, src, child, 2.0 * cell, Moves::Outline, scratch);
        morton_sort(&mut tris, |t| {
            let p = [0, 1, 2].map(|k| p3(positions[t[k] as usize]));
            [(p[0][0] + p[1][0] + p[2][0]) / 3.0, (p[0][1] + p[1][1] + p[2][1]) / 3.0, (p[0][2] + p[1][2] + p[2][2]) / 3.0]
        });
        let first = self.facet_of.len() as u32;
        for t in &tris {
            self.indices.extend_from_slice(&t[..3]);
            self.facet_of.push(t[3]);
        }
        let surface = tris.len() as u32;
        let skirt = self.add_skirt(first, surface, positions, SKIRT_CELLS * cell);
        let (center, radius) = merge_spheres(nl.center, nl.radius, nr.center, nr.radius);
        let radius = self.reach(center, radius, first + surface, skirt);
        let (cone_axis, cone_cos) = merge_cones(nl.cone_axis, nl.cone_cos, nr.cone_axis, nr.cone_cos);
        let (cone_axis, cone_cos) = widen_cone(cone_axis, cone_cos, &tris, positions);
        self.nodes.push(Node {
            center,
            radius: radius * 1.0001 + 1e-6,
            cone_axis,
            cone_cos,
            first,
            count: surface + skirt,
            surface,
            edge: cell,
            children: [l, r],
        });
        ((self.nodes.len() - 1) as u32, held)
    }

    /// The cut for one view: triangle ranges `[first, count]` into `indices`,
    /// adjacent ones merged, and the nodes drawn into `drawn` when given.
    /// Returns the box round the spheres of the nodes drawn, mesh frame, or
    /// `None` when nothing is.
    pub fn select(
        &self,
        view: &View,
        out: &mut Vec<[u32; 2]>,
        mut drawn: Option<&mut Vec<u32>>,
    ) -> Option<([f32; 3], [f32; 3])> {
        out.clear();
        if let Some(d) = drawn.as_deref_mut() {
            d.clear();
        }
        if self.nodes.is_empty() {
            return None;
        }
        let (mut lo, mut hi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
        let mut stack = vec![self.root];
        while let Some(i) = stack.pop() {
            let n = &self.nodes[i as usize];
            if view
                .planes
                .iter()
                .any(|p| p[0] * n.center[0] + p[1] * n.center[1] + p[2] * n.center[2] + p[3] < -n.radius)
            {
                continue;
            }
            if n.cone_cos > -1.0 {
                // Back-facing throughout: the view direction within 90 deg,
                // less the cone, less the sphere's own angle, of the axis.
                let half = n.cone_cos.clamp(-1.0, 1.0).acos();
                let back = match view.facing {
                    Facing::Any => false,
                    Facing::Point(eye) => {
                        let v = sub(n.center, eye);
                        let d = len(v);
                        let spread = half + (n.radius / d.max(n.radius)).asin();
                        d > n.radius
                            && spread < std::f32::consts::FRAC_PI_2
                            && dot(v, n.cone_axis) / d > spread.sin()
                    }
                    Facing::Along(dir) => {
                        half < std::f32::consts::FRAC_PI_2 && dot(dir, n.cone_axis) > half.sin()
                    }
                };
                if back {
                    continue;
                }
            }
            let (per_distance, at_least) = match &view.seen {
                Some(seen)
                    if seen.planes.iter().all(|p| {
                        p[0] * n.center[0] + p[1] * n.center[1] + p[2] * n.center[2] + p[3] >= -n.radius
                    }) =>
                {
                    (seen.edge_per_distance, seen.edge_max)
                }
                _ => (view.edge_per_distance, view.edge_max),
            };
            let allowed = match view.eye {
                Some(eye) => at_least.max(per_distance * (len(sub(n.center, eye)) - n.radius).max(1e-9)),
                None => at_least,
            };
            let fine = n.edge <= allowed;
            if fine || n.is_leaf() {
                if n.count == 0 {
                    continue;
                }
                for k in 0..3 {
                    lo[k] = lo[k].min(n.center[k] - n.radius);
                    hi[k] = hi[k].max(n.center[k] + n.radius);
                }
                if let Some(d) = drawn.as_deref_mut() {
                    d.push(i);
                }
                match out.last_mut() {
                    Some(last) if last[0] + last[1] == n.first => last[1] += n.count,
                    _ => out.push([n.first, n.count]),
                }
            } else {
                stack.push(n.children[1]);
                stack.push(n.children[0]);
            }
        }
        (!out.is_empty()).then_some((lo, hi))
    }

    /// The triangles a cut draws.
    pub fn count(ranges: &[[u32; 2]]) -> u64 {
        ranges.iter().map(|r| r[1] as u64).sum()
    }
}

/// The least cosine between a simplified triangle's normal and that of the
/// facet it stands for: past about 80 deg, it is folding over.
const UPRIGHT: f32 = 0.17;

/// What [`simplify`] may move.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Moves {
    /// The inside, the outline held where it is: the surface a node passes
    /// up, so that two children meet edge for edge in their parent.
    Inside,
    /// The outline alone, each vertex along it onto a neighbour: the surface
    /// a node draws, its edge as coarse as its inside.
    Outline,
}

#[derive(Default)]
struct Scratch {
    /// The surface's vertices, sorted: a local index is a position in here.
    vids: Vec<u32>,
    local: Vec<[u32; 3]>,
    pos: Vec<[f32; 3]>,
    around: Vec<Vec<u32>>,
    ring_u: Vec<u32>,
    ring_v: Vec<u32>,
    shared: Vec<u32>,
}

/// The vertices round `x` on live triangles, sorted.
fn ring(around: &[Vec<u32>], alive: &[bool], local: &[[u32; 3]], x: u32, out: &mut Vec<u32>) {
    out.clear();
    for &t in &around[x as usize] {
        if alive[t as usize] {
            out.extend(local[t as usize].iter().copied().filter(|&w| w != x));
        }
    }
    out.sort_unstable();
    out.dedup();
}

fn tri_normal(pos: &[[f32; 3]], t: [u32; 3]) -> [f32; 3] {
    let [a, b, c] = t.map(|i| pos[i as usize]);
    cross(sub(b, a), sub(c, a))
}

/// Simplify a surface by collapsing edges, shortest first and none longer
/// than `cell`: one end moves onto the other, and the triangles they shared
/// go. [`Moves::Inside`] stops at half the triangles. A collapse that would
/// leave an edge longer than `longest`, turn a triangle over or crush it
/// flat, or join two parts of the surface that only touched (the link
/// condition), is refused, so the triangles stay the size the node claims
/// and the surface stays
/// one sheet without holes -- which vertex clustering left, where cells met
/// and the triangles across them were cut along the wrong diagonals.
/// Vertices keep their places, so every node draws from the mesh's own
/// positions. On tables local to the surface, ties broken by the vertices'
/// numbers: the same result every run.
fn simplify(tris: &mut Vec<[u32; 4]>, src: &Source, cell: f32, longest: f32, moves: Moves, s: &mut Scratch) {
    let positions = src.positions;
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;

    s.vids.clear();
    for t in tris.iter() {
        s.vids.extend_from_slice(&t[..3]);
    }
    s.vids.sort_unstable();
    s.vids.dedup();
    let nv = s.vids.len();
    let local = |vids: &[u32], v: u32| vids.binary_search(&v).unwrap() as u32;
    s.local.clear();
    for t in tris.iter() {
        s.local.push([local(&s.vids, t[0]), local(&s.vids, t[1]), local(&s.vids, t[2])]);
    }
    s.pos.clear();
    s.pos.extend(s.vids.iter().map(|&v| p3(positions[v as usize])));
    let nt = s.local.len();

    let mut around = std::mem::take(&mut s.around);
    if around.len() < nv {
        around.resize_with(nv, Vec::new);
    }
    for a in &mut around[..nv] {
        a.clear();
    }
    for (t, tri) in s.local.iter().enumerate() {
        for &v in tri {
            around[v as usize].push(t as u32);
        }
    }
    // The outline: edges one triangle uses.
    let mut edges: Vec<(u32, u32)> = Vec::with_capacity(3 * nt);
    for tri in &s.local {
        for k in 0..3 {
            let (a, b) = (tri[k], tri[(k + 1) % 3]);
            edges.push((a.min(b), a.max(b)));
        }
    }
    edges.sort_unstable();
    let mut outline = vec![false; nv];
    let mut unique: Vec<(u32, u32, usize)> = Vec::with_capacity(edges.len() / 2 + 1);
    let mut i = 0;
    while i < edges.len() {
        let mut j = i + 1;
        while j < edges.len() && edges[j] == edges[i] {
            j += 1;
        }
        if j - i == 1 {
            outline[edges[i].0 as usize] = true;
            outline[edges[i].1 as usize] = true;
        }
        unique.push((edges[i].0, edges[i].1, j - i));
        i = j;
    }
    drop(edges);
    // Which edges may go: in `Inside` one with an end off the outline, in
    // `Outline` one of the outline's own.
    let movable = |a: u32, b: u32, uses: usize| match moves {
        Moves::Inside => !(outline[a as usize] && outline[b as usize]),
        Moves::Outline => uses == 1,
    };
    let length = |pos: &[[f32; 3]], a: u32, b: u32| len(sub(pos[a as usize], pos[b as usize]));
    let mut heap: BinaryHeap<Reverse<(u32, u32, u32)>> = unique
        .iter()
        .filter(|&&(a, b, uses)| movable(a, b, uses))
        .map(|&(a, b, _)| (length(&s.pos, a, b), a, b))
        .filter(|&(l, _, _)| l <= cell)
        .map(|(l, a, b)| Reverse((l.to_bits(), a, b)))
        .collect();
    drop(unique);

    let mut alive = vec![true; nt];
    let mut gone = vec![false; nv];
    let mut live = nt;
    let target = if moves == Moves::Inside { nt / 2 } else { 0 };
    let mut shared = std::mem::take(&mut s.shared);
    let mut ring_u = std::mem::take(&mut s.ring_u);
    let mut ring_v = std::mem::take(&mut s.ring_v);
    while live > target {
        let Some(Reverse((_, a, b))) = heap.pop() else { break };
        if gone[a as usize] || gone[b as usize] {
            continue;
        }
        shared.clear();
        shared.extend(
            around[a as usize]
                .iter()
                .copied()
                .filter(|&t| alive[t as usize] && s.local[t as usize].contains(&b)),
        );
        if shared.is_empty() || (moves == Moves::Outline && shared.len() != 1) {
            continue;
        }
        for (u, v) in [(a, b), (b, a)] {
            if moves == Moves::Inside && outline[u as usize] {
                continue;
            }
            // The link condition: the two ends share no neighbour but the
            // triangles' they share.
            ring(&around, &alive, &s.local, u, &mut ring_u);
            ring(&around, &alive, &s.local, v, &mut ring_v);
            if ring_u.iter().filter(|w| ring_v.binary_search(w).is_ok()).count() != shared.len() {
                continue;
            }
            if ring_u.iter().any(|&w| w != v && length(&s.pos, v, w) > longest) {
                continue;
            }
            // No triangle turned over, none crushed flat, none standing up
            // off the facet it stands for: a run of collapses, each turning
            // a triangle less than over, can fold it all the same.
            let bad = around[u as usize].iter().any(|&t| {
                if !alive[t as usize] || shared.contains(&t) {
                    return false;
                }
                let tri = s.local[t as usize];
                let (n0, n1) = (tri_normal(&s.pos, tri), tri_normal(&s.pos, tri.map(|w| if w == u { v } else { w })));
                let facet = p3(src.normals[tris[t as usize][3] as usize]);
                dot(n0, n1) <= 0.0 || len(n1) <= 1e-6 * len(n0) || dot(n1, facet) <= UPRIGHT * len(n1)
            });
            if bad {
                continue;
            }
            for &t in &shared {
                alive[t as usize] = false;
            }
            live -= shared.len();
            let mut moved = std::mem::take(&mut around[u as usize]);
            for &t in &moved {
                if alive[t as usize] {
                    for w in s.local[t as usize].iter_mut() {
                        if *w == u {
                            *w = v;
                        }
                    }
                    around[v as usize].push(t);
                }
            }
            moved.clear();
            around[u as usize] = moved;
            around[v as usize].retain(|&t| alive[t as usize]);
            gone[u as usize] = true;
            // The edges round `v` are new.
            ring(&around, &alive, &s.local, v, &mut ring_v);
            for &w in &ring_v {
                let l = length(&s.pos, v, w);
                if l > cell {
                    continue;
                }
                let uses = around[v as usize].iter().filter(|&&t| s.local[t as usize].contains(&w)).count();
                if movable(v, w, uses) {
                    heap.push(Reverse((l.to_bits(), v.min(w), v.max(w))));
                }
            }
            break;
        }
    }

    let kept: Vec<[u32; 4]> = tris
        .iter()
        .zip(s.local.iter().zip(&alive))
        .filter(|(_, (_, a))| **a)
        .map(|(t, (l, _))| [s.vids[l[0] as usize], s.vids[l[1] as usize], s.vids[l[2] as usize], t[3]])
        .collect();
    *tris = kept;
    s.around = around;
    s.shared = shared;
    s.ring_u = ring_u;
    s.ring_v = ring_v;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An icosphere of `level` subdivisions: facets and their normals.
    fn icosphere(level: u32) -> (Vec<Vec3>, Vec<u32>, Vec<Vec3>) {
        let g = (1.0 + 5f32.sqrt()) / 2.0;
        let mut v: Vec<Vec3> = [
            (-1.0, g, 0.0), (1.0, g, 0.0), (-1.0, -g, 0.0), (1.0, -g, 0.0), (0.0, -1.0, g), (0.0, 1.0, g),
            (0.0, -1.0, -g), (0.0, 1.0, -g), (g, 0.0, -1.0), (g, 0.0, 1.0), (-g, 0.0, -1.0), (-g, 0.0, 1.0),
        ]
        .iter()
        .map(|&(x, y, z)| Vec3::new(x, y, z).normalize())
        .collect();
        let mut f: Vec<[u32; 3]> = vec![
            [0, 11, 5], [0, 5, 1], [0, 1, 7], [0, 7, 10], [0, 10, 11], [1, 5, 9], [5, 11, 4], [11, 10, 2],
            [10, 7, 6], [7, 1, 8], [3, 9, 4], [3, 4, 2], [3, 2, 6], [3, 6, 8], [3, 8, 9], [4, 9, 5],
            [2, 4, 11], [6, 2, 10], [8, 6, 7], [9, 8, 1],
        ];
        for _ in 0..level {
            let mut mid = std::collections::HashMap::new();
            let mut next = Vec::new();
            let mut m = |a: u32, b: u32, v: &mut Vec<Vec3>| -> u32 {
                *mid.entry((a.min(b), a.max(b))).or_insert_with(|| {
                    v.push(((v[a as usize] + v[b as usize]) * 0.5).normalize());
                    (v.len() - 1) as u32
                })
            };
            for t in &f {
                let (ab, bc, ca) = (m(t[0], t[1], &mut v), m(t[1], t[2], &mut v), m(t[2], t[0], &mut v));
                next.extend([[t[0], ab, ca], [ab, t[1], bc], [ca, bc, t[2]], [ab, bc, ca]]);
            }
            f = next;
        }
        let normals = f
            .iter()
            .map(|t| (v[t[1] as usize] - v[t[0] as usize]).cross(v[t[2] as usize] - v[t[0] as usize]).normalize())
            .collect();
        (v, f.concat(), normals)
    }

    fn view_from(eye: [f32; 3], edge_per_distance: f32) -> View {
        View { planes: vec![], facing: Facing::Point(eye), eye: Some(eye), edge_per_distance, edge_max: 0.0, seen: None }
    }

    /// Every edge of every node's outline hangs a skirt: two triangles down
    /// to copies of its ends moved toward the centre by the node's depth,
    /// standing for the facet above, and each copy knowing the end it hangs
    /// from (`skirt_top`), which is where it is lit. That is what covers the
    /// gap between neighbours drawn at different levels.
    #[test]
    fn every_outline_edge_hangs_a_skirt() {
        let (v, f, n) = icosphere(6);
        let lod = Lod::build(&v, &f, &n);
        let pos = |i: u32| -> [f32; 3] {
            if i < lod.n_positions { lod.positions[i as usize] } else { lod.skirt[(i - lod.n_positions) as usize] }
        };
        let mut checked = 0;
        for node in &lod.nodes {
            let mut edges = std::collections::HashMap::new();
            for t in node.first..node.first + node.surface {
                let i = 3 * t as usize;
                for k in 0..3 {
                    let (a, b) = (lod.indices[i + k], lod.indices[i + (k + 1) % 3]);
                    assert!(a < lod.n_positions, "a surface uses the mesh's own vertices");
                    *edges.entry((a.min(b), a.max(b))).or_insert(0) += 1;
                }
            }
            let outline = edges.values().filter(|&&c| c == 1).count() as u32;
            assert_eq!(node.count - node.surface, 2 * outline, "two skirt triangles an outline edge");
            for t in (node.first + node.surface..node.first + node.count).step_by(2) {
                let i = 3 * t as usize;
                let (b, a, a2) = (lod.indices[i], lod.indices[i + 1], lod.indices[i + 2]);
                let b2 = lod.indices[i + 5];
                assert_eq!(edges.get(&(a.min(b), a.max(b))), Some(&1), "on the outline");
                let top = |v: u32| lod.skirt_top[(v - lod.n_positions) as usize];
                assert_eq!((top(a2), top(b2)), (a, b), "each copy knows the end it hangs from");
                let (pa, pa2) = (pos(a), pos(a2));
                let up = [pa[0] / len(pa), pa[1] / len(pa), pa[2] / len(pa)];
                let drop = sub(pa, pa2);
                // Down the surface's normal, which on a sphere is nearly the
                // radius, and out across it.
                let (deep, flare) = (SKIRT_CELLS * node.edge, SKIRT_FLARE_CELLS * node.edge);
                let reach = (deep * deep + flare * flare).sqrt();
                assert!((len(drop) - reach).abs() < 1e-3 * reach + 1e-6, "{} long", len(drop));
                let depth = dot(drop, up);
                assert!((depth - deep).abs() < 0.25 * deep, "{depth} deep of {deep}");
                checked += 1;
            }
        }
        assert!(checked > 1000, "{checked}");
    }

    /// A coarse node is coarse throughout: its outline simplified with its
    /// interior, so the root of an 81,920-facet sphere is a few thousand
    /// triangles, skirt included, not an outline of the original facets.
    #[test]
    fn a_coarse_node_is_coarse_to_its_edge() {
        let (v, f, n) = icosphere(6);
        let lod = Lod::build(&v, &f, &n);
        let root = &lod.nodes[lod.root as usize];
        assert!(root.count < 3 * LEAF_TRIANGLES as u32, "root {} triangles", root.count);
    }

    /// Two builds of one mesh are the same tree, threads or not: what a frame
    /// draws must not depend on which thread finished first, or on a hash.
    #[test]
    fn a_build_is_the_same_every_time() {
        let (v, f, n) = icosphere(6);
        let a = Lod::build(&v, &f, &n);
        let b = Lod::build(&v, &f, &n);
        assert_eq!(a.indices, b.indices);
        assert_eq!(a.facet_of, b.facet_of);
        assert_eq!(a.nodes.len(), b.nodes.len());
    }

    /// The farther, the fewer: the same mesh seen from twice as far draws
    /// fewer triangles, and from far enough the root alone. Without the back
    /// faces culled, which would hide more of a sphere the nearer the eye.
    #[test]
    fn a_farther_view_draws_fewer_triangles() {
        let (v, f, n) = icosphere(6);
        let mut lod = Lod::build(&v, &f, &n);
        for node in &mut lod.nodes {
            node.cone_cos = -2.0;
        }
        let mut ranges = Vec::new();
        let mut last = u64::MAX;
        for d in [1.1, 2.0, 4.0, 8.0, 1000.0] {
            lod.select(&view_from([0.0, 0.0, d], 0.01), &mut ranges, None);
            let c = Lod::count(&ranges);
            assert!(c <= last, "at {d}: {c} after {last}");
            last = c;
        }
        let root = &lod.nodes[lod.root as usize];
        assert_eq!(last, root.count as u64, "from far away, the root alone");
        assert!((root.count as usize) < f.len() / 3 / 8, "the root is much coarser: {}", root.count);
    }

    /// Where the camera looks, a shadow cut draws the camera's own triangles,
    /// however much coarser it is elsewhere: every camera node in the seen
    /// region facing the light is in the shadow cut too.
    #[test]
    fn a_shadow_cut_matches_the_camera_where_it_looks() {
        let (v, f, n) = icosphere(7);
        let lod = Lod::build(&v, &f, &n);
        let eye = [0.0f32, 0.0, 3.0];
        let s = 0.15f32;
        let planes: Vec<[f32; 4]> = [[1.0, 0.0, s], [-1.0, 0.0, s], [0.0, 1.0, s], [0.0, -1.0, s]]
            .iter()
            .map(|p: &[f32; 3]| {
                let l = len(*p);
                let nn = [p[0] / l, p[1] / l, -p[2] / l];
                [nn[0], nn[1], nn[2], -dot(nn, eye)]
            })
            .collect();
        let camera = View {
            planes: planes.clone(),
            facing: Facing::Point(eye),
            eye: Some(eye),
            edge_per_distance: 0.004,
            edge_max: 0.0,
            seen: None,
        };
        let (mut ranges, mut nodes) = (Vec::new(), Vec::new());
        lod.select(&camera, &mut ranges, Some(&mut nodes));
        // The Sun straight behind the camera: everything the camera sees is lit.
        let shadow = View {
            planes: vec![],
            facing: Facing::Along([0.0, 0.0, -1.0]),
            eye: Some(eye),
            edge_per_distance: 0.05,
            edge_max: 0.0,
            seen: Some(Seen { planes, edge_per_distance: 0.004, edge_max: 0.0 }),
        };
        let mut shadow_nodes = Vec::new();
        lod.select(&shadow, &mut ranges, Some(&mut shadow_nodes));
        assert!(nodes.len() > 10, "{} camera nodes", nodes.len());
        let missing = nodes.iter().filter(|k| !shadow_nodes.contains(k)).count();
        assert_eq!(missing, 0, "{missing} of {} camera nodes not in the shadow cut", nodes.len());
        // And coarser outside it.
        let coarse = View { seen: None, ..shadow };
        lod.select(&coarse, &mut ranges, Some(&mut shadow_nodes));
        assert!(nodes.iter().filter(|k| !shadow_nodes.contains(k)).count() > 0);
    }

    /// Culling never drops a facet that faces the camera inside the view:
    /// every such facet's triangle range, or an ancestor's, is drawn.
    #[test]
    fn culling_keeps_every_facet_that_faces_the_camera() {
        let (v, f, n) = icosphere(7); // 327,680 facets, 80 leaves
        let lod = Lod::build(&v, &f, &n);
        let eye = [0.0f32, 0.3, 2.5];
        // A narrow view down -z: four side planes through the eye.
        let s = 0.2f32;
        let planes: Vec<[f32; 4]> = [[1.0, 0.0, s], [-1.0, 0.0, s], [0.0, 1.0, s], [0.0, -1.0, s]]
            .iter()
            .map(|p: &[f32; 3]| {
                let l = len(*p);
                let nn = [p[0] / l, p[1] / l, -p[2] / l];
                [nn[0], nn[1], nn[2], -dot(nn, eye)]
            })
            .collect();
        let view = View {
            planes: planes.clone(),
            facing: Facing::Point(eye),
            eye: Some(eye),
            edge_per_distance: 0.0,
            edge_max: 0.0,
            seen: None,
        };
        let mut ranges = Vec::new();
        lod.select(&view, &mut ranges, None);
        let drawn: std::collections::HashSet<u32> =
            ranges.iter().flat_map(|r| (r[0]..r[0] + r[1]).map(|t| lod.facet_of[t as usize])).collect();
        let mut missing = 0;
        let mut wanted = 0;
        for k in 0..f.len() / 3 {
            let c = [0, 1, 2].map(|j| p3(v[f[3 * k + j] as usize]));
            let cen = [(c[0][0] + c[1][0] + c[2][0]) / 3.0, (c[0][1] + c[1][1] + c[2][1]) / 3.0, (c[0][2] + c[1][2] + c[2][2]) / 3.0];
            let inside = planes.iter().all(|p| p[0] * cen[0] + p[1] * cen[1] + p[2] * cen[2] + p[3] > 0.0);
            let facing = dot(p3(n[k]), sub(eye, cen)) > 0.0;
            if inside && facing {
                wanted += 1;
                missing += (!drawn.contains(&(k as u32))) as usize;
            }
        }
        assert!(wanted > 1000, "{wanted}");
        assert_eq!(missing, 0, "{missing} of {wanted} visible facets culled");
        assert!(
            Lod::count(&ranges) < (f.len() / 3) as u64 / 4,
            "and most of the sphere was not drawn: {} of {}",
            Lod::count(&ranges),
            f.len() / 3
        );
    }
}
