use crate::Float;
use numpy::ndarray::Array1;

#[derive(Clone, Debug, Default)]
pub struct Column {
    pub z: Array1<Float>,
    pub t: Array1<Float>,
    pub d: Array1<Float>,
    // pub g1: Array1<Float>,
    // pub g2: Array1<Float>,
    // pub a: Array1<Float>,
    // pub b: Array1<Float>,
}

impl Column {
    pub fn new(z: Array1<Float>, prop: super::properties::Properties, t_init: Float) -> Self {
        let mut t = z.clone();
        t.fill(t_init);

        let mut d = z.clone();
        d.fill(prop.diffusivity);

        Self { z, t, d }
    }
}

pub type Interior = Vec<Column>;
