pub mod axes;
pub mod body;
pub mod cargo;
pub mod config;
pub mod facet_id;
pub mod facet_shadow;
pub mod frame;
pub mod gui;
pub mod hosted;
#[cfg(target_os = "macos")]
pub mod macos;
pub mod hemicube;
pub mod gpu;
pub mod pass;
pub mod simulation;
pub mod uniform;
pub mod window;

#[cfg(feature = "python")]
use pyo3::prelude::*;
use std::{cell::RefCell, rc::Rc, sync::Arc};

use crate::Float;

/// State a script reaches while the loop is running.
///
/// Separate from `App` because the loop holds `&mut App` for its whole
/// duration -- winit's `run_app(self)` -- and Python reaches the app through
/// an `Rc<RefCell<App>>`, so that borrow is live from the moment `start()` is
/// called until the window closes. Anything left on `App` is unreachable from
/// Python for the entire run: assigning `app.before_render` or calling
/// `app.log` panicked with `RefCell already borrowed`.
///
/// `config` and `simulation` never had the problem, because they were always
/// separate handles. This is the same arrangement for everything else a
/// script touches.
/// Put a window in or out of fullscreen.
///
/// On macOS this is the *simple* fullscreen -- the pre-Lion kind, the one
/// Electron gives VSCode. The window grows to cover the screen and stays on
/// the Space it is on: no new desktop, no swipe to reach it, and other
/// windows can still sit on top.
///
/// Native fullscreen, `Fullscreen::Borderless`, is what this used to do, and
/// it moves the window to a Space of its own. That is where the transition
/// cost came from -- the Space animation starves the drawable pool, measured
/// at 104 ms and 588 ms of `nextDrawable` -- and where the scale-factor change
/// that used to crash it came from too. Simple fullscreen has neither: it is
/// a resize.
///
/// Elsewhere it is `Fullscreen::Borderless`, which is the closest thing those
/// platforms have.
fn set_window_fullscreen(window: &winit::window::Window, on: bool) {
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::WindowExtMacOS as _;
        // Refuses while the window is in *native* fullscreen, which is the
        // one case it cannot get out of; nothing here puts it there.
        window.set_simple_fullscreen(on);
    }

    #[cfg(not(target_os = "macos"))]
    {
        window.set_fullscreen(on.then(|| winit::window::Fullscreen::Borderless(None)));
    }
}

pub struct Shared {
    /// Runs before the frame is drawn: set body transforms, camera and sun
    /// here. Exposed to Python as `before_render` (and `tick`).
    pub before_render: Option<Tick>,
    /// Runs after the frame is drawn, once GPU results for this frame exist
    /// -- notably `Simulation::facet_shadow_result`, which is only filled
    /// once the shadow map holds this frame's geometry.
    ///
    /// Scene changes made here take effect on the *next* frame: the GPU work
    /// for this one is already submitted.
    pub after_render: Option<Tick>,
    /// What the editor's `Run` button calls.
    #[cfg(feature = "python")]
    pub script_runner: Option<ScriptRunner>,
    /// False once the window has closed. `step()` returns it, so
    /// `while app.step():` ends on its own.
    pub running: bool,
    /// `close()` cannot call `exit()` itself -- that needs the
    /// `ActiveEventLoop`, which only exists inside a handler -- so it raises
    /// this and the next pump acts on it.
    pub exit_requested: bool,
    /// Script buffer set before the window exists, handed to the editor when
    /// it is built. `python -m kalast some.py` fills this.
    pub pending_script: Option<(String, String)>,
    /// Where the UI last saw the pointer, in egui points, and the size of
    /// the area it is measured against.
    ///
    /// `None` when the UI has never seen one -- which is the usual state of a
    /// window that is not focused, since macOS only delivers mouse-moved
    /// events to the front application. Worth being able to tell apart from
    /// "the pointer is somewhere that reveals nothing".
    pub pointer: Option<(f32, f32)>,
    pub ui_size: (f32, f32),

    /// Which panels the last frame drew: top, bottom, left, right.
    ///
    /// All four in the ordinary layout. In focus mode, only the ones the
    /// pointer has summoned. Exposed so the edge behaviour can be checked
    /// without a camera -- it is the one part of the UI with no other visible
    /// effect.
    pub panels_shown: [bool; 4],

    /// The iteration the frame on screen was drawn for.
    ///
    /// Not `state.iteration`, which counts iterations *completed*: it is
    /// incremented at the end of a frame, so once the frame for iteration 0
    /// has been drawn it already reads 1. The toolbar wants the number you
    /// are looking at, which is the value the frame was drawn with.
    pub drawn_iteration: usize,

    /// This window is a compiled example the editor launched, not a script
    /// it is hosting. There is no script to run or restart -- the program
    /// *is* the script, and it is already running -- so the transport
    /// controls its simulation directly.
    pub native: bool,
    /// Lines for the editor's log panel. Here rather than on the editor so
    /// `app.log()` works before the window exists as well as during the run.
    pub log: crate::app::gui::Log,
    /// A run asked for from outside the UI -- `app.run_script()`. Here
    /// rather than on the editor because it can be raised before the window
    /// exists.
    pub run_requested: bool,
    /// The same, but from Restart: rebuild and stop at the start.
    pub restart_requested: bool,
    /// Read the file named in the script panel's path field, as its `open`
    /// button does.
    pub open_requested: bool,
    /// Launch the compiled example without waiting for Play. Set when a `.rs`
    /// is named on the command line and its binary is already current: there
    /// is nothing this process can render for it, so showing an empty
    /// viewport and waiting is showing nothing and asking for a click.
    pub launch_requested: bool,
    /// An example to load, and at which profile. Set inside a frame, acted on
    /// between two -- loading runs the example's `main`, which for a driven
    /// one calls `step()`, and stepping from inside a frame re-enters the
    /// event loop.
    pub load_requested: Option<bool>,
    /// A load failed on a library that is stale or built against a different
    /// kalast. Build it and try again -- once.
    pub rebuild_then_load: bool,
    /// A script the UI has asked to run, waiting for the caller to take it.
    ///
    /// The frame cannot run it: a driven script's own loop cannot nest inside
    /// the frame that is drawing it. So Play leaves it here and the loop
    /// owner picks it up *between* frames, where the script runs as the
    /// program it is -- whichever shape it has.
    /// `(path, source, paused)` -- `paused` when it came from Restart,
    /// which rebuilds the scene and stops at the start rather than running
    /// it. Play sets it false: pressing Play and having nothing move is the
    /// confusion this whole button started as.
    pub script_pending: Option<(String, String, bool)>,
    /// Whether the buffer on screen is what is actually running. Here rather
    /// than on the editor so a launcher can set it before the window exists.
    pub script_ran: bool,
}

impl Shared {
    fn new() -> Self {
        Self {
            before_render: None,
            after_render: None,
            #[cfg(feature = "python")]
            script_runner: None,
            running: true,
            exit_requested: false,
            pending_script: None,
            pointer: None,
            ui_size: (0.0, 0.0),
            panels_shown: [true; 4],
            drawn_iteration: 0,
            native: false,
            log: crate::app::gui::Log::new(2000),
            run_requested: false,
            restart_requested: false,
            open_requested: false,
            launch_requested: false,
            load_requested: None,
            rebuild_then_load: false,
            script_pending: None,
            script_ran: false,
        }
    }
}

pub struct App {
    /// Shared, not owned: `app.config` in Python holds the same handle, so it
    /// stays reachable while the app itself is mutably borrowed for the whole
    /// run loop. Without this, touching any option from `before_render`
    /// panicked with `Already mutably borrowed`.
    /// The **application's** settings: window size now, panel layout and
    /// colours as the editor grows.
    ///
    /// Not the simulation's -- that lives on `Simulation`, which is the thing
    /// it describes. Held as its own handle for the same reason everything
    /// else is: the loop borrows `App` for its whole duration.
    pub config: Rc<RefCell<crate::app::config::AppConfig>>,
    pub window: Option<crate::app::window::Window>,

    pub now: std::time::Instant,
    pub dt: Float,

    pub simulation: Rc<RefCell<crate::app::simulation::Simulation>>,
    /// Everything a script can reach while the loop runs. See `Shared`.
    pub shared: Rc<RefCell<Shared>>,

    pub controller: frame::Controller,

    /// Frames per second for the HUDs, averaged over a fixed window rather
    /// than smoothed per frame. An exponential average still moves every
    /// frame, so the digits churn faster than they can be read; this holds a
    /// value steady for `HUD_RATE_WINDOW` and then replaces it.
    fps_shown: Float,
    fps_window_secs: Float,
    fps_window_frames: u32,

    /// Held only while the caller drives the loop with `step()`. `start()`
    /// takes it and hands it to `run_app`, which never gives it back --
    /// a platform event loop cannot be created twice in one process, so
    /// the two modes cannot both own one.
    event_loop: Option<winit::event_loop::EventLoop<crate::app::window::Window>>,
    /// Set when a frame reaches the end of the redraw handler. `step()`
    /// pumps until it flips, which is what makes one call mean one frame
    /// rather than one batch of events.
    frame_drawn: bool,
    /// Whether this app has already handed its scene to a host. Once only:
    /// the second hand-over would replace the simulation the host is midway
    /// through rendering with an identical one.
    gave_scene: bool,
    /// Last cursor position in physical pixels, for turning a click into a
    /// ray. The editor knows the pointer in egui points; a plain window does
    /// not, and this is what both fall back on.
    /// The example currently loaded into this process, held for as long as
    /// the callbacks it installed can run. Dropping it unmaps the code those
    /// callbacks point at.
    loaded_example: Option<libloading::Library>,
    cursor: Option<(f64, f64)>,
    /// Where the left button went down, so a click can be told from a drag:
    /// only a press and release in the same place is a selection.
    left_press: Option<(f64, f64)>,
    /// Whether the loop is being driven by `step()` rather than owned by
    /// `start()`. Only the stepped one has to stop at one frame per call.
    stepping: bool,

    /// The values the live window was built with, to diff the config
    /// against. `None` until there is a window.
    realised: Option<Realised>,

    /// stdout and stderr, mirrored into the log panel. Editor only: a
    /// terminal run's output belongs on the terminal, untouched.
    stdio: Option<crate::app::gui::StdioCapture>,

    /// The editor shell. `None` for a terminal run, which is every script
    /// that calls `start()` or `step()` -- those keep drawing the scene
    /// straight to the swapchain, unchanged.
    editor: Option<crate::app::gui::Editor>,
    /// Whether this app was launched as the editor. Read in `resumed`,
    /// where the window and the GPU device first exist.
    want_editor: bool,
    /// Whether the platform event loop has ever been created. See
    /// `ensure_event_loop`.
    event_loop_built: bool,
    /// Whether the window was zoomed last frame, to catch the edge.
    ///
    /// macOS only, and only because the green button is not winit's to
    /// intercept: native fullscreen is turned off for the window, which makes
    /// that button a plain zoom, and a zoom is then read as "fullscreen was
    /// asked for".
    zoomed: bool,
}

/// How long `{fps}` and `{its}` average over before updating, in seconds.
const HUD_RATE_WINDOW: Float = 1.0;

/// What the window and its GPU resources were actually built with.
///
/// These options used to be read once, when the window was created, and any
/// later change was silently ignored -- `CONFIG.md` called them *startup
/// only*. Keeping what was realised lets each frame notice a difference and
/// act on it, and only on a difference: reconfiguring a surface or
/// recompiling a pipeline every frame would be ruinous.
#[derive(Clone, PartialEq)]
struct Realised {
    title: String,
    /// The window, from `AppConfig`.
    width: u32,
    height: u32,
    /// The image, from the simulation's config. `(0, 0)` follows the window.
    render: (u32, u32),
    fullscreen: bool,
    vsync: bool,
    msaa: u32,
    render_back_face: bool,
    shadow_resolution: u32,
    hud_font: String,
    export_dir: String,
    export_sync: bool,
    export_max_queued: u32,
}

impl Realised {
    fn of(c: &crate::app::config::Config, a: &crate::app::config::AppConfig) -> Self {
        Self {
            title: c.title.clone(),
            // The window from the app config, the image from the
            // simulation's -- two different questions since the editor made
            // them two different sizes.
            width: a.width,
            height: a.height,
            render: (c.width, c.height),
            fullscreen: c.fullscreen,
            vsync: c.vsync,
            msaa: c.msaa,
            render_back_face: c.render_back_face,
            shadow_resolution: c.shadow_resolution,
            hud_font: c.hud_font.clone(),
            export_dir: c.export_dir.clone(),
            export_sync: c.export_sync,
            export_max_queued: c.export_max_queued,
        }
    }

    /// Whether any of these differs from the config, without building a
    /// snapshot to compare against.
    ///
    /// Worth the extra method: `of` clones three `String`s, and doing that
    /// every frame to discover that nothing changed -- which is every frame
    /// of an ordinary run -- is an allocation in the frame path for nothing.
    fn matches(&self, c: &crate::app::config::Config, a: &crate::app::config::AppConfig) -> bool {
        self.width == a.width
            && self.height == a.height
            && self.render == (c.width, c.height)
            && self.fullscreen == c.fullscreen
            && self.vsync == c.vsync
            && self.msaa == c.msaa
            && self.render_back_face == c.render_back_face
            && self.shadow_resolution == c.shadow_resolution
            && self.export_sync == c.export_sync
            && self.export_max_queued == c.export_max_queued
            && self.title == c.title
            && self.hud_font == c.hud_font
            && self.export_dir == c.export_dir
    }
}

/// A number the host and a loaded example must agree on.
///
/// Passing an `&mut App` across a dynamic library boundary is sound only
/// while both sides were built from the same crate with the same features:
/// Rust has no stable ABI, and this crate's `python` feature genuinely
/// changes layout -- it adds a field to `Shared` and a variant to `Tick`. A
/// guest built without it, handed a host's `App` that has it, would read the
/// wrong bytes and keep going.
///
/// So the guest exports this and the host checks it before calling anything.
/// It is a coarse check, not a proof: it catches the mismatch that can
/// actually happen here -- a dylib built with the wrong feature set, or
/// against a different version of this crate -- and turns it into a message
/// instead of corruption.
pub fn abi_fingerprint() -> u64 {
    use std::hash::{Hash as _, Hasher as _};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    env!("CARGO_PKG_VERSION").hash(&mut h);
    std::mem::size_of::<App>().hash(&mut h);
    std::mem::size_of::<Shared>().hash(&mut h);
    std::mem::size_of::<simulation::Simulation>().hash(&mut h);
    std::mem::size_of::<Tick>().hash(&mut h);
    std::mem::size_of::<hosted::HostApi>().hash(&mut h);
    cfg!(feature = "python").hash(&mut h);

    // Sizes alone are too coarse: a `bool` added to `Shared` fit in existing
    // padding, left the size unchanged, and moved nothing this hash could
    // see -- so a library from before that change loaded anyway. Offsets of
    // the fields actually reached across the boundary catch a field added or
    // reordered ahead of them, which is the realistic way these two drift.
    std::mem::offset_of!(Shared, before_render).hash(&mut h);
    std::mem::offset_of!(Shared, after_render).hash(&mut h);
    std::mem::offset_of!(Shared, running).hash(&mut h);
    std::mem::offset_of!(simulation::Simulation, state).hash(&mut h);
    std::mem::offset_of!(simulation::Simulation, bodies).hash(&mut h);
    std::mem::offset_of!(simulation::Simulation, huds).hash(&mut h);

    h.finish()
}

/// One turn of the editor's loop.
///
/// `Run` is handed out between frames, never inside one, which is what lets a
/// script own a `while app.step():` loop of its own.
pub enum EditorTick {
    /// The window closed. Stop.
    Closed,
    /// A frame was drawn and nothing else is wanted.
    Frame,
    /// Play or Restart asked for this script. Run it, then carry on ticking.
    Run { path: String, source: String },
}

/// Fills one HUD's template in for this frame.
///
/// Deliberately a scan-and-replace rather than a format library: an
/// unrecognised `{name}` is passed through untouched, so a HUD string that
/// happens to contain braces renders instead of erroring or panicking on a
/// user's typo.
///
/// A placeholder may carry a precision, `{fps:.2}`. Rates default to **zero**
/// decimals: a frame rate quoted to a tenth is noise, and the digit changes
/// every update without telling the reader anything.
pub(crate) fn expand_hud(
    template: &str,
    state: &crate::app::simulation::State,
    rate: Float,
    diag: &crate::app::simulation::Diagnostics,
    drawn: usize,
) -> String {
    let its = if state.is_paused { 0.0 } else { rate };
    let nit = match state.pause_at {
        Some(n) => n.to_string(),
        None => "?".to_string(),
    };

    // `{name}` or `{name:.N}`; anything else is not a placeholder.
    let split = |key: &str| -> (String, usize) {
        match key.split_once(":.") {
            Some((name, prec)) => match prec.trim_end_matches('f').parse::<usize>() {
                Ok(p) => (name.to_string(), p),
                Err(_) => (key.to_string(), 0),
            },
            None => (key.to_string(), 0),
        }
    };

    let mut out = String::with_capacity(template.len() + 32);
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            // Unbalanced: emit the rest verbatim.
            out.push_str(&rest[open..]);
            return out;
        };
        let raw = &after[..close];
        let (name, prec) = split(raw);
        match name.as_str() {
            "it" => out.push_str(&state.iteration.to_string()),
            // The frame you are looking at, not how many have been begun.
            // Once the frame for iteration 0 is drawn `{it}` is already 1,
            // and a caption of "1" under a picture of 0 is a lie of exactly
            // one frame.
            "drawn" => out.push_str(&drawn.to_string()),
            "nit" => out.push_str(&nit),
            "its" => out.push_str(&format!("{its:.prec$}")),
            "fps" => out.push_str(&format!("{rate:.prec$}")),
            "ms" => {
                let ms = if rate > 0.0 { 1000.0 / rate } else { 0.0 };
                // Milliseconds are the one rate where a decimal earns its
                // place: whole numbers cannot separate 8 ms from 8.4 ms.
                let prec = if raw.contains(":.") { prec } else { 1 };
                out.push_str(&format!("{ms:.prec$}"));
            }
            "paused" => out.push_str(if state.is_paused { "PAUSED" } else { "" }),

            // Scene diagnostics. `{bodies}` is the one to reach for: it reads
            // "2/3" and only mentions a reason when something is missing.
            "bodies" => {
                out.push_str(&format!("{}/{}", diag.n_visible, diag.n_bodies));
                let mut why = Vec::new();
                if diag.out_near > 0 {
                    why.push(format!("{} behind", diag.out_near));
                }
                if diag.out_far > 0 {
                    why.push(format!("{} past far", diag.out_far));
                }
                if diag.out_side > 0 {
                    why.push(format!("{} off-frame", diag.out_side));
                }
                if !why.is_empty() {
                    out.push_str(&format!(" ({})", why.join(", ")));
                }
            }
            "n_bodies" => out.push_str(&diag.n_bodies.to_string()),
            "n_visible" => out.push_str(&diag.n_visible.to_string()),
            "n_behind" => out.push_str(&diag.out_near.to_string()),
            "n_past_far" => out.push_str(&diag.out_far.to_string()),
            "n_offframe" => out.push_str(&diag.out_side.to_string()),

            // Empty unless something is actually wrong, so a template can
            // carry it permanently without adding a line to every frame.
            "warn" => {
                if diag.light_cube_clipped {
                    out.push_str(
                        "light cube is past the camera far plane (set camera.projection.far)",
                    );
                }
            }
            // Unknown: leave it exactly as written.
            _ => {
                out.push('{');
                out.push_str(raw);
                out.push('}');
            }
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

impl App {
    pub fn new() -> Self {
        Self::new_with_config(crate::app::config::Config::default())
    }

    pub fn new_with_config(config: crate::app::config::Config) -> Self {
        // Set by the editor when it launches a compiled example: draw the
        // editor around whatever this program builds. Nothing else sets it,
        // so a terminal run is exactly as it was.
        let launched_by_editor = std::env::var_os("KALAST_EDITOR").is_some();
        let simulation = Rc::new(RefCell::new(crate::app::simulation::Simulation::new()));
        let config_rc = simulation.borrow().config.clone();
        *config_rc.borrow_mut() = config;
        let controller = {
            let c = config_rc.borrow();
            frame::Controller::new(
                c.sensitivity_move,
                c.sensitivity_look,
                c.sensitivity_rotate,
                c.sensitivity_zoom,
            )
        };

        let app_config = {
            let mut c = crate::app::config::AppConfig::default();
            if launched_by_editor {
                c.editor = true;
                // The editor needs more room than a bare render window --
                // four panels around an 800x600 scene leave it in a corner --
                // but the amount is decided from the monitor in `resumed`,
                // not fixed here. It used to be 2000x1300, which is larger
                // than a 1920x1080 screen: chosen on a display that could
                // take it, and off the edge of one that cannot.
            }
            c
        };

        Self {
            config: Rc::new(RefCell::new(app_config)),
            window: None,

            now: std::time::Instant::now(),
            dt: 0.0,

            simulation,
            shared: Rc::new(RefCell::new({
                let mut s = Shared::new();
                s.native = launched_by_editor;
                // The file this was built from, put in the Script panel so
                // the window says what it is running. Read here rather than
                // by the example, which should not have to know it is being
                // shown.
                if let Some(path) = std::env::var_os("KALAST_SCRIPT") {
                    let path = path.to_string_lossy().into_owned();
                    if let Ok(source) = std::fs::read_to_string(&path) {
                        s.pending_script = Some((path, source));
                    }
                }
                s
            })),

            controller,
            fps_shown: 0.0,
            fps_window_secs: 0.0,
            fps_window_frames: 0,

            event_loop: None,
            frame_drawn: false,
            gave_scene: false,
            stepping: false,
            loaded_example: None,
            cursor: None,
            left_press: None,
            realised: None,
            editor: None,
            stdio: None,
            want_editor: false,
            event_loop_built: false,
            zoomed: false,
        }
    }

    /// Build the platform event loop, once.
    ///
    /// Both `start()` and `step()` need one and neither may create a second:
    /// on every platform here `EventLoop::build` fails if one already exists
    /// in the process.
    fn ensure_event_loop(&mut self) {
        if self.event_loop.is_some() {
            return;
        }
        // Built once per process, and this says whether that has happened --
        // `event_loop` being `None` does not, since `step()` takes it for the
        // duration of a pump. If something unwound through that, the loop is
        // gone for good and rebuilding raises `RecreationAttempt`; report the
        // app as stopped instead of panicking on the way out.
        if self.event_loop_built {
            eprintln!("[APP] the event loop was lost, most likely to a panic; stopping");
            self.shared.borrow_mut().running = false;
            return;
        }
        self.event_loop_built = true;
        self.apply_config_at_start();
        // `init` panics on a second call, and `step()` reaches here from a
        // process that may already have run one app.
        let _ = env_logger::try_init();
        self.event_loop = Some(
            winit::event_loop::EventLoop::with_user_event()
                .build()
                .unwrap(),
        );
    }

    /// Run the editor shell to completion. **Blocks until the window
    /// closes.**
    ///
    /// The same loop `start()` runs. The difference is where the scene lands:
    /// into `render_texture` at the viewport panel's size, which egui samples
    /// into the centre of a layout, rather than blitted to the swapchain.
    pub fn start_editor(&mut self) {
        self.want_editor = true;
        self.config.borrow_mut().editor = true;
        // Stopped until told otherwise, the way Blender and Unity open. The
        // loop still runs -- the window draws, the camera moves, the panels
        // respond -- but `state.iteration` stays put and the callbacks do
        // not fire, so an empty scene does not sit there counting.
        self.simulation.borrow_mut().state.is_paused = true;
        self.start();
    }

    /// Put a script in the editor's buffer.
    ///
    /// Works before the window exists -- which is when a launcher sets it,
    /// the editor being built only once there is a GPU device -- and after.
    pub fn set_script(&mut self, path: String, source: String) {
        match self.editor.as_mut() {
            Some(editor) => {
                editor.script_path = path;
                editor.script = source;
                editor.script_dirty = false;
            }
            None => self.shared.borrow_mut().pending_script = Some((path, source)),
        }
    }

    /// Put stdout and stderr back and flush what is left in the pipe.
    ///
    /// Called at exit rather than left to `Drop`, which does not run: a
    /// callback's `__globals__` refers to the app, so the app refers to
    /// itself through Python, and a `#[pyclass]` is opaque to Python's cycle
    /// collector. The app is never freed and the last thing a script printed
    /// went with the pipe.
    pub fn flush_output(&mut self) {
        self.stdio = None; // `Drop` restores and flushes
    }

    /// Append a line to the editor's log panel.
    ///
    /// A no-op without one, deliberately. The panel is fed by teeing
    /// `sys.stdout`, so a fallback to `println!` here would print every line
    /// twice in a terminal run -- once from the real stream and once from
    /// this. `print` is how a script writes to a terminal; this is how it
    /// writes to the panel.
    pub fn log(&mut self, line: &str) {
        self.shared.borrow_mut().log.push(line);
    }

    /// Run the loop to completion. **Blocks until the window closes.**
    pub fn start(&mut self) {
        // Hosted: the loop is the editor's and is already running. An example
        // ends its `main` here, so this is the last chance to hand over what
        // it built -- and then it simply returns, the way `start()` does for
        // a script the Python editor runs.
        if hosted::hosted() {
            self.give_scene_to_host();
            return;
        }

        // `start()` owns the loop and draws continuously; the one-frame
        // guard belongs only to the stepped path.
        self.stepping = false;
        self.ensure_event_loop();
        // `run_app` consumes the loop, so this app cannot be started or
        // stepped again afterwards -- which is the truth on the platform
        // as well, not a restriction added here.
        let ev = self.event_loop.take().unwrap();
        ev.run_app(self).unwrap();
        self.shared.borrow_mut().running = false;
    }

    /// Draw exactly one frame and return whether the app is still running.
    ///
    /// The caller owns the loop:
    ///
    /// ```no_run
    /// # let mut app = kalast::app::App::new();
    /// while app.step() {
    ///     // between frames: place bodies, read last frame's GPU results
    /// }
    /// ```
    ///
    /// Rendering still happens inside winit's handler, which is what
    /// `pump_app_events` requires -- macOS drives drawing from `drawRect`
    /// and expects it finished before the callback returns. Only the
    /// caller's own work happens outside.
    ///
    /// One call is one *frame*, not one pump: events are pumped until the
    /// redraw handler has run, because the redraw a pump requests is only
    /// delivered by the next one. At startup that also covers creating the
    /// window and configuring the surface, so the first `step()` costs more
    /// than the rest.
    pub fn step(&mut self) -> bool {
        use winit::platform::pump_events::EventLoopExtPumpEvents;

        // Hosted: this app is an example the editor loaded, and the window
        // belongs to the host. Hand over the scene the example has built so
        // far, then let the host draw -- from its own copy of the crate, so
        // there is one winit talking to the platform and not two.
        if hosted::hosted() {
            self.give_scene_to_host();
            // `false` once another example has been asked for, so a driven
            // example's own loop ends and hands the flow back.
            return hosted::step().unwrap_or(false);
        }

        if !self.shared.borrow().running {
            return false;
        }
        self.ensure_event_loop();
        let mut ev = match self.event_loop.take() {
            Some(ev) => ev,
            // `start()` consumed it. Stepping afterwards is a caller error,
            // but reporting "not running" beats panicking inside a loop.
            None => {
                self.shared.borrow_mut().running = false;
                return false;
            }
        };

        self.stepping = true;
        self.frame_drawn = false;
        // A frame that never arrives would hang the caller's loop with no
        // way out, so give up rather than spin forever -- a window that
        // cannot configure its surface is a real failure, not a slow frame.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while self.shared.borrow().running && !self.frame_drawn {
            if let winit::platform::pump_events::PumpStatus::Exit(_) =
                ev.pump_app_events(Some(std::time::Duration::ZERO), self)
            {
                self.shared.borrow_mut().running = false;
            }
            if !self.frame_drawn && std::time::Instant::now() > deadline {
                eprintln!("[APP] step() saw no frame in 5 s; giving up on the loop");
                self.shared.borrow_mut().running = false;
            }
        }

        self.event_loop = Some(ev);
        self.shared.borrow().running
    }

    /// Realise any option that changed since the window was built.
    ///
    /// Runs at the top of each frame, so a change made between two `step()`s,
    /// or by the previous frame's callbacks, takes effect on this one. Each
    /// branch is guarded by a comparison: nothing here runs on a frame where
    /// nothing changed, which is every frame in an ordinary run.
    fn apply_live_config(&mut self) {
        // The green button. Native fullscreen is off for this window, so the
        // button zooms instead -- instantly, with no Space -- and a zoom is
        // taken to mean fullscreen was asked for. The zoom is undone before
        // the simple fullscreen replaces it, so the window remembers the size
        // it had and `is_zoomed` stops reporting the same press for ever.
        #[cfg(target_os = "macos")]
        if let Some(win) = self.window.as_ref() {
            let zoomed = crate::app::macos::is_zoomed(&win.window);
            if zoomed && !self.zoomed {
                crate::app::macos::unzoom(&win.window);
                let cfg = self.sim_config();
                let want = !cfg.borrow().fullscreen;
                cfg.borrow_mut().fullscreen = want;
            }
            self.zoomed = zoomed;
        }

        // Cheap enough to copy unconditionally -- plain scalars into a struct
        // this owns, no GPU resource behind them.
        {
            let c = self.sim_config();
            let c = c.borrow();
            self.controller.sensitivity_move = c.sensitivity_move;
            self.controller.sensitivity_look = c.sensitivity_look;
            self.controller.sensitivity_rotate = c.sensitivity_rotate;
            self.controller.sensitivity_zoom = c.sensitivity_zoom;
            self.controller.emulate_middle_button = c.emulate_middle_button;
        }

        // Cloned so the config is not borrowed while `self.window` is held
        // mutably -- both are fields of `self`.
        let config = self.sim_config();
        let c = config.borrow();
        let app_config = self.config.clone();
        let a = app_config.borrow();

        // The early out for the common case, before anything is cloned.
        if self.window.is_none() {
            return;
        }
        if let Some(was) = self.realised.as_ref() {
            if was.matches(&c, &a) {
                return;
            }
        } else {
            self.realised = Some(Realised::of(&c, &a));
            return;
        }

        let want = Realised::of(&c, &a);
        let was = self.realised.clone().unwrap();
        let win = self.window.as_mut().unwrap();

        if was.title != want.title {
            win.window.set_title(&want.title);
        }

        if was.fullscreen != want.fullscreen {
            set_window_fullscreen(&win.window, want.fullscreen);
        }

        // The image. `(0, 0)` follows the window, which is what a terminal
        // run does and what every script did when there was one pair of
        // these. The editor overrides it from the viewport panel each frame,
        // so a pinned render size only holds outside the editor.
        if was.render != want.render && self.editor.is_none() {
            let (w, h) = if want.render == (0, 0) {
                (win.surface_config.width, win.surface_config.height)
            } else {
                want.render
            };
            win.set_render_size(w, h);
        }

        if (was.width, was.height) != (want.width, want.height) {
            // A request, not a command: a tiling window manager or a
            // fullscreen window may refuse it. The `Resized` event that
            // follows a granted request is what actually reconfigures the
            // surface, so nothing is done here beyond asking.
            let _ = win
                .window
                .request_inner_size(winit::dpi::PhysicalSize::new(want.width, want.height));
        }

        if was.vsync != want.vsync {
            win.set_vsync(want.vsync);
        }

        // Both are baked into the pipelines, so one rebuild covers them.
        if (was.msaa, was.render_back_face) != (want.msaa, want.render_back_face) {
            win.rebuild_passes(&c);
        }

        // Rebuilds the passes too, so only when the pipelines were not
        // already rebuilt just above.
        if was.shadow_resolution != want.shadow_resolution {
            win.set_shadow_resolution(&c);
        }

        if was.hud_font != want.hud_font {
            win.set_hud_font(&c);
        }

        if (was.export_dir, was.export_sync, was.export_max_queued)
            != (
                want.export_dir.clone(),
                want.export_sync,
                want.export_max_queued,
            )
        {
            win.set_export_config(&c);
        }

        self.realised = Some(want);
    }

    /// Act on whatever the editor's buttons asked for last frame.
    ///
    /// Deliberately after the frame rather than inside the UI closure:
    /// running a script re-enters Python, which can load meshes and rewrite
    /// the scene, and doing that while egui holds its layout -- and while
    /// `Simulation` is borrowed for the panels -- is how a `RefCell` panic
    /// happens.
    fn serve_editor_requests(&mut self) {
        // A script set while the window is up. `resumed` only takes this at
        // creation, so before this `app.set_script()` mid-run went nowhere.
        let pending = self.shared.borrow_mut().pending_script.take();
        if let (Some((path, source)), Some(editor)) = (pending, self.editor.as_mut()) {
            editor.script_path = path;
            editor.script = source;
            editor.script_dirty = false;
            self.shared.borrow_mut().script_ran = false;
        }

        // Nothing is taken until there is an editor to act on it. These are
        // set before the first frame -- `editor_start` raises them from the
        // command line -- and the editor is built when the window opens, one
        // or more frames later. Taking them first threw them away on whatever
        // frames came in between, silently: a `.rs` named on the command line
        // loaded on one run and not the next, depending on that timing.
        if self.editor.is_none() {
            return;
        }

        let (asked, asked_restart, asked_open, asked_launch, retry_build) = {
            let mut s = self.shared.borrow_mut();
            (
                std::mem::take(&mut s.run_requested),
                std::mem::take(&mut s.restart_requested),
                std::mem::take(&mut s.open_requested),
                std::mem::take(&mut s.launch_requested),
                std::mem::take(&mut s.rebuild_then_load),
            )
        };
        let asked = asked | asked_restart;
        let Some(editor) = self.editor.as_mut() else { return };
        // Rust first, and on its own: building or launching an example has
        // nothing to do with the Python path below, and a `.rs` in the panel
        // never reaches the script runner.
        let mut rust_messages: Vec<String> = Vec::new();
        let mut load: Option<bool> = None;
        {
            // Reading `Cargo.toml` and stat-ing a file, so not every frame:
            // only when the path or profile changes, or a compile just
            // finished and may have produced the binary.
            let key = (
                editor.script_path.trim_end().to_string(),
                editor.rust_release,
            );
            let busy = editor.building.load(std::sync::atomic::Ordering::SeqCst);
            let finished = editor.was_building && !busy;
            if editor.rust_key != key || finished {
                editor.rust_key = key.clone();
                editor.rust_built = crate::app::cargo::is_current(key.1, &key.0);
            }
            editor.was_building = busy;
            // A build started because a load failed: take it up again now
            // that there is something new to load.
            if finished && std::mem::take(&mut editor.load_after_build) {
                editor.launch_request = true;
            }
        }
        {
            let (build, launch, release) = (
                std::mem::take(&mut editor.build_request),
                std::mem::take(&mut editor.launch_request) | asked_launch,
                editor.rust_release,
            );
            let mut retry_build = retry_build;
            if build || launch || retry_build {
                let path = editor.script_path.trim_end().to_string();
                let busy = editor.building.clone();
                // A load of a library that is out of date would run code
                // the panel is not showing -- and, worse, an old `hosted`
                // against a new host. Build first and load when it lands.
                let stale = launch && !crate::app::cargo::is_current(release, &path);
                let retry = std::mem::take(&mut retry_build) || stale;
                if build || retry {
                    busy.store(true, std::sync::atomic::Ordering::SeqCst);
                    crate::app::cargo::build_hosted(
                        std::path::Path::new(&path),
                        release,
                        busy.clone(),
                    );
                    editor.load_after_build = retry;
                }
                if launch && !stale {
                    load = Some(release);
                }
            }
        }

        for m in rust_messages.drain(..) {
            self.log(&m);
        }
        // Recorded, not done: this is inside a frame, and an example's `main`
        // may call `step()`. `editor_tick` picks it up between two.
        if let Some(release) = load {
            self.shared.borrow_mut().load_requested = Some(release);
        }
        let Some(editor) = self.editor.as_mut() else { return };

        let (run, open, save) = (
            asked | std::mem::take(&mut editor.run_request),
            asked_open | std::mem::take(&mut editor.open_request),
            std::mem::take(&mut editor.save_request),
        );
        if !(run || open || save) {
            return;
        }
        let path = editor.script_path.clone();
        let source = editor.script.clone();

        // Nothing is borrowed across the work below. Messages are collected
        // and flushed at the end, because `log` borrows `shared` and so does
        // reading the script runner -- holding one across the other panicked
        // with `RefCell already mutably borrowed`.
        let mut messages: Vec<String> = Vec::new();
        let mut opened: Option<String> = None;
        let mut saved = false;
        // A file just read is not what is running.
        let mut fresh = false;

        if open {
            match std::fs::read_to_string(&path) {
                Ok(text) => {
                    messages.push(format!("opened {path}"));
                    opened = Some(text);
                }
                Err(e) => messages.push(format!("cannot open {path}: {e}")),
            }
        }

        if save {
            match std::fs::write(&path, &source) {
                Ok(()) => {
                    messages.push(format!("saved {path}"));
                    saved = true;
                }
                Err(e) => messages.push(format!("cannot save {path}: {e}")),
            }
        }

        if opened.is_some() || saved {
            if let Some(editor) = self.editor.as_mut() {
                if let Some(text) = opened {
                    editor.script = text;
                    fresh = true;
                }
                editor.script_dirty = false;
            }
            if fresh {
                let mut shared = self.shared.borrow_mut();
                shared.script_ran = false;
                // Opening a file *shows* it: build the scene and hold at
                // iteration 0, the same as naming one on the command line.
                // Without this the viewport stayed black until Play, and Step
                // stayed grey because nothing had run.
                //
                // Next frame, not this one: `source` was read from the buffer
                // at the top of this function, before the open replaced it,
                // so running now would run the file we just closed.
                shared.restart_requested = true;
            }
        }

        if run {
            // Not executed here. This is inside a frame, and a script that
            // drives its own `while app.step():` cannot run inside one -- it
            // would be a loop inside the loop it is trying to drive, which is
            // exactly what froze the window.
            //
            // So the request is left standing for the caller to take between
            // frames, where a script of either shape runs as the program it
            // is. `App::take_script_request` is that handoff.
            let paused = asked_restart
                | self
                    .editor
                    .as_mut()
                    .map(|e| std::mem::take(&mut e.restart_request))
                    .unwrap_or(false);
            self.shared.borrow_mut().script_pending = Some((path.clone(), source, paused));
        }

        for m in messages {
            self.log(&m);
        }
    }

    /// Ask for the scene to be rebuilt from the script and stopped at the
    /// start, as the Restart button does.
    pub fn restart_script(&mut self) {
        self.shared.borrow_mut().restart_requested = true;
    }

    /// The editor's run loop: open it, act on the arguments, and keep it
    /// going until the window closes.
    ///
    /// This is the whole of what `python -m kalast` used to do in Python. It
    /// lives here because the engine is Rust and Python is a binding -- an
    /// editor that could only be opened from Python was a feature the core
    /// did not have. See "Rust core, Python wrapper" in `CLAUDE.md`.
    ///
    /// `run_script` is the one part that cannot be: executing a `.py` file
    /// needs CPython. So Rust owns the loop and calls out for the
    /// interpreter, rather than an interpreter owning a loop Rust cannot
    /// enter. Callers with no Python -- the `kalast` binary -- pass a closure
    /// that says so; `.rs` and `.obj` arguments work either way, since a Rust
    /// example is built and launched through cargo rather than run in-process.
    ///
    /// Arguments are taken as they are typed: `.py` and `.rs` open in the
    /// script panel, `.obj` loads as a mesh, anything else is reported rather
    /// than guessed at -- passing a `.rs` to the OBJ parser used to produce no
    /// vertices and then take the process down inside wgpu.
    pub fn run_editor<F>(&mut self, args: &[String], mut run_script: F)
    where
        F: FnMut(&mut Self, &str, &str),
    {
        self.editor_start(args);
        loop {
            match self.editor_tick() {
                EditorTick::Closed => break,
                EditorTick::Frame => {}
                // Between frames, so a script that drives its own loop nests
                // here rather than inside the frame, and runs to completion
                // before this loop resumes.
                EditorTick::Run { path, source } => run_script(self, &path, &source),
            }
        }
    }

    /// Open what the command line named, and decide what runs at startup.
    ///
    /// The first half of `run_editor`, separate for the same reason
    /// `editor_tick` is: a caller driving the loop itself still needs it.
    pub fn editor_start(&mut self, args: &[String]) {
        self.config.borrow_mut().editor = true;
        self.sim_config().borrow_mut().title = "kalast".to_string();

        let mut opened_python = false;
        let mut opened_rust: Option<String> = None;
        for arg in args {
            if arg.starts_with('-') {
                continue;
            }
            let path = std::path::Path::new(arg);
            match path.extension().and_then(|e| e.to_str()) {
                Some(ext @ ("py" | "rs")) => match std::fs::read_to_string(path) {
                    Ok(source) => {
                        self.set_script(arg.clone(), source);
                        opened_python |= ext == "py";
                        if ext == "rs" {
                            opened_rust = Some(arg.clone());
                        }
                    }
                    Err(e) => eprintln!("cannot read {arg}: {e}"),
                },
                Some("obj") => {
                    self.simulation
                        .borrow_mut()
                        .load_mesh(path, crate::Mat4::IDENTITY, true);
                }
                _ => eprintln!("don't know what to do with {arg}: expected .py, .rs or .obj"),
            }
        }

        // Held, always: nothing here owns a simulation worth advancing until a
        // script has built one, and a counter climbing over an empty scene
        // gives you nothing to press and no way back to the start.
        self.simulation.borrow_mut().state.is_paused = true;

        // A script named on the command line is *shown*: built and rendered at
        // iteration 0, then held. Opening a file and getting a black viewport
        // until you find Play is no way to open a file.
        if opened_python {
            self.restart_script();
        }

        // A Rust example is shown by loading it -- there is nothing else to
        // render for one -- and if its library is out of date, by compiling
        // it first. Both are the load path's business; opening the file only
        // says that running it is what was meant.
        if opened_rust.is_some() {
            self.shared.borrow_mut().launch_requested = true;
        }
    }

    /// What one turn of the editor loop produced.
    ///
    /// Split out from `run_editor` so the loop can be driven from outside it,
    /// which the Python front door has to do: it reaches the app through a
    /// `RefCell`, and a `run_editor` holding that borrow for the whole session
    /// means a script cannot call back into the app at all. That is what broke
    /// `while app.step():` scripts inside the editor -- not the frame, the
    /// borrow. Driving `editor_tick` takes it for one turn and drops it before
    /// the script runs, so the script gets the app to itself. The policy stays
    /// here, in one copy, whoever is turning the handle.
    pub fn editor_tick(&mut self) -> EditorTick {
        if !self.step() {
            return EditorTick::Closed;
        }
        // Between frames, which is the only place an example may run: its
        // `main` owns a loop of its own if it wants one, and that nests here
        // rather than re-entering the frame that asked for it.
        let load = self.shared.borrow_mut().load_requested.take();
        if let Some(release) = load {
            self.load_example(release);
            return EditorTick::Frame;
        }
        let Some((path, source, paused)) = self.take_script_request() else {
            return EditorTick::Frame;
        };
        self.begin_script(paused);
        EditorTick::Run { path, source }
    }

    /// Clear the scene and set the clock for a script about to run.
    pub fn begin_script(&mut self, paused: bool) {
        // `load_mesh` appends, so a run without this stacks the scene: two
        // craters, and Restart looking like it did nothing.
        self.simulation.borrow_mut().reset();
        // Play rebuilds and runs. Restart rebuilds and runs *one* iteration,
        // then stops -- not "does not run at all", which leaves a black
        // viewport. One iteration means the callbacks fire once, so a script
        // that places its bodies per iteration shows them where iteration 0
        // puts them rather than at the origin.
        if paused {
            self.simulation.borrow_mut().state.pause_at = Some(1);
        }
        self.simulation.borrow_mut().state.is_paused = false;
        self.shared.borrow_mut().script_ran = true;
    }

    /// Take a script the UI has asked to run, if there is one.
    ///
    /// Call it between frames -- `while app.step(): ...` -- and execute what
    /// comes back. Doing it there rather than inside the frame is what lets a
    /// script drive its own loop.
    pub fn take_script_request(&mut self) -> Option<(String, String, bool)> {
        self.shared.borrow_mut().script_pending.take()
    }

    /// Ask the window to close. The next `step()` returns `false`.
    /// Take a guest example's scene as this window's own.
    ///
    /// Called from the guest, through `HostApi::adopt`. The simulation
    /// replaces this one wholesale -- it carries the bodies, the camera and
    /// the config the example just built -- and the callbacks move across
    /// with it. Everything else about this app stays: the window, the editor,
    /// the log.
    pub(crate) fn adopt_scene(
        &mut self,
        simulation: Rc<RefCell<simulation::Simulation>>,
        guest_shared: Rc<RefCell<Shared>>,
    ) {
        self.simulation = simulation;
        // The GPU buffers were built from the bodies that are being replaced.
        self.simulation.borrow_mut().meshes_dirty = true;

        // Only the callbacks. The rest of `Shared` is this window's -- its
        // log, its panel state, the request flags the editor sets -- and
        // taking the guest's would replace a live editor with a blank one.
        let mut guest = guest_shared.borrow_mut();
        let (before, after) = (guest.before_render.take(), guest.after_render.take());
        drop(guest);
        let mut shared = self.shared.borrow_mut();
        shared.before_render = before;
        shared.after_render = after;
        shared.script_ran = true;
        drop(shared);

        // Shown, then held, the same as a `.py` named on the command line:
        // one iteration so the callbacks fire and the scene is where
        // iteration 0 puts it, then stop.
        //
        // Here rather than after the call that loaded it, because a driven
        // example -- one with its own `while` -- does not return until its
        // loop ends. Anything set afterwards would be set when the run was
        // already over, which is why it played straight through.
        let mut sim = self.simulation.borrow_mut();
        sim.state.pause_at = Some(sim.state.iteration + 1);
        sim.state.is_paused = false;
    }

    /// Compile-free half of running a Rust example: load it and let it build
    /// the scene in this window.
    ///
    /// The Python front door runs a `.py` in the process you are looking at;
    /// this is the same for a `.rs`, and the reason the editor no longer
    /// closes and reopens to show one.
    fn load_example(&mut self, release: bool) {
        // Order matters, and the wrong order is a crash rather than a bug.
        // The callbacks currently armed are function pointers into the
        // library about to be unloaded, so they go first; the scene goes with
        // them, because a second load would otherwise stack meshes.
        {
            let mut shared = self.shared.borrow_mut();
            shared.before_render = None;
            shared.after_render = None;
        }
        self.simulation.borrow_mut().reset();
        drop(self.loaded_example.take());

        match crate::app::cargo::load_example(release, self) {
            Ok(library) => self.loaded_example = Some(library),
            Err(e) => {
                // `eprintln!` only: the editor tees stdout and stderr into
                // its own log panel, so logging it again printed it twice.
                eprintln!("{e}");
                // A stale or mismatched library is not something to ask
                // someone to fix by hand -- the editor knows how to build
                // it. Once, so a library that is wrong for another reason
                // does not compile in a loop.
                self.shared.borrow_mut().rebuild_then_load = true;
            }
        }
    }

    /// Pick the facet under the pointer and toggle its selection.
    ///
    /// The ray is built from the same view-projection the frame was drawn
    /// with, so what is picked is what is under the cursor rather than what
    /// would be under it next frame.
    fn select_at_cursor(&mut self) {
        let Some((cx, cy)) = self.cursor else { return };
        let Some(win) = self.window.as_ref() else { return };
        let (rw, rh) = win.render_size;
        if rw == 0 || rh == 0 {
            return;
        }

        // Where the click landed inside the *image*, 0..1. In the editor the
        // scene is fitted into the viewport panel and letterboxed, so the
        // panel rectangle is not the image rectangle.
        let (u, v) = match self.editor.as_ref() {
            Some(editor) if editor.viewport_rect.width() > 0.0 => {
                let ppp = editor.scale();
                let (px, py) = (cx as f32 / ppp, cy as f32 / ppp);
                let into = editor.viewport_rect;
                let aspect = rw as f32 / rh as f32;
                let mut size = into.size();
                if size.x / size.y > aspect {
                    size.x = size.y * aspect;
                } else {
                    size.y = size.x / aspect;
                }
                let min = into.center() - size * 0.5;
                ((px - min.x) / size.x, (py - min.y) / size.y)
            }
            _ => {
                let size = win.window.inner_size();
                (
                    cx as f32 / size.width.max(1) as f32,
                    cy as f32 / size.height.max(1) as f32,
                )
            }
        };
        if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
            return;
        }

        let (origin, dir) = {
            let sim = self.simulation.borrow();
            let aspect = rw as Float / rh as Float;
            let Ok(view_proj) = sim.camera.view_proj(aspect) else {
                return;
            };
            let inverse = view_proj.inverse();
            // wgpu clip space: x and y in -1..1 with y up, depth in 0..1.
            let (x, y) = ((2.0 * u as Float) - 1.0, 1.0 - (2.0 * v as Float));
            let near = inverse.project_point3(crate::Vec3::new(x, y, 0.0));
            let far = inverse.project_point3(crate::Vec3::new(x, y, 1.0));
            let dir = (far - near).normalize_or_zero();
            if dir == crate::Vec3::ZERO {
                return;
            }
            (near, dir)
        };

        let picked = self.simulation.borrow().pick_facet(origin, dir);
        let Some((body, facet, world, local)) = picked else {
            println!("nothing under the pointer");
            return;
        };

        let color = {
            let c = self.sim_config();
            let c = c.borrow().selection_color;
            crate::Vec3::new(c.r as Float, c.g as Float, c.b as Float)
        };
        let now_selected = self
            .simulation
            .borrow_mut()
            .toggle_facet(body, facet, color);

        let (lat, lon) = crate::app::simulation::lat_lon(local);
        let (list, geometry) = {
            let sim = self.simulation.borrow();

            // Sorted as numbers, not as text: string order put facet 1322
            // ahead of 553.
            let mut pairs: Vec<(usize, usize)> = sim
                .selected_facets
                .iter()
                .map(|s| (s.body, s.facet))
                .collect();
            pairs.sort_unstable();
            let many = sim.bodies.len() > 1;
            let list: Vec<String> = pairs
                .iter()
                .map(|(b, f)| {
                    if many {
                        format!("{b}:{f}")
                    } else {
                        f.to_string()
                    }
                })
                .collect();

            // Everything below is in the body's own frame, which is where the
            // mesh data lives and what a script indexes.
            let geometry = sim.bodies.get(body).and_then(|b| b.mesh.as_ref()).map(|m| {
                let m = m.borrow();
                let f = m.facets[facet];
                let v = m.get_facet_positions(facet).map(|p| *p);
                (f.normal, f.pos, f.area, v)
            });
            (list, geometry)
        };

        println!(
            "{} body {body} facet {facet}",
            if now_selected { "selected" } else { "deselected" }
        );
        println!(
            "  hit    {:.6} {:.6} {:.6}   lat {lat:.4} lon {lon:.4}",
            local.x, local.y, local.z
        );
        // Only when the two differ. With the body at the origin unrotated
        // they are the same numbers, and printing them twice says nothing.
        if (world - local).length() > 1e-9 {
            println!(
                "  world  {:.6} {:.6} {:.6}",
                world.x, world.y, world.z
            );
        }
        if let Some((normal, center, area, v)) = geometry {
            println!(
                "  normal {:.6} {:.6} {:.6}",
                normal.x, normal.y, normal.z
            );
            println!(
                "  center {:.6} {:.6} {:.6}   area {area:.6}",
                center.x, center.y, center.z
            );
            for (i, p) in v.iter().enumerate() {
                println!("  v{i}     {:.6} {:.6} {:.6}", p.x, p.y, p.z);
            }
        }
        println!(
            "  selected ({}): {}",
            list.len(),
            if list.is_empty() {
                "none".to_string()
            } else {
                list.join(" ")
            }
        );
    }

    pub fn close(&mut self) {
        self.shared.borrow_mut().exit_requested = true;
    }

    /// Whether the window is still open.
    /// Give the host this app's simulation and callbacks, once.
    ///
    /// Everything an example builds lives in those two: bodies, camera,
    /// config, and the `before_render`/`after_render` it installed. The App
    /// around them is scaffolding -- a window it never opened, an event loop
    /// it will never pump.
    fn give_scene_to_host(&mut self) {
        if self.gave_scene {
            return;
        }
        self.gave_scene = hosted::adopt(&self.simulation, &self.shared);
    }

    pub fn is_running(&self) -> bool {
        // Hosted: the window is the host's, and so is the answer. Without
        // this a driven example's `while app.is_running()` would spin on its
        // own flag long after the editor's window had closed.
        if let Some(running) = hosted::is_running() {
            return running;
        }
        self.shared.borrow().running
    }

    /// The simulation's config, as a handle.
    ///
    /// Cloned out under a short borrow, so the caller can hold it across a
    /// `borrow_mut` of the simulation itself -- they are different cells.
    /// Must not be called while the simulation is already mutably borrowed.
    pub fn sim_config(&self) -> Rc<RefCell<crate::app::config::Config>> {
        self.simulation.borrow().config.clone()
    }

    /// Kept for callers that set up a controller before any frame runs.
    /// `apply_live_config` does the same copy at the top of every frame, so
    /// these no longer need to be set before `start()`.
    pub fn apply_config_at_start(&mut self) {
        let c = self.sim_config();
        let c = c.borrow();
        self.controller.sensitivity_move = c.sensitivity_move;
        self.controller.sensitivity_look = c.sensitivity_look;
        self.controller.sensitivity_rotate = c.sensitivity_rotate;
        self.controller.sensitivity_zoom = c.sensitivity_zoom;
        self.controller.emulate_middle_button = c.emulate_middle_button;
    }

    pub fn set_tick<F>(&mut self, f: F)
    where
        F: Fn(&mut simulation::Simulation, Float) + 'static,
    {
        self.shared.borrow_mut().before_render = Some(Tick::Rust(Box::new(f)));
    }

    pub fn with_tick<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut simulation::Simulation, Float) + 'static,
    {
        self.set_tick(f);
        self
    }

    pub fn set_after_render<F>(&mut self, f: F)
    where
        F: Fn(&mut simulation::Simulation, Float) + 'static,
    {
        self.shared.borrow_mut().after_render = Some(Tick::Rust(Box::new(f)));
    }

    /// Invokes one of the two frame callbacks. Both take the same arguments
    /// and differ only in when the app calls them.
    ///
    /// The tick is taken out of `shared` for the duration of the call and put
    /// back after. A callback is allowed to assign `app.before_render` from
    /// inside itself, and holding the borrow across the call would panic the
    /// moment one did; putting it back only when the slot is still empty
    /// means a callback that replaces itself keeps the replacement.
    fn run_callback(
        shared: &Rc<RefCell<Shared>>,
        before: bool,
        sim: &Rc<RefCell<simulation::Simulation>>,
        dt: Float,
    ) {
        let taken = {
            let mut s = shared.borrow_mut();
            if before {
                s.before_render.take()
            } else {
                s.after_render.take()
            }
        };

        match &taken {
            Some(Tick::Rust(f)) => {
                f(&mut sim.borrow_mut(), dt);
            }
            #[cfg(feature = "python")]
            Some(Tick::Python {
                callback,
                simulation,
            }) => {
                let failed = Python::attach(|py: Python<'_>| {
                    match callback.call1(py, (simulation.clone(), dt)) {
                        Ok(_) => false,
                        Err(e) => {
                            // Printed, not unwrapped. A mistake in a callback
                            // used to abort the process: `.unwrap()` on the
                            // `PyErr` panicked, and the panic unwound through
                            // `step()`, which had taken the event loop and so
                            // never gave it back.
                            e.print(py);
                            true
                        }
                    }
                });
                if failed {
                    // Dropped rather than left to raise every frame. A
                    // callback that fails once fails every time, and a
                    // traceback per frame buries the first one.
                    let mut s = shared.borrow_mut();
                    if before {
                        s.before_render = None;
                    } else {
                        s.after_render = None;
                    }
                    s.log.push(if before {
                        "before_render raised; it has been disconnected -- fix it and press Restart"
                    } else {
                        "after_render raised; it has been disconnected -- fix it and press Restart"
                    });
                    return;
                }
            }
            None => {}
        }

        let mut s = shared.borrow_mut();
        let slot = if before {
            &mut s.before_render
        } else {
            &mut s.after_render
        };
        if slot.is_none() {
            *slot = taken;
        }
    }

    pub fn exit(&mut self, ev: &winit::event_loop::ActiveEventLoop) {
        let win = self.window.as_mut().unwrap();

        if self.simulation.borrow().camera.control == frame::Control::WASD {
            win.reset_cursor();
        }

        // win.get_window().screenshot()

        // Block until every queued/in-flight frame export has actually been
        // written to disk, otherwise anything still in the pipeline when
        // this process exits is silently lost -- there is no resuming a
        // killed background thread.
        let device = win.device.clone();
        win.frame_exporter.finish(&device);

        self.shared.borrow_mut().running = false;
        ev.exit()
    }

    pub fn toggle_export_frame(&mut self) {
        self.window.as_mut().unwrap().toggle_export_frame();
    }
}

impl winit::application::ApplicationHandler<crate::app::window::Window> for crate::app::App {
    fn resumed(&mut self, ev: &winit::event_loop::ActiveEventLoop) {
        // Poll, not Wait. With Wait the loop sleeps until an event arrives,
        // and the only thing that woke it was the redraw it had queued
        // itself -- see `about_to_wait`.
        ev.set_control_flow(winit::event_loop::ControlFlow::Poll);

        // A zero width or height means "pick one", and the pick is made from
        // the monitor rather than from a constant. Passing 0 through to winit
        // got its own fallback, which is a fixed 800x600 that knows nothing
        // about the screen it lands on -- fine on the display it was chosen
        // for, wrong on anything else.
        //
        // 70% of the *work area*, not the full bounds, so the taskbar or dock
        // does not eat the bottom of the window. Each axis is resolved on its
        // own, so setting only `width` still gets a sensible height.
        let (want_w, want_h) = {
            let c = self.config.borrow();
            (c.width, c.height)
        };
        // The editor wraps the scene in panels, so it wants more of the
        // screen than a bare render window does. Neither takes all of it:
        // the remainder is what the title bar and taskbar live in, and what
        // lets the window be moved without being dragged off-screen.
        let fraction = if self.want_editor || self.config.borrow().editor {
            0.85
        } else {
            0.7
        };
        let monitor = ev.primary_monitor().or_else(|| ev.available_monitors().next());
        let (auto_w, auto_h) = monitor
            .as_ref()
            .map(|m| {
                let s = m.size();
                (
                    (s.width as f32 * fraction) as u32,
                    (s.height as f32 * fraction) as u32,
                )
            })
            // No monitor to ask -- headless, or a compositor that will not
            // say. The old fixed default is as good a guess as any.
            .unwrap_or((800, 600));

        let size = winit::dpi::PhysicalSize::new(
            if want_w == 0 { auto_w.max(320) } else { want_w },
            if want_h == 0 { auto_h.max(240) } else { want_h },
        );
        let mut attrs = winit::window::Window::default_attributes()
            .with_inner_size(size)
            .with_title(&self.sim_config().borrow().title);

        // Centre on *one* monitor, not on the desktop. Left to the window
        // manager, a window on a multi-monitor desktop is centred on the
        // whole virtual area -- on two 1920-wide screens that puts a
        // 1648-wide window at x = 1096, straddling the join, so it reads as
        // being bigger than a screen when it is only in the wrong place.
        //
        // `monitor.position()` is the monitor's own origin in desktop
        // coordinates, so this works whichever monitor is primary and
        // whatever their arrangement.
        if let Some(m) = monitor.as_ref() {
            let origin = m.position();
            let s = m.size();
            let x = origin.x + ((s.width as i32 - size.width as i32) / 2).max(0);
            let y = origin.y + ((s.height as i32 - size.height as i32) / 2).max(0);
            attrs = attrs.with_position(winit::dpi::PhysicalPosition::new(x, y));
        }

        let win = Arc::new(ev.create_window(attrs).unwrap());
        // After creation, not through `with_fullscreen`: that attribute can
        // only ask for the native kind, and simple fullscreen is a call on a
        // window that exists.
        // Before anything else can press it.
        #[cfg(target_os = "macos")]
        crate::app::macos::disable_native_fullscreen(&win);

        if self.sim_config().borrow().fullscreen {
            set_window_fullscreen(&win, true);
        }

        let sim_cfg = self.sim_config();
        self.window = Some(pollster::block_on(crate::app::window::Window::new(
            ev.owned_display_handle(),
            win.clone(),
            &sim_cfg.borrow(),
            &self.simulation.borrow(),
        )));

        if self.want_editor || self.config.borrow().editor {
            let w = self.window.as_ref().unwrap();
            let mut editor = crate::app::gui::Editor::new(&win, &w.device, w.surface_config.format);
            if let Some((path, source)) = self.shared.borrow_mut().pending_script.take() {
                editor.script_path = path;
                editor.script = source;
            }
            self.editor = Some(editor);
            // Only now, so a run that never opens an editor keeps its
            // descriptors as they were.
            self.stdio = crate::app::gui::StdioCapture::new();
        }
    }

    /// Keep a redraw pending, every time the event queue empties.
    ///
    /// The redraw chain used to be self-perpetuating: the only
    /// `request_redraw` was *inside* the `RedrawRequested` handler, so each
    /// frame asked for the next. macOS stops delivering redraws to an
    /// occluded window, and a single dropped event therefore broke the chain
    /// for good -- the simulation sat idle indefinitely, and clicking the
    /// window to give it focus was what restarted it, since that made AppKit
    /// issue a redraw of its own.
    ///
    /// Requesting from here instead makes the loop independent of whether the
    /// window is visible. Together with the `Occluded` fix in `window.rs`,
    /// which lets a frame run without a drawable, a covered window now runs at
    /// full speed rather than stopping.
    fn about_to_wait(&mut self, ev: &winit::event_loop::ActiveEventLoop) {
        // `close()` runs outside any handler and so has no `ActiveEventLoop`
        // to exit with. Here is the first place that does.
        if self.shared.borrow().exit_requested && self.window.is_some() {
            self.shared.borrow_mut().exit_requested = false;
            self.exit(ev);
            return;
        }
        if let Some(win) = self.window.as_ref() {
            win.get_window().request_redraw();
        }
    }

    fn window_event(
        &mut self,
        ev: &winit::event_loop::ActiveEventLoop,
        _id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        // Cloned once, up front: the simulation's config is a different cell
        // from the simulation, so this can be held across a `borrow_mut` of
        // the scene -- but obtaining it cannot, since it reads the field.
        let sim_cfg = self.sim_config();

        // The UI gets first refusal. Without this a drag on a slider would
        // also orbit the camera behind the panel.
        if let (Some(editor), Some(win)) = (self.editor.as_mut(), self.window.as_ref()) {
            let window = win.window.clone();
            let consumed = editor.on_window_event(&window, &event);

            // Pointer events belong to whatever the pointer is over, and
            // `consumed` alone does not decide it in either direction.
            //
            // egui claims every pointer event over one of its widgets, and
            // the viewport *is* one -- an `Image` -- so the camera saw no
            // drag on the scene at all. But egui does *not* claim a scroll
            // over a panel's background, so that fell through to the camera
            // and scrolling the script panel zoomed the render.
            //
            // So: the scene gets pointer events only while the pointer is on
            // it, and the UI gets them the rest of the time.
            let pointer = matches!(
                event,
                winit::event::WindowEvent::MouseInput { .. }
                    | winit::event::WindowEvent::MouseWheel { .. }
                    | winit::event::WindowEvent::CursorMoved { .. }
                    | winit::event::WindowEvent::CursorLeft { .. }
            );
            if pointer {
                // A drag that began on the scene keeps it, even once the
                // pointer wanders over a panel -- releasing the button
                // outside the viewport must still end the drag, or the camera
                // would be left spinning.
                // Any button held, not `is_dragging()`, which only knows
                // about the middle button and its alt-left stand-in: a plain
                // left drag has to survive straying over a panel too.
                let held = self.controller.left_pressed || self.controller.middle_pressed;
                if !editor.pointer_on_scene() && !held {
                    return;
                }
            } else if consumed
                && !matches!(event, winit::event::WindowEvent::RedrawRequested)
            {
                return;
            }
        }

        match event {
            winit::event::WindowEvent::CloseRequested => self.exit(ev),
            winit::event::WindowEvent::Resized(size) => {
                let win = self.window.as_mut().unwrap();
                win.resize(size.width, size.height, &sim_cfg.borrow());
            }

            // Was not handled at all, and the surface was left describing the
            // old backing scale. Entering fullscreen with the green button is
            // one of the ways macOS sends this -- a `Resized` may or may not
            // follow, so waiting for one leaves the swapchain at a size the
            // drawable no longer has.
            //
            // Reconfiguring from the window's own size, rather than scaling
            // the old one, since that is what the drawable will be.
            winit::event::WindowEvent::ScaleFactorChanged { .. } => {
                let win = self.window.as_mut().unwrap();
                let size = win.window.inner_size();
                win.resize(size.width, size.height, &sim_cfg.borrow());
            }
            winit::event::WindowEvent::RedrawRequested => {
                // One `step()` is one frame, and this is what makes that
                // true. The handler re-requests a redraw on entry, so a
                // single `pump_app_events` dispatches every redraw it can
                // feed itself: a tight Rust loop got about five frames per
                // call, five `update()`s, and the work done before the call
                // applied to only the first of them -- exactly the mismatch
                // between a moved Sun and the shadow map that the
                // before/after-`step()` idiom exists to avoid. Python's
                // slower loop happened to get one, so it never showed there.
                //
                // The redraw stays requested, so the next `step()` has one
                // waiting and loses nothing.
                if self.stepping && self.frame_drawn {
                    return;
                }
                {
                    let win = self.window.as_mut().unwrap();
                    win.window.request_redraw();

                    if !win.is_surface_configured {
                        if self.sim_config().borrow().debug_window {
                            println!("[WINDOW] surface is not configured yet")
                        }
                        return;
                    }
                }

                // Before anything reads the config this frame, so a value
                // changed between two `step()`s takes effect on this frame
                // rather than the next.
                if let Some(stdio) = self.stdio.as_mut() {
                    stdio.drain(&mut self.shared.borrow_mut().log);
                }

                self.apply_live_config();

                let now = std::time::Instant::now();
                self.dt = (now - self.now).as_secs_f64() as _;
                self.now = now;

                self.fps_window_secs += self.dt;
                self.fps_window_frames += 1;
                if self.fps_window_secs >= HUD_RATE_WINDOW {
                    self.fps_shown = self.fps_window_frames as Float / self.fps_window_secs;
                    self.fps_window_secs = 0.0;
                    self.fps_window_frames = 0;
                }

                // Pause has to gate the callbacks, not just the iteration
                // counter. Every Python-driven run puts its physics in
                // `before_render`/`after_render`, so gating only
                // `Simulation::update` -- which does nothing but increment
                // `state.iteration` -- left P with no effect on any of them.
                // The frame itself still runs and still presents, so the
                // window keeps drawing the paused scene and stays responsive
                // to input; only the simulation stops advancing.
                let paused = self.simulation.borrow().state.is_paused;

                // Held across the borrow below: the editor draws after it,
                // because the UI needs `&mut Simulation::state` for its
                // play/pause buttons and cannot take it while the frame does.
                let mut editor_surface: Option<wgpu::SurfaceTexture> = None;

                if !paused {
                    Self::run_callback(&self.shared, true, &self.simulation, self.dt);
                }

                {
                    let mut sim = self.simulation.borrow_mut();
                    let win = self.window.as_mut().unwrap();

                    sim.camera
                        .update_with_controller(&mut self.controller, self.dt);

                    win.update(&mut sim, &sim_cfg.borrow());

                    // Which iteration this frame is *for*. Running, that is
                    // the counter as it stands -- it moves on only after the
                    // frame. Paused, it has already moved past what is on
                    // screen, so the last advancing frame's number is the
                    // honest one.
                    let drawn = if paused {
                        self.shared.borrow().drawn_iteration
                    } else {
                        sim.state.iteration
                    };

                    // The HUDs are shared handles, so this reads whatever
                    // `before_render` just wrote into them. Only the text is
                    // expanded; position, size and colour are used as they
                    // stand.
                    let huds: Vec<crate::app::config::Hud> = sim
                        .huds
                        .iter()
                        .map(|h| {
                            let h = h.borrow();
                            crate::app::config::Hud {
                                // A pinned HUD is the editor's, not the
                                // script's; `text` goes on being written and
                                // goes on being ignored.
                                text: expand_hud(
                                    h.pin.as_deref().unwrap_or(&h.text),
                                    &sim.state,
                                    self.fps_shown,
                                    &sim.diagnostics,
                                    drawn,
                                ),
                                ..h.clone()
                            }
                        })
                        .collect();

                    // Acquired here, not at the top of the frame.
                    //
                    // A drawable is a scarce resource -- the surface is
                    // configured for two frames of latency -- and holding one
                    // across `before_render` meant holding it across
                    // arbitrary user Python: SPICE lookups, a TPM step,
                    // whatever the script does. In a native-fullscreen window
                    // that starved the pool until `nextDrawable` hit its
                    // one-second timeout, measured at 1001 ms and 3725 ms of
                    // `acquire drawable` while the same window merely
                    // maximised was fine.
                    //
                    // Occlusion is still deliberately not an early return: an
                    // occluded window yields no drawable, and skipping the
                    // frame on that basis halted the simulation outright
                    // rather than just not drawing it. The frame runs either
                    // way; only the present is skipped.
                    let surface_texture = win.get_surface_texture(&sim_cfg.borrow());
                    if self.editor.is_some() {
                        // The scene goes offscreen and the swapchain is left
                        // to the UI. `render(None, ..)` is exactly that, and
                        // it is the same path an occluded window already
                        // takes -- a full frame minus the blit and present.
                        win.render(None, &sim_cfg.borrow(), &huds);
                        editor_surface = surface_texture;
                    } else {
                        win.render(surface_texture, &sim_cfg.borrow(), &huds);
                    }

                    // After render: the shadow map now holds this frame's
                    // geometry, so a query here answers for the scene
                    // before_render just set up.
                    let one_off = sim.facet_shadow_request.take();
                    if sim_cfg.borrow().access_shadow_map || one_off.is_some() {
                        let n = sim.bodies.len();
                        sim.facet_shadow_result.resize(n, vec![]);

                        for body in 0..n {
                            let wanted =
                                sim_cfg.borrow().access_shadow_map || one_off == Some(body);
                            if wanted {
                                sim.facet_shadow_result[body] =
                                    win.facet_shadow_fractions(body);
                            } else {
                                // Stale results would silently describe an
                                // older frame's geometry.
                                sim.facet_shadow_result[body].clear();
                            }
                        }
                    } else if !sim.facet_shadow_result.is_empty() {
                        sim.facet_shadow_result.clear();
                    }

                    // Same reasoning as the shadow query: the ID pass draws
                    // the scene the callbacks just positioned, so it belongs
                    // after the render, and its result is dropped when not
                    // requested rather than left to describe an older frame.
                    if let Some((body, facets, res, batch)) = sim.hemicube_request.take() {
                        let mesh = sim
                            .bodies
                            .get(body)
                            .and_then(|b| b.mesh.as_ref())
                            .map(|m| m.borrow().clone());
                        // Same scene fit the shadow pass uses: the frustum has
                        // to cover the companion, not just the body it sits on.
                        let scene = sim.scene_bounds();
                        sim.hemicube_result = mesh.map(|m| {
                            let (rows, offsets, n_total) =
                                win.hemicube_rows(body, &m, scene, &facets, res, batch);
                            (rows, facets.len(), n_total as usize, offsets)
                        });
                    } else {
                        sim.hemicube_result = None;
                    }

                    if sim.facet_id_request {
                        sim.facet_id_request = false;
                        sim.facet_id_result = Some(win.facet_id_map());
                    } else {
                        sim.facet_id_result = None;
                    }

                    sim.export_once = false;
                }

                // Outside the borrow above: the callback takes the
                // Simulation itself, so it cannot run while it is held.
                if !paused {
                    Self::run_callback(&self.shared, false, &self.simulation, self.dt);
                }

                if let (Some(editor), Some(texture)) = (self.editor.as_mut(), editor_surface) {
                    let win = self.window.as_mut().unwrap();
                    let view = texture
                        .texture
                        .create_view(&wgpu::TextureViewDescriptor::default());
                    let scene_size = win.render_size;
                    let scene_generation = win.render_generation;
                    let window = win.window.clone();
                    let wanted = {
                        let mut sim = self.simulation.borrow_mut();
                        editor.draw(
                            &window,
                            &win.device,
                            &win.queue,
                            &view,
                            (win.surface_config.width, win.surface_config.height),
                            &win.passes.render.render_texture,
                            scene_size,
                            scene_generation,
                            &mut sim_cfg.borrow_mut(),
                            &mut self.config.borrow_mut(),
                            &mut sim,
                            &mut self.shared.borrow_mut(),
                            self.fps_shown as f32,
                        )
                    };
                    // Applied for the *next* frame: this one is already drawn
                    // at the old size, and reallocating the targets underneath
                    // it would throw the image away mid-frame.
                    win.set_render_size(wanted.0, wanted.1);
                    win.queue.present(texture);
                }

                self.serve_editor_requests();

                // What the frame just drawn was drawn for, before the
                // counter moves past it.
                //
                // Only when the frame advanced. A paused frame redraws the
                // same scene, and by then `state.iteration` has already moved
                // one past what is on screen -- taking it there would report
                // the number the pause stopped *at* rather than the one being
                // shown, which is exactly the off-by-one this is here to fix.
                if !paused {
                    self.shared.borrow_mut().drawn_iteration =
                        self.simulation.borrow().state.iteration;
                }

                // Advance only now that both callbacks have run, so they
                // agree on which frame they are in -- a loop deriving an
                // epoch from `state.iteration` would otherwise see two
                // different times within one frame.
                //
                // Gated on the pause state this frame *started* with, not the
                // one now. The UI is drawn near the end of the frame, so
                // pressing Step unpauses after the callbacks have already
                // been skipped and nothing new has been rendered -- and
                // `update()` re-reading the flag would then count an
                // iteration that never ran. `pause_at` fired immediately
                // afterwards, so Step advanced the counter, drew nothing, and
                // looked stuck.
                //
                // Skipping it here leaves the change to the next frame, which
                // does run the callbacks and render, which is what Step
                // means.
                if !paused {
                    self.simulation.borrow_mut().update();
                }

                // Reached only by a frame that actually rendered: the
                // early return above, for a surface that is not configured
                // yet, deliberately leaves this unset so `step()` keeps
                // pumping rather than reporting a frame that did nothing.
                self.frame_drawn = true;
            }

            winit::event::WindowEvent::KeyboardInput {
                event:
                    winit::event::KeyEvent {
                        physical_key: winit::keyboard::PhysicalKey::Code(code),
                        state: key_state,
                        ..
                    },
                ..
            } => {
                let is_pressed = key_state.is_pressed();
                self.controller.handle_key(code, is_pressed);

                match (code, is_pressed) {
                    // `Escape` is deliberately not bound. It quit, which is a
                    // long run thrown away by the key most often pressed to
                    // mean "stop what you are doing" -- and quitting is
                    // already the window's close button, and Cmd-Q.
                    (winit::keyboard::KeyCode::Space, true) => {
                        // let win = self.window.as_mut().unwrap();
                        // win.toggle_color_xy = !win.toggle_color_xy;
                    }
                    (winit::keyboard::KeyCode::KeyP, true) => {
                        let pause = self.simulation.borrow_mut().state.toggle_pause();
                        if self.sim_config().borrow().debug_app {
                            println!("[APP] Simulation paused={}", pause);
                        }
                    }

                    // A way out of fullscreen. Needed on macOS in particular:
                    // simple fullscreen hides the title bar, and with it the
                    // green button that got you there.
                    (winit::keyboard::KeyCode::KeyF, true) => {
                        let cfg = self.sim_config();
                        let want = !cfg.borrow().fullscreen;
                        cfg.borrow_mut().fullscreen = want;
                    }

                    // One iteration, then hold: exactly what the editor's
                    // Step button does, so the two cannot drift apart.
                    //
                    // Restart deliberately has no key. It clears the scene and
                    // runs the script again, which is a long run thrown away
                    // by a keystroke -- worth having to aim for.
                    (winit::keyboard::KeyCode::KeyK, true) => {
                        let mut sim = self.simulation.borrow_mut();
                        sim.state.pause_at = Some(sim.state.iteration + 1);
                        sim.state.is_paused = false;
                    }

                    (winit::keyboard::KeyCode::KeyT, true) => {
                        // switch camera type
                        self.simulation.borrow_mut().camera.control.toggle();
                        let control = self.simulation.borrow().camera.control;
                        if self.sim_config().borrow().debug_app {
                            println!("[APP] Camera control changed, now is {:?}", control);
                        }
                        match control {
                            frame::Control::Arcball => {
                                // reset cursor middle
                                let win = self.window.as_ref().unwrap();
                                win.reset_cursor();
                            }
                            frame::Control::WASD => {
                                // no cursor in WASD
                                let win = self.window.as_ref().unwrap();
                                win.center_cursor();
                                win.window.set_cursor_visible(false);
                                win.window
                                    .set_cursor_grab(winit::window::CursorGrabMode::Confined)
                                    .or_else(|_e| {
                                        win.window
                                            .set_cursor_grab(winit::window::CursorGrabMode::Locked)
                                    })
                                    .unwrap();
                            }
                            frame::Control::None => {}
                        }
                    }

                    (winit::keyboard::KeyCode::KeyH, true) => {
                        println!(
                            "camera: pos={} up={} dir={} anchor={} projection={:?}",
                            self.simulation.borrow().camera.pos,
                            self.simulation.borrow().camera.up,
                            self.simulation.borrow().camera.dir,
                            self.simulation.borrow().camera.anchor,
                            self.simulation.borrow().camera.projection
                        );
                    }

                    _ => {}
                };
            }

            winit::event::WindowEvent::PinchGesture { delta, .. } => {
                if self.simulation.borrow().camera.control == frame::Control::Arcball {
                    self.controller.zoom(delta as Float);
                }
            }

            // The camera orbits from `DeviceEvent::MouseMotion`, which is
            // raw and carries no position, so this is the only place the
            // pointer's actual location arrives.
            winit::event::WindowEvent::CursorMoved { position, .. } => {
                self.cursor = Some((position.x, position.y));
            }

            winit::event::WindowEvent::MouseInput { state, button, .. } => match button {
                winit::event::MouseButton::Middle => {
                    self.controller.middle_pressed = state.is_pressed();
                }
                winit::event::MouseButton::Left => {
                    self.controller.left_pressed = state.is_pressed();

                    // A plain left click picks a facet. Not alt-left, which
                    // is the orbit drag, and not a left drag either -- only a
                    // press and release within a few pixels, so pointing at
                    // something and moving the camera stay distinguishable.
                    if state.is_pressed() {
                        self.left_press = self.cursor;
                    } else if let (Some(down), Some(up)) = (self.left_press.take(), self.cursor) {
                        let moved = (down.0 - up.0).hypot(down.1 - up.1);
                        if moved < 4.0 && !self.controller.alt_pressed {
                            self.select_at_cursor();
                        }
                    }
                }
                _ => {}
            },

            winit::event::WindowEvent::ModifiersChanged(modifiers) => {
                self.controller.shift_pressed = modifiers.state().shift_key();
                self.controller.alt_pressed = modifiers.state().alt_key();
            }

            _ => {}
        };
    }

    fn device_event(
        &mut self,
        _ev_loop: &winit::event_loop::ActiveEventLoop,
        _id: winit::event::DeviceId,
        ev: winit::event::DeviceEvent,
    ) {
        // Device events are raw input, not addressed to a window, so they
        // never reach `window_event` and none of its routing applied to them
        // -- which is the whole of the camera's look and zoom. Scrolling
        // anywhere at all, a panel included, zoomed the scene.
        //
        // They carry no position either, so where the pointer is has to come
        // from the last position the UI saw.
        if let Some(editor) = self.editor.as_ref() {
            // WASD grabs the cursor, so its position says nothing and every
            // motion is a look. A drag already under way keeps the scene even
            // if the pointer has wandered onto a panel.
            let wasd = self.simulation.borrow().camera.control == frame::Control::WASD;
            let held = self.controller.left_pressed || self.controller.middle_pressed;
            if !wasd && !held && !editor.pointer_on_scene() {
                return;
            }
        }

        match ev {
            winit::event::DeviceEvent::MouseMotion { delta: (dx, dy) } => {
                match self.simulation.borrow().camera.control {
                    // WASD grabs the cursor, so every motion is a look.
                    frame::Control::WASD => {
                        self.controller.mouse_motion(dx as Float, dy as Float);
                    }
                    // Arcball only reacts during a drag -- middle button, or
                    // alt + left where there is no middle button to press --
                    // leaving the cursor free for everything else.
                    frame::Control::Arcball if self.controller.is_dragging() => {
                        self.controller.drag(dx as Float, dy as Float);
                    }
                    _ => {}
                }
            }

            winit::event::DeviceEvent::MouseWheel { delta } => {
                // A wheel reports discrete notches, a trackpad reports
                // pixels. Normalising them here is what lets one sensitivity
                // constant feel right on both -- previously a notch was
                // multiplied by 100 and fed to rotation, so a mouse could
                // only spin the camera in huge single-axis jumps.
                let notches = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, dy) => dy as Float,
                    winit::event::MouseScrollDelta::PixelDelta(winit::dpi::PhysicalPosition {
                        y,
                        ..
                    }) => y as Float / 50.0,
                };

                if self.simulation.borrow().camera.control == frame::Control::Arcball {
                    self.controller.zoom(notches);
                }
            }
            _ => {}
        };
    }
}

/// What the editor's `Run` button calls.
#[cfg(feature = "python")]
pub struct ScriptRunner {
    pub callback: Py<PyAny>,
    /// A handle of its own on the same app, exactly as `Tick::Python` carries
    /// one. Handing the script the object it was installed from would work
    /// only until that object's own method is on the stack -- which, during
    /// `start_editor`, it always is.
    pub app: crate::py::app::App,
}

pub enum Tick {
    Rust(Box<dyn for<'a> Fn(&'a mut simulation::Simulation, Float)>),
    // Only exists with the bindings: it holds a `Py<PyAny>`, and the engine
    // built without them has no Python to call.
    #[cfg(feature = "python")]
    Python {
        callback: Py<PyAny>,
        /// The simulation, which carries its own config. A handle of its own,
        /// not one reached through the app: the pyclass whose `start()` is on
        /// the stack is borrowed for the whole run loop.
        simulation: crate::py::app::simulation::Simulation,
    },
}

#[cfg(test)]
mod hud_tests {
    use super::*;
    use crate::app::simulation::State;

    fn state(iteration: usize, paused: bool, pause_at: Option<usize>) -> State {
        let mut s = State::new();
        s.iteration = iteration;
        s.is_paused = paused;
        s.pause_at = pause_at;
        s
    }

    /// `{drawn}` is the frame on screen and `{it}` the counter, and the
    /// two differ by one for the whole of every frame -- which is the
    /// reason `{drawn}` exists.
    #[test]
    fn drawn_is_not_the_iteration_counter() {
        let s = state(7, false, None);
        assert_eq!(
            expand_hud("{drawn} of {it}", &s, 60.0, &Default::default(), 6),
            "6 of 7"
        );
    }

    #[test]
    fn expands_the_documented_placeholders() {
        let s = state(42, false, Some(500));
        assert_eq!(
            expand_hud("{it}/{nit} ({its} it/s)", &s, 60.4, &Default::default(), s.iteration),
            "42/500 (60 it/s)"
        );
    }

    /// Rates are whole numbers unless asked otherwise: a tenth of a frame per
    /// second is noise, and the digit churns without informing anyone.
    #[test]
    fn rates_are_integers_by_default_and_precision_is_opt_in() {
        let s = state(1, false, None);
        assert_eq!(expand_hud("{fps}", &s, 59.62, &Default::default(), s.iteration), "60");
        assert_eq!(expand_hud("{fps:.1}", &s, 59.62, &Default::default(), s.iteration), "59.6");
        assert_eq!(expand_hud("{fps:.2f}", &s, 59.62, &Default::default(), s.iteration), "59.62");
    }

    /// Milliseconds are the exception -- whole numbers cannot separate 8 from
    /// 8.4 ms, which is the difference between hitting and missing 120 Hz.
    #[test]
    fn milliseconds_keep_one_decimal_by_default() {
        let s = state(1, false, None);
        assert_eq!(expand_hud("{ms}", &s, 120.0, &Default::default(), s.iteration), "8.3");
        assert_eq!(expand_hud("{ms:.0}", &s, 120.0, &Default::default(), s.iteration), "8");
    }

    /// `?` rather than a made-up number: the run length is genuinely unknown
    /// unless something has been told to stop at it.
    #[test]
    fn unknown_run_length_reads_as_a_question_mark() {
        assert_eq!(expand_hud("{nit}", &state(1, false, None), 60.0, &Default::default(), 1), "?");
    }

    /// The counter is not advancing while paused, so reporting the frame rate
    /// as an iteration rate would be a lie. `{fps}` still reports frames.
    #[test]
    fn iteration_rate_is_zero_while_paused_but_frame_rate_is_not() {
        let s = state(7, true, None);
        assert_eq!(expand_hud("{its}|{fps}|{paused}", &s, 120.0, &Default::default(), s.iteration), "0|120|PAUSED");
    }

    /// A typo must render, not panic or swallow the text around it.
    #[test]
    fn unknown_and_unbalanced_braces_pass_through() {
        let s = state(1, false, None);
        assert_eq!(expand_hud("{nope} x {it}", &s, 60.0, &Default::default(), s.iteration), "{nope} x 1");
        assert_eq!(expand_hud("a {unclosed", &s, 60.0, &Default::default(), s.iteration), "a {unclosed");
        assert_eq!(expand_hud("no braces", &s, 60.0, &Default::default(), s.iteration), "no braces");
    }

}
