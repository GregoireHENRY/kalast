#!/usr/bin/env python
"""Generate the editor's config panel from `src/app/config.rs`.

Generated, not hand-written, for the reason the `.pyi` stubs are: a second
mirror of the config written by hand goes stale silently. A field added to the
Rust struct gets a widget here without anyone remembering to add one, and
`tests/test_config_panel.py` fails if the checked-in file drifts.

What the Rust source cannot say, the doc comments can:

    /// :range: 0..=16          slider bounds instead of a drag field
    /// :step: 0.01             drag speed
    /// :skip:                  no widget (edited from a script, not by hand)
    /// :group: Shadows         override the group it lands in

Everything else is inferred from the type.
"""

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent

# Enums, with the file their variants are declared in. A combo box is the
# right widget and the variants can be read out of the source, so they do not
# have to be repeated here.
ENUMS = {
    "crate::app::axes::AxesStyle": ("src/app/axes.rs", "AxesStyle"),
    "AxesStyle": ("src/app/axes.rs", "AxesStyle"),
    "HudAnchor": ("src/app/config.rs", "HudAnchor"),
}
SOURCE = ROOT / "src/app/config.rs"
TARGET = ROOT / "src/app/gui/config_panel.rs"

# Which collapsing header a field lands in, by name prefix. First match wins,
# so the order matters: `debug_light_cube_show` is a debug flag, not a light.
GROUPS = [
    ("debug_", "Debug"),
    ("shadow_", "Shadows"),
    ("axes", "Axes"),
    ("colorbar", "Colour bar"),
    ("wireframe_", "Wireframe"),
    ("export_", "Export"),
    ("light_", "Lighting"),
    ("ambient_", "Lighting"),
    ("sensitivity_", "Controls"),
    ("emulate_", "Controls"),
    ("hud_", "HUD"),
    ("value_", "Data colouring"),
    ("colormap", "Data colouring"),
    ("color_mode", "Shading"),
    ("srgb_", "Shading"),
    ("gamma", "Shading"),
    ("color", "Shading"),
    ("background", "Shading"),
    ("render_back_face", "Shading"),
    ("msaa", "Shading"),
    ("access_shadow_map", "GPU results"),
]
FALLBACK_GROUP = "Window"
GROUP_ORDER = [
    "Shading", "Shadows", "Wireframe", "Lighting", "Data colouring",
    "Axes", "Colour bar", "HUD", "Export", "GPU results", "Controls",
    "Window", "Debug",
]


def variants(rust: str) -> list[str] | None:
    """Variant names of a known enum, read from its own source file."""
    if rust not in ENUMS:
        return None
    path, name = ENUMS[rust]
    text = (ROOT / path).read_text()
    m = re.search(r"pub enum %s \{(.*?)\n\}" % name, text, re.S)
    if not m:
        return None
    body = re.sub(r"//[^\n]*", "", m.group(1))
    return re.findall(r"^\s*(\w+)\s*,", body, re.M)


def fields(struct: str, src: str):
    """-> [(name, rust_type, doc)] for one struct, in declaration order."""
    m = re.search(r"pub struct %s \{(.*?)\n\}" % struct, src, re.S)
    if not m:
        return []
    out = []
    for fm in re.finditer(
        r"((?:^[ \t]*///[^\n]*\n)*)[ \t]*pub (\w+): ([^,\n]+),", m.group(1), re.M
    ):
        doc = "\n".join(
            line.strip().removeprefix("///").strip()
            for line in (fm.group(1) or "").splitlines()
        ).strip()
        out.append((fm.group(2), fm.group(3).strip(), doc))
    return out


def marker(doc: str, name: str):
    m = re.search(rf"^:{name}:\s*(.*)$", doc, re.M)
    return m.group(1).strip() if m else None


def summary(doc: str) -> str:
    """First sentence of the doc, for the hover text."""
    body = "\n".join(l for l in doc.splitlines() if not l.startswith(":"))
    body = body.strip().split("\n\n")[0].replace("\n", " ").strip()
    return body.replace("\\", "\\\\").replace('"', '\\"')


def group_of(name: str, doc: str) -> str:
    if forced := marker(doc, "group"):
        return forced
    for prefix, group in GROUPS:
        if name.startswith(prefix):
            return group
    return FALLBACK_GROUP


def widget(path: str, name: str, rust: str, doc: str) -> list[str]:
    """Rust lines drawing one field, or [] to skip it."""
    if marker(doc, "skip") is not None:
        return []
    hover = summary(doc)
    rng = marker(doc, "range")
    step = marker(doc, "step")
    label = name

    def hovered(expr: str) -> list[str]:
        if not hover:
            return [f"        {expr};"]
        return [f'        {expr}.on_hover_text("{hover}");']

    if rust == "bool":
        return hovered(f'ui.checkbox(&mut {path}, "{label}")')

    if rust in ("u32", "usize", "f32", "f64", "Float"):
        if rng:
            return hovered(
                f'ui.add(egui::Slider::new(&mut {path}, {rng}).text("{label}"))'
            )
        speed = step or ("0.01" if rust in ("f32", "f64", "Float") else "1.0")
        return hovered(
            f'ui.add(egui::DragValue::new(&mut {path}).speed({speed}).prefix("{label}  "))'
        )

    if rust in ("Option<f32>", "Option<Float>"):
        # Automatic when None, which is what these mean; the checkbox is
        # "override it", and the value only appears once overridden.
        return [
            "        {",
            f"            let mut on = {path}.is_some();",
            f'            if ui.checkbox(&mut on, "{label}").on_hover_text("{hover}").changed() {{',
            f"                {path} = if on {{ Some(0.0) }} else {{ None }};",
            "            }",
            f"            if let Some(v) = {path}.as_mut() {{",
            "                ui.add(egui::DragValue::new(v).speed(1e-6));",
            "            }",
            "        }",
        ]

    if rust == "Option<bool>":
        return [
            "        {",
            f"            let mut on = {path}.is_some();",
            f'            if ui.checkbox(&mut on, "{label}").on_hover_text("{hover}").changed() {{',
            f"                {path} = if on {{ Some(true) }} else {{ None }};",
            "            }",
            f"            if let Some(v) = {path}.as_mut() {{",
            '                ui.checkbox(v, "");',
            "            }",
            "        }",
        ]

    if rust == "String":
        return [
            "        ui.horizontal(|ui| {",
            f'            ui.label("{label}").on_hover_text("{hover}");',
            f"            ui.add(egui::TextEdit::singleline(&mut {path}).desired_width(120.0));",
            "        });",
        ]

    if rust == "wgpu::Color":
        return [
            "        ui.horizontal(|ui| {",
            f'            ui.label("{label}").on_hover_text("{hover}");',
            f"            let mut rgba = [{path}.r as f32, {path}.g as f32,",
            f"                            {path}.b as f32, {path}.a as f32];",
            "            if ui.color_edit_button_rgba_unmultiplied(&mut rgba).changed() {",
            f"                {path} = wgpu::Color {{",
            "                    r: rgba[0] as f64, g: rgba[1] as f64,",
            "                    b: rgba[2] as f64, a: rgba[3] as f64,",
            "                };",
            "            }",
            "        });",
        ]

    if rust == "[f32; 3]":
        return [
            "        ui.horizontal(|ui| {",
            f'            ui.label("{label}").on_hover_text("{hover}");',
            f"            ui.color_edit_button_rgb(&mut {path});",
            "        });",
        ]

    if rust == "[f32; 4]":
        return [
            "        ui.horizontal(|ui| {",
            f'            ui.label("{label}").on_hover_text("{hover}");',
            f"            ui.color_edit_button_rgba_unmultiplied(&mut {path});",
            "        });",
        ]

    if (vs := variants(rust)) is not None:
        ty = rust.split("::")[-1]
        use = "crate::app::axes::AxesStyle" if ty == "AxesStyle" else f"crate::app::config::{ty}"
        lines = [
            "        ui.horizontal(|ui| {",
            f'            ui.label("{label}").on_hover_text("{hover}");',
            f'            egui::ComboBox::from_id_salt("{path}")',
            f"                .selected_text(format!(\"{{:?}}\", {path}))",
            "                .show_ui(ui, |ui| {",
        ]
        for v in vs:
            lines.append(
                f'                    ui.selectable_value(&mut {path}, {use}::{v}, "{v}");'
            )
        lines += ["                });", "        });"]
        return lines

    # Anything left -- a colormap table, say -- has no obvious widget. Show
    # the path so the panel is honest about what it does not cover, and so
    # `tests/test_config_panel.py` can still see the field is accounted for.
    return [
        f'        ui.label(egui::RichText::new("{label}: set from a script").weak())',
        f'            .on_hover_text("{path} -- {hover}");',
    ]


def main() -> int:
    src = SOURCE.read_text()
    grouped: dict[str, list[str]] = {}

    for name, rust, doc in fields("Config", src):
        lines = widget(f"c.{name}", name, rust, doc)
        if lines:
            grouped.setdefault(group_of(name, doc), []).extend(lines)

    # The colour bar is a struct of its own; flatten it into its own group.
    for name, rust, doc in fields("Colorbar", src):
        lines = widget(f"c.colorbar.{name}", name, rust, doc)
        if lines:
            grouped.setdefault("Colour bar", []).extend(lines)

    out = [
        "// Generated by tools/gen_config_panel.py -- do not edit by hand.",
        "//",
        "// Regenerate after changing `Config`:  python tools/gen_config_panel.py",
        "",
        "use crate::app::config::Config;",
        "",
        "/// Every option on the simulation's config, grouped.",
        "///",
        "/// Written straight into the live config, so a change takes effect on",
        "/// the next frame -- including the ones that rebuild a pipeline or",
        "/// reallocate the shadow map, which `App::apply_live_config` notices.",
        "pub fn config_panel(ui: &mut egui::Ui, c: &mut Config) {",
    ]
    for group in GROUP_ORDER + sorted(set(grouped) - set(GROUP_ORDER)):
        if group not in grouped:
            continue
        out.append(f'    ui.collapsing("{group}", |ui| {{')
        out += grouped[group]
        out.append("    });")
    out.append("}")
    text = "\n".join(out) + "\n"

    if "--check" in sys.argv:
        current = TARGET.read_text() if TARGET.exists() else ""
        if current != text:
            print("config panel is out of date; run python tools/gen_config_panel.py")
            return 1
        print("config panel is current")
        return 0

    TARGET.parent.mkdir(parents=True, exist_ok=True)
    TARGET.write_text(text)
    n = sum(1 for line in text.splitlines() if "ui." in line)
    print(f"wrote {TARGET.relative_to(ROOT)}: {len(grouped)} groups, ~{n} widgets")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
