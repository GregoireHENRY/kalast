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
}
