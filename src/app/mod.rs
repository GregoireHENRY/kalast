pub mod axes;
pub mod body;
pub mod config;
pub mod facet_id;
pub mod facet_shadow;
pub mod frame;
pub mod gui;
pub mod hemicube;
pub mod gpu;
pub mod pass;
pub mod simulation;
pub mod uniform;
pub mod window;

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
    /// Lines for the editor's log panel. Here rather than on the editor so
    /// `app.log()` works before the window exists as well as during the run.
    pub log: crate::app::gui::Log,
    /// A run asked for from outside the UI -- `app.run_script()`. Here
    /// rather than on the editor because it can be raised before the window
    /// exists.
    pub run_requested: bool,
    /// The same, but from Restart: rebuild and stop at the start.
    pub restart_requested: bool,
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
            script_runner: None,
            running: true,
            exit_requested: false,
            pending_script: None,
            log: crate::app::gui::Log::new(2000),
            run_requested: false,
            restart_requested: false,
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

    /// The values the live window was built with, to diff the config
    /// against. `None` until there is a window.
    realised: Option<Realised>,

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
fn expand_hud(
    template: &str,
    state: &crate::app::simulation::State,
    rate: Float,
    diag: &crate::app::simulation::Diagnostics,
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

        Self {
            config: Rc::new(RefCell::new(crate::app::config::AppConfig::default())),
            window: None,

            now: std::time::Instant::now(),
            dt: 0.0,

            simulation,
            shared: Rc::new(RefCell::new(Shared::new())),

            controller,
            fps_shown: 0.0,
            fps_window_secs: 0.0,
            fps_window_frames: 0,

            event_loop: None,
            frame_drawn: false,
            realised: None,
            editor: None,
            want_editor: false,
            event_loop_built: false,
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
            win.window.set_fullscreen(want.fullscreen.then(|| {
                winit::window::Fullscreen::Borderless(None)
            }));
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
        let (asked, asked_restart) = {
            let mut s = self.shared.borrow_mut();
            (
                std::mem::take(&mut s.run_requested),
                std::mem::take(&mut s.restart_requested),
            )
        };
        let asked = asked | asked_restart;
        let Some(editor) = self.editor.as_mut() else { return };
        let (run, open, save) = (
            asked | std::mem::take(&mut editor.run_request),
            std::mem::take(&mut editor.open_request),
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
                self.shared.borrow_mut().script_ran = false;
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

    /// Take a script the UI has asked to run, if there is one.
    ///
    /// Call it between frames -- `while app.step(): ...` -- and execute what
    /// comes back. Doing it there rather than inside the frame is what lets a
    /// script drive its own loop.
    pub fn take_script_request(&mut self) -> Option<(String, String, bool)> {
        self.shared.borrow_mut().script_pending.take()
    }

    /// Ask the window to close. The next `step()` returns `false`.
    pub fn close(&mut self) {
        self.shared.borrow_mut().exit_requested = true;
    }

    /// Whether the window is still open.
    pub fn is_running(&self) -> bool {
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

        let size = winit::dpi::PhysicalSize::new(
            self.config.borrow().width,
            self.config.borrow().height,
        );
        let mut attrs = winit::window::Window::default_attributes()
            .with_inner_size(size)
            .with_title(&self.sim_config().borrow().title);

        if self.sim_config().borrow().fullscreen {
            // Borderless on the current monitor: `None` means "wherever the
            // window lands", which is what a user pressing the green button
            // would get.
            attrs = attrs.with_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
        }

        let win = Arc::new(ev.create_window(attrs).unwrap());

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
            if consumed && !matches!(event, winit::event::WindowEvent::RedrawRequested) {
                return;
            }
        }

        match event {
            winit::event::WindowEvent::CloseRequested => self.exit(ev),
            winit::event::WindowEvent::Resized(size) => {
                let win = self.window.as_mut().unwrap();
                win.resize(size.width, size.height, &sim_cfg.borrow());
            }
            winit::event::WindowEvent::RedrawRequested => {
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
                                text: expand_hud(&h.text, &sim.state, self.fps_shown, &sim.diagnostics),
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
                            &win.passes.render.render_texture,
                            scene_size,
                            scene_generation,
                            &mut sim_cfg.borrow_mut(),
                            &mut sim.state,
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

                // Advance only now that both callbacks have run, so they
                // agree on which frame they are in -- a loop deriving an
                // epoch from `state.iteration` would otherwise see two
                // different times within one frame.
                self.simulation.borrow_mut().update();

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
                    (winit::keyboard::KeyCode::Escape, true) => self.exit(ev),
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

            winit::event::WindowEvent::MouseInput { state, button, .. } => match button {
                winit::event::MouseButton::Middle => {
                    self.controller.middle_pressed = state.is_pressed();
                }
                winit::event::MouseButton::Left => {
                    self.controller.left_pressed = state.is_pressed();
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

    #[test]
    fn expands_the_documented_placeholders() {
        let s = state(42, false, Some(500));
        assert_eq!(
            expand_hud("{it}/{nit} ({its} it/s)", &s, 60.4, &Default::default()),
            "42/500 (60 it/s)"
        );
    }

    /// Rates are whole numbers unless asked otherwise: a tenth of a frame per
    /// second is noise, and the digit churns without informing anyone.
    #[test]
    fn rates_are_integers_by_default_and_precision_is_opt_in() {
        let s = state(1, false, None);
        assert_eq!(expand_hud("{fps}", &s, 59.62, &Default::default()), "60");
        assert_eq!(expand_hud("{fps:.1}", &s, 59.62, &Default::default()), "59.6");
        assert_eq!(expand_hud("{fps:.2f}", &s, 59.62, &Default::default()), "59.62");
    }

    /// Milliseconds are the exception -- whole numbers cannot separate 8 from
    /// 8.4 ms, which is the difference between hitting and missing 120 Hz.
    #[test]
    fn milliseconds_keep_one_decimal_by_default() {
        let s = state(1, false, None);
        assert_eq!(expand_hud("{ms}", &s, 120.0, &Default::default()), "8.3");
        assert_eq!(expand_hud("{ms:.0}", &s, 120.0, &Default::default()), "8");
    }

    /// `?` rather than a made-up number: the run length is genuinely unknown
    /// unless something has been told to stop at it.
    #[test]
    fn unknown_run_length_reads_as_a_question_mark() {
        assert_eq!(expand_hud("{nit}", &state(1, false, None), 60.0, &Default::default()), "?");
    }

    /// The counter is not advancing while paused, so reporting the frame rate
    /// as an iteration rate would be a lie. `{fps}` still reports frames.
    #[test]
    fn iteration_rate_is_zero_while_paused_but_frame_rate_is_not() {
        let s = state(7, true, None);
        assert_eq!(expand_hud("{its}|{fps}|{paused}", &s, 120.0, &Default::default()), "0|120|PAUSED");
    }

    /// A typo must render, not panic or swallow the text around it.
    #[test]
    fn unknown_and_unbalanced_braces_pass_through() {
        let s = state(1, false, None);
        assert_eq!(expand_hud("{nope} x {it}", &s, 60.0, &Default::default()), "{nope} x 1");
        assert_eq!(expand_hud("a {unclosed", &s, 60.0, &Default::default()), "a {unclosed");
        assert_eq!(expand_hud("no braces", &s, 60.0, &Default::default()), "no braces");
    }

}
