use crate::Mat4;
use std::{cell::RefCell, rc::Rc};

#[derive(Debug)]
pub struct Simulation {
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
}

impl Simulation {
    pub fn new() -> Self {
        let mut sun = crate::app::frame::Eye::new();
        sun.projection.mode = crate::app::frame::ProjectionMode::Orthographic;

        Self {
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

    pub fn update(&mut self) {
        if self.state.is_paused {
            return;
        }

        self.state.iteration += 1;
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
