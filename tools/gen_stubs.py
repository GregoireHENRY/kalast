#!/usr/bin/env python
"""Generate .pyi stubs for the PyO3 classes, so editors can complete them.

A compiled extension exposes no attributes to a static analyser, so without
stubs `app.config.<tab>` offers nothing. Hand-written stubs for a surface this
size rot within a week, so they are generated from the Rust source instead --
`#[getter]`/`#[setter]` become attributes, everything else a method.

Run:  python tools/gen_stubs.py          # write
      python tools/gen_stubs.py --check  # fail if stale (used by a test)
"""

import argparse
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent

# Rust type -> Python annotation. Anything unmatched falls back to a bare name
# rather than guessing, so a wrong stub is never invented silently.
SCALARS = {
    "bool": "bool",
    "u8": "int", "u32": "int", "u64": "int", "usize": "int",
    "i32": "int", "i64": "int", "isize": "int",
    "f32": "float", "f64": "float", "Float": "float",
    "String": "str", "str": "str", "&str": "str",
    "()": "None",
    "PyAny": "object",
    "PyObject": "object",
}


def py_type(rust: str) -> str:
    t = rust.strip().removeprefix("pyo3::").removeprefix("crate::")
    t = re.sub(r"^&('[a-z]+ )?", "", t).strip()
    if t in SCALARS:
        return SCALARS[t]
    if m := re.fullmatch(r"Option<(.+)>", t):
        return f"{py_type(m.group(1))} | None"
    if m := re.fullmatch(r"(?:Py)?Result<(.+)>", t):
        return py_type(m.group(1))
    if m := re.fullmatch(r"Vec<(.+)>", t):
        return f"list[{py_type(m.group(1))}]"
    if m := re.fullmatch(r"\[(.+); *\d+\]", t):
        return f"list[{py_type(m.group(1))}]"
    if m := re.fullmatch(r"\((.+)\)", t):          # tuple
        parts, depth, cur = [], 0, ""
        for ch in m.group(1):
            if ch == "," and depth == 0:
                parts.append(cur); cur = ""
                continue
            depth += (ch in "<([") - (ch in ">)]")
            cur += ch
        parts.append(cur)
        return "tuple[" + ", ".join(py_type(p) for p in parts if p.strip()) + "]"
    if "PyArray" in t or "numpy" in t:
        return "numpy.ndarray"
    if "Bound<" in t or "Py<" in t:
        inner = re.search(r"Bound<[^,]+, *(.+)>", t) or re.search(r"Py<(.+)>", t)
        return py_type(inner.group(1)) if inner else "object"
    # A pyclass elsewhere in the tree: keep the bare name.
    t = t.split("::")[-1]
    return t if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", t) else "object"


def parse(src: str):
    """-> [(class, [(kind, name, type, doc)])] for one .rs file."""
    classes = []
    for m in re.finditer(r"#\[pyclass[^\]]*\]\s*(?:#\[[^\]]*\]\s*)*pub struct (\w+)", src):
        classes.append((m.group(1), []))
    if not classes:
        return []
    by_name = dict(classes)

    for im in re.finditer(r"#\[pymethods\]\s*impl (\w+) \{", src):
        cls = im.group(1)
        if cls not in by_name:
            continue
        # brace-match the impl block
        i = src.index("{", im.end() - 1)
        depth, j = 0, i
        while j < len(src):
            depth += (src[j] == "{") - (src[j] == "}")
            if depth == 0:
                break
            j += 1
        # Rust line comments inside an argument list would otherwise be
        # emitted verbatim and make the stub invalid Python.
        body = re.sub(r"//[^\n]*", "", src[i:j])

        for fm in re.finditer(
            r"((?:\s*///[^\n]*\n)*)\s*((?:#\[(?:[^\[\]]|\[[^\]]*\])*\]\s*)*)"
            r"(?:pub )?fn (\w+)\s*(?:<[^>]*>)?\s*\(([^)]*)\)(?:\s*->\s*([^{]+))?",
            body,
        ):
            doc, attrs, name, args, ret = fm.groups()
            if "#[new]" in attrs:
                name = "__init__"
                ret = "()"          # a constructor returns None in Python
            if name.startswith("__") and name != "__init__":
                continue
            doc = "\n".join(
                l.strip().removeprefix("///").strip() for l in doc.strip().splitlines()
            ).strip()
            gm = re.search(r"#\[getter(?:\((\w+)\))?\]", attrs)
            sm = re.search(r"#\[setter(?:\((\w+)\))?\]", attrs)
            if gm:
                # pyo3 strips a `get_` prefix unless the attribute renames it.
                attr = gm.group(1) or name.removeprefix("get_")
                by_name[cls].append(("attr", attr, py_type(ret or "object"), doc))
            elif sm:
                # A setter with no getter is still a settable attribute --
                # `app.before_render` is one, and skipping setters outright
                # left three of those out of the stub entirely.
                attr = sm.group(1) or name.removeprefix("set_")
                typ = "object"
                for a in args.split(","):
                    a = a.strip()
                    if a and not a.startswith(("&self", "self", "mut self", "py:")) and ":" in a:
                        typ = py_type(a.split(":", 1)[1])
                by_name[cls].append(("setter", attr, typ, doc))
            else:
                params = []
                for a in args.split(","):
                    a = a.strip()
                    if not a or a.startswith(("&self", "self", "slf", "mut self", "py:")):
                        continue
                    if ":" in a:
                        pn, pt = a.split(":", 1)
                        params.append(f"{pn.strip()}: {py_type(pt)}")
                by_name[cls].append(
                    ("meth", name, (py_type(ret) if ret else "None"), doc, params)
                )
    return classes


def merge(members):
    """Getter and setter for one attribute are one attribute in Python."""
    seen, out = {}, []
    for m in members:
        if m[0] in ("attr", "setter"):
            if m[1] in seen:
                if m[0] == "attr":            # a real getter beats a setter
                    out[seen[m[1]]] = ("attr",) + m[1:]
                continue
            seen[m[1]] = len(out)
            out.append(("attr",) + m[1:])
        else:
            out.append(m)
    return out


KNOWN_BUILTINS = {"int", "float", "str", "bool", "object", "None", "list",
                  "tuple", "dict", "numpy"}


def resolve(annotation: str, defined: set, index: dict) -> str:
    """Replace names that cannot be imported with `object`.

    An unresolved name makes a type checker treat the whole attribute as an
    error, which is worse than a vague-but-valid `object`.
    """
    def sub(m):
        name = m.group(0)
        if name in KNOWN_BUILTINS or name in defined or name in index:
            return name
        return "object"

    return re.sub(r"[A-Za-z_][A-Za-z0-9_]*", sub, annotation)


def render(classes, index=None) -> str:
    defined = {c for c, _ in classes}
    referenced = set()
    for _, members in classes:
        for m in merge(members):
            for t in re.findall(r"[A-Za-z_][A-Za-z0-9_]*", m[2] if m[0] == "attr" else m[2]):
                referenced.add(t)
            if m[0] == "meth":
                for p in m[4]:
                    referenced.update(re.findall(r"[A-Za-z_][A-Za-z0-9_]*", p))

    imports = []
    for name in sorted(referenced - defined):
        mod = (index or {}).get(name)
        if mod:
            imports.append(f"from {mod} import {name}")

    out = [
        "# Generated by tools/gen_stubs.py -- do not edit by hand.",
        "#",
        "# Regenerate after changing any #[pyclass]:  python tools/gen_stubs.py",
        "",
        "import numpy  # noqa: F401",
    ]
    out += imports
    out.append("")
    for cls, members in classes:
        members = merge(members)
        out.append(f"class {cls}:")
        if not members:
            out.append("    ...")
            out.append("")
            continue
        for m in members:
            if m[0] == "attr":
                _, name, typ, doc = m
                if doc:
                    out.append(f'    """{doc}"""' if False else "")
                    out[-1:] = []
                out.append(f"    {name}: {resolve(typ, defined, index or {})}")
            else:
                _, name, ret, doc, params = m
                params = [resolve(p, defined, index or {}) for p in params]
                ret = resolve(ret, defined, index or {})
                sig = ", ".join(["self"] + params)
                out.append(f"    def {name}({sig}) -> {ret}:")
                if doc:
                    first = doc.splitlines()[0]
                    out.append(f'        """{first}"""')
                    out.append("        ...")
                else:
                    out.append("        ...")
        out.append("")
    return "\n".join(out) + "\n"


TARGETS = {
    "src/py/app/config.rs": "kalast/app/config.pyi",
    "src/py/app/simulation.rs": "kalast/app/simulation.pyi",
    "src/py/app/frame.rs": "kalast/app/frame.pyi",
    "src/py/app/body.rs": "kalast/app/body.pyi",
    "src/py/app/gpu.rs": "kalast/app/gpu.pyi",
    "src/py/app/mod.rs": "kalast/app/_core.pyi",
    "src/py/entity.rs": "kalast/entity.pyi",
    "src/py/mesh.rs": "kalast/mesh.pyi",
    "src/py/routines/setup.rs": "kalast/routines/setup.pyi",
    "src/py/tpm/properties.rs": "kalast/tpm/properties.pyi",
    "src/py/tpm/column.rs": "kalast/tpm/column.pyi",
}


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true")
    args = ap.parse_args()

    parsed = {rs: parse((ROOT / rs).read_text()) for rs in TARGETS}
    index = {
        cls: TARGETS[rs].removesuffix(".pyi").replace("/", ".")
        for rs, classes in parsed.items()
        for cls, _ in classes
    }

    stale = []
    for rs, pyi in TARGETS.items():
        text = render(parsed[rs], index)
        path = ROOT / pyi
        if args.check:
            if not path.exists() or path.read_text() != text:
                stale.append(pyi)
        else:
            # A stub that does not parse is worse than none: a checker
            # discards the whole file and silently offers nothing.
            import ast

            ast.parse(text)
            path.write_text(text)
            print(f"wrote {pyi}")
    if stale:
        print("stale stubs (run tools/gen_stubs.py):", ", ".join(stale), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
