//! What the simulation currently *is*, beside what it is configured to be.
//!
//! Hand-written, unlike `config_panel`, because this is runtime state rather
//! than a struct of settings: a generator reading field names would have
//! nothing sensible to say about a `Mat4` or an `Option<Rc<RefCell<Mesh>>>`,
//! and the useful thing to show for a body is its facet count, not its
//! transform's sixteen numbers.

use crate::app::simulation::Simulation;
use crate::app::config::Config;
use crate::Float;
use super::config_panel::*;
use super::icons::codicon;
use super::theme::palette;
use super::widgets::{line, note, panel, section, setting, subheading};

/// The simulation tab's topics, a tab each down its navigation bar, as
/// Blender's properties editor has its contexts (the user: "in simulation tab
/// the sun/bodies/selection/camera/sun could be the left side vertical sub
/// tabs"): a title, named on hover; a Codicon in a colour; and whether a gap
/// sets it apart from the tab before, as Blender groups its tabs -- the run;
/// what is in the scene; how it is drawn; the rest.
pub const TOPICS: [(&str, &str, egui::Color32, bool); 15] = [
    ("Run", codicon::PLAY_CIRCLE, palette::GREEN, false),
    ("Bodies", codicon::CIRCLE_LARGE_FILLED, palette::PEACH, true),
    ("Selection", codicon::TARGET, palette::RED, false),
    ("Camera", codicon::DEVICE_CAMERA, palette::BLUE, false),
    ("Sun", codicon::STAR_FULL, palette::YELLOW, false),
    ("Shading", codicon::PAINTCAN, palette::MAUVE, true),
    ("Shadows", codicon::COLOR_MODE, palette::LAVENDER, false),
    ("Wireframe", codicon::LAYERS, palette::TEAL, false),
    ("Data colouring", codicon::SYMBOL_COLOR, palette::PINK, false),
    ("Axes & grid", codicon::MOVE, palette::SKY, false),
    ("HUD", codicon::TEXT_SIZE, palette::LAVENDER, false),
    ("Image", codicon::FILE_MEDIA, palette::SAPPHIRE, false),
    ("Controls", codicon::RECORD_KEYS, palette::FLAMINGO, true),
    ("Export", codicon::EXPORT, palette::GREEN, false),
    ("Debug", codicon::DEBUG, palette::MAROON, false),
];

fn topic_id() -> egui::Id {
    egui::Id::new("kalast simulation topic")
}

/// The topic whose tab is chosen, for the session; the first at first.
pub fn chosen_topic(ctx: &egui::Context) -> String {
    ctx.data_mut(|d| d.get_persisted::<String>(topic_id())).unwrap_or_else(|| TOPICS[0].0.to_owned())
}

/// The topics' tabs, down the navigation bar (`widgets::nav_bar`), the chosen
/// one's page drawn beside them. A setting revealed by the search
/// (`widgets::reveal`) chooses its own; while the settings are searched, a
/// tab with nothing that matches is faint, and the one chosen, if it has
/// nothing, gives way to the first that has, as in Blender's properties
/// editor.
pub fn topic_tabs(ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    let mut chosen = chosen_topic(&ctx);
    if let Some(first) = super::widgets::revealing(&ctx).filter(|place| !place.app).and_then(|place| place.path.first().cloned()) {
        chosen = first;
    }
    let matches: Vec<Option<u32>> = TOPICS.iter().map(|(title, ..)| super::widgets::tab_matches(&ctx, title)).collect();
    let here = TOPICS.iter().position(|(title, ..)| *title == chosen);
    if here.is_none_or(|i| matches[i] == Some(0)) {
        if let Some(j) = matches.iter().position(|m| m.is_some_and(|m| m > 0)) {
            chosen = TOPICS[j].0.to_owned();
        }
    }
    for (i, (title, icon, color, gap)) in TOPICS.iter().enumerate() {
        if *gap {
            ui.add_space(8.0);
        }
        let dimmed = matches[i] == Some(0);
        if super::widgets::nav_tab(ui, chosen == *title, icon, *color, dimmed).on_hover_text(*title).clicked() {
            chosen = (*title).to_owned();
        }
    }
    ctx.data_mut(|d| d.insert_persisted(topic_id(), chosen));
}

/// A topic's page: its panels, drawn while its tab is chosen
/// (`widgets::tab_page`), under ids of its own, so that two topics' panels
/// of the same name keep their own state.
fn topic(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    debug_assert!(TOPICS.iter().any(|(t, ..)| *t == title), "{title} has no tab");
    let selected = chosen_topic(ui.ctx()) == title;
    ui.push_id(("topic", title), |ui| super::widgets::tab_page(ui, title, selected, add));
}

/// A readout, laid out as a setting is, so a value and a control line up: in
/// the panels' font, as Blender writes what cannot be edited, cut short with
/// an ellipsis rather than past the panel's edge. Returns the value's
/// response, for the rows that want a hover on it.
fn row(ui: &mut egui::Ui, label: &str, value: impl Into<String>) -> egui::Response {
    setting(ui, label, "", |ui| ui.add(egui::Label::new(value.into()).truncate()))
}

/// Three drag fields for a vector, returning whether any changed: Blender's
/// column of them, "pos X", "Y", "Z" against one block of three fields, its
/// corners round only at its top and its bottom.
fn vec3(ui: &mut egui::Ui, label: &str, hover: &str, v: &mut crate::Vec3, speed: f64) -> bool {
    let values = vec![("X", &mut v.x), ("Y", &mut v.y), ("Z", &mut v.z)];
    super::widgets::components(ui, label, hover, values, |d| d.speed(speed).max_decimals(3))
}

/// Where the file is, without the file: its name is already the header just
/// above, and a shape model's full path is routinely wider than the panel,
/// so spelling it out again both repeats itself and sets the panel's width
/// for every other row. The whole path is one hover away.
fn dir_row(ui: &mut egui::Ui, path: Option<&std::path::Path>) {
    let Some(path) = path else { return };
    let dir = path.parent().unwrap_or(std::path::Path::new("."));
    let dir = dir.to_string_lossy();
    row(ui, "in", if dir.is_empty() { ".".into() } else { dir })
        .on_hover_text(path.to_string_lossy());
}

/// A body's file name, or a stand-in for one built in memory.
/// What a body is called in the panel: its shape model's file name without
/// the extension. Every one of them is `.obj`, so the extension is four
/// characters that say nothing and push the part that identifies the model
/// out of the header.
fn body_name(body: &crate::app::body::Body) -> String {
    body.mesh
        .as_ref()
        .and_then(|m| {
            m.borrow()
                .path
                .as_ref()
                .and_then(|p| p.file_stem().map(|f| f.to_string_lossy().into_owned()))
        })
        .unwrap_or_else(|| "built in memory".to_string())
}

/// The whole path, for the header's hover: which of two decimations of one
/// model a body is comes from the directory as often as the name.
fn body_path(body: &crate::app::body::Body) -> String {
    body.mesh
        .as_ref()
        .and_then(|m| {
            m.borrow()
                .path
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "built in memory".to_string())
}

/// A string kept in egui's own memory, for the text fields this panel needs
/// but has nowhere to store: it is a function over the simulation, not a
/// struct with state of its own.
fn remembered(ui: &egui::Ui, id: egui::Id, initial: impl FnOnce() -> String) -> String {
    ui.data_mut(|d| d.get_temp::<String>(id))
        .unwrap_or_else(initial)
}

fn remember(ui: &egui::Ui, id: egui::Id, value: String) {
    ui.data_mut(|d| d.insert_temp(id, value));
}

/// The page's files a colour map can be read from, by their extension: the
/// page's own and the visitor's folder's, the examples and `res/` left out.
#[cfg(target_arch = "wasm32")]
fn colour_files() -> Vec<String> {
    let mut found = Vec::new();
    let mut folders: Vec<(std::path::PathBuf, usize)> = vec![(".".into(), 0)];
    while let Some((dir, depth)) = folders.pop() {
        let Ok(entries) = crate::fs::list(&dir) else { continue };
        for (name, is_dir) in entries {
            if name.starts_with('.') || name == "__pycache__" {
                continue;
            }
            let path = if depth == 0 { std::path::PathBuf::from(&name) } else { dir.join(&name) };
            if is_dir {
                if depth < 4 && !(depth == 0 && matches!(name.as_str(), "examples" | "res" | "out")) {
                    folders.push((path, depth + 1));
                }
            } else if matches!(path.extension().and_then(|e| e.to_str()), Some("csv" | "txt" | "dat" | "tsv")) {
                found.push(path.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    found.sort();
    found.truncate(40);
    found
}

/// Read a mesh without taking the process down with it.
///
/// `Mesh::load` unwraps its way through the file: a path that is not there,
/// or an `.obj` it cannot parse, is a panic. That is defensible for a script
/// -- it fails on the line that asked -- but not for a text field someone is
/// still typing into, where a half-finished path would end the session and
/// everything running in it.
fn try_load(path: &str, flat: bool) -> Result<crate::mesh::Mesh, String> {
    let p = std::path::Path::new(path);
    // Through `crate::fs`, which in the page is the page's own files:
    // `std`'s saw none of them, and `res/cube.obj` was not found there.
    if crate::fs::len(p).is_err() {
        return Err(format!("no such file: {path}"));
    }
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut mesh = crate::mesh::Mesh::load(p, |v| v);
        if flat {
            mesh.flatten();
        }
        mesh
    }))
    .map_err(|_| format!("could not read {path} as a mesh"))
}

/// A body added to the scene from the shape model at `path`, as the side
/// panel's "add body" adds one: for the search (`search.rs`) too.
pub(super) fn add_body(sim: &mut Simulation, path: &str, flat: bool) -> Result<(), String> {
    sim.add_mesh(try_load(path, flat)?, crate::Mat4::IDENTITY);
    sim.meshes_dirty = true;
    sim.light_for_viewing();
    Ok(())
}

/// The shapes kalast ships in `res/`, `(name, path)`, those this copy has --
/// the page carries some of them (`web/build.rs`) -- named as Blender names
/// its own: a cube, ico spheres and UV spheres, then the asteroids and the
/// crater planes.
pub(super) fn shapes() -> Vec<(String, String)> {
    let Ok(files) = crate::fs::list(std::path::Path::new("res")) else { return Vec::new() };
    let mut shapes: Vec<(u32, String, String)> = files
        .into_iter()
        .filter(|(name, folder)| !folder && name.ends_with(".obj"))
        .map(|(name, _)| {
            let (order, label) = shape_name(name.trim_end_matches(".obj"));
            (order, label, format!("res/{name}"))
        })
        .collect();
    shapes.sort();
    // Two of one name -- two crater planes -- told apart by their files.
    let named = |label: &str| shapes.iter().filter(|(_, l, _)| l == label).count();
    let twice: Vec<bool> = shapes.iter().map(|(_, label, _)| named(label) > 1).collect();
    shapes
        .into_iter()
        .zip(twice)
        .map(|((_, label, path), twice)| {
            let stem = path.trim_start_matches("res/").trim_end_matches(".obj");
            (if twice { format!("{label} ({stem})") } else { label }, path)
        })
        .collect()
}

/// A shipped shape's name, and where it goes in the list.
fn shape_name(stem: &str) -> (u32, String) {
    const ICO: [u32; 7] = [80, 320, 1280, 5120, 20480, 81920, 327680];
    const UV: [u32; 4] = [960, 3968, 15872, 63488];
    let level = |prefix: &str, most: usize| {
        stem.strip_prefix(prefix).and_then(|n| n.parse::<usize>().ok()).filter(|n| (1..=most).contains(n))
    };
    let facets = |n: u32| {
        let digits = n.to_string();
        let mut out = String::new();
        for (i, c) in digits.chars().enumerate() {
            if i > 0 && (digits.len() - i) % 3 == 0 {
                out.push(',');
            }
            out.push(c);
        }
        out
    };
    if let Some(n) = level("ico", ICO.len()) {
        return (10 + n as u32, format!("Ico sphere, {} facets", facets(ICO[n - 1])));
    }
    if let Some(n) = level("sph", UV.len()) {
        return (20 + n as u32, format!("UV sphere, {} facets", facets(UV[n - 1])));
    }
    match stem {
        "cube" => (0, "Cube".into()),
        "cube-tex" => (1, "Cube, textured".into()),
        s if s.starts_with("didymos") => (30, "Didymos, DART's model, 10,000 facets".into()),
        s if s.starts_with("dimorphos") => (31, "Dimorphos, DART's model, 10,000 facets".into()),
        // Its file's parameters are for its maker; the file is on hover.
        s if s.starts_with("plane_crater_") => (40, "Crater in a plane".into()),
        s => (50, s.to_string()),
    }
}

/// The colour map: a built-in chosen by name, reversed, or read from a file
/// -- `data.colormap` is a table, which the generated widgets leave alone
/// (`:skip:`). The list shows the name the table was made from, `_r` if
/// reversed, found by comparing it with the built-ins, and a strip its
/// colours.
fn colormap_ui(ui: &mut egui::Ui, c: &mut Config) {
    use crate::app::config::{builtin_colormap, colormap_from_file, colormap_name, colormap_names};
    let id = ui.id().with("colormap");
    let table = &c.data.colormap;
    let name = colormap_name(table);
    let shown = name.clone().unwrap_or_else(|| format!("custom, {} colours", table.len()));
    let mut chosen: Option<Vec<[f32; 3]>> = None;

    setting(
        ui,
        "colormap",
        "data.colormap -- the colours the values are drawn in: a built-in, reversed with _r after its name, or a table from a script or a file",
        |ui| {
            // matplotlib's, each with its colours beside its name.
            super::widgets::ComboBox::from_id_salt(id).selected_text(&shown).height(360.0).show_ui(ui, |ui| {
                for n in colormap_names() {
                    // Its swatch and its name, a row shown only if the name
                    // matches what the menu is searched for.
                    if !super::widgets::in_menu_search(ui, n) {
                        continue;
                    }
                    let current = name.as_deref().is_some_and(|m| m.strip_suffix("_r").unwrap_or(m) == n);
                    let table = builtin_colormap(n).unwrap_or_default();
                    ui.horizontal(|ui| {
                        let (rect, _) = ui.allocate_exact_size(egui::vec2(48.0, 10.0), egui::Sense::hover());
                        gradient(ui, rect, &table, 24);
                        if super::widgets::choice_label(ui, current, n).clicked() {
                            chosen = Some(table.clone());
                        }
                    });
                }
            });
        },
    );
    line(ui, |ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 12.0), egui::Sense::hover());
        gradient(ui, rect, table, 96);
    });
    line(ui, |ui| {
        if ui.button("reverse").on_hover_text("The same colours the other way round").clicked() {
            let mut reversed = if table.is_empty() { builtin_colormap("grey").unwrap_or_default() } else { table.clone() };
            reversed.reverse();
            chosen = Some(reversed);
        }
        let load = ui.button("load file...").on_hover_text(
            "A text file with a colour a line -- red, green and blue, in 0..1 or 0..255, apart by commas or spaces",
        );
        if load.clicked() {
            #[cfg(not(target_arch = "wasm32"))]
            let picked = rfd::FileDialog::new()
                .set_title("A colour map")
                .add_filter("colours", &["csv", "txt", "dat", "tsv"])
                .pick_file();
            // The page has no file dialog: the button opens, under it, the
            // colour files among the page's own -- the visitor's folder's
            // with them -- and a click on one loads it.
            #[cfg(target_arch = "wasm32")]
            let picked: Option<std::path::PathBuf> = {
                let open = remembered(ui, id.with("pick"), String::new).is_empty();
                remember(ui, id.with("pick"), if open { "open".into() } else { String::new() });
                None
            };
            if let Some(path) = picked {
                match colormap_from_file(&path) {
                    Ok(t) => {
                        chosen = Some(t);
                        remember(ui, id.with("error"), String::new());
                    }
                    Err(e) => remember(ui, id.with("error"), e),
                }
            }
        }
    });
    #[cfg(target_arch = "wasm32")]
    if !remembered(ui, id.with("pick"), String::new).is_empty() {
        let files = colour_files();
        if files.is_empty() {
            line(ui, |ui| {
                ui.weak("no colour file here: put one (.csv, .txt, .dat, .tsv) in the folder the page works in")
            });
        }
        for path in files {
            let clicked = line(ui, |ui| ui.link(&path).clicked());
            if clicked {
                match colormap_from_file(std::path::Path::new(&path)) {
                    Ok(t) => {
                        chosen = Some(t);
                        remember(ui, id.with("error"), String::new());
                        remember(ui, id.with("pick"), String::new());
                    }
                    Err(e) => remember(ui, id.with("error"), e),
                }
            }
        }
    }
    let error = remembered(ui, id.with("error"), String::new);
    if !error.is_empty() {
        line(ui, |ui| ui.colored_label(egui::Color32::from_rgb(220, 120, 120), error));
    }
    if let Some(t) = chosen {
        c.data.colormap = t;
    }
}

/// A colour table's colours across `rect`, in `steps` slices, each the
/// table's row nearest -- no resampling, drawn every frame a list is open.
/// Empty is the renderer's greyscale.
fn gradient(ui: &egui::Ui, rect: egui::Rect, table: &[[f32; 3]], steps: usize) {
    let w = rect.width() / steps as f32;
    for i in 0..steps {
        let t = (i as f32 + 0.5) / steps as f32;
        let c = match table.len() {
            0 => [t, t, t],
            n => table[((t * (n - 1) as f32).round() as usize).min(n - 1)],
        };
        let slice = egui::Rect::from_min_size(rect.min + egui::vec2(i as f32 * w, 0.0), egui::vec2(w + 0.5, rect.height()));
        ui.painter().rect_filled(slice, 0.0, color32(c));
    }
}

/// The bodies in the scene, and the means to change which ones they are.
fn bodies_ui(ui: &mut egui::Ui, sim: &mut Simulation) {
    if sim.bodies.is_empty() {
        note(ui, "none loaded");
    }

    // Both are applied after the loop: removing a body while iterating over
    // them shifts every index behind it.
    let mut remove = None;
    let mut dirty = false;

    for i in 0..sim.bodies.len() {
        let name = body_name(&sim.bodies[i]);
        let path = body_path(&sim.bodies[i]);
        let mut drop_it = false;
        section(ui, &format!("{i}  {name}"), ("body", i), |ui| {
            dirty |= body_ui(ui, i, &mut sim.bodies[i]);
            line(ui, |ui| {
                if super::widgets::button(ui, "remove").on_hover_text("Take this body out of the scene").clicked() {
                    drop_it = true;
                }
            });
        })
        .on_hover_text(&path);
        if drop_it {
            remove = Some(i);
        }
    }

    if let Some(i) = remove {
        sim.bodies.remove(i);
        dirty = true;
    }

    // Adding one: a shape kalast ships, chosen from a list as Blender's Add
    // menu offers its cube and spheres, or a file of one's own by its path.
    panel(ui, "Add a body", |ui| add_body_ui(ui, sim, &mut dirty));

    sim.meshes_dirty |= dirty;
}

/// The Bodies tab's means of adding one (`bodies_ui`).
fn add_body_ui(ui: &mut egui::Ui, sim: &mut Simulation, dirty: &mut bool) {
    let id = ui.id().with("add");
    let shapes = shapes();
    let mut shape = remembered(ui, id.with("shape"), || shapes.first().map(|(_, p)| p.clone()).unwrap_or_default());
    let mut path = remembered(ui, id, String::new);
    let mut flat = ui.data_mut(|d| d.get_temp::<bool>(id.with("flat"))).unwrap_or(true);
    setting(ui, "shape", "One of the shape models kalast ships, or a file of yours", |ui| {
        let shown = shapes.iter().find(|(_, p)| *p == shape).map_or("a file of yours...", |(label, _)| label.as_str());
        super::widgets::ComboBox::from_id_salt(id.with("shapes")).selected_text(shown).show_ui(ui, |ui| {
            for (label, file) in &shapes {
                super::widgets::choice(ui, &mut shape, file.clone(), label.as_str()).on_hover_text(file.as_str());
            }
            super::widgets::choice(ui, &mut shape, String::new(), "a file of yours...");
        })
    });
    if shape.is_empty() {
        setting(ui, "path", "The shape model to load, an .obj", |ui| {
            super::widgets::text_field(ui, egui::TextEdit::singleline(&mut path).hint_text("path to an .obj").desired_width(f32::INFINITY))
        });
    }
    super::widgets::toggle(ui, "flat", "Give every facet its own vertices, as `flatten=True` does", &mut flat);
    line(ui, |ui| {
        let from = if shape.is_empty() { path.clone() } else { shape.clone() };
        if super::widgets::button(ui, "add body").clicked() && !from.is_empty() {
            match add_body(sim, &from, flat) {
                Ok(()) => {
                    *dirty = true;
                    remember(ui, id.with("error"), String::new());
                    path.clear();
                }
                Err(e) => remember(ui, id.with("error"), e),
            }
        }
    });
    let error = remembered(ui, id.with("error"), String::new);
    if !error.is_empty() {
        line(ui, |ui| ui.colored_label(egui::Color32::from_rgb(220, 120, 120), error));
    }
    remember(ui, id, path);
    remember(ui, id.with("shape"), shape);
    ui.data_mut(|d| d.insert_temp(id.with("flat"), flat));
}

/// One body: which file it is, and the shape of what was loaded. Counts and
/// bounds rather than the arrays themselves -- 3.1M facets is not something
/// to put in a side panel, and the questions actually asked of a mesh here
/// are "did it flatten", "how big is it" and "does it carry values".
///
/// Returns whether the GPU buffers have to be rebuilt.
fn body_ui(ui: &mut egui::Ui, i: usize, body: &mut crate::app::body::Body) -> bool {
    let mut dirty = false;
    let Some(handle) = body.mesh.clone() else {
        note(ui, "no mesh");
        return false;
    };

    // The path is editable and takes effect on `reload`, so the same field
    // both says where this body came from and points it somewhere else. What
    // is typed is kept per body and per file: kept by place alone, a body
    // taking another's -- a script run again, one removed and another added
    // -- was shown the other's file, `reload` ready to load it over this one.
    let current = handle
        .borrow()
        .path
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let id = ui.id().with(("path", i, current.as_str()));
    let mut path = remembered(ui, id, || current.clone());
    setting(ui, "file", "Where the mesh came from: edit it and reload to point the body at another", |ui| {
        super::widgets::text_field(ui, egui::TextEdit::singleline(&mut path).desired_width(f32::INFINITY))
    });
    line(ui, |ui| {
        let changed = path != current;
        // Two halves when `revert` is there too, as Blender shares a row
        // between its buttons: after a `reload` as wide as the row, `revert`
        // was a sliver at its edge.
        let gap = ui.spacing().item_spacing.x;
        let width = if changed { ((ui.available_width() - gap) / 2.0).floor() } else { ui.available_width() };
        if ui
            .add_enabled_ui(changed, |ui| {
                ui.set_max_width(width);
                super::widgets::button(ui, "reload")
            })
            .inner
            .on_hover_text("Replace this body's mesh with the file above, keeping its transform")
            .clicked()
        {
            let flat = handle.borrow().is_flat();
            match try_load(&path, flat) {
                Ok(mesh) => {
                    // Written into the existing handle rather than swapped
                    // for a new one, so a Python `Mesh` holding this body
                    // keeps pointing at it.
                    *handle.borrow_mut() = mesh;
                    dirty = true;
                    remember(ui, id.with("error"), String::new());
                }
                Err(e) => remember(ui, id.with("error"), e),
            }
        }
        if changed
            && ui
                .scope(|ui| {
                    ui.set_max_width(width);
                    super::widgets::button(ui, "revert")
                })
                .inner
                .on_hover_text("Put this body's own file back in the field")
                .clicked()
        {
            path = current.clone();
        }
    });
    let error = remembered(ui, id.with("error"), String::new);
    if !error.is_empty() {
        line(ui, |ui| ui.colored_label(egui::Color32::from_rgb(220, 120, 120), error));
    }
    remember(ui, id, path);

    let mut mesh = handle.borrow_mut();
    dirty |= mesh_ui(ui, &mut mesh);

    // The values a colormap reads, if a script has written any. Their range
    // is the useful part: an all-zero column and a missing one look
    // identical in the render.
    if mesh.values.is_empty() {
        row(ui, "values", "none");
    } else {
        let lo = mesh.values.iter().copied().fold(Float::INFINITY, Float::min);
        let hi = mesh
            .values
            .iter()
            .copied()
            .fold(Float::NEG_INFINITY, Float::max);
        row(
            ui,
            "values",
            format!("{} facets, {lo:.4} to {hi:.4}", mesh.values.len()),
        );
    }
    drop(mesh);

    scattering_ui(ui, i, &mut body.scattering);
    atmosphere_ui(ui, i, &mut body.atmosphere);

    subheading(ui, "shadows");
    super::widgets::toggle(
        ui,
        "horizon_map",
        "The body's own shadows from how high its terrain rises round each facet, worked out once on the GPU \
         (seconds for millions of facets), instead of drawing it into its own shadow map every frame. For a \
         body each direction from whose centre crosses its surface once, with shadows.per_body on",
        &mut body.horizon_map,
    );

    if let Some(shadow) = body.shadow_mesh.as_ref() {
        let mut shadow = shadow.borrow_mut();
        subheading(ui, "shadow mesh");
        row(
            ui,
            "file",
            shadow
                .path
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_default(),
        );
        dir_row(ui, shadow.path.as_deref());
        dirty |= mesh_ui(ui, &mut shadow);
    }

    // Model to world, editable. Usually written by a script every frame, in
    // which case an edit here lasts one frame -- but for a scene that is
    // placed once, this is where to place it.
    subheading(ui, "mat")
        .on_hover_text("Model to world. A script that sets `body.mat` every iteration wins over this.");
    for r in 0..4 {
        line(ui, |ui| {
            // Four to a row, each a quarter of it, Blender's number fields.
            ui.spacing_mut().item_spacing.x = 2.0;
            let width = ((ui.available_width() - 6.0) / 4.0).max(28.0);
            for c in 0..4 {
                ui.allocate_ui(egui::vec2(width, 20.0), |ui| {
                    super::widgets::number(ui, &mut body.mat.col_mut(c)[r], |d| d.speed(0.01).max_decimals(4))
                });
            }
        });
    }

    dirty
}

/// The shadow settings for one use (`Quality`): a button each, the one the
/// settings are now picked out, its hover the fields it sets, a line each.
///
/// A row of its own across the panel, as many lines high as the buttons
/// wrap to. As a `setting` they stood in the control's column, a button a
/// line in a narrow panel, past the row's fixed height and over the
/// resolution and PCF rows below.
fn quality_ui(ui: &mut egui::Ui, c: &mut Config) {
    use crate::app::config::Quality;
    let now = c.quality();
    let hover = "The shadow settings for one use, from the quickest look to the most exact; \
                 each one's hover in the menu lists the fields it sets. \"custom\" when they are set otherwise.";
    setting(ui, "quality", hover, |ui| {
        super::widgets::ComboBox::from_id_salt("shadow quality")
            .selected_text(now.map_or("custom", |q| q.name()))
            .show_ui(ui, |ui| {
                for q in Quality::ALL {
                    if super::widgets::choice_label(ui, now == Some(q), q.name()).on_hover_text(q.hover()).clicked() {
                        c.set_quality(q);
                    }
                }
            });
    });
}

/// The shadows, where they show: in the shaded colour mode. The data mode is
/// unlit, and the settings would change nothing on screen --
/// except the per-facet shadows the thermophysical model reads, which follow
/// them whatever the image shows, so they stay while those are read every
/// frame (`access_shadow_map`).
fn shadows_ui(ui: &mut egui::Ui, sim: &mut Simulation, c: &mut Config) {
    let shaded = c.shading.color_mode == crate::app::config::ColorMode::Shaded;
    panel(ui, "Shadows", |ui| {
        if !shaded {
            if !c.shadows.access_shadow_map {
                note(ui, "unlit colour mode: shadows show in \"Shaded\"");
                return;
            }
            note(ui, "unlit image: these are the per-facet shadows'");
        }
        quality_ui(ui, c);
        group_shadows(ui, c);
    });
    // The reference image's own panel, under the method that makes one.
    if !shaded || c.shadows.method != crate::app::config::ShadowMethod::Reference {
        return;
    }
    panel(ui, "Reference image", |ui| {
        group_reference(ui, c);
        use crate::app::simulation::ReferenceAsk;
        let max = c.reference.max_samples;
        let r = &mut sim.reference;
        note(ui, &super::reference_words(r, max, c.reference.error));
        if !r.done {
            if line(ui, |ui| super::widgets::button(ui, "Stop here").on_hover_text("The mean so far is the image, done")).clicked() {
                r.ask = Some(ReferenceAsk::Stop);
            }
        } else if r.stopped
            && line(ui, |ui| ui.add_enabled_ui(r.samples < max, |ui| super::widgets::button(ui, "Go on")).inner)
                .on_hover_text("Sum on from where it was stopped")
                .clicked()
        {
            r.ask = Some(ReferenceAsk::Resume);
        }
        if !sim.rays {
            note(ui, "no ray queries on this GPU: shadows from the maps");
        }
    })
    .on_hover_text(
        "A still drawn as exactly as kalast can, ray traced and summed a sample of every pixel a frame \
         until its error is under `error`, then held: for a figure, or to check a faster method against. \
         Anything that moves starts it again.",
    );
}

/// How a body's surface reflects sunlight in the image (`body.scattering`):
/// Lambert, the Lommel-Seeliger/Lambert mix or Hapke, and its numbers. A law
/// chosen again comes back as it was; chosen first, it is its defaults. An
/// edit the law would refuse -- Hapke's roughness at 90 deg -- is not taken.
fn scattering_ui(ui: &mut egui::Ui, i: usize, law: &mut Option<crate::lightcurve::Law>) {
    use crate::lightcurve::Law;
    use crate::scattering::{Hapke, LommelSeeligerLambert};
    subheading(ui, "scattering");
    let mix_id = ui.id().with(("scattering mix", i));
    let hapke_id = ui.id().with(("scattering hapke", i));
    match *law {
        Some(Law::Mix(m)) => ui.data_mut(|d| {
            d.insert_temp(mix_id, m);
        }),
        Some(Law::Hapke(h)) => ui.data_mut(|d| {
            d.insert_temp(hapke_id, h);
        }),
        None => {}
    }
    let names = ["Lambert", "Lommel-Seeliger/Lambert", "Hapke"];
    let mut kind = match law {
        None => 0,
        Some(Law::Mix(_)) => 1,
        Some(Law::Hapke(_)) => 2,
    };
    setting(
        ui,
        "law",
        "How the surface reflects sunlight in the image, seen from where the camera is. Lambert: a lit pixel is the \
         colour times cos i. A law: the colour times the law's I/F, so a colour of 1 is the law's own albedo",
        |ui| {
            super::widgets::ComboBox::from_id_salt(("scattering law", i))
                .selected_text(names[kind])
                .show_ui(ui, |ui| {
                    for (k, name) in names.iter().enumerate() {
                        super::widgets::choice(ui, &mut kind, k, *name);
                    }
                })
        },
    );
    *law = match (kind, *law) {
        (0, _) => None,
        (1, Some(Law::Mix(m))) => Some(Law::Mix(m)),
        (1, _) => Some(Law::Mix(ui.data_mut(|d| d.get_temp::<LommelSeeligerLambert>(mix_id)).unwrap_or_default())),
        (_, Some(Law::Hapke(h))) => Some(Law::Hapke(h)),
        (_, _) => Some(Law::Hapke(ui.data_mut(|d| d.get_temp::<Hapke>(hapke_id)).unwrap_or_default())),
    };
    let number = |ui: &mut egui::Ui, name: &str, hover: &str, v: &mut Float, range: std::ops::RangeInclusive<f64>, speed: f64| {
        setting(ui, name, hover, |ui| super::widgets::number(ui, v, |d| d.range(range).speed(speed).max_decimals(4)));
    };
    match law {
        Some(Law::Mix(m)) => {
            number(ui, "w", "Single-scattering albedo", &mut m.w, 0.0..=1.0, 0.001);
            number(ui, "c", "Lommel-Seeliger's share: 1 pure Lommel-Seeliger, 0 pure Lambert", &mut m.c, 0.0..=1.0, 0.001);
        }
        Some(Law::Hapke(h)) => {
            let mut edit = *h;
            number(ui, "w", "Single-scattering albedo", &mut edit.w, 0.0..=1.0, 0.001);
            number(ui, "b", "The Henyey-Greenstein lobes' width", &mut edit.b, 0.0..=1.0, 0.001);
            number(ui, "c", "The backward lobe's share", &mut edit.c, 0.0..=1.0, 0.001);
            number(ui, "b0", "The opposition surge's amplitude", &mut edit.b0, 0.0..=10.0, 0.01);
            number(ui, "h", "The opposition surge's angular width, radians", &mut edit.h, 1e-6..=2.0, 0.001);
            // In degrees here, radians in scripts (`Hapke(theta_bar=...)`).
            let mut deg = edit.theta_bar.to_degrees();
            setting(
                ui,
                "theta_bar",
                "Macroscopic roughness, the mean slope of the relief within a facet, in degrees here (radians in a \
                 script): 0 a smooth surface, 20-30 for most asteroids",
                |ui| super::widgets::number(ui, &mut deg, |d| d.range(0.0..=89.9).speed(0.1).max_decimals(2).suffix(" deg")),
            );
            edit.theta_bar = deg.to_radians();
            number(ui, "k", "Hapke's porosity factor, 1 for none: 1.19 for Phobos, 1.21 for Deimos", &mut edit.k, 1.0..=10.0, 0.001);
            if edit.check().is_ok() {
                *h = edit;
            }
        }
        None => {}
    }
}

/// The atmosphere over a body's surface in the image (`body.atmosphere`): on
/// or off, and its numbers. Turned off and on again it comes back as it was;
/// on for the first time, it is Mars's, in km. An edit the atmosphere would
/// refuse -- a lobe's asymmetry of 1 -- is not taken.
fn atmosphere_ui(ui: &mut egui::Ui, i: usize, atmosphere: &mut Option<crate::atmosphere::Atmosphere>) {
    use crate::atmosphere::Atmosphere;
    subheading(ui, "atmosphere");
    let kept = ui.id().with(("atmosphere", i));
    let mut on = atmosphere.is_some();
    super::widgets::toggle(
        ui,
        "on",
        "A dusty atmosphere over the surface in the image: the dust's own light, and the surface seen \
         through it, lit by the beam that got through and by the sky. Mars's, in km, when first turned on",
        &mut on,
    );
    match (on, atmosphere.take()) {
        (true, None) => *atmosphere = Some(ui.data_mut(|d| d.get_temp::<Atmosphere>(kept)).unwrap_or_default()),
        (false, Some(was)) => ui.data_mut(|d| {
            d.insert_temp(kept, was);
        }),
        (_, was) => *atmosphere = was,
    };
    let Some(air) = atmosphere.as_mut() else {
        return;
    };
    let mut edit = *air;
    let number = |ui: &mut egui::Ui, name: &str, hover: &str, v: &mut Float, range: std::ops::RangeInclusive<f64>, speed: f64| {
        setting(ui, name, hover, |ui| super::widgets::number(ui, v, |d| d.range(range).speed(speed).max_decimals(4)));
    };
    number(ui, "tau", "Vertical optical depth of the dust at the surface", &mut edit.tau, 0.0..=10.0, 0.01);
    number(ui, "scale_height", "The dust's scale height, in the scene's units", &mut edit.scale_height, 1e-6..=1e12, 0.1);
    number(ui, "radius", "The planet's equatorial radius, in the scene's units", &mut edit.radius, 1e-6..=1e12, 1.0);
    let mut ellipsoid = edit.polar_radius.is_some();
    setting(ui, "polar_radius", "The ellipsoid's polar radius, about the body's own z axis; off, a sphere of `radius`", |ui| {
        super::widgets::checkbox(ui, &mut ellipsoid, "");
        edit.polar_radius = ellipsoid.then(|| {
            let mut c = edit.polar_radius.unwrap_or(edit.radius);
            super::widgets::number(ui, &mut c, |d| d.range(1e-6..=1e12).speed(1.0).max_decimals(4));
            c
        });
    });
    number(ui, "omega", "The dust's single-scattering albedo", &mut edit.omega, 0.0..=1.0, 0.001);
    number(ui, "g1", "The asymmetry of the phase function's first lobe", &mut edit.g1, -0.999..=0.999, 0.001);
    number(ui, "g2", "The asymmetry of its second lobe", &mut edit.g2, -0.999..=0.999, 0.001);
    number(ui, "q", "The first lobe's weight", &mut edit.q, 0.0..=1.0, 0.001);
    let mut fixed = edit.albedo.is_some();
    setting(
        ui,
        "albedo",
        "The surface's mean albedo round about, for the light the surface and the dust send each other; off, each facet's own",
        |ui| {
            super::widgets::checkbox(ui, &mut fixed, "");
            edit.albedo = fixed.then(|| {
                let mut v = edit.albedo.unwrap_or(0.2);
                super::widgets::number(ui, &mut v, |d| d.range(0.0..=1.0).speed(0.001).max_decimals(4));
                v
            });
        },
    );
    if edit.check().is_ok() {
        *air = edit;
    }
}

/// Counts, shading and extent for one mesh. Returns whether it was changed
/// in a way the GPU buffers have to follow.
fn mesh_ui(ui: &mut egui::Ui, mesh: &mut crate::mesh::Mesh) -> bool {
    // One number per row, for the same reason the bounds are split: the
    // widest row in a side panel is the panel's width.
    row(ui, "facets", mesh.facets.len().to_string());
    row(ui, "vertices", mesh.positions.len().to_string());
    row(ui, "indices", mesh.indices.len().to_string());

    let was = mesh.is_flat();
    let mut flat = was;
    setting(
        ui,
        "shading",
        "Flat gives every facet its own three vertices instead of sharing \
         corners with its neighbours, so each shades as a plate and a \
         per-facet value colours exactly one triangle. What flatten=True asks for.",
        |ui| super::widgets::segmented(ui, &mut flat, &[(true, "flat"), (false, "smooth")]),
    );
    let changed = flat != was;
    if changed {
        if flat {
            mesh.flatten();
        } else {
            mesh.smoothen();
        }
    }

    let b = &mesh.bounds;
    row(ui, "min", format!("{:.3} {:.3} {:.3}", b.min.x, b.min.y, b.min.z));
    row(ui, "max", format!("{:.3} {:.3} {:.3}", b.max.x, b.max.y, b.max.z));
    let e = b.max - b.min;
    row(ui, "extent", format!("{:.3} {:.3} {:.3}", e.x, e.y, e.z));
    if let Some(id) = mesh.material_id {
        row(ui, "material", id.to_string());
    }

    changed
}

fn eye_ui(ui: &mut egui::Ui, eye: &mut crate::app::frame::Eye, bodies: &[String], is_sun: bool) {
    vec3(ui, "pos", "Where it stands, in world coordinates", &mut eye.pos, 0.05);

    // The Sun's frame is not a camera anyone reframes: it looks at the scene
    // from wherever `pos` puts it, orthographically, and its anchor follows
    // the geometry. Showing those as settings would only invite changing
    // something that is not a choice.
    if !is_sun {
        // Direction and up must stay unit vectors -- a short one panics the
        // renderer, inside a callback that cannot unwind -- so they are
        // renormalised on every edit rather than trusted.
        if vec3(ui, "dir", "Where it looks; kept a unit vector", &mut eye.dir, 0.01) {
            eye.dir = eye.dir.normalize_or_zero();
        }
        if vec3(ui, "up", "Which way is up on screen; kept a unit vector", &mut eye.up, 0.01) {
            eye.up = eye.up.normalize_or_zero();
        }

        vec3(ui, "anchor", "The point it turns about and zooms toward", &mut eye.anchor, 0.05);

        setting(ui, "anchor body", "A body whose centre the anchor follows", |ui| {
            let selected = match eye.anchor_body {
                Some(i) => bodies
                    .get(i)
                    .map(|n| format!("{i}  {n}"))
                    .unwrap_or_else(|| i.to_string()),
                None => "none".to_string(),
            };
            super::widgets::ComboBox::from_id_salt("anchor_body")
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    super::widgets::choice(ui, &mut eye.anchor_body, None, "none");
                    for (i, name) in bodies.iter().enumerate() {
                        super::widgets::choice(ui, &mut eye.anchor_body, Some(i), format!("{i}  {name}"));
                    }
                });
        });
    }

    let p = &mut eye.projection;

    if !is_sun {
        setting(ui, "mode", "Perspective, or orthographic: parallel rays, no vanishing point", |ui| {
            super::widgets::ComboBox::from_id_salt("mode")
                .selected_text(format!("{:?}", p.mode))
                .show_ui(ui, |ui| {
                    use crate::app::frame::ProjectionMode::*;
                    super::widgets::choice(ui, &mut p.mode, Perspective, "Perspective");
                    super::widgets::choice(ui, &mut p.mode, Orthographic, "Orthographic");
                });
        });

        // An orthographic frustum has no field of view -- `side` is its
        // width -- so a dead angle here would only invite editing it.
        if p.mode == crate::app::frame::ProjectionMode::Perspective {
            setting(ui, "fovy", "The vertical field of view, spanning the window's height", |ui| {
                let mut deg = p.fovy.to_degrees();
                if super::widgets::number(ui, &mut deg, |d| d.speed(0.2).suffix("\u{b0}")).changed()
                {
                    p.fovy = deg.to_radians();
                }
            });
        }
    }

    // Each plane is either pinned to a number or fitted to the scene every
    // frame -- but for a camera's `side`, which follows where the camera
    // stands (`Eye::fit_projection`). The tick is which of the two, and the
    // value beside it is what is actually in the matrix either way -- untick
    // and it goes back to following, from the number it was last fitted to.
    let fitted = p.resolved();
    let fitted_to_scene = "Pin this plane. Unticked, it is fitted to the scene every frame.";
    let side_hover = if is_sun {
        fitted_to_scene
    } else {
        "Pin the orthographic half-height. Unticked, it is the perspective view's at the anchor."
    };
    for (name, field, value, hover) in [
        ("near", &mut p.near, fitted.near, fitted_to_scene),
        ("far", &mut p.far, fitted.far, fitted_to_scene),
        ("side", &mut p.side, fitted.side, side_hover),
    ] {
        setting(ui, name, hover, |ui| {
            let mut pinned = field.is_some();
            if super::widgets::checkbox(ui, &mut pinned, "").changed() {
                *field = pinned.then_some(value);
            }
            match field {
                Some(v) => {
                    super::widgets::number(ui, v, |d| d.speed(0.01));
                }
                None => {
                    // The value fitted, in a field shown disabled, as
                    // Blender shows one that cannot be edited: at half its
                    // colours.
                    let mut shown = value;
                    ui.add_enabled_ui(false, |ui| super::widgets::number(ui, &mut shown, |d| d.speed(0.01)));
                }
            }
        });
    }
}

/// The ground grid, shown or hidden, and its settings while it is shown. It is
/// the `"blender"` axes style's -- Blender's viewport floor, the gizmo with it
/// -- so `show` switches the style between that and the gizmo alone:
/// `grid.enabled`, which this was taken for, picks a shaded grid or segments,
/// a grid either way, and nothing under the default gizmo (the user:
/// "toggling the grid doesnt seem to do anything anymore").
fn grid_ui(ui: &mut egui::Ui, c: &mut Config) {
    use crate::app::axes::AxesStyle;
    let mut shown = c.axes.style == AxesStyle::Blender;
    let hover = "The ground grid, as Blender's viewport floor: axes.style \"blender\" when ticked, \"gizmo\" when not";
    if super::widgets::toggle(ui, "show", hover, &mut shown).changed() {
        c.axes.style = if shown { AxesStyle::Blender } else { AxesStyle::Gizmo };
    }
    if shown {
        group_grid(ui, c);
    }
}

/// The text overlays: what each says, where it sits, how it looks.
///
/// `text` is the *template*, not the line on screen -- `{it}`, `{fps}` and
/// the rest are filled in every frame. A script that assigns `hud.text`
/// each iteration owns it, and an edit here lasts until its next
/// assignment; the expanded result is shown underneath so both are visible.
fn huds_ui(ui: &mut egui::Ui, sim: &mut Simulation) {
    use crate::app::config::{HAlign, Hud, HudAnchor, VAlign};

    if sim.huds.is_empty() {
        note(ui, "none");
    }

    let mut remove = None;
    for (i, handle) in sim.huds.iter().enumerate() {
        let mut hud = handle.borrow_mut();
        section(ui, &format!("hud {i}"), ("hud", i), |ui| {
                // Typing here takes the HUD off the script: what is typed
                // goes in `pin`, which the frame uses in place of `text`.
                // Without that an edit cannot survive at all -- a callback
                // assigns `text` every iteration, and a *driven* script goes
                // on assigning even while paused, since pausing stops the
                // iteration counter and not a `while` loop the script owns.
                let pinned = hud.pin.is_some();
                let mut buf = hud.pin.clone().unwrap_or_else(|| hud.text.clone());
                ui.add_space(2.0);
                let edited = ui
                    .add(
                        egui::TextEdit::multiline(&mut buf)
                            .desired_rows(1)
                            .desired_width(f32::INFINITY)
                            .font(egui::TextStyle::Monospace),
                    )
                    .on_hover_text(
                        "Template. {it} {drawn} {nit} {its} {fps} {ms} {bodies} {paused} {warn} {gpu}, \
                         with an optional precision as {fps:.1}. Typing takes this HUD \
                         off the script.",
                    )
                    .changed();
                if edited || pinned {
                    hud.pin = Some(buf);
                }

                ui.add_space(2.0);
                if hud.pin.is_some() {
                    line(ui, |ui| {
                        ui.label(
                            egui::RichText::new("pinned here, not the script")
                                .weak()
                                .small(),
                        );
                        if ui
                            .small_button("release")
                            .on_hover_text("Give the HUD back to the script")
                            .clicked()
                        {
                            hud.pin = None;
                        }
                    });
                }

                // What that template comes out as this frame, but only when
                // the two differ. A dim copy of the field directly under the
                // field reads as a second field, and clicking it does
                // nothing.
                let template = hud.pin.as_deref().unwrap_or(&hud.text);
                let shown = crate::app::expand_hud(
                    template,
                    &sim.state,
                    0.0,
                    &sim.diagnostics,
                    sim.state.iteration,
                );
                if shown != *template {
                    line(ui, |ui| {
                        ui.label(egui::RichText::new("shows as").weak().small());
                        ui.label(egui::RichText::new(shown).monospace().small());
                    });
                }

                setting(ui, "anchor", "The corner, edge or centre of the image it is placed against", |ui| {
                    super::widgets::ComboBox::from_id_salt("anchor")
                        .selected_text(format!("{:?}", hud.anchor))
                        .show_ui(ui, |ui| {
                            for a in [
                                HudAnchor::TopLeft,
                                HudAnchor::TopCenter,
                                HudAnchor::TopRight,
                                HudAnchor::MiddleLeft,
                                HudAnchor::MiddleCenter,
                                HudAnchor::MiddleRight,
                                HudAnchor::BottomLeft,
                                HudAnchor::BottomCenter,
                                HudAnchor::BottomRight,
                            ] {
                                let label = format!("{a:?}");
                                super::widgets::choice(ui, &mut hud.anchor, a, label);
                            }
                        });
                });
                let h = &mut *hud;
                let inset = vec![("X", &mut h.x), ("Y", &mut h.y)];
                super::widgets::components(ui, "inset", "Pixels from the anchor, across and down", inset, |d| d.speed(1.0));
                setting(ui, "size", "Letter height in pixels", |ui| {
                    super::widgets::number(ui, &mut hud.size, |d| d.speed(0.5).range(1.0..=200.0).clamp_existing_to_range(false))
                });
                setting(ui, "color", "The letters' colour", |ui| {
                    // Through a copy: egui's button writes its HSV round
                    // trip back every frame, changing what a script set.
                    let mut color = hud.color;
                    if super::widgets::color_rgba(ui, &mut color).changed() {
                        hud.color = color;
                    }
                });

                // `None` means "follow the anchor", which is what a HUD
                // wants unless it is being pinned somewhere unusual -- so
                // the default stays reachable rather than being overwritten
                // the moment the picker is touched.
                setting(ui, "align", "Which of its edges sits on the anchor; \"anchor\" follows it", |ui| {
                    // Two lists side by side, halving the row.
                    ui.spacing_mut().combo_width = (ui.available_width() - 8.0) / 2.0;
                    super::widgets::ComboBox::from_id_salt("align_h")
                        .selected_text(match hud.align_h {
                            Some(a) => format!("{a:?}"),
                            None => "anchor".to_string(),
                        })
                        .show_ui(ui, |ui| {
                            super::widgets::choice(ui, &mut hud.align_h, None, "anchor");
                            for a in [HAlign::Left, HAlign::Center, HAlign::Right] {
                                let label = format!("{a:?}");
                                super::widgets::choice(ui, &mut hud.align_h, Some(a), label);
                            }
                        });
                    super::widgets::ComboBox::from_id_salt("align_v")
                        .selected_text(match hud.align_v {
                            Some(a) => format!("{a:?}"),
                            None => "anchor".to_string(),
                        })
                        .show_ui(ui, |ui| {
                            super::widgets::choice(ui, &mut hud.align_v, None, "anchor");
                            for a in [VAlign::Top, VAlign::Center, VAlign::Bottom] {
                                let label = format!("{a:?}");
                                super::widgets::choice(ui, &mut hud.align_v, Some(a), label);
                            }
                        });
                });

                line(ui, |ui| {
                    if super::widgets::button(ui, "remove").clicked() {
                        remove = Some(i);
                    }
                });
        });
    }

    if let Some(i) = remove {
        sim.huds.remove(i);
    }

    if line(ui, |ui| super::widgets::button(ui, "add HUD").clicked()) {
        // A template rather than empty text, so a new HUD says something the
        // moment it appears and shows what a placeholder looks like.
        sim.huds.push(crate::locked::Locked::new(Hud::new(
            "it={drawn}  {fps} fps",
        )));
    }
}

/// A selected facet's value: four decimals where they read, else in powers
/// of ten.
fn format_value(v: f32) -> String {
    let a = v.abs();
    if !v.is_finite() || a == 0.0 || (1e-3..1e6).contains(&a) {
        format!("{v:.4}")
    } else {
        format!("{v:.4e}")
    }
}

/// A colour of `0..1` channels, as the renderer stores them -- sRGB.
fn color32(c: [f32; 3]) -> egui::Color32 {
    let b = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
    egui::Color32::from_rgb(b(c[0]), b(c[1]), b(c[2]))
}

fn swatch(ui: &mut egui::Ui, c: [f32; 3]) -> egui::Response {
    egui::widgets::color_picker::show_color(ui, color32(c), egui::vec2(14.0, 14.0))
}

/// The picked facets: what is selected -- each with its value, the colour the
/// colormap gives it and its own -- and the two ways to change it that a
/// pointer cannot do: clearing the lot, and naming one by index.
// The config is handed in, not read from `sim.config`: the panels are drawn
// with that `RefCell` borrowed mutably, and reading it here crashed the UI app
// the moment the Selection header opened.
fn selection_ui(ui: &mut egui::Ui, sim: &mut Simulation, config: &Config, color: crate::Vec3) {
    note(ui, "click a facet in the scene to select it; click it again to drop it");

    if sim.selected_facets.is_empty() {
        note(ui, "none selected");
    }

    // Applied after the loop: dropping one mid-iteration shifts the rest.
    let mut drop_it = None;
    let many = sim.bodies.len() > 1;
    // What a value's colour is read over: the range of the frame drawn while
    // the data map is shown, the one it would be drawn with otherwise.
    let mut range = None;
    for (i, s) in sim.selected_facets.iter().enumerate() {
        line(ui, |ui| {
            let name = if many {
                format!("body {} facet {}", s.body, s.facet)
            } else {
                format!("facet {}", s.facet)
            };
            ui.label(egui::RichText::new(name).monospace());
            if ui.small_button("\u{00d7}").on_hover_text("Deselect").clicked() {
                drop_it = Some(i);
            }
        });
        // Read every frame: a script writing `mesh.values` as it runs is
        // followed.
        let value = sim
            .bodies
            .get(s.body)
            .and_then(|b| b.mesh.as_ref())
            .and_then(|m| m.borrow().values.get(s.facet).copied());
        if let Some(v) = value {
            let v = v as f32;
            setting(ui, "value", "Its entry in mesh.values, as the script last set it", |ui| {
                ui.label(egui::RichText::new(format_value(v)).monospace());
            });
            if v.is_finite() {
                let r = *range.get_or_insert_with(|| sim.color_range_with(config));
                let c = crate::app::config::colormap_color(&config.data.colormap, r, v);
                let hover = format!(
                    "Its colour in the colormap, red, green and blue from 0 to 1, over values {} to {}",
                    format_value(r.0),
                    format_value(r.1)
                );
                setting(ui, "colormap", &hover, |ui| {
                    swatch(ui, c);
                    ui.label(egui::RichText::new(crate::app::config::rgb(c)).monospace());
                });
            }
        }
        if let Some(c) = s.color() {
            setting(ui, "colour", "Its own colour under the selection's, red, green and blue from 0 to 1", |ui| {
                swatch(ui, c);
                ui.label(egui::RichText::new(crate::app::config::rgb(c)).monospace());
            });
        }
    }
    if let Some(i) = drop_it {
        let (body, facet) = {
            let s = &sim.selected_facets[i];
            (s.body, s.facet)
        };
        sim.toggle_facet(body, facet, color);
    }

    line(ui, |ui| {
        if ui
            .add_enabled(
                !sim.selected_facets.is_empty(),
                egui::Button::new("clear all"),
            )
            .on_hover_text("Put every selected facet back to the colour it had")
            .clicked()
        {
            sim.clear_selection();
        }

        // By index, for a facet found in a data product rather than on
        // screen -- and for one facing away from the camera, which no click
        // can reach.
        let id = ui.id().with("add");
        let mut body = ui.data_mut(|d| d.get_temp::<usize>(id.with("body"))).unwrap_or(0);
        let mut facet = ui.data_mut(|d| d.get_temp::<usize>(id)).unwrap_or(0);
        if many {
            ui.label(egui::RichText::new("body").weak());
            ui.add(egui::DragValue::new(&mut body).range(0..=sim.bodies.len().saturating_sub(1)));
        }
        ui.label(egui::RichText::new("facet").weak());
        let facets = sim
            .bodies
            .get(body)
            .and_then(|b| b.mesh.as_ref())
            .map(|m| m.borrow().facets.len())
            .unwrap_or(0);
        ui.add(egui::DragValue::new(&mut facet).range(0..=facets.saturating_sub(1)));
        if ui
            .add_enabled(facets > 0, egui::Button::new("add"))
            .on_hover_text("Select this facet by index")
            .clicked()
        {
            sim.toggle_facet(body, facet, color);
        }
        ui.data_mut(|d| {
            d.insert_temp(id, facet);
            d.insert_temp(id.with("body"), body);
        });
    });
}

/// The right-hand panel: everything about the scene, by topic.
///
/// One panel where there were two. It used to be the simulation's *entities*
/// (state, bodies, camera, Sun) stacked above a "Config" section generated
/// from the settings struct -- a split by where the data lived, not by what
/// it was about, so the Sun's position and the Sun's colour were nine headers
/// apart, "Selection" and "Export" each appeared twice, and the HUD list sat
/// above one section while its font sat in the other.
///
/// Now `Config` is a struct of sub-structs and each topic header shows the
/// entity beside its own group: `sim.sun` with `config.light`, the picked
/// facet with `config.selection`, the HUD list with `config.hud`. The group
/// widgets come from `config_panel.rs`, generated one function per group, so
/// a field added to the struct lands under the right header without anyone
/// remembering.
///
/// Each topic is a tab down the panel's navigation bar (`TOPICS`,
/// `topic_tabs`), its icon and colour to be found at a glance, and only the
/// chosen one's page is drawn: the tabs are the table of contents, where a
/// closed section each was -- before those, Run, Bodies, Selection and HUD
/// started open and pushed everything else below the fold. A page's panels
/// start open, a body's and a HUD's closed.
pub fn simulation_panel(
    ui: &mut egui::Ui,
    sim: &mut Simulation,
    config: &mut Config,
) {
    ui.push_id("simulation", |ui| pages(ui, sim, config));
}

/// Each topic's page, its panels by what they hold -- the Sun's place and its
/// light two of them (the user: "in sun you would have two foldable category
/// sun and light").
fn pages(ui: &mut egui::Ui, sim: &mut Simulation, c: &mut Config) {
    let sel = c.selection.color;
    let selection_color = crate::Vec3::new(sel.r as Float, sel.g as Float, sel.b as Float);

    topic(ui, "Run", |ui| {
        panel(ui, "Run", |ui| {
            super::widgets::toggle(ui, "is_paused", "sim.state.is_paused -- hold the simulation; P toggles it", &mut sim.state.is_paused);
            // A cap on the frame rate, for watching something that otherwise
            // flashes past. The slider is live whether or not the cap is on,
            // so the value can be set first and the cap switched on to it;
            // off, the value is kept and does nothing. Logarithmic: the useful
            // range runs from one frame every few seconds to a few hundred a
            // second.
            setting(
                ui,
                "rate_limited",
                "sim.state.rate_limited -- cap the frame rate at rate_limit; one step is one frame, so the run \
                 slows with it. Off runs as fast as it can. The value can be set before or after switching the \
                 cap on; kept, and idle, while the cap is off",
                |ui| {
                    super::widgets::checkbox(ui, &mut sim.state.rate_limited, "");
                    ui.spacing_mut().slider_width = (ui.available_width() - 76.0).max(40.0);
                    ui.add(
                        super::widgets::Slider::new(&mut sim.state.rate_limit, 0.1..=1000.0)
                            .logarithmic(true)
                            .suffix(" fps"),
                    );
                },
            );
            setting(
                ui,
                "pause_after_iteration",
                "sim.state.pause_after_iteration -- pause once this iteration has run",
                |ui| {
                    let mut on = sim.state.pause_after_iteration.is_some();
                    if super::widgets::checkbox(ui, &mut on, "").changed() {
                        sim.state.pause_after_iteration = on.then_some(sim.state.iteration);
                    }
                    if let Some(n) = sim.state.pause_after_iteration.as_mut() {
                        super::widgets::number(ui, n, |d| d.speed(1.0));
                    }
                },
            );
        });
    });

    topic(ui, "Bodies", |ui| bodies_ui(ui, sim));

    // Named for the anchor-body picker below, and collected first because
    // that reads `bodies` while the picker holds `camera` mutably.
    let names: Vec<String> = sim.bodies.iter().map(body_name).collect();

    topic(ui, "Selection", |ui| {
        panel(ui, "Selected", |ui| selection_ui(ui, sim, c, selection_color));
        panel(ui, "Display", |ui| group_selection(ui, c));
    });

    topic(ui, "Camera", |ui| {
        panel(ui, "Camera", |ui| eye_ui(ui, &mut sim.camera, &names, false));
    });

    // Where it is, then what it does as a light.
    topic(ui, "Sun", |ui| {
        panel(ui, "Sun", |ui| eye_ui(ui, &mut sim.sun, &names, true));
        panel(ui, "Light", |ui| group_light(ui, c));
    });

    topic(ui, "Shading", |ui| {
        panel(ui, "Shading", |ui| group_shading(ui, c));
    });
    topic(ui, "Shadows", |ui| shadows_ui(ui, sim, c));
    topic(ui, "Wireframe", |ui| {
        panel(ui, "Wireframe", |ui| group_wireframe(ui, c));
    });

    topic(ui, "Data colouring", |ui| {
        panel(ui, "Data colouring", |ui| {
            colormap_ui(ui, c);
            group_data(ui, c);
        });
        panel(ui, "Colour bar", |ui| group_colorbar(ui, c));
    });

    topic(ui, "Axes & grid", |ui| {
        panel(ui, "Axes", |ui| group_axes(ui, c));
        panel(ui, "Grid", |ui| grid_ui(ui, c));
    });

    topic(ui, "HUD", |ui| {
        huds_ui(ui, sim);
        panel(ui, "Font", |ui| group_hud(ui, c));
    });

    // The image drawn into the window. The window itself is the app's, in the
    // side panel's app tab; namespaced all the same, since the two have
    // `width` and `height` widgets of their own.
    topic(ui, "Image", |ui| {
        panel(ui, "Image", |ui| {
            note(ui, "0 follows the window");
            ui.push_id("image", |ui| group_image(ui, c));
        });
        panel(ui, "Camera response", |ui| {
            ui.push_id("response", |ui| group_response(ui, c));
        })
        .on_hover_text(
            "What the camera does to the image it records: its point spread, a Gaussian core and two \
             exponential halos, and a smear, the image's motion over the exposure. Applied to the image's \
             floats by an FFT, before the overlays.",
        );
        panel(ui, "Stretch", |ui| {
            ui.push_id("stretch", |ui| group_stretch(ui, c));
            if let Some([low, high, least, greatest]) = sim.stretch_limits {
                note(ui, &format!("now {} to {}, the image {} to {}", short(low), short(high), short(least), short(greatest)));
            }
        })
        .on_hover_text(
            "How the image's values are shown, as SAO DS9 scales an image: each taken from between two limits, \
             black at the low one and white at the high one, through a function -- a logarithm, a square root, \
             ... -- for faint and bright to show together. The window and exported frames show it; a script \
             still reads the values themselves.",
        );
        panel(ui, "Photometry", |ui| {
            ui.push_id("photometry", |ui| group_photometry(ui, c));
            if let Some(f) = sim.photometry_last.filter(|_| c.photometry.enabled) {
                let cut = if f.clipped { ", a body cut by the frame" } else { "" };
                note(ui, &format!("{:.6e} at iteration {}{cut}", f.flux, f.iteration));
            }
        })
        .on_hover_text(
            "The flux of what the camera sees, from the frame itself: the image drawn for photometry -- full \
             meshes with their own normals, floats, no ambient light -- and every pixel's I/F summed times \
             its area, the disc integral kalast.lightcurve.flux gives, for sim.fluxes(). An orthographic \
             camera only.",
        );
        // Only once a script has made a telescope frame (`sim.expose`):
        // before that there is none to show, and the switch would do nothing.
        if sim.telescope_frame {
            panel(ui, "Telescope", |ui| {
                ui.push_id("telescope", |ui| group_telescope(ui, c));
            })
            .on_hover_text(
                "A telescope's frames, made on the GPU from the rendered light (sim.expose): the last one shown \
                 in place of the scene, the bodies still drawn for their light.",
            );
        }
    });

    topic(ui, "Controls", |ui| {
        panel(ui, "Controls", |ui| group_controls(ui, c));
    });

    topic(ui, "Export", |ui| {
        panel(ui, "Export", |ui| {
            super::widgets::toggle(ui, "export", "Write every frame from now on", &mut sim.export);
            setting(ui, "one frame", "Write the next frame only", |ui| {
                if super::widgets::button(ui, "export one frame").clicked() {
                    sim.export_once = true;
                }
            });
        });
        panel(ui, "Output", |ui| group_export(ui, c));
    });

    // What the last frame could actually see, then the switches. The renderer
    // writes the diagnostics after fitting the frustums, and they are the
    // quickest answer to "why is my body not on screen".
    topic(ui, "Debug", |ui| {
        panel(ui, "Diagnostics", |ui| {
            let d = &sim.diagnostics;
            row(ui, "bodies", format!("{} of {} visible", d.n_visible, d.n_bodies));
            row(ui, "clipped near", d.out_near.to_string());
            row(ui, "clipped far", d.out_far.to_string());
            row(ui, "outside sides", d.out_side.to_string());

            // The rows above answer "could the camera see it". This one
            // answers "did it appear": a body inside the frustum but wholly
            // behind another counts as visible up there and not here.
            if d.occlusion.valid {
                row(
                    ui,
                    "drew pixels",
                    format!("{} of {}", d.occlusion.n_drawn(), d.occlusion.n),
                );
                let occluded = d.n_visible.saturating_sub(d.occlusion.n_drawn());
                if occluded > 0 {
                    note(ui, &format!("{occluded} in frustum but hidden behind another body"));
                }
            }
            if d.light_cube_clipped {
                note(ui, "light cube is outside the camera's far plane");
            }
        });
        panel(ui, "Switches", |ui| group_debug(ui, c));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every text a frame drew.
    fn texts(output: &egui::FullOutput) -> Vec<String> {
        fn walk(shape: &egui::Shape, out: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(t) => out.push(t.galley.text().to_owned()),
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        output.shapes.iter().for_each(|c| walk(&c.shape, &mut out));
        out
    }

    /// Showing a section changes no setting: every generated group drawn over
    /// a config holding values its widgets would not offer. A slider clamped
    /// the value it was shown with -- the colour bar's 320-pixel length
    /// became 1, and the bar vanished, when "Data colouring" opened.
    #[test]
    fn drawing_the_panel_changes_no_setting() {
        let mut c = Config::default();
        c.light.ambient = 1.5; // 0..=1
        c.colorbar.ticks = 40; // 1..=20
        c.colorbar.length = 900.0;
        c.colorbar.min_max = true;
        c.data.colormap = vec![[1.0, 0.0, 0.0], [0.0, 0.0, 1.0]]; // one of its own
        let before = format!("{c:?}");
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 8000.0))),
            ..Default::default()
        };
        let mut output = ctx.run_ui(raw, |ui| {
            for group in [
                colormap_ui, group_shading, group_light, group_shadows, group_wireframe, group_selection,
                group_data, group_colorbar, group_axes, group_grid, group_hud, group_export, group_controls,
                group_image, group_debug,
            ] {
                group(ui, &mut c);
            }
        });
        output.textures_delta.clear();
        assert_eq!(format!("{c:?}"), before, "drawn, the panel changed the config");
    }

    /// The colour map is named in the panel as it was made -- a built-in,
    /// `_r` when reversed, "custom" for a table of its own -- with the means
    /// to reverse it or read one from a file. Asked for: "i cant seem to be
    /// able to change the colormap ... in data colouring in the UI".
    #[test]
    fn the_colormap_is_shown_by_name() {
        for (table, shown) in [
            (crate::app::config::builtin_colormap("inferno_r").unwrap(), "inferno_r"),
            (vec![[1.0, 0.0, 0.0], [0.0, 0.0, 1.0]], "custom, 2 colours"),
            (Vec::new(), "gray"),
        ] {
            let mut c = Config::default();
            c.data.colormap = table;
            let ctx = egui::Context::default();
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 400.0))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(raw, |ui| colormap_ui(ui, &mut c));
            output.textures_delta.clear();
            let texts = texts(&output);
            for text in [shown, "reverse", "load file..."] {
                assert!(texts.iter().any(|t| t == text), "{text}: {texts:?}");
            }
        }
    }

    /// A selected facet shows its value, the colour the colormap gives it
    /// over the range the frame was drawn with, and its own colour -- asked
    /// for: "when clicking on a facet also shows facet color and value",
    /// "also show color as per the colormap".
    #[test]
    fn a_selected_facet_shows_its_value_and_colours() {
        let mut sim = Simulation::new();
        sim.load_mesh("res/cube.obj", crate::Mat4::IDENTITY, false);
        {
            let mut mesh = sim.bodies[0].mesh.as_ref().unwrap().borrow_mut();
            let n = mesh.facets.len();
            mesh.values = (0..n).map(|i| 100.0 + 10.0 * i as Float).collect();
        }
        sim.config.borrow_mut().shading.color_mode = crate::app::config::ColorMode::Data;
        sim.value_range = (100.0, 200.0);
        sim.toggle_facet(0, 3, crate::Vec3::X);
        let own = crate::app::config::rgb(sim.selected_facets[0].color().unwrap());

        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 400.0))),
            ..Default::default()
        };
        // Drawn as the editor draws it, with the config borrowed mutably:
        // reading `sim.config` inside then panics, as it did in the app.
        let config = sim.config.clone();
        let c = config.borrow_mut();
        let mut output = ctx.run_ui(raw, |ui| selection_ui(ui, &mut sim, &c, crate::Vec3::X));
        drop(c);
        output.textures_delta.clear();
        let texts = texts(&output);
        assert!(texts.iter().any(|t| t == "facet 3"), "{texts:?}");
        assert!(texts.iter().any(|t| t == "130.0000"), "its value: {texts:?}");
        // Greyscale, 130 over 100..200: 0.3 of the way.
        assert!(texts.iter().any(|t| t == "0.300 0.300 0.300"), "its colormap colour: {texts:?}");
        for name in ["value", "colormap", "colour"] {
            assert!(texts.iter().any(|t| t == name), "{name}: {texts:?}");
        }
        assert!(texts.iter().any(|t| *t == own), "its own colour {own}: {texts:?}");
    }

    /// The shading toggle is `flatten`/`smoothen` and nothing else, so it
    /// has to survive being pressed more than once. It used to rebuild the
    /// vertex array each way -- `flatten` stashing the vertices it replaced,
    /// and a second call with nothing restored stashing the flattened ones
    /// and losing the originals. Neither touches the geometry now: what
    /// changes is `attrs`, per facet or per vertex, and `normals`.
    #[test]
    fn shading_toggles_back_and_forth() {
        let mut mesh = crate::mesh::Mesh::load("res/ico3.obj", |v| v);
        let vertices = mesh.positions.len();
        let facets = mesh.facets.len();
        assert!(!mesh.is_flat());

        for _ in 0..3 {
            mesh.flatten();
            assert!(mesh.is_flat());
            assert_eq!(mesh.attrs.len(), facets, "a flat mesh is coloured per facet");
            assert!(mesh.normals.is_empty(), "and takes its normals from them");

            mesh.smoothen();
            assert!(!mesh.is_flat());
            assert_eq!(mesh.attrs.len(), vertices, "a smooth one per vertex");
            assert_eq!(mesh.normals.len(), vertices);
            // The geometry is the same either way, which is the point.
            assert_eq!(mesh.positions.len(), vertices);
            assert_eq!(mesh.facets.len(), facets);
        }
    }

    /// A path being typed into is a path that does not exist yet, and
    /// `Mesh::load` unwraps -- so the panel must not reach it until the file
    /// is there.
    #[test]
    fn a_missing_file_is_an_error_not_a_panic() {
        assert!(try_load("res/there-is-no-such-mesh.obj", true).is_err());
        assert!(try_load("", true).is_err());
        // A directory is not a mesh either, and `is_file` is what says so.
        assert!(try_load("res", true).is_err());
    }

    /// Loading flat or smooth gives the same geometry and differs in the
    /// shading attributes: it used to give a different vertex array.
    #[test]
    fn loading_flat_and_smooth_differ_only_in_the_attributes() {
        let flat = try_load("res/ico3.obj", true).unwrap();
        let smooth = try_load("res/ico3.obj", false).unwrap();

        assert!(flat.is_flat() && !smooth.is_flat());
        assert_eq!(flat.positions, smooth.positions);
        assert_eq!(flat.indices, smooth.indices);
        assert_eq!(flat.attrs.len(), flat.facets.len());
        assert_eq!(smooth.attrs.len(), smooth.positions.len());
    }
}

#[cfg(test)]
mod naming_tests {
    use super::*;

    /// The header names a body by its shape model's stem. Every one of them
    /// is `.obj`, so the extension is four characters that say nothing; the
    /// full path is the hover, because which decimation of a model a body is
    /// often comes from the directory rather than the name.
    #[test]
    fn a_body_is_named_by_its_file_stem() {
        let mut mesh = crate::mesh::Mesh::new();
        mesh.path = Some(std::path::PathBuf::from(
            "/data/mesh/didymos/g_01165mm_spc_didy_v003_100k.obj",
        ));
        let body = crate::app::body::Body {
            mesh: Some(crate::locked::Locked::new(mesh)),
            ..Default::default()
        };
        assert_eq!(body_name(&body), "g_01165mm_spc_didy_v003_100k");
        assert_eq!(body_path(&body), "/data/mesh/didymos/g_01165mm_spc_didy_v003_100k.obj");

        let bare = crate::app::body::Body::default();
        assert_eq!(body_name(&bare), "built in memory");
    }
}

/// A value in four figures: plainly between a thousandth and ten thousand,
/// else with its power of ten.
fn short(v: f32) -> String {
    let a = v.abs();
    if a == 0.0 || (1e-3..1e4).contains(&a) {
        let digits = if a == 0.0 { 0 } else { (3 - a.log10().floor() as i32).clamp(0, 6) as usize };
        format!("{v:.digits$}")
    } else {
        format!("{v:.3e}")
    }
}
