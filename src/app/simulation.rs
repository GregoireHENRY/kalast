use crate::Mat4;
use std::{cell::RefCell, rc::Rc};

#[derive(Debug)]
pub struct Simulation {
    /// How this simulation is shaded, shadowed, labelled and exported.
    ///
    /// Owned here, not by `App`. Nearly every option describes the thing
    /// being simulated or the image made of it, so this is where it belongs;
    /// `App` has a config of its own for the application around it.
    ///
    /// A handle rather than a plain field because Python holds it too --
    /// `app.simulation.config` has to stay reachable while the render loop
    /// borrows the simulation.
    pub config: std::rc::Rc<std::cell::RefCell<crate::app::config::Config>>,

    pub state: State,

    pub bodies: Vec<crate::app::body::Body>,
    pub camera: crate::app::frame::Eye,
    pub sun: crate::app::frame::Eye,

    pub export: bool,
    pub export_once: bool,

    /// One-off request for a single body's fractions, for callers that only
    /// want them at particular epochs. `config.access_shadow_map` is the usual
    /// route and covers every body every frame.
    pub facet_shadow_request: Option<usize>,
    /// Per-facet occluded fractions, indexed by body. Empty for bodies not
    /// queried this frame. Written after the render pass, which is the first
    /// moment the shadow map reflects this frame's geometry.
    pub facet_shadow_result: Vec<Vec<f32>>,

    /// One-off request for the camera-view facet index map. Never a
    /// per-frame config flag: it costs a second geometry pass and a blocking
    /// readback, and it is wanted for the handful of frames a data product
    /// comes from, not for every frame of a run.
    pub facet_id_request: bool,
    /// `(pixels, offsets, width, height)` from the last request. Pixels hold
    /// `1 + offset[body] + facet`, or 0 where no facet was drawn.
    pub facet_id_result: Option<(Vec<u32>, Vec<u32>, u32, u32)>,

    /// Pending hemicube request: `(body, facets, resolution, batch)`.
    /// A one-off like the ID map, and for the same reason -- it is a
    /// precompute, not something a frame loop should carry.
    pub hemicube_request: Option<(usize, Vec<u32>, u32, u32)>,
    /// From the last request: row-major view factors, the number of rows,
    /// the total facet count across bodies, and each body's index offset.
    pub hemicube_result: Option<(Vec<f32>, usize, usize, Vec<u32>)>,

    /// Text drawn in the window's top-left corner, or empty for none.
    ///
    /// Set from a callback. Deliberately a free string rather than an
    /// automatic iteration counter: the number worth watching is usually not
    /// `state.iteration` -- a TPM run steps its own counter, which stops
    /// while a view-factor rebuild spans several frames, so the two diverge.
    ///
    /// Drawn onto the swapchain after the scene has been blitted to it, so it
    /// never reaches `render_texture` and never appears in exported frames.
    /// The live HUDs, shared with `Config::huds` -- the same objects, so
    /// editing one here is editing the one that gets drawn.
    pub huds: Vec<std::rc::Rc<std::cell::RefCell<crate::app::config::Hud>>>,

    /// Set when a body's geometry has been replaced rather than moved --
    /// a mesh reloaded from another file, flattened or smoothed, a body
    /// added in the middle or removed. The GPU buffers are built from the
    /// meshes once and then only their transforms are re-uploaded, so a
    /// change of topology is invisible until they are rebuilt; `sync_meshes`
    /// takes this flag and does that.
    ///
    /// Adding or removing at the *end* is noticed on its own, from the body
    /// count. This is for everything that leaves the count the same, or
    /// shifts which body an index means.
    pub meshes_dirty: bool,

    /// Facets picked by clicking, or added by index.
    ///
    /// Each remembers what its vertices looked like before, so deselecting
    /// puts the facet back rather than leaving it a colour nobody chose.
    pub selected_facets: Vec<Selection>,

    /// What the last rendered frame could actually see. Written by the
    /// renderer after the frustums are fitted, read by the HUD placeholders.
    pub diagnostics: Diagnostics,
}

/// Which bodies the camera frustum contains, and why the others are missing.
///
/// The automatic frustum fits to the bodies, so a body outside it is usually a
/// sign something else is wrong -- a stale transform, a body parked far away
/// to hide it, a pinned plane left over from an older script. Worth being able
/// to see rather than infer from an empty frame.
#[derive(Debug, Clone, Copy, Default)]
pub struct Diagnostics {
    pub n_bodies: usize,
    pub n_visible: usize,
    /// Bodies wholly outside one plane. A body can only be counted once, so
    /// these sum with `n_visible` to `n_bodies`.
    pub out_near: usize,
    pub out_far: usize,
    pub out_side: usize,
    /// The debug light cube is enabled but sits beyond the camera's far
    /// plane, so it is being drawn and clipped away.
    pub light_cube_clipped: bool,
    /// Per-pass GPU times, when `config.gpu_timing` is on. All zero and
    /// `valid: false` otherwise, and on an adapter without timestamp queries.
    pub gpu: crate::app::gpu_timing::Timings,
}

impl Simulation {
    pub fn new() -> Self {
        let mut sun = crate::app::frame::Eye::new();
        sun.projection.mode = crate::app::frame::ProjectionMode::Orthographic;

        Self {
            config: std::rc::Rc::new(std::cell::RefCell::new(
                crate::app::config::Config::default(),
            )),

            state: State::new(),

            bodies: vec![],
            camera: crate::app::frame::Eye::new(),
            sun,

            export: false,
            export_once: false,

            facet_shadow_request: None,
            facet_shadow_result: vec![],

            facet_id_request: false,
            facet_id_result: None,

            hemicube_request: None,
            hemicube_result: None,
            huds: Vec::new(),
            selected_facets: Vec::new(),
            meshes_dirty: false,
            diagnostics: Diagnostics::default(),
        }
    }

    pub fn load_mesh<P>(&mut self, path: P, mat: Mat4, flatten: bool)
    where
        P: AsRef<std::path::Path>,
    {
        self.load_mesh_with_shadow(path, mat, flatten, None::<&std::path::Path>);
    }

    /// As `load_mesh`, but renders `shadow_path` into the shadow map instead
    /// of the main mesh. See `Body::shadow_mesh` for why that is safe for
    /// facet-indexed data when swapping the main mesh would not be.
    pub fn load_mesh_with_shadow<P, S>(
        &mut self,
        path: P,
        mat: Mat4,
        flatten: bool,
        shadow_path: Option<S>,
    ) where
        P: AsRef<std::path::Path>,
        S: AsRef<std::path::Path>,
    {
        let mut mesh = crate::mesh::Mesh::load(path, |x| x);

        if flatten {
            mesh.flatten();
        }

        let shadow_mesh = shadow_path.map(|p| {
            let mut shadow = crate::mesh::Mesh::load(p, |x| x);

            // Match the main mesh's flattening: the shadow pass shares the
            // render pipeline's vertex layout and flat/indexed draw path.
            if flatten {
                shadow.flatten();
            }

            Rc::new(RefCell::new(shadow))
        });

        self.bodies.push(super::body::Body {
            mesh: Some(Rc::new(RefCell::new(mesh))),
            shadow_mesh,
            mat,
            ..Default::default()
        });
    }

    pub fn add_mesh(&mut self, mesh: crate::mesh::Mesh, mat: Mat4) {
        self.bodies.push(super::body::Body {
            mesh: Some(Rc::new(RefCell::new(mesh))),
            mat,
            ..Default::default()
        });
    }

    /// World-space bounds of every body, or `None` when there is nothing to
    /// bound. Feeds the automatic camera/light frustum fitting -- 8 corners
    /// per body per frame, so it is cheap enough to redo every frame as
    /// bodies move.
    pub fn scene_bounds(&self) -> Option<crate::mesh::Aabb> {
        let mut bounds = crate::mesh::Aabb::empty();

        for body in &self.bodies {
            let Some(mesh) = body.mesh.as_ref() else {
                continue;
            };
            bounds = bounds.union(&mesh.borrow().bounds.transform(&body.mat));
        }

        (!bounds.is_empty()).then_some(bounds)
    }

    /// World bounds of one body, for fitting that body's own shadow layer.
    pub fn body_bounds(&self, index: usize) -> Option<crate::mesh::Aabb> {
        let body = self.bodies.get(index)?;
        let mesh = body.mesh.as_ref()?;
        let b = mesh.borrow().bounds.transform(&body.mat);
        (!b.is_empty()).then_some(b)
    }

    /// Empty the scene, ready for a script to build it again.
    ///
    /// What Restart needs, and what a first Play needs too: `load_mesh`
    /// *appends*, so running a script twice without this loads its meshes
    /// twice and the bodies stack up.
    ///
    /// The config is deliberately **not** reset. It is what the script sets
    /// on its way through, and it is also what the panel edits by hand --
    /// wiping it would throw away the second along with the first.
    ///
    /// Pending GPU requests and their results go too: they describe a scene
    /// that no longer exists, and a stale `facet_shadow_result` indexed by
    /// body would be read against a different set of bodies.
    pub fn reset(&mut self) {
        self.bodies.clear();
        self.huds.clear();
        self.state.iteration = 0;
        self.state.pause_at = None;

        self.export_once = false;
        self.facet_shadow_request = None;
        self.facet_shadow_result.clear();
        self.facet_id_request = false;
        self.facet_id_result = None;
        self.hemicube_request = None;
        self.hemicube_result = None;
    }

    pub fn update(&mut self) {
        if self.state.is_paused {
            return;
        }

        self.state.iteration += 1;

        // `pause_at` was set in three places and enforced in none: the Step
        // button raised it and unpaused, and nothing ever paused again, so
        // Step ran on like Play. A HUD's `{nit}` read it the whole time.
        //
        // `==`, not `>=`: the counter moves one at a time, so exact is
        // enough, and it means Play after an automatic pause advances past
        // the mark instead of stopping on it again. `pause_at` is left set,
        // because `{nit}` uses it as the length of the run.
        if self.state.pause_at == Some(self.state.iteration) {
            self.state.is_paused = true;
        }
    }

    pub fn toggle_export(&mut self) {
        self.export = !self.export;
    }

    pub fn export_once(&mut self) {
        self.export_once = true;
    }

    /// Ask for `body`'s per-facet occluded fractions to be read back from
    /// the shadow map after this frame renders. Only needed when
    /// `config.access_shadow_map` is off and you want them for one frame.
    pub fn request_hemicube(&mut self, body: usize, facets: Vec<u32>, resolution: u32, batch: u32) {
        self.hemicube_request = Some((body, facets, resolution, batch));
    }

    pub fn hemicube_result(&self) -> Option<&(Vec<f32>, usize, usize, Vec<u32>)> {
        self.hemicube_result.as_ref()
    }

    pub fn request_facet_id(&mut self) {
        self.facet_id_request = true;
    }

    pub fn facet_id_map(&self) -> Option<&(Vec<u32>, Vec<u32>, u32, u32)> {
        self.facet_id_result.as_ref()
    }

    pub fn request_facet_shadow(&mut self, body: usize) {
        self.facet_shadow_request = Some(body);
    }

    /// Per-facet occluded fractions for `body`, or `None` if they were not
    /// computed this frame.
    pub fn facet_shadow(&self, body: usize) -> Option<&[f32]> {
        self.facet_shadow_result
            .get(body)
            .filter(|v| !v.is_empty())
            .map(|v| v.as_slice())
    }

    /// Per-facet direct insolation, normalised: `max(0, cos i) * (1 - occluded)`.
    ///
    /// **This, not `facet_shadow`, is what "lit" means.** The shadow map
    /// answers one question -- is anything between this facet and the Sun --
    /// and a facet with nothing in the way is still dark if it faces away.
    /// On a crater the difference is most of the far wall: unoccluded,
    /// pointing into the ground, receiving nothing. Counting those as lit
    /// overstates the illuminated fraction by however much of the body has
    /// its back to the Sun, which for a convex body is about half of it.
    ///
    /// Same quantity the shader shades with, without the `ambient_strength`
    /// floor: the `Lighting` colour bar is this plus ambient. The cosine is
    /// clamped at zero for the reason `tpm::core::radiation_sun` gives --
    /// a facet tilted away receives nothing, it does not radiate into the
    /// Sun -- so 0 means dark and 1 means facing the Sun with nothing in
    /// the way.
    ///
    /// Derived rather than read back: the occlusion comes from the GPU, the
    /// cosine is geometry this already has, and computing it here keeps the
    /// per-frame readback the size it was -- which matters at 3.1M facets.
    ///
    /// `None` when no shadow result for that body has been read this frame,
    /// exactly as `facet_shadow`.
    pub fn facet_illumination(&self, body: usize) -> Option<Vec<f32>> {
        let shadow = self.facet_shadow(body)?;
        let b = self.bodies.get(body)?;
        let mesh = b.mesh.as_ref()?.borrow();

        // Rotation (and any scale) but not translation: a normal is a
        // direction. Renormalised, so a scaled body still reports a cosine.
        let rot = crate::Mat3::from_mat4(b.mat);

        Some(
            mesh.facets
                .iter()
                .zip(shadow)
                .map(|(f, &occluded)| {
                    let pos = b.mat.transform_point3(f.pos);
                    let normal = (rot * f.normal).normalize_or_zero();
                    let to_sun = (self.sun.pos - pos).normalize_or_zero();
                    (normal.dot(to_sun).max(0.0) as f32) * (1.0 - occluded)
                })
                .collect(),
        )
    }
}

#[derive(Clone, Debug)]
pub struct State {
    /// Frames advanced so far. Both callbacks see the same value for a given
    /// frame: it increments only once both have run.
    pub iteration: usize,
    /// Whether `P` has paused the simulation.
    ///
    /// The render loop keeps running -- the window stays responsive and the camera
    /// still moves -- but `before_render` and `after_render` are both skipped, so a
    /// script does not need its own check.
    pub is_paused: bool,
    /// Pause automatically on reaching this iteration.
    ///
    /// Also what `{nit}` reads in a HUD template, since it is the only thing that
    /// tells the engine how long a run is meant to be.
    pub pause_at: Option<usize>,
}

impl State {
    pub fn new() -> Self {
        Self {
            iteration: 0,
            is_paused: false,
            pause_at: None,
        }
    }

    /// Flip the pause state, returning the new value.
    pub fn toggle_pause(&mut self) -> bool {
        self.is_paused = !self.is_paused;
        self.is_paused
    }
}

#[cfg(test)]
mod pause_tests {
    use super::*;

    /// `pause_at` used to be set and never acted on, which made Step behave
    /// as Play.
    #[test]
    fn pause_at_stops_the_counter_and_step_advances_exactly_one() {
        let mut sim = Simulation::new();
        sim.state.pause_at = Some(3);

        for _ in 0..10 {
            sim.update();
        }
        assert_eq!(sim.state.iteration, 3, "must stop on the mark");
        assert!(sim.state.is_paused);

        // What the Step button does: one more iteration, then hold again.
        sim.state.is_paused = false;
        sim.state.pause_at = Some(sim.state.iteration + 1);
        for _ in 0..10 {
            sim.update();
        }
        assert_eq!(sim.state.iteration, 4, "Step is one iteration, not a run");
        assert!(sim.state.is_paused);
    }

    /// Resuming past an automatic pause must not stop on the same mark again.
    #[test]
    fn resuming_advances_past_the_mark() {
        let mut sim = Simulation::new();
        sim.state.pause_at = Some(2);
        for _ in 0..5 {
            sim.update();
        }
        assert_eq!(sim.state.iteration, 2);

        sim.state.is_paused = false;
        for _ in 0..5 {
            sim.update();
        }
        assert_eq!(sim.state.iteration, 7, "Play carries on past `pause_at`");
        assert!(!sim.state.is_paused);
    }
}

#[cfg(test)]
mod illumination_tests {
    use super::*;

    /// Two facets, both with nothing between them and the Sun: one facing it,
    /// one facing away. The shadow map cannot tell them apart -- that is the
    /// bug this exists to fix.
    fn scene() -> Simulation {
        let mut mesh = crate::mesh::Mesh::new();
        mesh.facets = vec![
            crate::mesh::Facet {
                pos: crate::Vec3::ZERO,
                normal: crate::Vec3::Z,
                area: 1.0,
            },
            crate::mesh::Facet {
                pos: crate::Vec3::ZERO,
                normal: -crate::Vec3::Z,
                area: 1.0,
            },
        ];

        let mut sim = Simulation::new();
        sim.sun.pos = crate::Vec3::new(0.0, 0.0, 10.0);
        sim.bodies.push(crate::app::body::Body {
            mesh: Some(std::rc::Rc::new(std::cell::RefCell::new(mesh))),
            ..Default::default()
        });
        sim
    }

    #[test]
    fn a_facet_facing_away_is_dark_however_unshadowed_it_is() {
        let mut sim = scene();
        sim.facet_shadow_result = vec![vec![0.0, 0.0]];

        let illum = sim.facet_illumination(0).unwrap();
        assert_eq!(illum[0], 1.0, "facing the Sun, nothing in the way");
        assert_eq!(
            illum[1], 0.0,
            "facing away -- unshadowed, and receiving nothing"
        );
    }

    #[test]
    fn occlusion_scales_it_and_shadow_alone_does_not_answer_the_question() {
        let mut sim = scene();
        sim.facet_shadow_result = vec![vec![0.25, 0.0]];

        let illum = sim.facet_illumination(0).unwrap();
        assert_eq!(illum[0], 0.75, "a quarter occluded");

        // What the old `lit` counted: not-mostly-shadowed. Both facets pass,
        // and half of them receive nothing at all.
        let shadow = sim.facet_shadow(0).unwrap();
        assert_eq!(shadow.iter().filter(|&&s| s < 0.5).count(), 2);
        assert_eq!(illum.iter().filter(|&&i| i > 0.0).count(), 1);
    }

    #[test]
    fn no_shadow_result_is_none_not_zeros() {
        let sim = scene();
        assert!(sim.facet_illumination(0).is_none());
        assert!(sim.facet_illumination(9).is_none());
    }
}

/// One selected facet, and what it looked like before.
#[derive(Debug, Clone)]
pub struct Selection {
    pub body: usize,
    pub facet: usize,
    /// The three vertices' `(color, color_mode)` as they were, so deselecting
    /// restores rather than guesses. A script that repaints the mesh while a
    /// facet is selected will have that overwritten on deselect -- there is
    /// no way to tell an intervening change from the selection's own.
    previous: [(crate::Vec3, u32); 3],
}

impl Simulation {
    /// Which vertices a facet owns, in the order the mesh stores them.
    fn facet_vertices(mesh: &crate::mesh::Mesh, facet: usize) -> Option<[usize; 3]> {
        if mesh.is_flat() {
            let i = facet * 3;
            (i + 2 < mesh.vertices.len()).then_some([i, i + 1, i + 2])
        } else {
            let i = facet * 3;
            (i + 2 < mesh.indices.len()).then_some([
                mesh.indices[i] as usize,
                mesh.indices[i + 1] as usize,
                mesh.indices[i + 2] as usize,
            ])
        }
    }

    pub fn is_selected(&self, body: usize, facet: usize) -> bool {
        self.selected_facets
            .iter()
            .any(|s| s.body == body && s.facet == facet)
    }

    /// Select a facet, or deselect it if it already is. Returns whether it is
    /// selected afterwards.
    ///
    /// On an indexed mesh the three vertices are shared with neighbouring
    /// facets, so the colour bleeds into them. Load with `flatten=True` --
    /// which per-facet work wants anyway -- for a selection that stops at the
    /// facet's own edges.
    pub fn toggle_facet(&mut self, body: usize, facet: usize, color: crate::Vec3) -> bool {
        if let Some(i) = self
            .selected_facets
            .iter()
            .position(|s| s.body == body && s.facet == facet)
        {
            let s = self.selected_facets.remove(i);
            if let Some(mesh) = self.bodies.get(body).and_then(|b| b.mesh.as_ref()) {
                let mut mesh = mesh.borrow_mut();
                if let Some(v) = Self::facet_vertices(&mesh, facet) {
                    for (slot, (c, m)) in v.into_iter().zip(s.previous) {
                        mesh.vertices[slot].color = c;
                        mesh.vertices[slot].color_mode = m;
                    }
                    mesh.colors_dirty = true;
                }
            }
            return false;
        }

        let Some(handle) = self.bodies.get(body).and_then(|b| b.mesh.as_ref()) else {
            return false;
        };
        let mut mesh = handle.borrow_mut();
        let Some(v) = Self::facet_vertices(&mesh, facet) else {
            return false;
        };

        let mut previous = [(crate::Vec3::ZERO, 0u32); 3];
        for (slot, prev) in v.into_iter().zip(previous.iter_mut()) {
            let vertex = &mut mesh.vertices[slot];
            *prev = (vertex.color, vertex.color_mode);
            vertex.color = color;
            // The shader reads this per facet and it overrides the global
            // mode, so the rest of the body is untouched.
            vertex.color_mode = 1;
        }
        mesh.colors_dirty = true;
        drop(mesh);

        self.selected_facets.push(Selection {
            body,
            facet,
            previous,
        });
        true
    }

    /// Put every selected facet back and empty the list.
    pub fn clear_selection(&mut self) {
        let selected = std::mem::take(&mut self.selected_facets);
        for s in selected {
            if let Some(mesh) = self.bodies.get(s.body).and_then(|b| b.mesh.as_ref()) {
                let mut mesh = mesh.borrow_mut();
                if let Some(v) = Self::facet_vertices(&mesh, s.facet) {
                    for (slot, (c, m)) in v.into_iter().zip(s.previous) {
                        mesh.vertices[slot].color = c;
                        mesh.vertices[slot].color_mode = m;
                    }
                    mesh.colors_dirty = true;
                }
            }
        }
    }

    /// The nearest facet a ray hits, across every body.
    ///
    /// Returns `(body, facet, world point, body-frame point)`. The ray is
    /// carried into each body's own frame before intersecting, so the second
    /// point is in the coordinates a shape model is defined in -- which is
    /// what a latitude and longitude have to be computed from.
    pub fn pick_facet(
        &self,
        origin: crate::Vec3,
        dir: crate::Vec3,
    ) -> Option<(usize, usize, crate::Vec3, crate::Vec3)> {
        let mut best: Option<(crate::Float, usize, usize, crate::Vec3, crate::Vec3)> = None;

        for (i, body) in self.bodies.iter().enumerate() {
            let Some(mesh) = body.mesh.as_ref() else {
                continue;
            };
            let inverse = body.mat.inverse();
            let local_origin = inverse.transform_point3(origin);
            let local_dir = inverse.transform_vector3(dir).normalize_or_zero();
            if local_dir == crate::Vec3::ZERO {
                continue;
            }

            // `exit_first: false`. True returns the first facet in *mesh
            // order*, not the first along the ray -- so clicking the rim of a
            // crater selected whichever of the two surfaces the ray crosses
            // happens to be stored first, which is not the one you can see.
            let Some((facet, local_hit)) =
                mesh.borrow().intersect(&local_origin, &local_dir, false)
            else {
                continue;
            };
            let world = body.mat.transform_point3(local_hit);
            let distance = (world - origin).length();
            if best.as_ref().is_none_or(|b| distance < b.0) {
                best = Some((distance, i, facet, world, local_hit));
            }
        }

        best.map(|(_, body, facet, world, local)| (body, facet, world, local))
    }
}

/// Latitude and longitude of a point in a body's own frame, in degrees.
///
/// Planetocentric: latitude from the equator, longitude east from the prime
/// meridian, both straight out of the Cartesian coordinates. Undefined at the
/// origin, which reads as `(0, 0)`.
pub fn lat_lon(p: crate::Vec3) -> (crate::Float, crate::Float) {
    let r = p.length();
    if r <= crate::Float::EPSILON {
        return (0.0, 0.0);
    }
    (
        (p.z / r).clamp(-1.0, 1.0).asin().to_degrees(),
        p.y.atan2(p.x).to_degrees(),
    )
}

#[cfg(test)]
mod selection_tests {
    use super::*;

    fn cube() -> Simulation {
        let mut mesh = crate::mesh::Mesh::load("res/cube.obj", |v| v);
        mesh.flatten();
        let mut sim = Simulation::new();
        sim.bodies.push(crate::app::body::Body {
            mesh: Some(std::rc::Rc::new(std::cell::RefCell::new(mesh))),
            ..Default::default()
        });
        sim
    }

    #[test]
    fn selecting_paints_the_facet_and_deselecting_puts_it_back() {
        let mut sim = cube();
        let before = {
            let m = sim.bodies[0].mesh.as_ref().unwrap().borrow();
            (m.vertices[3].color, m.vertices[3].color_mode)
        };

        let yellow = crate::Vec3::new(1.0, 0.85, 0.1);
        assert!(sim.toggle_facet(0, 1, yellow), "first click selects");
        {
            let m = sim.bodies[0].mesh.as_ref().unwrap().borrow();
            assert_eq!(m.vertices[3].color, yellow);
            // Mode 1 on the facet's own vertices is what the shader reads to
            // override the global mode for this facet alone.
            assert_eq!(m.vertices[3].color_mode, 1);
            assert!(m.colors_dirty);
            // and nothing else moved
            assert_eq!(m.vertices[0].color_mode, before.1);
        }
        assert!(sim.is_selected(0, 1));

        assert!(!sim.toggle_facet(0, 1, yellow), "second click deselects");
        let m = sim.bodies[0].mesh.as_ref().unwrap().borrow();
        assert_eq!((m.vertices[3].color, m.vertices[3].color_mode), before);
        assert!(sim.selected_facets.is_empty());
    }

    #[test]
    fn clearing_restores_every_one() {
        let mut sim = cube();
        let yellow = crate::Vec3::new(1.0, 0.85, 0.1);
        let before = sim.bodies[0].mesh.as_ref().unwrap().borrow().vertices.clone();
        for f in [0, 3, 7] {
            sim.toggle_facet(0, f, yellow);
        }
        assert_eq!(sim.selected_facets.len(), 3);

        sim.clear_selection();
        assert!(sim.selected_facets.is_empty());
        let m = sim.bodies[0].mesh.as_ref().unwrap().borrow();
        for (a, b) in m.vertices.iter().zip(&before) {
            assert_eq!((a.color, a.color_mode), (b.color, b.color_mode));
        }
    }

    /// The hit must be the **nearest** face, from either side.
    ///
    /// A ray through a cube crosses two faces, and one direction alone cannot
    /// tell a nearest-hit search from a first-in-mesh-order one: whichever
    /// face is stored first passes by luck. Both directions cannot both be
    /// lucky. This is what clicking the rim of a crater got wrong -- it
    /// selected the wall inside rather than the surface in front of it.
    #[test]
    fn picking_returns_the_nearest_facet_from_either_side() {
        let sim = cube();
        let n = sim.bodies[0].mesh.as_ref().unwrap().borrow().facets.len();

        let (body, facet, world, local) = sim
            .pick_facet(crate::Vec3::new(0.0, 0.0, 10.0), -crate::Vec3::Z)
            .expect("a ray at the cube should hit it");
        assert_eq!(body, 0);
        assert!(facet < n);
        // The unit cube about the origin: coming down, the top at z = 1.
        assert!((world.z - 1.0).abs() < 1e-5, "from above, hit {world}");
        assert_eq!(world, local, "identity transform: the two frames agree");

        let (_, _, world, _) = sim
            .pick_facet(crate::Vec3::new(0.0, 0.0, -10.0), crate::Vec3::Z)
            .expect("and from below");
        assert!((world.z + 1.0).abs() < 1e-5, "from below, hit {world}");

        assert!(
            sim.pick_facet(crate::Vec3::new(0.0, 0.0, 10.0), crate::Vec3::Z).is_none(),
            "a ray pointing away hits nothing"
        );
    }

    #[test]
    fn lat_lon_is_planetocentric_degrees() {
        assert_eq!(lat_lon(crate::Vec3::new(1.0, 0.0, 0.0)), (0.0, 0.0));
        assert_eq!(lat_lon(crate::Vec3::new(0.0, 1.0, 0.0)), (0.0, 90.0));
        assert_eq!(lat_lon(crate::Vec3::new(0.0, 0.0, 2.0)).0, 90.0);
        assert_eq!(lat_lon(crate::Vec3::ZERO), (0.0, 0.0));
    }
}
