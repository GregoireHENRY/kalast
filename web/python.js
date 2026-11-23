// The page's Python: CPython compiled to WebAssembly (Pyodide), started as
// the page opens, as the UI app has its interpreter at hand, with kalast's
// Python package on it -- the repository's -- and, in place of the desktop's
// extension module, the page's own (`python/kalast/_rs`), which calls into
// the engine already running in the page (`src/python.rs`).
//
// kalast asks (`install`): run a script, serve the python tab. Each is
// answered in its own turn of the browser's loop, and `py_done` says when it
// is over -- a script that drives its own loop is over when its loop ends.

import { connected, refresh, write } from "./folder.js";

const PYODIDE = "https://cdn.jsdelivr.net/pyodide/v314.0.7/full/";
// A file a script writes past this goes to the visitor's folder alone, not
// into the page's files as well: kept twice in memory for nothing.
const BIG_OUTPUT = 64 << 20;

let kalast = null; // the engine's exports
let archive = null; // where kalast's Python package is
let fetched = null; // and its bytes, asked for as the page opens
let started = null; // Pyodide, once asked for
let pyodide_ = null; // and once started
let ready = false; // and kalast imported on it
let told = false; // that this browser cannot hold a script's step()
let running = false; // a script runs, or the python tab is served
// Python's own files as last had or sent, by path: their modification time.
const seen = new Map();

// Pyodide's packages by the names they are imported as.
let packages = new Map();

// What `kalast._rs._page` asks of JavaScript.
const glue = {
  package_for: (name) => packages.get(name),
  load: (pkg) => pyodide_.loadPackage(pkg, { messageCallback: (m) => /Loaded/.test(m) && say(m) }),
  is_array: (v) => Array.isArray(v),
  // A browser that can hold a Python call until a frame is drawn:
  // JavaScript Promise Integration, which `step()` needs.
  can_wait: () => typeof WebAssembly.Suspending === "function",
  // A step of the thermophysical model on numpy's own memory, so that the
  // script's array is the one stepped.
  step: (t, name, ...rest) => {
    const buf = t.getBuffer("f32");
    try {
      if (!buf.c_contiguous) throw new Error("TypeError: t must be C-contiguous, as kalast.tpm.core.columns makes it");
      return kalast["py_tpm_" + name](buf.data, ...rest);
    } finally {
      buf.release();
    }
  },
};

const say = (line) => kalast.py_fn("log_kalast", [line]);

// kalast's Python package, fetched as the page opens rather than when Python
// starts: the site holds its latest build's folder only, and a page opened
// before an update, asking for its package after it, got nothing back --
// its first script failing with Python's "is not a zip file".
function fetchPackage() {
  fetched = fetch(archive).then((r) => {
    if (!r.ok) throw new Error(`kalast's Python package could not be fetched (HTTP ${r.status}): reload the page`);
    return r.arrayBuffer();
  });
  // Said when Python starts, by `serve`, not before by the browser.
  fetched.catch(() => {});
}

async function start() {
  const began = performance.now();
  const { loadPyodide } = await import(PYODIDE + "pyodide.mjs");
  const pyodide = await loadPyodide({ indexURL: PYODIDE });
  pyodide_ = pyodide;
  try {
    const lock = await (await fetch(PYODIDE + "pyodide-lock.json")).json();
    for (const [name, p] of Object.entries(lock.packages)) for (const i of p.imports ?? []) packages.set(i, name);
  } catch {
    // Only the script's own imports are loaded then, before it runs.
  }
  await pyodide.loadPackage(["numpy"], { messageCallback: () => {} });
  const site = pyodide.runPython("import site; site.getsitepackages()[0]");
  // Fetched again if that failed, which may have been the network's.
  const zip = await fetched.catch(() => (fetchPackage(), fetched));
  pyodide.unpackArchive(zip, "zip", { extractDir: site });
  // What a script prints, on the browser's console as well as in the
  // script tab, for its developer tools.
  pyodide.registerJsModule("kalast_bridge", {
    ...kalast,
    py_fn: (name, args) => {
      if (name === "script_write") console.log(String(args[0]).replace(/\n$/, ""));
      return kalast.py_fn(name, args);
    },
  });
  pyodide.registerJsModule("kalast_glue", glue);
  // Figures drawn to files, as a script run from the UI app draws them.
  pyodide.runPython("import os; os.environ.setdefault('MPLBACKEND', 'agg')");
  // kalast imported now, so that a script's first run imports none of it,
  // and the python tab greeted, as the UI app greets it as it opens.
  await pyodide.runPythonAsync("import sys\nimport kalast.editor\nfrom kalast._rs.app._core import console_greet\n" +
                               "console_greet(sys.version, sys.platform)");
  ready = true;
  // Said on the browser's console, for its developer tools; the UI app's
  // log says nothing of its interpreter either.
  console.info(`Python ${pyodide.runPython("import sys; sys.version.split()[0]")} started in ` +
               `${((performance.now() - began) / 1000).toFixed(1)} s`);
  return pyodide;
}

// Python started, or started again after a start that failed, which may
// have been the network's.
function begin() {
  return start().catch((e) => {
    started = null;
    throw e;
  });
}

// The page's files in Python's own: what a script opens, the meshes, the
// examples, what was saved, a folder of the visitor's -- all of them the
// first time, then those changed since, before each run.
function files(pyodide) {
  const fs = pyodide.FS;
  for (const path of kalast.py_changes()) {
    if (kalast.py_has_file(path)) {
      const dir = path.includes("/") ? path.slice(0, path.lastIndexOf("/")) : "";
      if (dir) fs.mkdirTree(dir);
      fs.writeFile(path, kalast.py_file(path));
      seen.set(path, +fs.stat(path).mtime);
    } else {
      try {
        fs.unlink(path);
      } catch {
        // Not a file of Python's, or not there.
      }
      seen.delete(path);
    }
  }
}

function* walk(fs, dir) {
  for (const name of fs.readdir(dir)) {
    if (name === "." || name === ".." || name.startsWith(".") || name === "__pycache__") continue;
    const path = dir === "." ? name : `${dir}/${name}`;
    const stat = fs.stat(path);
    if (fs.isDir(stat.mode)) yield* walk(fs, path);
    else yield [path, +stat.mtime];
  }
}

// What a script wrote, new or changed, to the visitor's folder, where the
// desktop's would be on the disk -- through the page's files, which show it.
// With no folder, Python's own, as before.
function outputs(pyodide) {
  if (!pyodide || !connected()) return;
  const fs = pyodide.FS;
  try {
    for (const [path, mtime] of walk(fs, ".")) {
      if (seen.get(path) === mtime) continue;
      seen.set(path, mtime);
      const bytes = fs.readFile(path);
      if (bytes.length > BIG_OUTPUT) write(path, bytes).catch(() => {});
      else kalast.py_output(path, bytes);
    }
  } catch (e) {
    say(`what the script wrote could not all reach your folder: ${e}`);
  }
}

// And while a script runs, which may be until the page closes: a loop of
// `step()`s holds it.
setInterval(() => running && outputs(pyodide_), 2000);

// Python's packages a script imports -- matplotlib, scipy -- loaded first.
async function imports(pyodide, source) {
  try {
    await pyodide.loadPackagesFromImports(source, { messageCallback: (m) => /Loaded/.test(m) && say(m) });
  } catch (e) {
    say(`a package the script imports could not be loaded: ${e}`);
  }
}

async function serve(work, what) {
  try {
    started ??= begin();
    // Asked for as the page opens, before Python is there: the first visit
    // downloads it.
    if (!ready) say(`Python is still starting: the ${what} runs once it is ready`);
    const pyodide = await started;
    running = true;
    try {
      await work(pyodide);
    } finally {
      running = false;
      outputs(pyodide);
    }
  } catch (e) {
    kalast.py_fn("script_write", [`${e}\n`]);
  } finally {
    kalast.py_done();
  }
}

function run(path, source) {
  return serve(async (pyodide) => {
    if (!glue.can_wait() && !told) {
      told = true;
      say("this browser cannot hold a script's step() for a frame: callbacks and start() run");
    }
    // The visitor's folder as it is now: edited with their own tools.
    await refresh();
    files(pyodide);
    // Where scripts write, `out/`, as a clone of the repository has it.
    pyodide.FS.mkdirTree("out");
    await imports(pyodide, source);
    const g = pyodide.globals.get("dict")();
    g.set("path", path);
    g.set("source", source);
    try {
      await pyodide.runPythonAsync("from kalast._rs._page import run_script\nrun_script(path, source)", { globals: g });
    } finally {
      g.destroy();
    }
  }, "script");
}

function serveConsole() {
  return serve((pyodide) => pyodide.runPythonAsync("from kalast._rs._page import serve_console\nserve_console()"),
               "line");
}

// Give kalast its Python. `exports` are the engine's, `zip` where kalast's
// Python package is.
export function install(exports, zip) {
  kalast = exports;
  archive = zip;
  fetchPackage();
  kalast.py_install({
    run: (path, source) => setTimeout(() => run(path, source)),
    console: () => setTimeout(serveConsole),
  });
  // Python started now, as the UI app has its interpreter at hand: once the
  // page has drawn a frame, which its start would hold up. A start that
  // fails is said, and tried again when a script runs.
  requestAnimationFrame(() => setTimeout(() => {
    started ??= begin();
    started.catch((e) => say(`Python could not start: ${e}`));
  }));
}
