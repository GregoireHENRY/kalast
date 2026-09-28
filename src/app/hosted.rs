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
//! before a single call is made. See `docs/API.md`.

use std::cell::RefCell;
use std::rc::Rc;

/// What the guest may ask of the host, as plain function pointers.
///
/// `#[repr(C)]` because it crosses the library boundary. `ctx` is the host's
/// own and opaque to the guest, passed back to each call rather than
/// captured: the host reaches its app through it afresh on every call, so
/// nothing here holds a borrow between calls (`host::Host`).
#[repr(C)]
pub struct HostApi {
    pub ctx: *mut std::ffi::c_void,
    /// Draw one frame. Returns whether the window is still open.
    pub step: extern "C" fn(*mut std::ffi::c_void) -> bool,
    /// Whether the window is still open, without drawing anything.
    pub is_running: extern "C" fn(*mut std::ffi::c_void) -> bool,
    /// Ask the window to close.
    pub close: extern "C" fn(*mut std::ffi::c_void),
    /// Take the guest's simulation and callbacks as this window's own.
    ///
    /// Both pointers come from `Rc::into_raw` in the guest and are turned
    /// back into `Rc`s here. The two copies share an allocator -- one
    /// process, one system allocator -- so the counts and the free are sound.
    pub adopt: extern "C" fn(
        *mut std::ffi::c_void,
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
    pub superseded: extern "C" fn(*mut std::ffi::c_void) -> bool,
    /// A line for the host's log panel: what a guest whose `main` panicked
    /// says before it returns, since its own stderr reaches no panel.
    pub log: extern "C" fn(*mut std::ffi::c_void, *const u8, usize),
    /// kalast's own output from the guest's copy of the crate -- its
    /// `println!`, the mesh loader's line -- for the host's kalast tab. The
    /// guest's copy has no capture of its own to write it into
    /// (`gui::engine_write`).
    pub print: extern "C" fn(*const u8, usize),
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

/// A line into the host's log panel, when there is a host.
pub fn log_to_host(line: &str) -> bool {
    with_host(|host| (host.log)(host.ctx, line.as_ptr(), line.len())).is_some()
}

/// kalast's own output into the host's kalast tab, when there is a host.
pub fn print_to_host(text: &str) -> bool {
    with_host(|host| (host.print)(text.as_ptr(), text.len())).is_some()
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
            host.ctx,
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
    with_host(|host| (host.step)(host.ctx) && !(host.superseded)(host.ctx))
}

/// `false` when the window has closed **or** another example has been asked
/// for -- the two ways a hosted run is over, and `while app.is_running()`
/// has to end on both.
pub fn is_running() -> Option<bool> {
    with_host(|host| (host.is_running)(host.ctx) && !(host.superseded)(host.ctx))
}

pub fn close() -> bool {
    with_host(|host| (host.close)(host.ctx)).is_some()
}


/// The host's side of `HostApi`: plain functions over the host's context.
///
/// Free functions rather than methods because they cross a library boundary
/// as `extern "C"` pointers.
pub mod host {
    use super::*;
    use std::ffi::c_void;

    /// How the host reaches its app from a guest's call.
    enum Target {
        /// The editor's own loop, which owns the app outright.
        Owned(*mut crate::app::App),
        /// A front door's shared handle -- Python's, the bundle's --
        /// borrowed for the length of each call and never across one.
        Shared(*const RefCell<crate::app::App>),
    }

    /// What `HostApi::ctx` points at: on the host's stack for as long as
    /// the example's `main` runs.
    ///
    /// `shared` is the app's own, for the calls that need nothing else -- a
    /// log line, a close -- which may come from inside a frame, where the
    /// app is already borrowed. Adopting a scene keeps it; only the
    /// simulation is replaced.
    ///
    /// `between` runs after each frame the guest asks for, with the app not
    /// borrowed. A driven example owns the flow until its loop ends, so the
    /// editor's own turn does not come round while it runs; the front doors
    /// with an interpreter serve the log's python tab here instead, and it
    /// has `app` in hand.
    pub struct Host<'a> {
        target: Target,
        shared: Rc<RefCell<crate::app::Shared>>,
        between: Option<&'a mut dyn FnMut()>,
    }

    impl<'a> Host<'a> {
        /// For the editor's own loop.
        ///
        /// # Safety
        /// `app` must stay valid, and be reached only through this, until
        /// the example's `main` returns.
        pub unsafe fn owned(app: *mut crate::app::App) -> Self {
            let shared = unsafe { &*app }.shared.clone();
            Self { target: Target::Owned(app), shared, between: None }
        }

        /// For a front door that shares the app, and has something to run
        /// between the example's frames.
        pub fn shared(app: &Rc<RefCell<crate::app::App>>, between: &'a mut dyn FnMut()) -> Self {
            let shared = app.borrow().shared.clone();
            Self { target: Target::Shared(Rc::as_ptr(app)), shared, between: Some(between) }
        }

        fn with_app<T>(&mut self, f: impl FnOnce(&mut crate::app::App) -> T) -> T {
            match self.target {
                Target::Owned(app) => f(unsafe { &mut *app }),
                Target::Shared(app) => f(&mut unsafe { &*app }.borrow_mut()),
            }
        }
    }

    /// # Safety
    /// `ctx` must be the `Host` that `api` was given, still alive.
    unsafe fn host<'h>(ctx: *mut c_void) -> &'h mut Host<'h> {
        unsafe { &mut *(ctx as *mut Host<'h>) }
    }

    extern "C" fn step(ctx: *mut c_void) -> bool {
        let host = unsafe { host(ctx) };
        // The guest is between frames of its own, inside the host's
        // `editor_tick` turn, so this nests the way a driven Python script's
        // `step` does.
        let alive = host.with_app(|app| app.step());
        // With the app no longer borrowed: what runs here may take it.
        if let Some(between) = host.between.as_mut() {
            between();
        }
        alive
    }

    extern "C" fn is_running(ctx: *mut c_void) -> bool {
        // As `App::is_running` has it for the host, which is not hosted.
        let shared = unsafe { host(ctx) }.shared.borrow();
        shared.running && !shared.superseded()
    }

    extern "C" fn close(ctx: *mut c_void) {
        unsafe { host(ctx) }.shared.borrow_mut().exit_requested = true;
    }

    extern "C" fn superseded(ctx: *mut c_void) -> bool {
        unsafe { host(ctx) }.shared.borrow().load_requested.is_some()
    }

    extern "C" fn adopt(
        ctx: *mut c_void,
        simulation: *const RefCell<crate::app::simulation::Simulation>,
        shared: *const RefCell<crate::app::Shared>,
    ) {
        // Both were `Rc::into_raw`d by the guest; taking them back here
        // balances that, and the allocator is shared.
        let (simulation, shared) = unsafe { (Rc::from_raw(simulation), Rc::from_raw(shared)) };
        unsafe { host(ctx) }.with_app(|app| app.adopt_scene(simulation, shared));
    }

    extern "C" fn log(ctx: *mut c_void, ptr: *const u8, len: usize) {
        // The guest's bytes, copied before anything of the guest can go.
        let line = String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(ptr, len) }).into_owned();
        unsafe { host(ctx) }.shared.borrow_mut().log.push(&line);
    }

    extern "C" fn print(ptr: *const u8, len: usize) {
        // As `log`: copied first. The text carries its own newline.
        let text = String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(ptr, len) }).into_owned();
        crate::app::gui::engine_write(format_args!("{text}"), false);
    }

    /// The table handed to a guest for the length of its `main`, over
    /// `host`, which has to outlive it.
    pub fn api(host: &mut Host<'_>) -> HostApi {
        HostApi {
            ctx: host as *mut Host<'_> as *mut c_void,
            step,
            is_running,
            close,
            adopt,
            superseded,
            log,
            print,
        }
    }
}
