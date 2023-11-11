pub mod body;
pub mod camera;
pub mod config;
pub mod preferences;
pub mod simu;
pub mod sun;
pub mod window;

pub use body::{
    CfgBody, CfgFrameCenter, CfgInterior, CfgInteriorGrid1D, CfgMesh, CfgMeshKind, CfgMeshSource,
    CfgOrbitKepler, CfgOrbitSpeedControl, CfgState, CfgTemperatureInit,
};
pub use camera::CfgCamera;
pub use config::{read_cfg, Cfg, CfgError};
pub use preferences::CfgPreferences;
pub use simu::{CfgSimulation, CfgRoutines, CfgTimeExport};
pub use sun::CfgSun;
pub use window::{CfgColormap, CfgScalar, CfgWindow};
