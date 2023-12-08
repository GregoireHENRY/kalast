use crate::Routines;

pub trait RoutinesViewer: Routines {}

pub struct RoutinesViewerDefault {}

impl RoutinesViewerDefault {
    pub fn new() -> Self {
        Self {}
    }
}

impl Routines for RoutinesViewerDefault {}

impl RoutinesViewer for RoutinesViewerDefault {}
