use crate::Mat4;
use std::{cell::RefCell, rc::Rc};

#[derive(Debug, Clone, Default)]
pub struct Body {
    pub mesh: Option<Rc<RefCell<crate::mesh::Mesh>>>,
    pub mat: Mat4,
    pub entity: Option<crate::entity::Body>,
}
