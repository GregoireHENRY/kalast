mod core;
mod thermal;
mod viewer;

pub use crate::util::*;
pub use core::*;
pub use thermal::*;
pub use viewer::*;

#[derive(Clone, Debug, Default)]
pub struct State {
    pub init_spin: bool,
}

impl State {}
