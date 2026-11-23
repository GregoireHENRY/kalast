// A folder of the visitor's own, which the page works in once connected, as
// the desktop app works in its own folder: the page's examples and an empty
// scripts/ put there when it is picked (nothing there replaced), its files
// over the page's own, read again when the page is come back to and before
// each run, and what the page saves or a script writes, written there.
//
// The browser's File System Access API, which Chrome and Edge have. The
// folder is remembered for the next visit, where the browser asks once to
// allow it again. Nothing in it is deleted from the page but the old name of
// a file renamed there.

const DB = "kalast";
const STORE = "folder";
// A file past this is left in the folder, the page reading it into memory
// for nothing it could do with it.
const BIG = 256 << 20;
// Not read: hidden folders, Python's caches, build trees.
const SKIP = new Set(["__pycache__", "node_modules", "target"]);
// Where scripts write, out/: listed, not read -- exported frames and results
// by the hundred, which the page has no use for again and would hold in its
// memory at every visit.
const listedOnly = (path) => path === "out" || path.startsWith("out/");

let kalast = null; // the engine's exports
let root = null; // the folder, once the page works in it
let known = new Map(); // each path read or written -> "size:lastModified", or "dir"
let scanning = null; // the reading under way
let again = false; // and asked for again meanwhile
const writing = new Map(); // a file's writes, in order

const say = (line) => kalast.py_fn("log_kalast", [line]);

// --- the folder remembered, in the browser's IndexedDB -----------------------

function db() {
  return new Promise((resolve, reject) => {
    const open = indexedDB.open(DB, 1);
    open.onupgradeneeded = () => open.result.createObjectStore(STORE);
    open.onsuccess = () => resolve(open.result);
    open.onerror = () => reject(open.error);
  });
}

async function stored(mode, act) {
  const d = await db();
  return new Promise((resolve, reject) => {
    const r = act(d.transaction(STORE, mode).objectStore(STORE));
    r.onsuccess = () => resolve(r.result ?? null);
    r.onerror = () => reject(r.error);
  });
}

const remembered = () => stored("readonly", (s) => s.get("handle"));
const remember = (handle) => stored("readwrite", (s) => s.put(handle, "handle"));
const forget = () => stored("readwrite", (s) => s.delete("handle"));

// --- reading the folder into the page ----------------------------------------

async function* walk(dir, prefix) {
  for await (const [name, handle] of dir.entries()) {
    if (name.startsWith(".") || SKIP.has(name)) continue;
    const path = prefix ? `${prefix}/${name}` : name;
    yield [path, handle];
    if (handle.kind === "directory") yield* walk(handle, path);
  }
}

// The folder's files into the page's, those new or changed since last read;
// those gone from it, from the page's too. The editor is told of files it may
// hold.
async function scan() {
  const seen = new Set();
  const changed = [];
  let read = 0;
  for await (const [path, handle] of walk(root, "")) {
    seen.add(path);
    if (handle.kind === "directory") {
      if (!known.has(path) && !listedOnly(path)) kalast.folder_dir(path);
      known.set(path, "dir");
      continue;
    }
    if (listedOnly(path)) {
      if (!known.has(path)) known.set(path, "listed");
      continue;
    }
    const file = await handle.getFile();
    const stamp = `${file.size}:${file.lastModified}`;
    if (known.get(path) === stamp) continue;
    if (file.size > BIG) {
      if (!known.has(path)) say(`${path} is ${Math.round(file.size / 1e6)} MB: left in your folder, too big for the page`);
    } else {
      kalast.folder_put(path, new Uint8Array(await file.arrayBuffer()));
      if (known.has(path)) changed.push(path);
      read += 1;
    }
    known.set(path, stamp);
  }
  for (const path of [...known.keys()]) {
    if (!seen.has(path) && !writing.has(path)) {
      kalast.folder_gone(path);
      known.delete(path);
    }
  }
  if (changed.length) kalast.folder_changed(changed);
  return read;
}

// Read the folder again: when the page is come back to, before a run. One
// reading at a time; asked meanwhile, it reads once more after.
export function refresh() {
  if (!root) return Promise.resolve();
  if (scanning) {
    again = true;
    return scanning;
  }
  scanning = (async () => {
    try {
      do {
        again = false;
        await scan();
      } while (again);
    } catch (e) {
      say(`your folder could not be read: ${e.message}`);
    } finally {
      scanning = null;
    }
  })();
  return scanning;
}

export const connected = () => root !== null;

// --- writing to it -------------------------------------------------------------

async function dirOf(path, create) {
  let dir = root;
  for (const name of path.split("/").slice(0, -1)) dir = await dir.getDirectoryHandle(name, { create });
  return dir;
}

// A file the page saved or a script wrote: into the folder, after the
// writes of it before.
export function write(path, bytes) {
  const before = writing.get(path) ?? Promise.resolve();
  const done = before
    .then(async () => {
      const dir = await dirOf(path, true);
      const handle = await dir.getFileHandle(path.split("/").pop(), { create: true });
      const out = await handle.createWritable();
      await out.write(bytes);
      await out.close();
      const file = await handle.getFile();
      known.set(path, `${file.size}:${file.lastModified}`);
    })
    .catch((e) => {
      say(`${path} could not be written to your folder: ${e.message}`);
      throw e;
    });
  writing.set(path, done.catch(() => {}));
  done.catch(() => {}).finally(() => {
    if (writing.get(path) === done) writing.delete(path);
  });
  return done;
}

async function mkdir(path) {
  try {
    await dirOf(`${path}/-`, true);
    known.set(path, "dir");
  } catch (e) {
    say(`${path} could not be made in your folder: ${e.message}`);
  }
}

// A file's old name, once it is renamed: taken away after every write under
// way has landed -- the new name's among them -- and not if one failed.
async function remove(path) {
  const results = await Promise.allSettled([...writing.values()]);
  if (results.some((r) => r.status === "rejected")) {
    say(`${path} is kept in your folder: a write before it failed`);
    return;
  }
  try {
    const dir = await dirOf(path, false);
    await dir.removeEntry(path.split("/").pop(), { recursive: true });
  } catch (e) {
    if (e.name !== "NotFoundError") say(`${path} could not be taken away from your folder: ${e.message}`);
  }
  for (const p of [...known.keys()]) if (p === path || p.startsWith(`${path}/`)) known.delete(p);
}

// The names in one of the folder's directories, as last read or written:
// what an export numbers its frames after.
function names(dir) {
  const prefix = dir ? `${dir}/` : "";
  const out = [];
  for (const p of known.keys()) if (p.startsWith(prefix) && !p.slice(prefix.length).includes("/")) out.push(p.slice(prefix.length));
  return out;
}

// Whether a path is the folder's, or holds what is: the files tab keeps it.
function has(path) {
  if (!root) return false;
  for (const p of known.keys()) if (p === path || p.startsWith(`${path}/`)) return true;
  return false;
}

// --- connecting ------------------------------------------------------------------

// The page's examples, and an empty scripts/, into a folder just picked, as a
// bundle has them: those missing there, nothing replaced.
async function seed() {
  let put = 0;
  for (const path of kalast.py_files()) {
    if (!path.startsWith("examples/")) continue;
    try {
      await (await dirOf(path, false)).getFileHandle(path.split("/").pop());
    } catch {
      await write(path, kalast.py_file(path)).then(() => (put += 1), () => {});
    }
  }
  await root.getDirectoryHandle("scripts", { create: true });
  return put;
}

async function open(handle, picked) {
  root = handle;
  known = new Map();
  const put = picked ? await seed() : 0;
  await refresh();
  kalast.folder_state(handle.name, undefined);
  say(`working in your folder "${handle.name}"` + (put ? `: the page's examples put there, ${put} files` : ""));
}

async function pick() {
  let handle;
  try {
    handle = await window.showDirectoryPicker({ id: "kalast", mode: "readwrite" });
  } catch (e) {
    if (e.name !== "AbortError") say(`no folder: ${e.message}`);
    return;
  }
  await remember(handle).catch(() => {});
  await open(handle, true);
}

async function reconnect() {
  const handle = await remembered().catch(() => null);
  if (!handle) return;
  if ((await handle.requestPermission({ mode: "readwrite" })) === "granted") await open(handle, false);
  else say(`"${handle.name}" was not allowed: the page works in its own files`);
}

// Back to the page's own files: the folder forgotten, the page loaded again
// without it. The folder is left as it is.
async function disconnect() {
  await forget().catch(() => {});
  location.reload();
}

// Give kalast its way to a folder, where the browser has one, and work in the
// one from the last visit if the browser still allows it.
export async function install(exports) {
  kalast = exports;
  if (typeof window.showDirectoryPicker !== "function") return;
  kalast.folder_install({ pick, reconnect, disconnect, has, names, write, mkdir, remove });
  addEventListener("focus", () => refresh());
  document.addEventListener("visibilitychange", () => document.visibilityState === "visible" && refresh());
  const handle = await remembered().catch(() => null);
  if (!handle) return;
  if ((await handle.queryPermission({ mode: "readwrite" })) === "granted") await open(handle, false);
  else kalast.folder_state(undefined, handle.name);
}
