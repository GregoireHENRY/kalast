#!/usr/bin/env python
"""Generate the Python bindings for `Config` and `AppConfig` from the Rust struct.

The third mirror of the config, and the last one to be generated. The editor
panel and the `.pyi` stubs were already produced from `src/app/config.rs`; the
`#[getter]`/`#[setter]` pairs were written by hand, which is how an option
came to be complete everywhere except from a script four separate times.

`Config` is a struct of sub-structs and the Python surface follows it: one
proxy class per group, each holding the same `Rc<RefCell<Config>>` as the
root and reading its own group through it. That indirection is the whole
reason these are generated rather than being `#[pyclass(get_all)]` on the
groups themselves -- a `get_all` getter hands Python a *copy*, and
`config.grid.color = ...` on a copy sets nothing, silently.

    Config.<group>            -> <Group>Config proxy, a getter on the root
    <Group>Config.<field>     -> getter + setter through the shared Rc
    AppConfig.<field>         -> the same, flat

What the type cannot say, a doc marker can:

    /// :py_custom:   no accessor generated; the hand-written file has one
                      (the colormap, which parses names and arrays)

Old flat names keep working for a release: `__getattr__` / `__setattr__` on
the root forward `config.grid_color` to `config.grid.color` with a
`DeprecationWarning`, from the table in `tools/config_renames.py`. The three
window properties that moved to `app.config` raise an error saying so.

Run:  python tools/gen_bindings.py          # write
      python tools/gen_bindings.py --check  # fail if stale
"""

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "tools"))
from config_renames import GROUPS, RENAMES  # noqa: E402

SOURCE = ROOT / "src/app/config.rs"
TARGET = ROOT / "src/py/app/config_gen.rs"

RUST_CONFIG = "crate::app::config::Config"
RUST_APP = "crate::app::config::AppConfig"


def fields(struct: str, src: str):
    m = re.search(r"pub struct %s \{(.*?)\n\}" % struct, src, re.S)
    if not m:
        raise SystemExit(f"{struct} not found in {SOURCE}")
    out = []
    for fm in re.finditer(
        r"((?:^[ \t]*///[^\n]*\n)*)[ \t]*pub (\w+): ([^,\n]+),", m.group(1), re.M
    ):
        doc = [line.strip().removeprefix("///").removeprefix(" ").rstrip()
               for line in (fm.group(1) or "").splitlines()]
        out.append((fm.group(2), fm.group(3).strip(), doc))
    return out


def has_marker(doc, name):
    return any(l.strip().startswith(f":{name}:") for l in doc)


def rust_doc(doc, indent="    "):
    """The field's doc as a `///` block, minus the generator markers."""
    keep = [l for l in doc if not re.match(r"^:\w+:", l.strip())]
    while keep and not keep[0]:
        keep.pop(0)
    while keep and not keep[-1]:
        keep.pop()
    return [f"{indent}///{(' ' + l) if l else ''}" for l in keep]


# Rust type -> how it crosses. `get` and `set` are expressions over `v`; the
# setter may be fallible (`PyResult<()>`) when a value can be rejected.
def accessor(path: str, name: str, rust: str, doc):
    """-> [lines] of a getter/setter pair for one field at `path`."""
    lines = rust_doc(doc)
    g = f"{path}.{name}"
    cfg = "self.config.borrow()"
    cfg_mut = "self.config.borrow_mut()"

    if rust in ("bool", "u32", "usize", "f32", "f64", "Float", "[f32; 3]", "[f32; 4]",
                "Option<f32>", "Option<Float>", "Option<bool>"):
        ty = rust
        lines += [
            "    #[getter]",
            f"    fn {name}(&self) -> {ty} {{ {cfg}.{g} }}",
            "    #[setter]",
            f"    fn set_{name}(&mut self, v: {ty}) {{ {cfg_mut}.{g} = v; }}",
        ]
    elif rust == "String":
        lines += [
            "    #[getter]",
            f"    fn {name}(&self) -> String {{ {cfg}.{g}.clone() }}",
            "    #[setter]",
            f"    fn set_{name}(&mut self, v: &str) {{ {cfg_mut}.{g} = v.to_string(); }}",
        ]
    elif rust == "wgpu::Color":
        lines += [
            "    #[getter]",
            f"    fn {name}(&self) -> [Float; 4] {{",
            f"        let c = {cfg}.{g};",
            "        [c.r as Float, c.g as Float, c.b as Float, c.a as Float]",
            "    }",
            "    #[setter]",
            f"    fn set_{name}(&mut self, v: [Float; 4]) {{",
            f"        {cfg_mut}.{g} = wgpu::Color {{",
            "            r: v[0] as f64, g: v[1] as f64, b: v[2] as f64, a: v[3] as f64,",
            "        };",
            "    }",
        ]
    elif rust == "HudAnchor":
        lines += [
            "    #[getter]",
            f"    fn {name}(&self) -> String {{ {cfg}.{g}.name().to_string() }}",
            "    #[setter]",
            f"    fn set_{name}(&mut self, v: &str) -> PyResult<()> {{",
            "        let a = crate::app::config::HudAnchor::parse(v).ok_or_else(|| {",
            "            pyo3::exceptions::PyValueError::new_err(format!(",
            '                "unknown anchor {v:?}: expected one of top-left, top-center, \\',
            '                 top-right, middle-left, middle-center, middle-right, \\',
            '                 bottom-left, bottom-center, bottom-right"',
            "            ))",
            "        })?;",
            f"        {cfg_mut}.{g} = a;",
            "        Ok(())",
            "    }",
        ]
    elif rust in ("crate::app::axes::AxesStyle", "AxesStyle"):
        lines += [
            "    #[getter]",
            f"    fn {name}(&self) -> String {{ {cfg}.{g}.name().to_string() }}",
            "    #[setter]",
            f"    fn set_{name}(&mut self, v: &str) -> PyResult<()> {{",
            "        let s = crate::app::axes::AxesStyle::parse(v).ok_or_else(|| {",
            "            pyo3::exceptions::PyValueError::new_err(format!(",
            '                "unknown axes style {v:?}: expected off, box, panes, gizmo or blender"',
            "            ))",
            "        })?;",
            f"        {cfg_mut}.{g} = s;",
            "        Ok(())",
            "    }",
        ]
    else:
        raise SystemExit(
            f"{path}.{name}: no binding rule for `{rust}` -- add one to gen_bindings.py, "
            f"or mark the field `:py_custom:` and write it by hand"
        )
    return lines


def proxy_name(group: str) -> str:
    return "".join(p.capitalize() for p in group.split("_")) + "Config"


def main() -> int:
    src = SOURCE.read_text()
    out = [
        "// Generated by tools/gen_bindings.py -- do not edit by hand.",
        "//",
        "// Regenerate after changing `Config` or `AppConfig`:  python tools/gen_bindings.py",
        "//",
        "// Hand-written accessors with real logic live in `config.rs`, on fields",
        "// marked `:py_custom:`; everything here is mechanical.",
        "",
        "use std::{cell::RefCell, rc::Rc};",
        "",
        "use pyo3::prelude::*;",
        "",
        "use crate::Float;",
        "",
    ]

    groups = fields("Config", src)
    docs = {g: doc for g, _, doc in groups}
    for gfield, gtype, gdoc in groups:
        cls = proxy_name(gfield)
        out += rust_doc(gdoc, indent="")
        out += [
            "///",
            f"/// `app.simulation.config.{gfield}`. Reads and writes the live config",
            "/// through the same handle as every other view of it.",
            "#[pyclass(unsendable)]",
            f"pub struct {cls} {{",
            f"    pub config: Rc<RefCell<{RUST_CONFIG}>>,",
            "}",
            "",
            "#[pymethods]",
            f"impl {cls} {{",
        ]
        for name, rust, doc in fields(gtype, src):
            if has_marker(doc, "py_custom"):
                continue
            out += accessor(gfield, name, rust, doc)
        out += [
            "    fn __repr__(&self) -> String {",
            f"        format!(\"{{:?}}\", self.config.borrow().{gfield})",
            "    }",
            "}",
            "",
        ]

    # --- the root: one getter per group, plus the deprecation shim ---
    out += [
        "/// The groups, as live views. Generated beside the group classes so a",
        "/// group added to the struct appears here without anyone remembering.",
        "#[pymethods]",
        "impl super::config::Config {",
    ]
    for gfield, _, gdoc in groups:
        cls = proxy_name(gfield)
        out += rust_doc(gdoc)
        out += [
            "    #[getter]",
            f"    fn {gfield}(&self) -> {cls} {{ {cls} {{ config: self.config.clone() }} }}",
        ]
    # the shim
    moved_app = sorted(k for k, (g, _) in RENAMES.items() if g == "APP")
    out += [
        "",
        "    /// Old flat names, for one release: forwarded to the group they",
        "    /// moved into, with a `DeprecationWarning` naming the new path.",
        "    /// Table in `tools/config_renames.py`.",
        "    fn __getattr__(slf: &Bound<'_, Self>, name: &str) -> PyResult<Py<PyAny>> {",
        "        let py = slf.py();",
        "        if let Some((group, new)) = renamed(name) {",
        "            deprecated(py, name, group, new)?;",
        "            return Ok(slf.getattr(group)?.getattr(new)?.unbind());",
        "        }",
        "        if MOVED_TO_APP.contains(&name) {",
        "            return Err(pyo3::exceptions::PyAttributeError::new_err(format!(",
        "                \"config.{name} is a window property and moved to app.config.{name}\"",
        "            )));",
        "        }",
        "        Err(pyo3::exceptions::PyAttributeError::new_err(format!(",
        "            \"'Config' object has no attribute {name:?}\"",
        "        )))",
        "    }",
        "",
        "    /// The setter half of the shim. Anything not in the table takes the",
        "    /// ordinary path, so the real setters on this class still run.",
        "    fn __setattr__(slf: &Bound<'_, Self>, name: &str, value: Bound<'_, PyAny>) -> PyResult<()> {",
        "        let py = slf.py();",
        "        if let Some((group, new)) = renamed(name) {",
        "            deprecated(py, name, group, new)?;",
        "            return slf.getattr(group)?.setattr(new, value);",
        "        }",
        "        if MOVED_TO_APP.contains(&name) {",
        "            return Err(pyo3::exceptions::PyAttributeError::new_err(format!(",
        "                \"config.{name} is a window property and moved to app.config.{name}\"",
        "            )));",
        "        }",
        "        // The generic path, by hand: calling `slf.setattr` here would come",
        "        // straight back to this function.",
        "        let key = pyo3::types::PyString::new(py, name);",
        "        let rc = unsafe {",
        "            pyo3::ffi::PyObject_GenericSetAttr(slf.as_ptr(), key.as_ptr(), value.as_ptr())",
        "        };",
        "        if rc == -1 {",
        "            return Err(PyErr::fetch(py));",
        "        }",
        "        Ok(())",
        "    }",
        "}",
        "",
        "/// Where a flat name went, if it was one.",
        "fn renamed(name: &str) -> Option<(&'static str, &'static str)> {",
        "    match name {",
    ]
    for old, (g, new) in sorted(RENAMES.items()):
        if g == "APP":
            continue
        out.append(f'        "{old}" => Some(("{g}", "{new}")),')
    out += [
        "        _ => None,",
        "    }",
        "}",
        "",
        "const MOVED_TO_APP: &[&str] = &[" + ", ".join(f'"{n}"' for n in moved_app) + "];",
        "",
        "fn deprecated(py: Python<'_>, old: &str, group: &str, new: &str) -> PyResult<()> {",
        "    let msg = std::ffi::CString::new(format!(",
        "        \"config.{old} is deprecated; use config.{group}.{new}\"",
        "    ))?;",
        "    PyErr::warn(",
        "        py,",
        "        &py.get_type::<pyo3::exceptions::PyDeprecationWarning>(),",
        "        msg.as_c_str(),",
        "        2,",
        "    )",
        "}",
        "",
        "/// The application's own settings, flat.",
        "#[pymethods]",
        "impl super::config::AppConfig {",
    ]
    for name, rust, doc in fields("AppConfig", src):
        if has_marker(doc, "py_custom"):
            continue
        out += [l.replace(f"self.config.borrow()", "self.config.borrow()")
                for l in accessor("", name, rust, doc)]
    out += ["}", ""]

    text = "\n".join(out).replace("borrow()..", "borrow().").replace("borrow_mut()..", "borrow_mut().") + "\n"
    # `accessor("")` produces `.name` paths with a leading dot for AppConfig; tidy them.
    text = re.sub(r"borrow\(\)\.\.", "borrow().", text)

    if "--check" in sys.argv:
        cur = TARGET.read_text() if TARGET.exists() else ""
        if cur != text:
            print("bindings are out of date; run python tools/gen_bindings.py")
            return 1
        print("bindings are current")
        return 0
    TARGET.write_text(text)
    n = text.count("#[getter]")
    print(f"wrote {TARGET.relative_to(ROOT)}: {len(groups)} group classes, {n} getters")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
