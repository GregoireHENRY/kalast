use crate::util::*;

use itertools::Itertools;
use std::fmt;

#[derive(Debug, Clone)]
pub enum Interior {
    Grid(InteriorGrid),
}

impl Interior {
    pub fn as_grid(&self) -> &InteriorGrid {
        match &self {
            Self::Grid(g) => g,
        }
    }

    pub fn depth_at_index(&self, index: usize) -> Float {
        match &self {
            Self::Grid(g) => g.depth[index],
        }
    }
}

#[derive(Clone)]
pub struct InteriorGrid {
    pub depth: Vec<Float>,
}

impl fmt::Debug for InteriorGrid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "InteriorGrid")
    }
}

impl InteriorGrid {
    pub fn from_fn<F>(depth: F, size: usize) -> Self
    where
        F: Fn(usize) -> Float,
    {
        Self {
            depth: (0..size).map(|ii| depth(ii)).collect_vec(),
        }
    }
}
