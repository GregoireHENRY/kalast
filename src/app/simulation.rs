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
}

#[derive(Clone, Debug)]
pub struct State {
    pub iteration: usize,
    pub is_paused: bool,
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

    // return pause state after toggle
    pub fn toggle_pause(&mut self) -> bool {
        self.is_paused = !self.is_paused;
        self.is_paused
    }
}
