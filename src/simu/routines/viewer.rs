use crate::{simu::Scene, AirlessBody, CfgBody, Routines, RoutinesData};

pub struct ViewerData {}

impl RoutinesData for ViewerData {
    fn new(_asteroid: &AirlessBody, _cb: &CfgBody, _scene: &Scene) -> Self {
        Self {}
    }
}

impl ViewerData {}

pub trait RoutinesViewer: Routines {}

pub struct RoutinesViewerDefault {
    pub data: Vec<ViewerData>,
}

impl RoutinesViewerDefault {
    pub fn new() -> Self {
        RoutinesViewerDefault { data: vec![] }
    }
}

impl Routines for RoutinesViewerDefault {}

impl RoutinesViewer for RoutinesViewerDefault {}
