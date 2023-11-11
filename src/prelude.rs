//! Kalast's prelude.
//!
//! The purpose of this module is to alleviate imports of many commonly used items of the kalast crate
//! by adding a glob import to the top of kalast heavy modules:
//!
//! ```
//! # #![allow(unused_imports)]
//! use kalast::prelude::*;
//! ```

pub use crate::ast::{
    self,
    mesh,
    orbit,
    ray,
    Asteroid,
    ColorMode,
    FaceData,
    IntegratedShapeModel,
    Interior,
    InteriorGrid,
    Material,
    RawSurface,
    Surface,
    SurfaceBuilder,
    SurfaceError,
    Vertex, //
};

pub use crate::cfg::{
    self,
    Cfg,
    CfgBody,
    CfgCamera,
    CfgColormap,
    CfgError,
    CfgFrameCenter,
    CfgInterior,
    CfgInteriorGrid1D,
    CfgMesh,
    CfgMeshKind,
    CfgMeshSource,
    CfgOrbitKepler,
    CfgOrbitSpeedControl,
    CfgPreferences,
    CfgRoutines,
    CfgScalar,
    CfgSimulation,
    CfgState,
    CfgSun,
    CfgTemperatureInit,
    CfgTimeExport,
    CfgWindow, //
};
pub use crate::simu::{
    self,
    Body,
    Export,
    FoldersRun,
    Routines,
    RoutinesData,
    RoutinesThermal,
    RoutinesThermalDefault,
    RoutinesViewer,
    RoutinesViewerDefault,
    Scenario,
    Scene,
    ThermalData,
    Time, //
};
pub use crate::thermal::{self, bound, cond, flux};
pub use crate::util::{self, *};
pub use crate::win::scene_settings::SceneSettings;
pub use crate::win::window::{FrameEvent, Window};
pub use crate::win::window_settings::{Colormap, WindowSettings};

#[cfg(feature = "rust_spice")]
pub use spice;

pub use nalgebra::{
    self as na,
    DMatrix,
    DVector,
    DVectorView,
    Dyn,
    Matrix,
    Matrix3xX,
    MatrixView,
    SMatrix,
    SVector,
    VecStorage,
    ViewStorage,
    U1,
    U2,
    U3,
    U4, //
};
pub use nalgebra_glm::{self as glm, vec2, vec3, vec4};

pub use itertools::{iproduct, izip, Itertools};

pub use snafu::prelude::*;

pub use chrono::{Duration, Utc};

pub use notify_rust::Notification;
pub use polars::prelude::*;

pub use directories::UserDirs;

pub use downcast_rs::{DowncastSync, impl_downcast};

pub use std::collections::{BTreeMap, HashMap};
pub use std::env;
pub use std::fmt;
pub use std::fs::{self, File};
pub use std::io;
pub use std::path::{Path, PathBuf};

pub type Vec2 = glm::DVec2;
pub type Vec3 = glm::DVec3;
pub type Vec4 = glm::DVec4;
pub type Vec6 = nalgebra::SVector<Float, 6>;

pub type Mat2 = glm::DMat2;
pub type Mat3 = glm::DMat3;
pub type Mat4 = glm::DMat4;

pub type Float = f64;

pub const PI: Float = std::f64::consts::PI;
pub const TAU: Float = std::f64::consts::TAU;

pub type DRVector<T> = nalgebra::RowDVector<T>;
pub type DRVectorView<'a, T> = Matrix<T, U1, Dyn, ViewStorage<'a, Float, U1, Dyn, Dyn, Dyn>>;
pub type DRVectorRef<'a, T> = Matrix<T, U1, Dyn, ViewStorage<'a, Float, U1, Dyn, U1, Dyn>>;

pub type Matrix4xX<T> = Matrix<T, U4, Dyn, VecStorage<T, U4, Dyn>>;

pub type DMatrix2xXView<'a, T> = Matrix<T, U2, Dyn, ViewStorage<'a, Float, U2, Dyn, Dyn, Dyn>>;
pub type DMatrix3xXView<'a, T> = Matrix<T, U3, Dyn, ViewStorage<'a, Float, U3, Dyn, Dyn, Dyn>>;
pub type DMatrixView<'a, T> = Matrix<T, Dyn, Dyn, ViewStorage<'a, Float, Dyn, Dyn, Dyn, Dyn>>;

pub use sdl2::video::FullscreenType;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Snafu)]
pub enum Error {
    CfgError { source: CfgError },
    SurfaceError { source: SurfaceError },
}

impl From<CfgError> for Error {
    fn from(value: CfgError) -> Self {
        Self::CfgError { source: value }
    }
}

impl From<SurfaceError> for Error {
    fn from(value: SurfaceError) -> Self {
        Self::SurfaceError { source: value }
    }
}
