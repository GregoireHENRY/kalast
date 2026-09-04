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


def clean_doc(raw: str) -> str:
    """`/// ...` lines -> plain text, blank lines preserved."""
    return "\n".join(
        l.strip().removeprefix("///").strip() for l in (raw or "").strip().splitlines()
    ).strip()


def struct_field_docs(path: str, struct: str) -> dict:
    """Doc comments on a plain Rust struct's fields.

    The pyo3 accessors are mostly undocumented one-liners while the real
    explanation sits on the struct they wrap -- 71 documented fields on
    `Config` against 15 on its accessors. Hover should show the good one.
    """
    src = (ROOT / path).read_text()
    m = re.search(rf"pub struct {struct} \{{", src)
    if not m:
        return {}
    i = src.index("{", m.end() - 1)
    depth, j = 0, i
    while j < len(src):
        depth += (src[j] == "{") - (src[j] == "}")
        if depth == 0:
            break
        j += 1
    out = {}
    for fm in re.finditer(
        r"((?:^[ \t]*///[^\n]*\n)*)[ \t]*(?:#\[[^\]]*\]\s*)*pub (\w+):",
        src[i:j],
        re.M,
    ):
        if doc := clean_doc(fm.group(1)):
            out[fm.group(2)] = doc
    return out


def parse(src: str):
    """-> [(class, [(kind, name, type, doc)])] for one .rs file."""
    classes = []
    for m in re.finditer(
        r"((?:^[ \t]*///[^\n]*\n)*)[ \t]*#\[pyclass[^\]]*\]\s*"
        r"(?:#\[[^\]]*\]\s*)*pub struct (\w+)",
        src,
        re.M,
    ):
        classes.append((m.group(2), [], clean_doc(m.group(1))))
    if not classes:
        return []
    by_name = {c: members for c, members, _ in classes}

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
        # `(?<!/)` as well as `(?!/)`: without the lookbehind this matches from
        # the *second* slash of `///`, stripping the doc and leaving a stray
        # `/` behind, so every doc comment silently vanished.
        body = re.sub(r"(?<!/)//(?!/)[^\n]*", "", src[i:j])

        for fm in re.finditer(
            # Docs and attributes may interleave in either order -- pyo3 code
            # commonly writes `#[getter]` then `///`. Capture the whole
            # prelude and pull the two apart afterwards.
            r"((?:[ \t]*(?:///[^\n]*|#\[(?:[^\[\]]|\[[^\]]*\])*\])[ \t]*\n)*)"
            r"[ \t]*(?:pub )?fn (\w+)\s*(?:<[^>]*>)?\s*\(([^)]*)\)(?:\s*->\s*([^{]+))?",
            body,
        ):
            prelude, name, args, ret = fm.groups()
            doc = "\n".join(
                l for l in (prelude or "").splitlines() if l.strip().startswith("///")
            )
            attrs = "\n".join(
                l for l in (prelude or "").splitlines() if l.strip().startswith("#[")
            )
            if "#[new]" in attrs:
                name = "__init__"
                ret = "()"          # a constructor returns None in Python
            if name.startswith("__") and name != "__init__":
                continue
            doc = clean_doc(doc)
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


def docstring(text, indent):
    """A triple-quoted docstring, escaped and indented."""
    text = text.replace(chr(92), chr(92) * 2).replace('"""', "'''")
    lines = text.splitlines()
    if len(lines) == 1:
        return [indent + '"""' + lines[0] + '"""']
    out = [indent + '"""' + lines[0]]
    out += [(indent + l).rstrip() for l in lines[1:]]
    out.append(indent + '"""')
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
    defined = {c for c, _, _ in classes}
    referenced = set()
    for _, members, _ in classes:
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
    for cls, members, cls_doc in classes:
        members = merge(members)
        out.append(f"class {cls}:")
        if cls_doc:
            out += docstring(cls_doc, "    ")
        if not members:
            out.append("    ...")
            out.append("")
            continue
        for m in members:
            if m[0] == "attr":
                _, name, typ, doc = m
                out.append(f"    {name}: {resolve(typ, defined, index or {})}")
                # An attribute docstring is a bare string literal after the
                # annotation: Python ignores it, editors show it on hover.
                if doc:
                    out += docstring(doc, "    ")
            else:
                _, name, ret, doc, params = m
                params = [
                    # Only the annotation: resolving the whole "name: type"
                    # rewrote parameter *names* to `object` as well.
                    f"{p.split(':', 1)[0]}:{resolve(p.split(':', 1)[1], defined, index or {})}"
                    if ":" in p
                    else p
                    for p in params
                ]
                ret = resolve(ret, defined, index or {})
                sig = ", ".join(["self"] + params)
                out.append(f"    def {name}({sig}) -> {ret}:")
                if doc:
                    out += docstring(doc, "        ")
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

    # Where a pyo3 accessor carries no doc of its own, fall back to the doc on
    # the field it wraps: that is where the explanation actually lives.
    fallbacks = {
        "Config": struct_field_docs("src/app/config.rs", "Config"),
        "Hud": struct_field_docs("src/app/config.rs", "Hud"),
        "State": struct_field_docs("src/app/simulation.rs", "State"),
    }
    for classes in parsed.values():
        for cls, members, _ in classes:
            src_docs = fallbacks.get(cls, {})
            for i, m in enumerate(members):
                if m[0] in ("attr", "setter") and not m[3] and m[1] in src_docs:
                    members[i] = (m[0], m[1], m[2], src_docs[m[1]])

    index = {
        cls: TARGETS[rs].removesuffix(".pyi").replace("/", ".")
        for rs, classes in parsed.items()
        for cls, _, _ in classes
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
