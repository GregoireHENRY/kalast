//! The page's Python: kalast's Python API for CPython running in the page,
//! the engine it drives being the page's own.
//!
//! The page runs CPython compiled to WebAssembly (Pyodide) beside kalast,
//! with the repository's `kalast` package on it unchanged. In place of the
//! extension module that package wraps on the desktop -- `kalast._rs`, the
//! bindings of `src/py` -- it has `web/python/kalast/_rs`: the same classes,
//! each attribute and method a call into the functions below through
//! JavaScript (`web/python.js`). What they do is what the desktop's bindings
//! do, on the same engine; the scripts run as the UI app's scripts' thread
//! runs them, by `kalast.editor`, and `step()` waits for the page's next
//! frame.
//!
//! An object is a kind and an index (`py_get`, `py_set`, `py_call`): `App`,
//! `AppConfig`, `Config` and its groups, `Simulation`, `State`, `Eye` (0 the
//! camera, 1 the Sun), `Projection`, `Body` and `Mesh` (by body, or past
//! `MESH_OWN` one made on its own), `Facets`, `Hud` (by the page's own list
//! of them).

use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::rc::Rc;

use kalast::app::config::{AppConfig, Config, Hud};
use kalast::app::simulation::Simulation;
use kalast::locked::Locked;
use kalast::{Float, Mat4, Vec3};
use wasm_bindgen::prelude::*;

use crate::config_gen;
use crate::convert::*;

/// What the page's Python reaches: the app's handles, taken before the
/// page's loop takes the app, as the desktop's `py::app::Handles` are.
struct Page {
    simulation: Locked<Simulation>,
    shared: Locked<kalast::app::Shared>,
    config: Locked<AppConfig>,
    /// Every HUD Python has had, by the index it knows it by.
    huds: RefCell<Vec<Locked<Hud>>>,
}

/// The page's answer to kalast's asks (`kalast::app::PagePython`): the
/// object `python.js` installed, with `run`, `console` and `frame`.
struct Hook {
    js: JsValue,
    busy: Cell<bool>,
    /// What is busy is a script, not the python tab.
    script: Cell<bool>,
}

thread_local! {
    static PAGE: RefCell<Option<Rc<Page>>> = const { RefCell::new(None) };
    static HOOK: RefCell<Option<Rc<Hook>>> = const { RefCell::new(None) };
    /// The `step()`s waiting for a frame, to be resolved by the next.
    static WAITING: RefCell<Vec<js_sys::Function>> = const { RefCell::new(Vec::new()) };
    /// Every `Ground` Python has made, by the index it knows it by: in the
    /// engine's memory, which the steps read without a copy.
    static GROUNDS: RefCell<Vec<kalast::tpm::core::Ground>> = const { RefCell::new(Vec::new()) };
    /// The meshes a script made on their own, `Mesh(path=...)`, known past
    /// `MESH_OWN`; `add_mesh` puts a copy in the scene, as on the desktop.
    /// Dropped as the next script starts.
    static MESHES: RefCell<Vec<Locked<kalast::mesh::Mesh>>> = const { RefCell::new(Vec::new()) };
    /// The page's files changed since Python's own had them (`py_changes`):
    /// none tracked before it has had them all once.
    static DIRTY: RefCell<Option<BTreeSet<String>>> = const { RefCell::new(None) };
    /// A change a script made, which Python's files have already.
    static FROM_PYTHON: Cell<bool> = const { Cell::new(false) };
}

/// A `Mesh` past this index is one a script made on its own; below it, a
/// body's, by the body's index.
const MESH_OWN: usize = 1 << 20;

/// A change in the page's files, for Python's own to take before the next
/// run (`py_changes`).
pub fn dirty(path: &str) {
    if FROM_PYTHON.with(|f| f.get()) {
        return;
    }
    DIRTY.with(|d| {
        if let Some(changed) = d.borrow_mut().as_mut() {
            changed.insert(path.to_string());
        }
    });
}

fn with_ground<R>(id: usize, f: impl FnOnce(&mut kalast::tpm::core::Ground) -> Result<R, String>) -> Result<R, String> {
    GROUNDS.with(|g| match g.borrow_mut().get_mut(id) {
        Some(ground) => f(ground),
        None => Err(format!("IndexError: no Ground {id}")),
    })
}

fn add_ground(ground: kalast::tpm::core::Ground) -> JsValue {
    let id = GROUNDS.with(|g| {
        let mut g = g.borrow_mut();
        g.push(ground);
        g.len() - 1
    });
    object("Ground", id)
}

/// Take the app's handles, before the page's loop takes the app.
pub fn install_app(app: &kalast::app::App) {
    let page = Page {
        simulation: app.simulation.clone(),
        shared: app.shared.clone(),
        config: app.config.clone(),
        huds: RefCell::new(Vec::new()),
    };
    PAGE.with(|p| *p.borrow_mut() = Some(Rc::new(page)));
}

fn page() -> Result<Rc<Page>, String> {
    PAGE.with(|p| p.borrow().clone()).ok_or_else(|| "RuntimeError: kalast has not started".to_string())
}

fn call_js(target: &JsValue, name: &str, args: &[JsValue]) {
    let f = js_sys::Reflect::get(target, &JsValue::from_str(name)).ok().and_then(|f| f.dyn_into::<js_sys::Function>().ok());
    if let Some(f) = f {
        let args: js_sys::Array = args.iter().cloned().collect();
        if let Err(e) = f.apply(target, &args) {
            log::warn!("python.js {name}: {e:?}");
        }
    }
}

impl kalast::app::PagePython for Hook {
    fn busy(&self) -> bool {
        self.busy.get()
    }

    fn run(&self, path: &str, source: &str) {
        self.busy.set(true);
        self.script.set(true);
        MESHES.with(|m| m.borrow_mut().clear());
        call_js(&self.js, "run", &[JsValue::from_str(path), JsValue::from_str(source)]);
    }

    fn console(&self) {
        self.busy.set(true);
        call_js(&self.js, "console", &[]);
    }

    fn frame(&self) {
        let waiting = WAITING.with(|w| std::mem::take(&mut *w.borrow_mut()));
        for resolve in waiting {
            let _ = resolve.call0(&JsValue::UNDEFINED);
        }
    }

    fn held(&self) -> bool {
        self.script.get() && WAITING.with(|w| w.borrow().is_empty())
    }
}

/// `python.js` gives kalast its Python: `run(path, source)`, `console()`.
#[wasm_bindgen]
pub fn py_install(js: JsValue) {
    let hook = Rc::new(Hook { js, busy: Cell::new(false), script: Cell::new(false) });
    HOOK.with(|h| *h.borrow_mut() = Some(hook.clone()));
    kalast::app::set_page_python(hook);
}

/// A run or the console served: the next may begin.
#[wasm_bindgen]
pub fn py_done() {
    HOOK.with(|h| {
        if let Some(hook) = h.borrow().as_ref() {
            hook.busy.set(false);
            hook.script.set(false);
        }
    });
}

/// Resolved once the page has drawn its next frame: what `step()` waits on.
#[wasm_bindgen]
pub fn py_next_frame() -> js_sys::Promise {
    js_sys::Promise::new(&mut |resolve, _| WAITING.with(|w| w.borrow_mut().push(resolve)))
}

/// The files of the page, for Python's own file system: every path.
#[wasm_bindgen]
pub fn py_files() -> js_sys::Array {
    fn walk(dir: &std::path::Path, out: &js_sys::Array) {
        for (name, is_dir) in kalast::fs::list(dir).unwrap_or_default() {
            let path = dir.join(&name);
            if is_dir {
                walk(&path, out);
            } else {
                out.push(&JsValue::from_str(&path.to_string_lossy()));
            }
        }
    }
    let out = js_sys::Array::new();
    walk(std::path::Path::new(""), &out);
    out
}

/// What Python's own files have to take again before a run: every file the
/// first time, then those the page's files changed since -- written, or
/// gone, which `py_has_file` tells apart. Folders are made with their files.
#[wasm_bindgen]
pub fn py_changes() -> js_sys::Array {
    let changed = DIRTY.with(|d| {
        let mut d = d.borrow_mut();
        match d.as_mut() {
            Some(changed) => Some(std::mem::take(changed)),
            None => {
                *d = Some(BTreeSet::new());
                None
            }
        }
    });
    match changed {
        None => py_files(),
        Some(changed) => changed
            .into_iter()
            .filter(|p| kalast::fs::list(std::path::Path::new(p)).is_err())
            .map(|p| JsValue::from_str(&p))
            .collect(),
    }
}

/// A file a script wrote or changed, into the page's files -- and from
/// there into the visitor's folder, where the desktop's would be on the
/// disk. Python's own files have it already.
#[wasm_bindgen]
pub fn py_output(path: &str, bytes: &[u8]) -> bool {
    FROM_PYTHON.with(|f| f.set(true));
    let written = py_write_file(path, bytes);
    FROM_PYTHON.with(|f| f.set(false));
    written
}

/// One of the page's files, its bytes.
#[wasm_bindgen]
pub fn py_file(path: &str) -> Option<Vec<u8>> {
    kalast::fs::read(path).ok()
}

/// Whether the page's files have `path`.
#[wasm_bindgen]
pub fn py_has_file(path: &str) -> bool {
    kalast::fs::exists(std::path::Path::new(path))
}

/// A file Python wrote, into the page's files: the files tab shows it, and
/// kalast reads it -- a mesh a script made, then loaded.
#[wasm_bindgen]
pub fn py_write_file(path: &str, bytes: &[u8]) -> bool {
    let path = std::path::Path::new(path);
    if let Some(dir) = path.parent() {
        let _ = kalast::fs::create_dir_all(dir);
    }
    kalast::fs::write(path, bytes).is_ok()
}

fn fail(e: String) -> JsValue {
    js_sys::Error::new(&e).into()
}

#[wasm_bindgen]
pub fn py_get(kind: &str, id: u32, name: &str) -> Result<JsValue, JsValue> {
    get(kind, id as usize, name).map_err(fail)
}

#[wasm_bindgen]
pub fn py_set(kind: &str, id: u32, name: &str, value: JsValue) -> Result<(), JsValue> {
    set(kind, id as usize, name, &value).map_err(fail)
}

#[wasm_bindgen]
pub fn py_call(kind: &str, id: u32, name: &str, args: js_sys::Array) -> Result<JsValue, JsValue> {
    call(kind, id as usize, name, &args).map_err(fail)
}

#[wasm_bindgen]
pub fn py_new(kind: &str, args: js_sys::Array) -> Result<JsValue, JsValue> {
    new(kind, &args).map_err(fail)
}

#[wasm_bindgen]
pub fn py_fn(name: &str, args: js_sys::Array) -> Result<JsValue, JsValue> {
    function(name, &args).map_err(fail)
}

fn no_attribute(kind: &str, name: &str) -> String {
    format!("AttributeError: '{kind}' object has no attribute {name:?} in the browser")
}

/// The simulation on screen: a Rust example's, while one is shown
/// (`Shared::shown`), else the app's own -- as the desktop's
/// `App.simulation` has it.
fn sim(page: &Page) -> Locked<Simulation> {
    page.shared.borrow().shown.clone().unwrap_or_else(|| page.simulation.clone())
}

fn sim_config(page: &Page) -> Locked<Config> {
    sim(page).borrow().config.clone()
}

fn get(kind: &str, id: usize, name: &str) -> Result<JsValue, String> {
    let page = page()?;
    match kind {
        "App" => app_get(&page, name),
        "AppConfig" => match name {
            "__repr__" => Ok(method()),
            _ => config_gen::app_get(&page.config.borrow(), name),
        },
        "Config" => match name {
            "huds" => Ok(huds_value(&page)),
            "__repr__" => Ok(method()),
            group => match config_gen::GROUPS.iter().position(|g| *g == group) {
                Some(i) => Ok(object("ConfigGroup", i)),
                None => Err(no_attribute("Config", name)),
            },
        },
        "ConfigGroup" => config_group_get(&page, id, name),
        "Simulation" => sim_get(&page, name),
        "State" => state_get(&page, name),
        "Eye" => eye_get(&page, id, name),
        "Projection" => projection_get(&page, id, name),
        "Body" => body_get(&page, id, name),
        "Mesh" => mesh_get(&page, id, name),
        "Facets" => match name {
            "__len__" | "__getitem__" => Ok(method()),
            _ => Err(no_attribute("Facets", name)),
        },
        "Hud" => hud_get(&page, id, name),
        "Ground" => ground_get(id, name),
        _ => Err(format!("AttributeError: no {kind} in the browser")),
    }
}

fn set(kind: &str, id: usize, name: &str, v: &JsValue) -> Result<(), String> {
    let page = page()?;
    match kind {
        "App" => app_set(&page, name, v),
        "AppConfig" => config_gen::app_set(&mut page.config.borrow_mut(), name, v),
        "Config" => match name {
            "huds" => set_huds(&page, v),
            _ => Err(no_attribute("Config", name)),
        },
        "ConfigGroup" => config_group_set(&page, id, name, v),
        "Simulation" => sim_set(&page, name, v),
        "State" => state_set(&page, name, v),
        "Eye" => eye_set(&page, id, name, v),
        "Projection" => projection_set(&page, id, name, v),
        "Body" => body_set(&page, id, name, v),
        "Mesh" => mesh_set(&page, id, name, v),
        "Hud" => hud_set(&page, id, name, v),
        "Ground" => ground_set(id, name, v),
        _ => Err(format!("AttributeError: no {kind} in the browser")),
    }
}

fn call(kind: &str, id: usize, name: &str, args: &js_sys::Array) -> Result<JsValue, String> {
    let page = page()?;
    match kind {
        "App" => app_call(&page, name, args),
        "AppConfig" => Ok(JsValue::from_str(&format!("{:?}", page.config.borrow()))),
        "Config" => Ok(JsValue::from_str(&format!("{:?}", sim_config(&page).borrow()))),
        "ConfigGroup" => {
            let group = config_gen::GROUPS.get(id).ok_or("IndexError: no such config group")?;
            Ok(JsValue::from_str(&config_gen::repr(&sim_config(&page).borrow(), group)))
        }
        "Simulation" => sim_call(&page, name, args),
        "State" => state_call(&page, name),
        "Eye" => eye_call(&page, id, name, args),
        "Projection" => projection_call(&page, id, name, args),
        "Mesh" => mesh_call(&page, id, name, args),
        "Facets" => facets_call(&page, id, name, args),
        "Hud" => Ok(JsValue::from_str(&format!("{:?}", hud(&page, id)?.borrow()))),
        "Ground" => ground_call(id, name, args),
        _ => Err(format!("AttributeError: no {kind} in the browser")),
    }
}

// --- the app -----------------------------------------------------------

fn app_get(page: &Page, name: &str) -> Result<JsValue, String> {
    let shared = page.shared.borrow();
    Ok(match name {
        "config" => object("AppConfig", 0),
        "simulation" => object("Simulation", 0),
        // As the desktop's: false once another run is pending, too.
        "running" => JsValue::from_bool(shared.running && !shared.superseded()),
        "script_requested" => JsValue::from_bool(shared.script_pending.is_some()),
        "script_ran" => JsValue::from_bool(shared.script_ran),
        "drawn_iteration" => JsValue::from_f64(shared.drawn_iteration as f64),
        "pointer" => shared.pointer.map_or(JsValue::UNDEFINED, |(x, y)| floats_value(&[x as f64, y as f64])),
        "ui_size" => floats_value(&[shared.ui_size.0 as f64, shared.ui_size.1 as f64]),
        "panels_shown" => list(shared.panels_shown.iter().map(|&b| JsValue::from_bool(b))),
        "start_editor" | "close" | "log" | "set_script" | "run_script" | "open_script" | "restart_script"
        | "take_script_request" | "flush_output" => method(),
        _ => return Err(no_attribute("App", name)),
    })
}

fn app_set(page: &Page, name: &str, v: &JsValue) -> Result<(), String> {
    match name {
        "script_ran" => page.shared.borrow_mut().script_ran = boolean(v, name)?,
        "before_render" | "tick" | "after_render" => {
            let tick = if absent(v) { None } else { Some(page_tick(v.clone(), name)?) };
            let mut shared = page.shared.borrow_mut();
            if name == "after_render" {
                shared.after_render = tick;
            } else {
                shared.before_render = tick;
            }
        }
        _ => return Err(no_attribute("App", name)),
    }
    Ok(())
}

/// A callback of the page's Python, as kalast calls it: the simulation not
/// borrowed, which the callback reaches through this bridge. `f` is
/// `python.js`'s wrapper of the Python function, which shows a traceback in
/// the log's script tab and answers false when it raised.
fn page_tick(f: JsValue, name: &str) -> Result<kalast::app::Tick, String> {
    let f: js_sys::Function = f.dyn_into().map_err(|_| format!("TypeError: {name} must be callable"))?;
    let simulation = object("Simulation", 0);
    Ok(kalast::app::Tick::Page(Box::new(move |dt: Float| {
        match f.call2(&JsValue::UNDEFINED, &simulation, &JsValue::from_f64(dt as f64)) {
            Ok(ok) => ok.as_bool().unwrap_or(true),
            Err(_) => false,
        }
    })))
}

fn app_call(page: &Page, name: &str, args: &js_sys::Array) -> Result<JsValue, String> {
    match name {
        "start_editor" | "flush_output" => {}
        "close" => page.shared.borrow_mut().exit_requested = true,
        "log" => page.shared.borrow_mut().log.push(string(&arg(args, 0), "line")?),
        "set_script" => {
            let (path, source) = (string(&arg(args, 0), "path")?, string(&arg(args, 1), "source")?);
            page.shared.borrow_mut().pending_script = Some((path, source));
        }
        "run_script" => page.shared.borrow_mut().run_requested = true,
        "open_script" => page.shared.borrow_mut().open_requested = true,
        "restart_script" => page.shared.borrow_mut().restart_requested = true,
        "take_script_request" => {
            return Ok(match page.shared.borrow_mut().take_script() {
                Some((path, source, paused)) => {
                    list([JsValue::from_str(&path), JsValue::from_str(&source), JsValue::from_bool(paused)])
                }
                None => JsValue::UNDEFINED,
            });
        }
        _ => return Err(no_attribute("App", name)),
    }
    Ok(JsValue::UNDEFINED)
}

// --- the config ----------------------------------------------------------

fn config_group_get(page: &Page, id: usize, name: &str) -> Result<JsValue, String> {
    let group = *config_gen::GROUPS.get(id).ok_or("IndexError: no such config group")?;
    if name == "__repr__" {
        return Ok(method());
    }
    let config = sim_config(page);
    let c = config.borrow();
    // The fields written by hand on the desktop too (`py::app::config`).
    match (group, name) {
        ("shading", "color_mode") => Ok(JsValue::from_str(c.shading.color_mode.name())),
        ("shading", "srgb_mode") => Ok(JsValue::from_str(c.shading.srgb_mode.name())),
        ("data", "colormap") => {
            let flat: Vec<f32> = c.data.colormap.iter().flatten().copied().collect();
            Ok(array_f32(&flat, &[c.data.colormap.len(), 3], false))
        }
        _ => config_gen::get(&c, group, name),
    }
}

fn config_group_set(page: &Page, id: usize, name: &str, v: &JsValue) -> Result<(), String> {
    use kalast::app::config::{ColorMode, ShadowMethod, SrgbMode};
    let group = *config_gen::GROUPS.get(id).ok_or("IndexError: no such config group")?;
    let config = sim_config(page);
    let mut c = config.borrow_mut();
    match (group, name) {
        ("shading", "color_mode") => {
            let names = "\"shaded\" or \"data\"";
            if let Some(s) = v.as_string() {
                c.shading.color_mode =
                    ColorMode::parse(&s).ok_or_else(|| format!("ValueError: color_mode is one of {names}, got {s:?}"))?;
            } else {
                // The numbers of before 12 October, as the desktop takes them.
                match unsigned(v, "color_mode")? {
                    3 => {
                        c.shading.color_mode = ColorMode::Shaded;
                        c.shadows.method = ShadowMethod::None;
                    }
                    2 => return Err("ValueError: config.shading.color_mode = 2, every body one flat colour, is gone: \"data\" shows each mesh's own colours, unlit".to_string()),
                    code => {
                        let mode = ColorMode::from_code(code as u32)
                            .ok_or_else(|| format!("ValueError: color_mode is one of {names}, got {code}"))?;
                        c.shading.color_mode = mode;
                        if mode == ColorMode::Shaded && c.shadows.method == ShadowMethod::None {
                            c.shadows.method = ShadowMethod::Maps;
                        }
                    }
                }
            }
        }
        ("shading", "srgb_mode") => {
            let names = "\"screen\" or \"linear\"";
            c.shading.srgb_mode = match v.as_string() {
                Some(s) => SrgbMode::parse(&s).ok_or_else(|| format!("ValueError: srgb_mode is one of {names}, got {s:?}"))?,
                None => {
                    let code = unsigned(v, "srgb_mode")?;
                    SrgbMode::from_code(code as u32).ok_or_else(|| format!("ValueError: srgb_mode is one of {names}, got {code}"))?
                }
            };
        }
        ("data", "colormap") => c.data.colormap = colormap(v)?,
        _ => config_gen::set(&mut c, group, name, v)?,
    }
    Ok(())
}

/// A colour table from Python, as the desktop's setter takes it: a
/// built-in's name, a file of colours, or rows of r, g, b (an Nx3 or Nx4
/// array, sent flat with its column count first, `[n_cols, ...]`).
fn colormap(v: &JsValue) -> Result<Vec<[f32; 3]>, String> {
    if let Some(name) = v.as_string() {
        if let Some(table) = kalast::app::config::builtin_colormap(&name) {
            return Ok(table);
        }
        let path = std::path::Path::new(&name);
        if kalast::fs::len(path).is_ok() {
            return kalast::app::config::colormap_from_file(path).map_err(|e| format!("ValueError: {e}"));
        }
        return Err(format!(
            "ValueError: unknown colormap {name:?}, and no such file: matplotlib's names are taken, \
             kalast.app.config.colormap_names() lists them, each reversed with _r after it; otherwise a \
             file of colours, or an Nx3 array"
        ));
    }
    let flat = float_vec(v, "colormap")?;
    let (cols, values) = flat.split_first().ok_or("ValueError: colormap is empty")?;
    let cols = *cols as usize;
    if cols < 3 {
        return Err(format!("ValueError: colormap rows must have at least 3 columns (r, g, b), got {cols}"));
    }
    let rows: Vec<[f32; 3]> = values.chunks_exact(cols).map(|r| [r[0] as f32, r[1] as f32, r[2] as f32]).collect();
    if rows.is_empty() {
        return Err("ValueError: colormap is empty".to_string());
    }
    Ok(rows)
}

// --- HUDs --------------------------------------------------------------------

fn hud(page: &Page, id: usize) -> Result<Locked<Hud>, String> {
    page.huds.borrow().get(id).cloned().ok_or_else(|| format!("IndexError: no HUD {id}"))
}

/// The index Python knows `h` by, given one if it has none yet.
fn hud_id(page: &Page, h: &Locked<Hud>) -> usize {
    let mut huds = page.huds.borrow_mut();
    match huds.iter().position(|k| Locked::ptr_eq(k, h)) {
        Some(i) => i,
        None => {
            huds.push(h.clone());
            huds.len() - 1
        }
    }
}

fn huds_value(page: &Page) -> JsValue {
    let simulation = sim(page);
    let huds: Vec<Locked<Hud>> = simulation.borrow().huds.clone();
    list(huds.iter().map(|h| object("Hud", hud_id(page, h))))
}

fn set_huds(page: &Page, v: &JsValue) -> Result<(), String> {
    let ids = index_vec(v, "huds")?;
    let huds = ids.into_iter().map(|i| hud(page, i)).collect::<Result<Vec<_>, _>>()?;
    sim(page).borrow_mut().huds = huds;
    Ok(())
}

fn hud_get(page: &Page, id: usize, name: &str) -> Result<JsValue, String> {
    let h = hud(page, id)?;
    let h = h.borrow();
    Ok(match name {
        "text" => JsValue::from_str(&h.text),
        "pin" => h.pin.as_deref().map_or(JsValue::UNDEFINED, JsValue::from_str),
        "anchor" => JsValue::from_str(h.anchor.name()),
        "x" => JsValue::from_f64(h.x as f64),
        "y" => JsValue::from_f64(h.y as f64),
        "size" => JsValue::from_f64(h.size as f64),
        "color" => floats_value(&h.color.map(|c| c as f64)),
        "__repr__" => method(),
        _ => return Err(no_attribute("Hud", name)),
    })
}

fn hud_set(page: &Page, id: usize, name: &str, v: &JsValue) -> Result<(), String> {
    let h = hud(page, id)?;
    let mut h = h.borrow_mut();
    match name {
        "text" => h.text = string(v, name)?,
        "pin" => h.pin = if absent(v) { None } else { Some(string(v, name)?) },
        "anchor" => h.anchor = anchor(&string(v, name)?)?,
        "x" => h.x = number(v, name)? as f32,
        "y" => h.y = number(v, name)? as f32,
        "size" => h.size = number(v, name)? as f32,
        "color" => h.color = floats::<4>(v, name)?.map(|c| c as f32),
        _ => return Err(no_attribute("Hud", name)),
    }
    Ok(())
}

fn anchor(s: &str) -> Result<kalast::app::config::HudAnchor, String> {
    kalast::app::config::HudAnchor::parse(s).ok_or_else(|| {
        format!(
            "ValueError: unknown hud anchor {s:?}: expected one of top-left, top-center, top-right, middle-left, \
             middle-center, middle-right, bottom-left, bottom-center, bottom-right"
        )
    })
}

/// A file the page's files do not have, said as Python says it, with where
/// the page's files come from.
fn no_file(path: &str) -> String {
    format!(
        "FileNotFoundError: no file {path:?} in the page: it carries the examples and the meshes they load (res/), \
         what you save, and a folder of yours once opened in the files tab"
    )
}

fn new(kind: &str, args: &js_sys::Array) -> Result<JsValue, String> {
    let page = page()?;
    match kind {
        // `Mesh(path=...)`: a mesh of the script's own, from one of the
        // page's files, not in the scene until `add_mesh`.
        "Mesh" => {
            let path = string(&arg(args, 0), "path")?;
            if kalast::fs::len(std::path::Path::new(&path)).is_err() {
                return Err(no_file(&path));
            }
            let mesh = kalast::mesh::Mesh::load(&path, |v| v);
            let id = MESHES.with(|m| {
                let mut m = m.borrow_mut();
                m.push(Locked::new(mesh));
                m.len() - 1
            });
            Ok(object("Mesh", MESH_OWN + id))
        }
        // `Hud(text, anchor="top-left", x=None, y=None, size=18.0, color=None, align_h=None, align_v=None)`.
        "Hud" => {
            let mut h = Hud::new(&string(&arg(args, 0), "text")?);
            let a = arg(args, 1);
            if !absent(&a) {
                h.anchor = anchor(&string(&a, "anchor")?)?;
            }
            if let Some(x) = arg(args, 2).as_f64() {
                h.x = x as f32;
            }
            if let Some(y) = arg(args, 3).as_f64() {
                h.y = y as f32;
            }
            if let Some(size) = arg(args, 4).as_f64() {
                h.size = size as f32;
            }
            let color = arg(args, 5);
            if !absent(&color) {
                h.color = floats::<4>(&color, "color")?.map(|c| c as f32);
            }
            let align_h = arg(args, 6);
            if !absent(&align_h) {
                let s = string(&align_h, "align_h")?;
                h.align_h = Some(kalast::app::config::HAlign::parse(&s).ok_or_else(|| {
                    format!("ValueError: unknown align_h {s:?}: expected left, center or right")
                })?);
            }
            let align_v = arg(args, 7);
            if !absent(&align_v) {
                let s = string(&align_v, "align_v")?;
                h.align_v = Some(kalast::app::config::VAlign::parse(&s).ok_or_else(|| {
                    format!("ValueError: unknown align_v {s:?}: expected top, center or bottom")
                })?);
            }
            let h = Locked::new(h);
            Ok(object("Hud", hud_id(&page, &h)))
        }
        "App" => Ok(object("App", 0)),
        _ => Err(format!("TypeError: {kind}() cannot be made in the browser yet")),
    }
}

// --- the simulation ----------------------------------------------------------

fn sim_get(page: &Page, name: &str) -> Result<JsValue, String> {
    let simulation = sim(page);
    let s = simulation.borrow();
    Ok(match name {
        "config" => object("Config", 0),
        "state" => object("State", 0),
        "bodies" => list((0..s.bodies.len()).map(|i| object("Body", i))),
        "camera" => object("Eye", 0),
        "sun" => object("Eye", 1),
        "huds" => {
            drop(s);
            huds_value(page)
        }
        "export" => JsValue::from_bool(s.export),
        "rays" => JsValue::from_bool(s.rays),
        "image_size" => floats_value(&[s.image_size.0 as f64, s.image_size.1 as f64]),
        "selected_facets" => {
            list(s.selected_facets.iter().map(|f| floats_value(&[f.body as f64, f.facet as f64])))
        }
        "reset" | "frame_all" | "load_mesh" | "remove_body" | "rebuild_meshes" | "update" | "toggle_export"
        | "export_once" | "request_facet_shadow" | "facet_shadow" | "facet_illumination" | "facet_incidence"
        | "facet_mean_incidence" | "meridian_facets" | "toggle_facet" | "clear_selection" | "project"
        | "project_body" | "project_facet" | "pick_facet" | "visibility" | "__repr__" => method(),
        _ => return Err(no_attribute("Simulation", name)),
    })
}

fn sim_set(page: &Page, name: &str, v: &JsValue) -> Result<(), String> {
    match name {
        "huds" => set_huds(page, v),
        "export" => {
            sim(page).borrow_mut().export = boolean(v, name)?;
            Ok(())
        }
        _ => Err(no_attribute("Simulation", name)),
    }
}

/// A 4x4 matrix from Python, row-major as numpy has it, as kalast's.
fn matrix(v: &JsValue, name: &str) -> Result<Mat4, String> {
    let m = floats::<16>(v, name)?;
    let m: [Float; 16] = m.map(|x| x as Float);
    Ok(Mat4::from_cols_array(&m).transpose())
}

fn vec3(v: &JsValue, name: &str) -> Result<Vec3, String> {
    let p = floats::<3>(v, name)?;
    Ok(Vec3::new(p[0] as Float, p[1] as Float, p[2] as Float))
}

fn vec3_value(v: Vec3) -> JsValue {
    array_f64(&[v.x as f64, v.y as f64, v.z as f64], &[3], false)
}

fn floats_array(v: &[Float]) -> JsValue {
    let v: Vec<f32> = v.iter().map(|&x| x as f32).collect();
    array_f32(&v, &[v.len()], false)
}

fn body_index(s: &Simulation, body: &JsValue) -> Result<usize, String> {
    let i = unsigned(body, "body")? as usize;
    if i >= s.bodies.len() {
        return Err(format!("IndexError: no body {i}: {} loaded", s.bodies.len()));
    }
    Ok(i)
}

fn sim_call(page: &Page, name: &str, args: &js_sys::Array) -> Result<JsValue, String> {
    let simulation = sim(page);
    let none = JsValue::UNDEFINED;
    match name {
        "reset" => simulation.borrow_mut().reset(),
        "frame_all" => simulation.borrow_mut().frame_all(),
        // `load_mesh(path, mat=None, smooth=False, shadow_path=None)`, the
        // file read from the page's files.
        "load_mesh" => {
            let path = string(&arg(args, 0), "path")?;
            if kalast::fs::len(std::path::Path::new(&path)).is_err() {
                return Err(no_file(&path));
            }
            let m = arg(args, 1);
            let mat = if absent(&m) { Mat4::IDENTITY } else { matrix(&m, "mat")? };
            let smooth = arg(args, 2).as_bool().unwrap_or(false);
            let shadow = arg(args, 3).as_string();
            let body = Simulation::read_body(&path, mat, smooth, shadow.as_deref());
            simulation.borrow_mut().bodies.push(body);
        }
        // `add_mesh(mesh, mat=None)`: a copy of `mesh` in the scene, a body
        // of its own, as the desktop's binding adds it.
        "add_mesh" => {
            let id = unsigned(&arg(args, 0), "mesh")? as usize;
            let m = arg(args, 1);
            let mat = if absent(&m) { Mat4::IDENTITY } else { matrix(&m, "mat")? };
            let mesh = mesh(page, id)?.borrow().clone();
            simulation.borrow_mut().add_mesh(mesh, mat);
        }
        "remove_body" => {
            let mut s = simulation.borrow_mut();
            let i = body_index(&s, &arg(args, 0))?;
            s.bodies.remove(i);
            s.meshes_dirty = true;
        }
        "rebuild_meshes" => simulation.borrow_mut().meshes_dirty = true,
        "update" => simulation.borrow_mut().update(),
        "toggle_export" => simulation.borrow_mut().toggle_export(),
        "export_once" => simulation.borrow_mut().export_once(),
        "request_facet_shadow" => {
            let mut s = simulation.borrow_mut();
            let i = unsigned(&arg(args, 0), "body")? as usize;
            s.request_facet_shadow(i);
        }
        "facet_shadow" => {
            let s = simulation.borrow();
            let i = arg(args, 0).as_f64().unwrap_or(0.0) as usize;
            return Ok(s.facet_shadow(i).map_or(JsValue::UNDEFINED, |v| array_f32(v, &[v.len()], false)));
        }
        "facet_illumination" => {
            let s = simulation.borrow();
            let i = unsigned(&arg(args, 0), "body")? as usize;
            return Ok(s.facet_illumination(i).map_or(JsValue::UNDEFINED, |v| array_f32(&v, &[v.len()], false)));
        }
        "facet_incidence" => {
            let s = simulation.borrow();
            let i = body_index(&s, &arg(args, 0))?;
            let v = s.facet_incidence(i).ok_or_else(|| format!("ValueError: body {i} has no mesh"))?;
            return Ok(floats_array(&v));
        }
        "facet_mean_incidence" => {
            let s = simulation.borrow();
            let i = body_index(&s, &arg(args, 0))?;
            let a = arg(args, 1);
            let axis = if absent(&a) { Vec3::Z } else { vec3(&a, "spin_axis")? };
            let v = s.facet_mean_incidence(i, axis).ok_or_else(|| format!("ValueError: body {i} has no mesh"))?;
            return Ok(floats_array(&v));
        }
        "meridian_facets" => {
            let s = simulation.borrow();
            let i = body_index(&s, &arg(args, 0))?;
            let longitude = arg(args, 1).as_f64().unwrap_or(0.0) as Float;
            let step = arg(args, 2).as_f64().unwrap_or(15.0) as Float;
            if !(step > 0.0 && step <= 180.0) {
                return Err(format!("ValueError: a meridian's step is in (0, 180] degrees: not {step}"));
            }
            let (facets, latitudes) =
                s.meridian_facets(i, longitude, step).ok_or_else(|| format!("ValueError: body {i} has no mesh"))?;
            return Ok(list([array_i64(&facets, &[facets.len()]), floats_array(&latitudes)]));
        }
        "toggle_facet" => {
            let (body, facet) = (unsigned(&arg(args, 0), "body")? as usize, unsigned(&arg(args, 1), "facet")? as usize);
            let color = {
                let s = simulation.borrow();
                let c = s.config.borrow().selection.color;
                Vec3::new(c.r as Float, c.g as Float, c.b as Float)
            };
            return Ok(JsValue::from_bool(simulation.borrow_mut().toggle_facet(body, facet, color)));
        }
        "clear_selection" => simulation.borrow_mut().clear_selection(),
        "project" => {
            let p = simulation.borrow().project(vec3(&arg(args, 0), "point")?);
            return Ok(p.map_or(JsValue::UNDEFINED, |p| floats_value(&[p.x as f64, p.y as f64])));
        }
        "project_body" => {
            let s = simulation.borrow();
            let i = body_index(&s, &arg(args, 0))?;
            return Ok(s.project_body(i).map_or(JsValue::UNDEFINED, |p| floats_value(&[p.x as f64, p.y as f64])));
        }
        "project_facet" => {
            let s = simulation.borrow();
            let i = body_index(&s, &arg(args, 0))?;
            let f = unsigned(&arg(args, 1), "facet")? as usize;
            return Ok(s.project_facet(i, f).map_or(JsValue::UNDEFINED, |p| floats_value(&[p.x as f64, p.y as f64])));
        }
        "pick_facet" => {
            let (o, d) = (vec3(&arg(args, 0), "origin")?, vec3(&arg(args, 1), "direction")?);
            return Ok(match simulation.borrow().pick_facet(o, d) {
                Some((b, f, world, local)) => list([
                    JsValue::from_f64(b as f64),
                    JsValue::from_f64(f as f64),
                    floats_value(&world.to_array().map(|x| x as f64)),
                    floats_value(&local.to_array().map(|x| x as f64)),
                ]),
                None => JsValue::UNDEFINED,
            });
        }
        "visibility" => {
            let s = simulation.borrow();
            let d = &s.diagnostics;
            let o = js_sys::Object::new();
            for (k, v) in [
                ("bodies", d.n_bodies),
                ("visible", d.n_visible),
                ("clipped_near", d.out_near),
                ("clipped_far", d.out_far),
                ("outside_sides", d.out_side),
            ] {
                let _ = js_sys::Reflect::set(&o, &JsValue::from_str(k), &JsValue::from_f64(v as f64));
            }
            return Ok(o.into());
        }
        "__repr__" => {
            let s = simulation.borrow();
            let facets: usize =
                s.bodies.iter().filter_map(|b| b.mesh.as_ref().map(|m| m.borrow().facets.len())).sum();
            return Ok(JsValue::from_str(&format!(
                "Simulation({} bodies, {} facets, iteration {}{})",
                s.bodies.len(),
                facets,
                s.state.iteration,
                if s.state.is_paused { ", paused" } else { "" },
            )));
        }
        _ => return Err(no_attribute("Simulation", name)),
    }
    Ok(none)
}

// --- the state ---------------------------------------------------------------

fn state_get(page: &Page, name: &str) -> Result<JsValue, String> {
    let simulation = sim(page);
    let s = &simulation.borrow().state;
    Ok(match name {
        "iteration" => JsValue::from_f64(s.iteration as f64),
        "is_paused" => JsValue::from_bool(s.is_paused),
        "pause_after_iteration" => s.pause_after_iteration.map_or(JsValue::UNDEFINED, |n| JsValue::from_f64(n as f64)),
        "rate_limited" => JsValue::from_bool(s.rate_limited),
        "rate_limit" => JsValue::from_f64(s.rate_limit as f64),
        "toggle_pause" | "__repr__" => method(),
        _ => return Err(no_attribute("State", name)),
    })
}

fn state_set(page: &Page, name: &str, v: &JsValue) -> Result<(), String> {
    let simulation = sim(page);
    let s = &mut simulation.borrow_mut().state;
    match name {
        "iteration" => s.iteration = unsigned(v, name)? as usize,
        "is_paused" => s.is_paused = boolean(v, name)?,
        "pause_after_iteration" => {
            s.set_pause_after_iteration(if absent(v) { None } else { Some(unsigned(v, name)? as usize) })
        }
        "rate_limited" => s.rate_limited = boolean(v, name)?,
        "rate_limit" => s.rate_limit = number(v, name)? as Float,
        _ => return Err(no_attribute("State", name)),
    }
    Ok(())
}

fn state_call(page: &Page, name: &str) -> Result<JsValue, String> {
    let simulation = sim(page);
    match name {
        "toggle_pause" => Ok(JsValue::from_bool(simulation.borrow_mut().state.toggle_pause())),
        "__repr__" => Ok(JsValue::from_str(&format!("{:?}", simulation.borrow().state))),
        _ => Err(no_attribute("State", name)),
    }
}

// --- the camera and the Sun --------------------------------------------------

fn with_eye<R>(page: &Page, id: usize, f: impl FnOnce(&mut kalast::app::frame::Eye) -> R) -> R {
    let simulation = sim(page);
    let mut s = simulation.borrow_mut();
    f(if id == 0 { &mut s.camera } else { &mut s.sun })
}

fn eye_get(page: &Page, id: usize, name: &str) -> Result<JsValue, String> {
    with_eye(page, id, |e| {
        Ok(match name {
            "pos" => vec3_value(e.pos),
            "dir" => vec3_value(e.dir),
            "up" => vec3_value(e.up),
            "anchor" => vec3_value(e.anchor),
            "up_world" => vec3_value(e.up_world),
            "anchor_body" => e.anchor_body.map_or(JsValue::UNDEFINED, |b| JsValue::from_f64(b as f64)),
            "projection" => object("Projection", id),
            "look_anchor" | "set_target" | "fix_up" | "view_along" | "target" | "right" | "distance_anchor"
            | "is_control_wasd" | "is_control_arcball" | "is_control_none" | "set_control_wasd"
            | "set_control_arcball" | "set_control_none" | "control_toggle" | "__repr__" => method(),
            _ => return Err(no_attribute("Eye", name)),
        })
    })
}

/// A direction, a unit vector, as the desktop's setter refuses a zero one.
fn unit(v: &JsValue, name: &str) -> Result<Vec3, String> {
    let v = vec3(v, name)?;
    v.try_normalize().ok_or_else(|| format!("ValueError: {name} must be a direction, a non-zero vector: not {v}"))
}

fn eye_set(page: &Page, id: usize, name: &str, v: &JsValue) -> Result<(), String> {
    let value = match name {
        "pos" | "anchor" => vec3(v, name)?,
        "dir" | "up" | "up_world" => unit(v, name)?,
        "anchor_body" => {
            let b = if absent(v) { None } else { Some(unsigned(v, name)? as usize) };
            with_eye(page, id, |e| e.anchor_body = b);
            return Ok(());
        }
        _ => return Err(no_attribute("Eye", name)),
    };
    with_eye(page, id, |e| match name {
        "pos" => e.pos = value,
        "anchor" => e.anchor = value,
        "dir" => e.dir = value,
        "up" => e.up = value,
        _ => e.up_world = value,
    });
    Ok(())
}

fn eye_call(page: &Page, id: usize, name: &str, args: &js_sys::Array) -> Result<JsValue, String> {
    use kalast::app::frame::Control;
    if name == "view_along" {
        let axis = string(&arg(args, 0), "axis")?;
        let parsed = kalast::app::frame::Axis::parse(&axis)
            .ok_or_else(|| format!("ValueError: unknown axis {axis:?}: expected x, y, z, or a plane like xy, yz, zx"))?;
        let orthographic = arg(args, 1).as_bool().unwrap_or(true);
        let positive = arg(args, 2).as_bool().unwrap_or(true);
        let sphere = sim(page).borrow().scene_sphere();
        if let Some((centre, radius)) = sphere {
            with_eye(page, id, |e| e.view_along_sphere(parsed, positive, centre, radius, orthographic));
        }
        return Ok(JsValue::UNDEFINED);
    }
    let target = if name == "set_target" { Some(vec3(&arg(args, 0), "target")?) } else { None };
    with_eye(page, id, |e| {
        Ok(match name {
            "look_anchor" => {
                e.look_anchor();
                JsValue::UNDEFINED
            }
            "set_target" => {
                e.set_target(target.unwrap());
                JsValue::UNDEFINED
            }
            "fix_up" => {
                e.fix_up();
                JsValue::UNDEFINED
            }
            "target" => vec3_value(e.target()),
            "right" => vec3_value(e.right()),
            "distance_anchor" => JsValue::from_f64(e.distance_anchor() as f64),
            "is_control_wasd" => JsValue::from_bool(e.control == Control::WASD),
            "is_control_arcball" => JsValue::from_bool(e.control == Control::Arcball),
            "is_control_none" => JsValue::from_bool(e.control == Control::None),
            "set_control_wasd" => {
                e.control = Control::WASD;
                JsValue::UNDEFINED
            }
            "set_control_arcball" => {
                e.control = Control::Arcball;
                JsValue::UNDEFINED
            }
            "set_control_none" => {
                e.control = Control::None;
                JsValue::UNDEFINED
            }
            "control_toggle" => {
                e.control.toggle();
                JsValue::UNDEFINED
            }
            "__repr__" => JsValue::from_str(&format!("{e:?}")),
            _ => return Err(no_attribute("Eye", name)),
        })
    })
}

fn projection_get(page: &Page, id: usize, name: &str) -> Result<JsValue, String> {
    with_eye(page, id, |e| {
        let p = &e.projection;
        let opt = |v: Option<Float>| v.map_or(JsValue::UNDEFINED, |x| JsValue::from_f64(x as f64));
        Ok(match name {
            "fovy" => JsValue::from_f64(p.fovy as f64),
            "near" => opt(p.near),
            "far" => opt(p.far),
            "side" => opt(p.side),
            "resolved_near" => JsValue::from_f64(p.resolved().near as f64),
            "resolved_far" => JsValue::from_f64(p.resolved().far as f64),
            "resolved_side" => JsValue::from_f64(p.resolved().side as f64),
            "is_orthographic" | "is_perspective" | "set_orthographic" | "set_perspective" | "__repr__" => method(),
            _ => return Err(no_attribute("Projection", name)),
        })
    })
}

fn projection_set(page: &Page, id: usize, name: &str, v: &JsValue) -> Result<(), String> {
    let opt = |v: &JsValue| -> Result<Option<Float>, String> {
        Ok(if absent(v) { None } else { Some(number(v, name)? as Float) })
    };
    let value = opt(v)?;
    with_eye(page, id, |e| {
        let p = &mut e.projection;
        match name {
            "fovy" => p.fovy = value.ok_or("TypeError: fovy must be a number")?,
            "near" => p.near = value,
            "far" => p.far = value,
            "side" => p.side = value,
            _ => return Err(no_attribute("Projection", name)),
        }
        Ok(())
    })
}

fn projection_call(page: &Page, id: usize, name: &str, _args: &js_sys::Array) -> Result<JsValue, String> {
    use kalast::app::frame::ProjectionMode;
    with_eye(page, id, |e| {
        let p = &mut e.projection;
        Ok(match name {
            "is_orthographic" => JsValue::from_bool(p.mode == ProjectionMode::Orthographic),
            "is_perspective" => JsValue::from_bool(p.mode == ProjectionMode::Perspective),
            "set_orthographic" => {
                p.mode = ProjectionMode::Orthographic;
                JsValue::UNDEFINED
            }
            "set_perspective" => {
                p.mode = ProjectionMode::Perspective;
                JsValue::UNDEFINED
            }
            "__repr__" => JsValue::from_str(&format!("{p:?}")),
            _ => return Err(no_attribute("Projection", name)),
        })
    })
}

// --- bodies and their meshes ------------------------------------------------

fn body_get(page: &Page, id: usize, name: &str) -> Result<JsValue, String> {
    let simulation = sim(page);
    let s = simulation.borrow();
    let body = s.bodies.get(id).ok_or_else(|| format!("IndexError: no body {id}: {} loaded", s.bodies.len()))?;
    Ok(match name {
        // Row-major, as numpy has it, and written back when changed in place.
        "mat" => {
            let m: Vec<f64> = body.mat.transpose().to_cols_array().iter().map(|&x| x as f64).collect();
            array_f64(&m, &[4, 4], true)
        }
        "mesh" => {
            if body.mesh.is_none() {
                return Err(format!("AttributeError: body {id} has no mesh"));
            }
            object("Mesh", id)
        }
        "horizon_map" => JsValue::from_bool(body.horizon_map),
        _ => return Err(no_attribute("Body", name)),
    })
}

fn body_set(page: &Page, id: usize, name: &str, v: &JsValue) -> Result<(), String> {
    let simulation = sim(page);
    let mut s = simulation.borrow_mut();
    let n = s.bodies.len();
    let body = s.bodies.get_mut(id).ok_or_else(|| format!("IndexError: no body {id}: {n} loaded"))?;
    match name {
        "mat" => body.mat = matrix(v, "mat")?,
        "horizon_map" => body.horizon_map = boolean(v, name)?,
        _ => return Err(no_attribute("Body", name)),
    }
    Ok(())
}

fn mesh(page: &Page, id: usize) -> Result<Locked<kalast::mesh::Mesh>, String> {
    if id >= MESH_OWN {
        return MESHES
            .with(|m| m.borrow().get(id - MESH_OWN).cloned())
            .ok_or_else(|| "RuntimeError: a mesh of an earlier run".to_string());
    }
    let simulation = sim(page);
    let s = simulation.borrow();
    s.bodies
        .get(id)
        .and_then(|b| b.mesh.clone())
        .ok_or_else(|| format!("AttributeError: body {id} has no mesh"))
}

fn mesh_get(page: &Page, id: usize, name: &str) -> Result<JsValue, String> {
    let m = mesh(page, id)?;
    let m = m.borrow();
    let vec3s = |v: &[Vec3]| {
        let flat: Vec<f32> = v.iter().flat_map(|p| [p.x as f32, p.y as f32, p.z as f32]).collect();
        flat
    };
    Ok(match name {
        "facets" => object("Facets", id),
        "positions" => array_f32(&vec3s(&m.positions), &[m.positions.len(), 3], true),
        "normals" => array_f32(&vec3s(&m.normals), &[m.normals.len(), 3], false),
        "colors" => {
            let colors: Vec<Vec3> = m.attrs.iter().map(|a| a.color).collect();
            array_f32(&vec3s(&colors), &[colors.len(), 3], true)
        }
        "values" => floats_array(&m.values),
        // Each vertex's: its colour as given (1), or shaded as the scene's.
        "color_modes" => {
            let modes: Vec<u32> = m.attrs.iter().map(|a| a.color_mode).collect();
            array(js_sys::Uint32Array::from(modes.as_slice()).into(), &[modes.len()], true)
        }
        "indices" => array_u32(&m.indices, &[m.indices.len()]),
        "material_id" => m.material_id.map_or(JsValue::UNDEFINED, |i| JsValue::from_f64(i as f64)),
        "flatten" | "smoothen" | "recompute_facets" | "update_gpu_colors" | "__repr__" => method(),
        _ => return Err(no_attribute("Mesh", name)),
    })
}

fn mesh_set(page: &Page, id: usize, name: &str, v: &JsValue) -> Result<(), String> {
    let m = mesh(page, id)?;
    let mut m = m.borrow_mut();
    match name {
        "values" => {
            let values = float_vec(v, "values")?;
            if values.len() != m.facets.len() {
                return Err(format!(
                    "ValueError: values has {} entries but the mesh has {} facets",
                    values.len(),
                    m.facets.len()
                ));
            }
            m.values = values.into_iter().map(|x| x as Float).collect();
            // Values ride the colour buffer, so they go up on the same flag.
            m.colors_dirty = true;
        }
        // A view's write-back (`_Linked`): the colours as the array has them.
        "colors" => {
            let flat = float_vec(v, "colors")?;
            if flat.len() != m.attrs.len() * 3 {
                return Err(format!("ValueError: colors takes {} rows of 3", m.attrs.len()));
            }
            for (a, c) in m.attrs.iter_mut().zip(flat.chunks_exact(3)) {
                a.color = Vec3::new(c[0] as Float, c[1] as Float, c[2] as Float);
            }
            m.colors_dirty = true;
        }
        "positions" => {
            let flat = float_vec(v, "positions")?;
            if flat.len() != m.positions.len() * 3 {
                return Err(format!("ValueError: positions takes {} rows of 3", m.positions.len()));
            }
            for (p, c) in m.positions.iter_mut().zip(flat.chunks_exact(3)) {
                *p = Vec3::new(c[0] as Float, c[1] as Float, c[2] as Float);
            }
        }
        "color_modes" => {
            let modes = index_vec(v, "color_modes")?;
            if modes.len() != m.attrs.len() {
                return Err(format!("ValueError: color_modes takes {} entries", m.attrs.len()));
            }
            for (a, mode) in m.attrs.iter_mut().zip(modes) {
                a.color_mode = mode as u32;
            }
            m.colors_dirty = true;
        }
        "material_id" => m.material_id = if absent(v) { None } else { Some(unsigned(v, name)? as usize) },
        _ => return Err(no_attribute("Mesh", name)),
    }
    Ok(())
}

fn mesh_call(page: &Page, id: usize, name: &str, _args: &js_sys::Array) -> Result<JsValue, String> {
    let m = mesh(page, id)?;
    match name {
        "flatten" => m.borrow_mut().flatten(),
        "smoothen" => {
            m.borrow_mut().smoothen();
        }
        "recompute_facets" => m.borrow_mut().recompute_facets(),
        "update_gpu_colors" => m.borrow_mut().colors_dirty = true,
        "__repr__" => return Ok(JsValue::from_str(&format!("Mesh({} facets)", m.borrow().facets.len()))),
        _ => return Err(no_attribute("Mesh", name)),
    }
    Ok(JsValue::UNDEFINED)
}

/// `mesh.facets`: its length, and a facet's centre, normal and area.
fn facets_call(page: &Page, id: usize, name: &str, args: &js_sys::Array) -> Result<JsValue, String> {
    let m = mesh(page, id)?;
    let m = m.borrow();
    match name {
        "__len__" => Ok(JsValue::from_f64(m.facets.len() as f64)),
        "__getitem__" => {
            let i = integer(&arg(args, 0), "index")?;
            let n = m.facets.len() as i64;
            let i = if i < 0 { i + n } else { i };
            let f = m.facets.get(i as usize).filter(|_| i >= 0).ok_or("IndexError: facet index out of range")?;
            Ok(list([vec3_value(f.pos), vec3_value(f.normal), JsValue::from_f64(f.area as f64)]))
        }
        _ => Err(no_attribute("Facets", name)),
    }
}

// --- functions ---------------------------------------------------------------

/// The module-level functions: `kalast._rs.<module>.<name>`.
fn function(name: &str, args: &js_sys::Array) -> Result<JsValue, String> {
    use kalast::app::gui;
    use kalast::tpm::{core, properties};
    let n = |i: u32, name: &str| -> Result<Float, String> { Ok(number(&arg(args, i), name)? as Float) };
    Ok(match name {
        // A line of kalast's own, for the kalast tab: Python starting.
        "log_kalast" => {
            page()?.shared.borrow_mut().kalast_log.push(string(&arg(args, 0), "line")?);
            JsValue::UNDEFINED
        }
        // The python tab and the script tab, as `kalast.editor` writes them.
        "script_write" => {
            gui::script_write(&string(&arg(args, 0), "text")?);
            JsValue::UNDEFINED
        }
        "console_take" => gui::console_take().map_or(JsValue::UNDEFINED, |l| JsValue::from_str(&l)),
        "console_write" => {
            gui::console_write(&string(&arg(args, 0), "text")?);
            JsValue::UNDEFINED
        }
        "console_set_more" => {
            gui::console_set_more(boolean(&arg(args, 0), "more")?);
            JsValue::UNDEFINED
        }
        "console_take_completion" => gui::console_take_completion().map_or(JsValue::UNDEFINED, |l| JsValue::from_str(&l)),
        "console_take_interrupt" => JsValue::from_bool(gui::console_take_interrupt()),
        "console_offer" => {
            let matches = js_sys::Array::from(&arg(args, 2)).iter().filter_map(|m| m.as_string()).collect();
            gui::console_offer(string(&arg(args, 0), "line")?, string(&arg(args, 1), "head")?, matches);
            JsValue::UNDEFINED
        }
        "console_greet" => {
            gui::console_greet(&string(&arg(args, 0), "version")?, &string(&arg(args, 1), "platform")?);
            JsValue::UNDEFINED
        }
        "console_submit" => {
            gui::console_submit(string(&arg(args, 0), "line")?);
            JsValue::UNDEFINED
        }
        // `kalast.app.config`.
        "colormap" => {
            let name = string(&arg(args, 0), "name")?;
            let table = kalast::app::config::builtin_colormap(&name).ok_or_else(|| {
                format!(
                    "ValueError: unknown colormap {name:?}: kalast.app.config.colormap_names() lists them, each \
                     reversed with _r after it"
                )
            })?;
            let table = kalast::app::config::resample_colormap(&table, kalast::app::uniform::COLORMAP_SIZE);
            let flat: Vec<f32> = table.iter().flatten().copied().collect();
            array_f32(&flat, &[table.len(), 3], false)
        }
        "colormap_names" => list(kalast::app::config::colormap_names().into_iter().map(JsValue::from_str)),
        // `kalast.entity`'s bodies, cameras and spacecraft, by name: each
        // one's fields.
        "entities" => {
            use kalast::entity as e;
            let o = js_sys::Object::new();
            let put = |target: &js_sys::Object, k: &str, v: JsValue| {
                let _ = js_sys::Reflect::set(target, &JsValue::from_str(k), &v);
            };
            let entity = |kind: &str, x: &e::Entity| {
                let d = js_sys::Object::new();
                put(&d, "kind", JsValue::from_str(kind));
                put(&d, "id", JsValue::from_f64(x.id as f64));
                put(&d, "name", JsValue::from_str(&x.name));
                put(&d, "frame", JsValue::from_str(&x.frame));
                put(&d, "label", JsValue::from_str(&x.label));
                d
            };
            for (k, b) in [
                ("EARTH", e::EARTH.clone()),
                ("MOON", e::MOON.clone()),
                ("MARS", e::MARS.clone()),
                ("PHOBOS", e::PHOBOS.clone()),
                ("DEIMOS", e::DEIMOS.clone()),
                ("DIDYMOS", e::DIDYMOS.clone()),
                ("DIMORPHOS", e::DIMORPHOS.clone()),
                ("DIMORPHOS_PRE", e::DIMORPHOS_PRE.clone()),
            ] {
                let d = entity("Body", &b.entity);
                put(&d, "radii", floats_value(&b.radii.to_array().map(|x| x as f64)));
                put(&d, "orbit_period", JsValue::from_f64(b.orbit_period as f64));
                put(&d, "spin_period", JsValue::from_f64(b.spin_period as f64));
                put(&o, k, d.into());
            }
            for (k, c) in [("TIRI", e::TIRI.clone()), ("AFC", e::AFC.clone())] {
                let d = entity("Camera", &c.entity);
                put(&d, "px", list([JsValue::from_f64(c.px.x as f64), JsValue::from_f64(c.px.y as f64)]));
                put(&d, "fovy", JsValue::from_f64(c.fovy as f64));
                put(&d, "filters", list(c.filters.iter().map(|f| JsValue::from_str(f))));
                put(&o, k, d.into());
            }
            for (k, s) in [("HERA", e::HERA.clone()), ("HALCA", e::HALCA.clone()), ("MEX", e::MEX.clone()), ("TGO", e::TGO.clone())] {
                let d = entity("Spacecraft", &s.entity);
                put(&d, "id_cameras", list(s.id_cameras.iter().map(|c| JsValue::from_str(c))));
                put(&o, k, d.into());
            }
            o.into()
        }
        // `kalast.util`'s constants, as the desktop's module has them.
        "util_constants" => {
            use kalast::util::*;
            let o = js_sys::Object::new();
            let set = |k: &str, v: JsValue| {
                let _ = js_sys::Reflect::set(&o, &JsValue::from_str(k), &v);
            };
            for (k, v) in [
                ("EPSILON", EPSILON),
                ("HOUR", HOUR),
                ("DAY", DAY),
                ("PI", PI),
                ("DPR", DPR),
                ("RPD", RPD),
                ("AU", AU),
                ("AU_KM", AU_KM),
                ("SOLAR_CONSTANT", SOLAR_CONSTANT),
                ("STEFAN_BOLTZMANN", STEFAN_BOLTZMANN),
                ("PLANK_CONSTANT", PLANK_CONSTANT),
                ("SPEED_LIGHT", SPEED_LIGHT),
                ("BOLTZMANN_CONSTANT", BOLTZMANN_CONSTANT),
                ("TWO_C", TWO_C),
                ("HC", HC),
                ("HC2", HC2),
                ("HC_PER_K", HC_PER_K),
                ("TWO_HC2", TWO_HC2),
                ("TEMP_SUN", TEMP_SUN),
                ("RADIUS_SUN", RADIUS_SUN),
                ("JANSKY", JANSKY),
                ("BAND_V0", BAND_V0),
                ("GRAVITATIONAL_CONSTANT", GRAVITATIONAL_CONSTANT),
                ("MASS_SUN", MASS_SUN),
                ("NEWTON_METHOD_THRESHOLD", NEWTON_METHOD_THRESHOLD),
                ("SFLUX_545", SFLUX_545),
            ] {
                set(k, JsValue::from_f64(v as f64));
            }
            set("NEWTON_METHOD_MAX_ITERATION", JsValue::from_f64(NEWTON_METHOD_MAX_ITERATION as f64));
            for (k, v) in [("SPICE_PICTUR_1", SPICE_PICTUR_1), ("SPICE_PICTUR_2", SPICE_PICTUR_2), ("SPICE_PICTUR_3", SPICE_PICTUR_3)] {
                set(k, JsValue::from_str(v));
            }
            o.into()
        }
        // `kalast.tpm.properties`: a body's properties go as their seven
        // numbers, in `Properties`' order.
        "conductivity" => JsValue::from_f64(properties::conductivity(n(0, "ti")?, n(1, "p")?, n(2, "c")?) as f64),
        "diffusivity" => JsValue::from_f64(properties::diffusivity(n(0, "k")?, n(1, "p")?, n(2, "c")?) as f64),
        "thermal_inertia" => JsValue::from_f64(properties::thermal_inertia(n(0, "k")?, n(1, "p")?, n(2, "c")?) as f64),
        "skin_depth_1" => JsValue::from_f64(properties::skin_depth_1(n(0, "d")?, n(1, "p")?) as f64),
        "skin_depth_2pi" => JsValue::from_f64(properties::skin_depth_2pi(n(0, "d")?, n(1, "p")?) as f64),
        "compute_conductivity_diffusivity" => {
            let mut p = props(&arg(args, 0))?;
            p.compute_conductivity_diffusivity();
            props_value(&p)
        }
        "preset" => props_value(&match string(&arg(args, 0), "name")?.as_str() {
            "DIDYMOS" => properties::DIDYMOS,
            "DIMORPHOS" => properties::DIMORPHOS,
            "MOON" => properties::MOON,
            "PHOBOS" => properties::PHOBOS,
            "DEIMOS" => properties::DEIMOS,
            other => return Err(format!("AttributeError: no preset {other}")),
        }),
        // `kalast.tpm.core.Ground(prop, layers, facets)` and `.graded(prop,
        // facets, dz, depth, ratio)`.
        "ground_new" => {
            let (layers, facets) = (unsigned(&arg(args, 1), "layers")? as usize, unsigned(&arg(args, 2), "facets")? as usize);
            let g = kalast::tpm::core::Ground::new(&props(&arg(args, 0))?, layers, facets)
                .map_err(|e| format!("ValueError: {e}"))?;
            add_ground(g)
        }
        "ground_graded" => {
            let facets = unsigned(&arg(args, 1), "facets")? as usize;
            let (g, _) = kalast::tpm::core::Ground::graded(&props(&arg(args, 0))?, facets, n(2, "dz")?, n(3, "depth")?, n(4, "ratio")?)
                .map_err(|e| format!("ValueError: {e}"))?;
            add_ground(g)
        }
        // `kalast.astro.Orbit(a, e)`: its period, and where it is.
        "orbit_period" => JsValue::from_f64(kalast::astro::Orbit::new(n(0, "a")?, n(1, "e")?).period() as f64),
        "orbit_position" => {
            let p = kalast::astro::Orbit::new(n(0, "a")?, n(1, "e")?).position(n(2, "t")?);
            array_f64(&[p.x as f64, p.y as f64, p.z as f64], &[3], false)
        }
        // `kalast.tpm.core`, the numbers; the arrays it steps in place are
        // `py_tpm_*`'s.
        "stability" => JsValue::from_f64(core::stability(n(0, "d")?, n(1, "dt")?, n(2, "dx2")?) as f64),
        "stability_maxdt" => JsValue::from_f64(core::stability_maxdt(n(0, "d")?, n(1, "dx2")?, n(2, "s")?) as f64),
        "conduction" => JsValue::from_f64(core::conduction(n(0, "t")?, n(1, "f")?, n(2, "k")?, n(3, "dx")?) as f64),
        "radiation_sun" => JsValue::from_f64(core::radiation_sun(n(0, "dau")?, n(1, "cosi")?, n(2, "a")?) as f64),
        "radiation_emitted" => JsValue::from_f64(core::radiation_emitted(n(0, "viewf")?, n(1, "t")?, n(2, "e")?) as f64),
        "radiation_sun_reflected" => JsValue::from_f64(
            core::radiation_sun_reflected(n(0, "viewf")?, n(1, "a")?, n(2, "cosi")?, n(3, "dau")?) as f64,
        ),
        "radiation_sun_reflected_reuse" => {
            JsValue::from_f64(core::radiation_sun_reflected_reuse(n(0, "viewf")?, n(1, "f")?, n(2, "a")?) as f64)
        }
        "newton_method_fn" => JsValue::from_f64(core::newton_method_fn(
            n(0, "t")?,
            n(1, "f")?,
            n(2, "set3")?,
            n(3, "k")?,
            n(4, "subt1")?,
            n(5, "subt2")?,
            n(6, "twodx")?,
        ) as f64),
        "newton_method_dfn" => JsValue::from_f64(core::newton_method_dfn(n(0, "set3")?, n(1, "k")?, n(2, "twodx")?) as f64),
        "newton_method" => JsValue::from_f64(
            core::newton_method(n(0, "t")?, n(1, "f")?, n(2, "se")?, n(3, "k")?, n(4, "subt1")?, n(5, "subt2")?, n(6, "twodx")?)
                .map_err(|e| format!("ValueError: {e}"))? as f64,
        ),
        "conduction_1d" | "conduction_1d_nonuniform" => {
            let column = |i: u32, name: &str| -> Result<kalast::ndarray::Array1<Float>, String> {
                Ok(float_vec(&arg(args, i), name)?.into_iter().map(|x| x as Float).collect())
            };
            let (t, d) = (column(0, "t")?, column(1, "d")?);
            let out = if name == "conduction_1d" {
                core::conduction_1d(t.view(), d.view(), column(2, "dtpdx2")?.view())
            } else {
                core::conduction_1d_nonuniform(t.view(), d.view(), column(2, "coef_lo")?.view(), column(3, "coef_hi")?.view())
            };
            floats_array(&out.to_vec())
        }
        // Each a flat array, broadcast by `_page.py`; the answers in its order.
        "effective_temperature" => {
            let dau = n(0, "dau")?;
            let (r, a, e) = (float_vec(&arg(args, 1), "r")?, float_vec(&arg(args, 2), "a")?, float_vec(&arg(args, 3), "e")?);
            let t: Vec<f32> = r
                .iter()
                .zip(&a)
                .zip(&e)
                .map(|((&r, &a), &e)| core::effective_temperature(dau, r as Float, a as Float, e as Float) as f32)
                .collect();
            array_f32(&t, &[t.len()], false)
        }
        "mean_incidence" => {
            let (lat, dec) = (float_vec(&arg(args, 0), "lat")?, float_vec(&arg(args, 1), "dec")?);
            let r: Vec<f32> =
                lat.iter().zip(&dec).map(|(&l, &d)| core::mean_incidence(l as Float, d as Float) as f32).collect();
            array_f32(&r, &[r.len()], false)
        }
        _ => return Err(format!("NotImplementedError: {name} is not in the browser yet: kalast's desktop app has it")),
    })
}

/// A body's `Properties` from its seven numbers.
fn props(v: &JsValue) -> Result<kalast::tpm::properties::Properties, String> {
    let p = floats::<7>(v, "prop")?.map(|x| x as Float);
    Ok(kalast::tpm::properties::Properties::new(p[0], p[1], p[2], p[3], p[4], p[5], p[6]))
}

fn props_value(p: &kalast::tpm::properties::Properties) -> JsValue {
    floats_value(&[
        p.albedo as f64,
        p.emissivity as f64,
        p.density as f64,
        p.heat_capacity as f64,
        p.thermal_inertia as f64,
        p.conductivity as f64,
        p.diffusivity as f64,
    ])
}

/// The temperatures `columns` made, `(layers, facets)` as numpy stores
/// them, for a step to change in place.
fn columns(t: &mut [f32], layers: usize) -> Result<kalast::ndarray::ArrayViewMut2<'_, f32>, JsValue> {
    if layers == 0 || t.len() % layers != 0 {
        return Err(fail(format!("ValueError: t has {} values, not {layers} layers of facets", t.len())));
    }
    let facets = t.len() / layers;
    kalast::ndarray::ArrayViewMut2::from_shape((layers, facets), t).map_err(|e| fail(format!("ValueError: {e}")))
}

/// `kalast.tpm.core.solar_bc(t, dau, cosi, prop, dz)`, `t` stepped in place:
/// `python.js` hands numpy's own memory, so the script's array is the one
/// changed, as on the desktop.
#[wasm_bindgen]
pub fn py_tpm_solar_bc(t: &mut [f32], layers: usize, dau: f32, cosi: &[f32], prop: JsValue, dz: f32) -> Result<(), JsValue> {
    let prop = props(&prop).map_err(fail)?;
    let t = columns(t, layers)?;
    kalast::tpm::core::solar_bc(t, dau as Float, kalast::ndarray::ArrayView1::from(cosi), &prop, dz as Float)
        .map_err(|e| fail(format!("ValueError: {e}")))
}

#[wasm_bindgen]
pub fn py_tpm_bottom_adiabatic(t: &mut [f32], layers: usize) -> Result<(), JsValue> {
    kalast::tpm::core::bottom_adiabatic(columns(t, layers)?);
    Ok(())
}

/// `solar_bc` with a `Ground`: each facet's own albedo, emissivity and
/// top-layer conductivity.
#[wasm_bindgen]
pub fn py_tpm_solar_bc_ground(t: &mut [f32], layers: usize, dau: f32, cosi: &[f32], ground: usize, dz: f32) -> Result<(), JsValue> {
    let t = columns(t, layers)?;
    with_ground(ground, |g| {
        kalast::tpm::core::solar_bc(t, dau as Float, kalast::ndarray::ArrayView1::from(cosi), &*g, dz as Float)
            .map_err(|e| format!("ValueError: {e}"))
    })
    .map_err(fail)
}

/// `heat_conduction` with a `Ground`, which carries what its steps leave
/// over from one to the next.
#[wasm_bindgen]
pub fn py_tpm_heat_conduction_ground(t: &mut [f32], layers: usize, ground: usize, dt: f32, dz: f32) -> Result<(), JsValue> {
    let t = columns(t, layers)?;
    with_ground(ground, |g| {
        kalast::tpm::core::heat_conduction(t, &*g, dt as Float, dz as Float).map_err(|e| format!("ValueError: {e}"))
    })
    .map_err(fail)
}

fn ground_get(id: usize, name: &str) -> Result<JsValue, String> {
    with_ground(id, |g| {
        let (layers, facets) = g.dim();
        let f32s = |a: &[Float]| a.iter().map(|&x| x as f32).collect::<Vec<f32>>();
        let row = |a: &kalast::ndarray::Array1<Float>| array_f32(&f32s(&a.to_vec()), &[a.len()], true);
        let node = |a: &kalast::ndarray::Array2<Float>| {
            let v: Vec<f32> = a.iter().map(|&x| x as f32).collect();
            array_f32(&v, &[a.nrows(), a.ncols()], true)
        };
        Ok(match name {
            "layers" => JsValue::from_f64(layers as f64),
            "facets" => JsValue::from_f64(facets as f64),
            "albedo" => row(&g.albedo),
            "emissivity" => row(&g.emissivity),
            "conductivity" => node(&g.conductivity),
            "density" => node(&g.density),
            "heat_capacity" => node(&g.heat_capacity),
            "stability_maxdt" | "__repr__" => method(),
            _ => return Err(no_attribute("Ground", name)),
        })
    })
}

/// A property of a `Ground`, given whole by `_page.py`: one a facet, or one
/// a layer and a facet.
fn ground_set(id: usize, name: &str, v: &JsValue) -> Result<(), String> {
    let values: Vec<Float> = float_vec(v, name)?.into_iter().map(|x| x as Float).collect();
    with_ground(id, |g| {
        let (layers, facets) = g.dim();
        let row = |values: Vec<Float>| {
            if values.len() != facets {
                return Err(format!("ValueError: {name} is one value a facet, {facets} of them"));
            }
            Ok(kalast::ndarray::Array1::from(values))
        };
        let node = |values: Vec<Float>| {
            kalast::ndarray::Array2::from_shape_vec((layers, facets), values)
                .map_err(|_| format!("ValueError: {name} is one value a layer and a facet, ({layers}, {facets})"))
        };
        match name {
            "albedo" => g.albedo = row(values)?,
            "emissivity" => g.emissivity = row(values)?,
            "conductivity" => g.conductivity = node(values)?,
            "density" => g.density = node(values)?,
            "heat_capacity" => g.heat_capacity = node(values)?,
            _ => return Err(no_attribute("Ground", name)),
        }
        Ok(())
    })
}

fn ground_call(id: usize, name: &str, args: &js_sys::Array) -> Result<JsValue, String> {
    with_ground(id, |g| {
        let (layers, facets) = g.dim();
        Ok(match name {
            "stability_maxdt" => {
                let dz = number(&arg(args, 0), "dz")? as Float;
                let s = arg(args, 1).as_f64().unwrap_or(0.5) as Float;
                JsValue::from_f64(g.stability_maxdt(dz, s).map_err(|e| format!("ValueError: {e}"))? as f64)
            }
            "__repr__" => JsValue::from_str(&format!("Ground({layers} layers, {facets} facets)")),
            _ => return Err(no_attribute("Ground", name)),
        })
    })
}

#[wasm_bindgen]
pub fn py_tpm_heat_conduction(t: &mut [f32], layers: usize, prop: JsValue, dt: f32, dz: f32) -> Result<(), JsValue> {
    let prop = props(&prop).map_err(fail)?;
    kalast::tpm::core::heat_conduction(columns(t, layers)?, &prop, dt as Float, dz as Float)
        .map_err(|e| fail(format!("ValueError: {e}")))
}
