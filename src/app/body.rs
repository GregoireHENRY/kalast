use crate::Mat4;
use std::{cell::RefCell, rc::Rc};

#[derive(Debug, Clone, Default)]
pub struct Body {
    pub mesh: Option<Rc<RefCell<crate::mesh::Mesh>>>,

    /// Optional lower-resolution stand-in rendered into the shadow map in
    /// place of `mesh`. The shadow map only decides which fragments are
    /// occluded from the light, so it can use coarser geometry than the
    /// camera view without touching any per-facet science data -- unlike
    /// swapping `mesh` itself, which would invalidate facet-indexed
    /// results (temperatures, radiance) tied to that topology.
    ///
    /// `None` (the default) renders `mesh` into the shadow map, i.e. the
    /// previous behaviour.
    pub shadow_mesh: Option<Rc<RefCell<crate::mesh::Mesh>>>,

    pub mat: Mat4,
    pub entity: Option<crate::entity::Body>,

    /// How the surface reflects sunlight in the image. `None` is Lambert:
    /// a lit pixel is the colour times `cos i`. A law makes it the colour
    /// times the law's I/F, `pi r(i, e, alpha) cos i`.
    pub scattering: Option<crate::lightcurve::Law>,

    /// A dusty atmosphere over the surface, as the camera sees it
    /// (`crate::atmosphere`). `None`: the bare surface.
    pub atmosphere: Option<crate::atmosphere::Atmosphere>,

    /// The body's own shadows from its horizon map (`app::horizon`), worked
    /// out once, rather than from drawing it into its own shadow layer every
    /// frame. For a body each direction from whose centre crosses its
    /// surface once.
    pub horizon_map: bool,

    /// `scattering`'s diffuse albedo, the last law asked for and its value:
    /// a few thousand evaluations of the law, so worked out once per law
    /// rather than every frame (`diffuse_albedo`).
    pub(crate) diffuse: std::cell::Cell<Option<(crate::lightcurve::Law, crate::Float)>>,
}

impl Body {
    /// How bright the surface is under a diffuse sky, per unit of a facet's
    /// colour: its law's bihemispherical albedo (`Law::diffuse_albedo`), and
    /// 1 for Lambert, whose colour is the albedo.
    pub fn diffuse_albedo(&self) -> crate::Float {
        let Some(law) = self.scattering else {
            return 1.0;
        };
        match self.diffuse.get() {
            Some((was, a)) if was == law => a,
            _ => {
                let a = law.diffuse_albedo();
                self.diffuse.set(Some((law, a)));
                a
            }
        }
    }
}
