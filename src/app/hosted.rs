//! Running an example inside the editor's own process.
//!
//! An example is an ordinary program: `fn main`, an `App` of its own, and
//! either `start()` or a `while app.step()` loop. Nothing in one is written
//! for the editor, and nothing should have to be -- the same file has to run
//! from a terminal, which is the whole point of an example.
//!
//! So the editor compiles it into a dynamic library through a wrapper it
//! generates itself, loads that, and calls its `main`. Two things then have
//! to be true for the example to be *this* window rather than a second one:
//!
//! - **The host drives the loop.** The guest is its own copy of the crate,
//!   with its own copy of winit's state. If `app.step()` in the guest pumped
//!   the event loop from there, it would be a second winit talking to the
//!   same platform. Instead `step`, `start`, `close` and `is_running` cross
//!   back to the host through the function pointers below, so the loop is
//!   always turned by the code that owns the window.
//!
//! - **The host adopts the guest's scene.** `App::new()` in the guest builds
//!   a real `App`; what matters of it is the simulation -- bodies, camera,
//!   config -- and the callbacks. Those are handed over on the first call
//!   that needs a frame, and the host renders them from then on.
//!
//! What makes any of this defensible is that both sides are the same crate
//! built by the same compiler in one invocation; `abi_fingerprint` is checked
//! before a single call is made. See `notes/API.md`.

use std::cell::RefCell;
use std::rc::Rc;

/// What the guest may ask of the host, as plain function pointers.
///
/// `#[repr(C)]` because it crosses the library boundary. The `app` pointer is
/// the host's own `App`, passed back to each call rather than captured, so
/// nothing here holds a borrow between calls.
#[repr(C)]
pub struct HostApi {
    pub app: *mut crate::app::App,
    /// Draw one frame. Returns whether the window is still open.
    pub step: extern "C" fn(*mut crate::app::App) -> bool,
    /// Whether the window is still open, without drawing anything.
    pub is_running: extern "C" fn(*mut crate::app::App) -> bool,
    /// Ask the window to close.
    pub close: extern "C" fn(*mut crate::app::App),
    /// Take the guest's simulation and callbacks as this window's own.
    ///
    /// Both pointers come from `Rc::into_raw` in the guest and are turned
    /// back into `Rc`s here. The two copies share an allocator -- one
    /// process, one system allocator -- so the counts and the free are sound.
    pub adopt: extern "C" fn(
        *mut crate::app::App,
        *const RefCell<crate::app::simulation::Simulation>,
        *const RefCell<crate::app::Shared>,
    ),
    /// Whether another example has been asked for while this one runs.
    ///
    /// A driven example owns the flow until its loop ends, so Restart cannot
    /// take effect by setting a flag the editor reads -- the editor does not
    /// get a turn until the example gives one back. So `step` and
    /// `is_running` report that the run is over, and the example's own
    /// `while` ends the way it ends when the window closes.
    ///
    /// Python raises through its script instead, because a `while True:`
    /// there has nothing to consult. A Rust loop reads one of these two.
    pub superseded: extern "C" fn(*mut crate::app::App) -> bool,
}



thread_local! {
    /// Set in the guest's copy of the crate for the length of its `main`.
    ///
    /// A thread-local rather than an argument because the example's `main`
    /// takes none: it is unmodified, and this is how its `App` finds out it
    /// is not alone.
    static HOST: RefCell<Option<&'static HostApi>> = const { RefCell::new(None) };
}

/// Install the host's API. Called by the generated wrapper, not by an example.
///
/// # Safety
///
/// `api` must outlive the call to the example's `main`, which the host
/// guarantees by keeping it on its own stack across the call.
pub unsafe fn set_host(api: *const HostApi) {
    HOST.with(|h| *h.borrow_mut() = unsafe { api.as_ref() });
}

/// Forget it again, so a later `App` in this process is an ordinary one.
pub fn clear_host() {
    HOST.with(|h| *h.borrow_mut() = None);
}

/// Whether this copy of the crate is running inside a host.
pub fn hosted() -> bool {
    HOST.with(|h| h.borrow().is_some())
}

fn with_host<T>(f: impl FnOnce(&HostApi) -> T) -> Option<T> {
    HOST.with(|h| h.borrow().map(f))
}

/// Hand the host this app's scene, once.
pub fn adopt(
    simulation: &Rc<RefCell<crate::app::simulation::Simulation>>,
    shared: &Rc<RefCell<crate::app::Shared>>,
) -> bool {
    with_host(|host| {
        (host.adopt)(
            host.app,
            Rc::into_raw(simulation.clone()),
            Rc::into_raw(shared.clone()),
        );
    })
    .is_some()
}

/// Draw one frame in the host's window.
///
/// `false` once another example has been asked for, so that a loop written
/// `while app.step()` ends there.
pub fn step() -> Option<bool> {
    with_host(|host| (host.step)(host.app) && !(host.superseded)(host.app))
}

/// `false` when the window has closed **or** another example has been asked
/// for -- the two ways a hosted run is over, and `while app.is_running()`
/// has to end on both.
pub fn is_running() -> Option<bool> {
    with_host(|host| (host.is_running)(host.app) && !(host.superseded)(host.app))
}

pub fn close() -> bool {
    with_host(|host| (host.close)(host.app)).is_some()
}


/// The host's side of `HostApi`: four plain functions over an `App` pointer.
///
/// Free functions rather than methods because they cross a library boundary
/// as `extern "C"` pointers, and each takes the app afresh so no borrow is
/// held between them.
pub mod host {
    use super::*;

    /// # Safety
    /// `app` must point at a live `App` for the duration of the call.
    pub extern "C" fn step(app: *mut crate::app::App) -> bool {
        let app = unsafe { &mut *app };
        // The guest is inside the host's own `editor_tick`, between frames,
        // so this nests the way a driven Python script does.
        app.step()
    }

    pub extern "C" fn is_running(app: *mut crate::app::App) -> bool {
        unsafe { &*app }.is_running()
    }

    pub extern "C" fn close(app: *mut crate::app::App) {
        unsafe { &mut *app }.close();
    }

    pub extern "C" fn superseded(app: *mut crate::app::App) -> bool {
        unsafe { &*app }.shared.borrow().load_requested.is_some()
    }

    pub extern "C" fn adopt(
        app: *mut crate::app::App,
        simulation: *const RefCell<crate::app::simulation::Simulation>,
        shared: *const RefCell<crate::app::Shared>,
    ) {
        // Both were `Rc::into_raw`d by the guest; taking them back here
        // balances that, and the allocator is shared.
        let (simulation, shared) = unsafe { (Rc::from_raw(simulation), Rc::from_raw(shared)) };
        unsafe { &mut *app }.adopt_scene(simulation, shared);
    }

    /// The table handed to a guest for the length of its `main`.
    pub fn api(app: *mut crate::app::App) -> HostApi {
        HostApi {
            app,
            step,
            is_running,
            close,
            adopt,
            superseded,
        }
    }
}
