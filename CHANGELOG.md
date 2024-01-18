# Changelog

All notable changes to this project will be documented in this file.

## v0.4.4 [unreleased] - 2024-01-18

### Added

- documentation for position vector km and distance in km or m
- camera mouse sensitivity correction for scroll wheel
- preference option and window settings for touchpad scroll wheel rotation or mouse (fixed
  mouse, no need of ward) with middle button and movement (need for warp repeat)

### Fixed

- position vectors are uniformized to km across the program
- example viewer-picker option names

## v0.4.3 - 2024-01-17

### Added

- possibility to change key for free camera movement and sensitivity in cfg preferences
- free camera movement and sensitivity also now in window settings
- shift key during movement for fast speed

### Changed

- renamed rotate and strafe camera into lock and free camera
- renamed camera methods for movements
- renamed camera constants
- camera sensitivity correction defaults
- simplified camera movements computations

### Fixed

- movement keys state down and up are now recorded to avoid waiting for key repetition for instant movement

### Removed

- camera direction struct, direction from movement key is computed in window events

## v0.4.2 - 2024-01-15

### Added

- example `hera` to reproduce Hera spacecraft FOV
- impl `Default` for cfg body/scene/sun/camera
- require `Default` for trait cfg
- `anchor` camera cfg to rotate around different point than origin
- kernels clear at beginning of program, note if the program is interrupted before the end kernels are not cleared
- share debug preference with window debug
- mouse movement during strafe as free camera
- rotation with mouse
- rotation now freed of pitch and yaw gimbal lock with rotation around world up and camera right axes
- impled defaults for thermal routines functions
- camera up world reference
- mouse warped center at start

### Changed

- `spice` is now default feature
- using latest version of `polars`, `snafu`, `serial_test`
- renamed main routines
- group routines to allow better re-use for custom impl
- renamed cfg state spice `frame_from` to `frame` and `frame_to` to `into_frame`
- moved cfg related to camera from window to camera (`direction`, `up`, `projection`)
- body data orientation is now identity and obliquity tilt computed in routine
- matrix orientation now expand in cartesian state with obliquity tilt
- camera settings sent to window at first routine iteration
- camera default values
- camera movements methods
- different keyboard and mouse speed correction
- how projection / frustum are stored and managed
- body/sun position with spice is now in km, every distance should be in km until computation
- updated body matrix model computation with above changes
- temperature initialization moved from thermal data init to first iteration routine
- thermal first iteration is now only for temperature initialization
- window lighting projection follow camera changes above

### Removed

- camera settings, it is now initialized and can be changed after

### Fixed

- camera with spice distance with origin

## v0.4.0

## v0.3.0

## v0.2.0 - 2023-11-08

- moved from gitlab to github
- reworked codebase modules
- added routines traits, viewer & thermal, with their default structs impl-ed
- simplified examples
- prepared examples viewer (with picker) and thermal (with binary shadows)