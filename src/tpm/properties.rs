#[cfg(feature = "python")]
use pyo3::prelude::*;

use crate::Float;

pub const DIDYMOS: Properties = Properties {
    albedo: 0.07,
    emissivity: 0.9,
    density: 2700.0,
    heat_capacity: 600.0,
    thermal_inertia: 320.0,
    conductivity: 0.0,
    diffusivity: 0.0,
};

pub const DIMORPHOS: Properties = Properties {
    albedo: 0.07,
    emissivity: 0.9,
    density: 2700.0,
    heat_capacity: 600.0,
    thermal_inertia: 320.0,
    conductivity: 0.0,
    diffusivity: 0.0,
};

pub const MOON: Properties = Properties {
    albedo: 0.12,
    emissivity: 0.95,
    density: 1500.0,
    heat_capacity: 600.0,
    thermal_inertia: 55.0,
    conductivity: 0.0,
    diffusivity: 0.0,
};

// Wargnier2025: conductivity = 0.0683 or 0.0837)
// Willner2010
pub const PHOBOS: Properties = Properties {
    albedo: 0.0683,
    emissivity: 0.95,
    density: 1860.0,
    heat_capacity: 600.0,
    thermal_inertia: 70.0,
    conductivity: 0.0,
    diffusivity: 0.0,
};

// Wargnier2025: thermal inertia = 20-85)
// Wargnier2025, Thomas1993, Jacobson2010
pub const DEIMOS: Properties = Properties {
    albedo: 0.068,
    emissivity: 0.95,
    density: 1490.0,
    heat_capacity: 600.0,
    thermal_inertia: 20.0,
    conductivity: 0.0,
    diffusivity: 0.0,
};

#[cfg_attr(feature = "python", pyfunction)]
#[cfg_attr(feature = "python", pyo3(signature = (k: "float", p: "float", c: "float") -> "float"))]
pub fn thermal_inertia(k: Float, p: Float, c: Float) -> Float {
    // k: conductivity (...)
    // p: density (...)
    // c: heat conductivity (...)
    (k * p * c).sqrt()
}

#[cfg_attr(feature = "python", pyfunction)]
#[cfg_attr(feature = "python", pyo3(signature = (ti: "float", p: "float", c: "float") -> "float"))]
pub fn conductivity(ti: Float, p: Float, c: Float) -> Float {
    // ti: thermal inertia (...)
    // p: density (...)
    // c: heat conductivity (...)
    ti.powi(2) / (p * c)
}

#[cfg_attr(feature = "python", pyfunction)]
#[cfg_attr(feature = "python", pyo3(signature = (k: "float", p: "float", c: "float") -> "float"))]
pub const fn diffusivity(k: Float, p: Float, c: Float) -> Float {
    // k: conductivity (...)
    // p: density (...)
    // c: heat conductivity (...)
    k / (p * c)
}

#[cfg_attr(feature = "python", pyfunction)]
pub fn skin_depth_1(d: Float, p: Float) -> Float {
    // skin depth @ e^-1
    //
    // d: diffusivity (...)
    // p: density (...)
    (d * p / crate::util::PI).sqrt()
}

#[cfg_attr(feature = "python", pyfunction)]
pub fn skin_depth_2pi(d: Float, p: Float) -> Float {
    // skin depth @ e^-2pi
    //
    // d: diffusivity (...)
    // p: density (...)
    (4.0 * crate::util::PI * d * p).sqrt()
}

#[derive(Clone, Default)]
pub struct Properties {
    pub albedo: Float,
    pub emissivity: Float,
    pub density: Float,
    pub heat_capacity: Float,
    pub thermal_inertia: Float,
    pub conductivity: Float,
    pub diffusivity: Float,
}

impl Properties {
    pub fn new(
        albedo: Float,
        emissivity: Float,
        density: Float,
        heat_capacity: Float,
        thermal_inertia: Float,
        conductivity: Float,
        diffusivity: Float,
    ) -> Self {
        Self {
            albedo,
            emissivity,
            density,
            heat_capacity,
            thermal_inertia,
            conductivity,
            diffusivity,
        }
    }

    pub fn compute_thermal_inertia(&mut self) {
        self.thermal_inertia = thermal_inertia(self.conductivity, self.density, self.heat_capacity);
    }

    pub fn compute_conductivity(&mut self) {
        self.conductivity = conductivity(self.thermal_inertia, self.density, self.heat_capacity);
    }

    pub fn compute_diffusivity(&mut self) {
        self.diffusivity = diffusivity(self.conductivity, self.density, self.heat_capacity);
    }

    pub fn compute_conductivity_diffusivity(&mut self) {
        self.compute_conductivity();
        self.compute_diffusivity();
    }
}

impl std::fmt::Debug for Properties {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Properties(albedo={}, emissivity={}, density={}, heat_capacity={}, thermal_inertia={}, conductivity={}, diffusivity={})",
            self.albedo,
            self.emissivity,
            self.density,
            self.heat_capacity,
            self.thermal_inertia,
            self.conductivity,
            self.diffusivity
        )
    }
}
