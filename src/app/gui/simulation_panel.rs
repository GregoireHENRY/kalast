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
use super::widgets::{line, note, section, setting, subheading, Icon};

/// A topic's section: its Codicon in its colour, kept open or shut by title.
fn topic(ui: &mut egui::Ui, icon: &'static str, color: egui::Color32, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    section(ui, Icon::Codicon(icon, color), title, title, add);
}

/// A readout, laid out as a setting is, so a value and a control line up.
/// Returns the value's response, for the rows that want a hover on it.
fn row(ui: &mut egui::Ui, label: &str, value: impl Into<String>) -> egui::Response {
    setting(ui, label, "", |ui| ui.label(egui::RichText::new(value.into()).monospace()))
}

/// Three drag fields for a vector, returning whether any changed.
fn vec3(ui: &mut egui::Ui, label: &str, hover: &str, v: &mut crate::Vec3, speed: f64) -> bool {
    setting(ui, label, hover, |ui| {
        // Three to a row, each a third of it: sized by their digits, a unit
        // vector's three ran past the panel's edge.
        let gap = 3.0;
        ui.spacing_mut().item_spacing.x = gap;
        let width = ((ui.available_width() - 2.0 * gap - 6.0) / 3.0).max(28.0);
        let mut changed = false;
        for c in [&mut v.x, &mut v.y, &mut v.z] {
            changed |= ui
                .add_sized([width, 18.0], egui::DragValue::new(c).speed(speed).max_decimals(3))
                .changed();
        }
        changed
    })
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

/// Read a mesh without taking the process down with it.
///
/// `Mesh::load` unwraps its way through the file: a path that is not there,
/// or an `.obj` it cannot parse, is a panic. That is defensible for a script
/// -- it fails on the line that asked -- but not for a text field someone is
/// still typing into, where a half-finished path would end the session and
/// everything running in it.
fn try_load(path: &str, flat: bool) -> Result<crate::mesh::Mesh, String> {
    let p = std::path::Path::new(path);
    if !p.is_file() {
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
            egui::ComboBox::from_id_salt(id).selected_text(&shown).height(360.0).show_ui(ui, |ui| {
                for n in colormap_names() {
                    let current = name.as_deref().is_some_and(|m| m.strip_suffix("_r").unwrap_or(m) == n);
                    let table = builtin_colormap(n).unwrap_or_default();
                    ui.horizontal(|ui| {
                        let (rect, _) = ui.allocate_exact_size(egui::vec2(48.0, 10.0), egui::Sense::hover());
                        gradient(ui, rect, &table, 24);
                        if ui.selectable_label(current, n).clicked() {
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
            let picked = rfd::FileDialog::new()
                .set_title("A colour map")
                .add_filter("colours", &["csv", "txt", "dat", "tsv"])
                .pick_file();
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
        let icon = Icon::Image(super::icons::for_file("body.obj"));
        section(ui, icon, &format!("{i}  {name}"), ("body", i), |ui| {
            dirty |= body_ui(ui, i, &mut sim.bodies[i]);
            line(ui, |ui| {
                if ui.button("remove").on_hover_text("Take this body out of the scene").clicked() {
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

    // Adding one. A path rather than a file dialog, the same way the script
    // panel opens a file -- there is no native picker in this window.
    subheading(ui, "add a body");
    let id = ui.id().with("add");
    let mut path = remembered(ui, id, String::new);
    let mut flat = ui.data_mut(|d| d.get_temp::<bool>(id.with("flat"))).unwrap_or(true);
    setting(ui, "path", "The shape model to load, an .obj", |ui| {
        ui.add(
            egui::TextEdit::singleline(&mut path)
                .hint_text("path to an .obj")
                .desired_width(f32::INFINITY),
        )
    });
    setting(ui, "flat", "Give every facet its own vertices, as `flatten=True` does", |ui| {
        ui.checkbox(&mut flat, "")
    });
    line(ui, |ui| {
        if ui.button("add body").clicked() && !path.is_empty() {
            match try_load(&path, flat) {
                Ok(mesh) => {
                    sim.add_mesh(mesh, crate::Mat4::IDENTITY);
                    dirty = true;
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
    ui.data_mut(|d| d.insert_temp(id.with("flat"), flat));

    sim.meshes_dirty |= dirty;
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
    // both says where this body came from and points it somewhere else.
    let id = ui.id().with(("path", i));
    let current = handle
        .borrow()
        .path
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut path = remembered(ui, id, || current.clone());
    setting(ui, "file", "Where the mesh came from: edit it and reload to point the body at another", |ui| {
        ui.add(
            egui::TextEdit::singleline(&mut path)
                .desired_width(f32::INFINITY)
                .font(egui::TextStyle::Monospace),
        )
    });
    line(ui, |ui| {
        let changed = path != current;
        if ui
            .add_enabled(changed, egui::Button::new("reload"))
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
        if changed && ui.small_button("revert").clicked() {
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
    setting(
        ui,
        "horizon_map",
        "The body's own shadows from how high its terrain rises round each facet, worked out once on the GPU \
         (seconds for millions of facets), instead of drawing it into its own shadow map every frame. For a \
         body each direction from whose centre crosses its surface once, with shadows.per_body on",
        |ui| ui.checkbox(&mut body.horizon_map, ""),
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
            // Four to a row, each a quarter of it.
            ui.spacing_mut().item_spacing.x = 3.0;
            let width = ((ui.available_width() - 9.0) / 4.0).max(28.0);
            for c in 0..4 {
                ui.add_sized([width, 18.0], egui::DragValue::new(&mut body.mat.col_mut(c)[r]).speed(0.01).max_decimals(4));
            }
        });
    }

    dirty
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
            egui::ComboBox::from_id_salt(("scattering law", i))
                .selected_text(names[kind])
                .show_ui(ui, |ui| {
                    for (k, name) in names.iter().enumerate() {
                        ui.selectable_value(&mut kind, k, *name);
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
        setting(ui, name, hover, |ui| ui.add(egui::DragValue::new(v).range(range).speed(speed).max_decimals(4)));
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
                |ui| ui.add(egui::DragValue::new(&mut deg).range(0.0..=89.9).speed(0.1).max_decimals(2).suffix(" deg")),
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
    setting(
        ui,
        "on",
        "A dusty atmosphere over the surface in the image: the dust's own light, and the surface seen \
         through it, lit by the beam that got through and by the sky. Mars's, in km, when first turned on",
        |ui| ui.checkbox(&mut on, ""),
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
        setting(ui, name, hover, |ui| ui.add(egui::DragValue::new(v).range(range).speed(speed).max_decimals(4)));
    };
    number(ui, "tau", "Vertical optical depth of the dust at the surface", &mut edit.tau, 0.0..=10.0, 0.01);
    number(ui, "scale_height", "The dust's scale height, in the scene's units", &mut edit.scale_height, 1e-6..=1e12, 0.1);
    number(ui, "radius", "The planet's equatorial radius, in the scene's units", &mut edit.radius, 1e-6..=1e12, 1.0);
    let mut ellipsoid = edit.polar_radius.is_some();
    setting(ui, "polar_radius", "The ellipsoid's polar radius, about the body's own z axis; off, a sphere of `radius`", |ui| {
        ui.checkbox(&mut ellipsoid, "");
        edit.polar_radius = ellipsoid.then(|| {
            let mut c = edit.polar_radius.unwrap_or(edit.radius);
            ui.add(egui::DragValue::new(&mut c).range(1e-6..=1e12).speed(1.0).max_decimals(4));
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
            ui.checkbox(&mut fixed, "");
            edit.albedo = fixed.then(|| {
                let mut v = edit.albedo.unwrap_or(0.2);
                ui.add(egui::DragValue::new(&mut v).range(0.0..=1.0).speed(0.001).max_decimals(4));
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
        |ui| {
            ui.selectable_value(&mut flat, true, "flat");
            ui.selectable_value(&mut flat, false, "smooth");
        },
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
            egui::ComboBox::from_id_salt("anchor_body")
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut eye.anchor_body, None, "none");
                    for (i, name) in bodies.iter().enumerate() {
                        ui.selectable_value(&mut eye.anchor_body, Some(i), format!("{i}  {name}"));
                    }
                });
        });
    }

    let p = &mut eye.projection;

    if !is_sun {
        setting(ui, "mode", "Perspective, or orthographic: parallel rays, no vanishing point", |ui| {
            egui::ComboBox::from_id_salt("mode")
                .selected_text(format!("{:?}", p.mode))
                .show_ui(ui, |ui| {
                    use crate::app::frame::ProjectionMode::*;
                    ui.selectable_value(&mut p.mode, Perspective, "Perspective");
                    ui.selectable_value(&mut p.mode, Orthographic, "Orthographic");
                });
        });

        // An orthographic frustum has no field of view -- `side` is its
        // width -- so a dead angle here would only invite editing it.
        if p.mode == crate::app::frame::ProjectionMode::Perspective {
            setting(ui, "fovy", "The vertical field of view, spanning the window's height", |ui| {
                let mut deg = p.fovy.to_degrees();
                if ui
                    .add(egui::DragValue::new(&mut deg).speed(0.2).suffix("\u{b0}"))
                    .changed()
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
            if ui.checkbox(&mut pinned, "").changed() {
                *field = pinned.then_some(value);
            }
            match field {
                Some(v) => {
                    ui.add(egui::DragValue::new(v).speed(0.01));
                }
                None => {
                    ui.label(egui::RichText::new(format!("{value:.4}")).monospace());
                    ui.label(egui::RichText::new("fitted").weak().small());
                }
            }
        });
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
        let icon = Icon::Codicon(super::icons::codicon::TEXT_SIZE, super::theme::palette::LAVENDER);
        section(ui, icon, &format!("hud {i}"), ("hud", i), |ui| {
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
                    egui::ComboBox::from_id_salt("anchor")
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
                                ui.selectable_value(&mut hud.anchor, a, label);
                            }
                        });
                });
                setting(ui, "inset", "Pixels from the anchor, across and down", |ui| {
                    ui.add(egui::DragValue::new(&mut hud.x).speed(1.0));
                    ui.add(egui::DragValue::new(&mut hud.y).speed(1.0));
                });
                setting(ui, "size", "Letter height in pixels, and the colour", |ui| {
                    ui.add(egui::DragValue::new(&mut hud.size).speed(0.5).range(1.0..=200.0).clamp_existing_to_range(false));
                    // Through a copy: egui's button writes its HSV round
                    // trip back every frame, changing what a script set.
                    let mut color = hud.color;
                    if ui.color_edit_button_rgba_unmultiplied(&mut color).changed() {
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
                    egui::ComboBox::from_id_salt("align_h")
                        .selected_text(match hud.align_h {
                            Some(a) => format!("{a:?}"),
                            None => "anchor".to_string(),
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut hud.align_h, None, "anchor");
                            for a in [HAlign::Left, HAlign::Center, HAlign::Right] {
                                let label = format!("{a:?}");
                                ui.selectable_value(&mut hud.align_h, Some(a), label);
                            }
                        });
                    egui::ComboBox::from_id_salt("align_v")
                        .selected_text(match hud.align_v {
                            Some(a) => format!("{a:?}"),
                            None => "anchor".to_string(),
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut hud.align_v, None, "anchor");
                            for a in [VAlign::Top, VAlign::Center, VAlign::Bottom] {
                                let label = format!("{a:?}");
                                ui.selectable_value(&mut hud.align_v, Some(a), label);
                            }
                        });
                });

                line(ui, |ui| {
                    if ui.button("remove").clicked() {
                        remove = Some(i);
                    }
                });
        });
    }

    if let Some(i) = remove {
        sim.huds.remove(i);
    }

    if line(ui, |ui| ui.button("add HUD").clicked()) {
        // A template rather than empty text, so a new HUD says something the
        // moment it appears and shows what a placeholder looks like.
        sim.huds.push(std::rc::Rc::new(std::cell::RefCell::new(Hud::new(
            "it={drawn}  {fps} fps",
        ))));
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
/// Every section starts closed, so the panel opens as a table of contents
/// rather than a wall -- Run, Bodies, Selection and HUD used to start open
/// and pushed everything else below the fold. Each has its icon and colour,
/// as the files tab has its file icons, to be found at a glance.
pub fn simulation_panel(
    ui: &mut egui::Ui,
    sim: &mut Simulation,
    config: &mut Config,
) {
    ui.push_id("simulation", |ui| panel(ui, sim, config));
}

fn panel(ui: &mut egui::Ui, sim: &mut Simulation, c: &mut Config) {
    let sel = c.selection.color;
    let selection_color = crate::Vec3::new(sel.r as Float, sel.g as Float, sel.b as Float);

    topic(ui, codicon::PLAY_CIRCLE, palette::GREEN, "Run", |ui| {
        setting(ui, "is_paused", "sim.state.is_paused -- hold the simulation; P toggles it", |ui| {
            ui.checkbox(&mut sim.state.is_paused, "")
        });
        // A cap on the frame rate, for watching something that otherwise
        // flashes past. The slider is live whether or not the cap is on, so
        // the value can be set first and the cap switched on to it; off, the
        // value is kept and does nothing. Logarithmic: the useful range runs
        // from one frame every few seconds to a few hundred a second.
        setting(
            ui,
            "rate_limited",
            "sim.state.rate_limited -- cap the frame rate at rate_limit; one step is one frame, so the run \
             slows with it. Off runs as fast as it can. The value can be set before or after switching the \
             cap on; kept, and idle, while the cap is off",
            |ui| {
                ui.checkbox(&mut sim.state.rate_limited, "");
                ui.spacing_mut().slider_width = (ui.available_width() - 76.0).max(40.0);
                ui.add(
                    egui::Slider::new(&mut sim.state.rate_limit, 0.1..=1000.0)
                        .clamping(egui::SliderClamping::Edits)
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
                if ui.checkbox(&mut on, "").changed() {
                    sim.state.pause_after_iteration = on.then_some(sim.state.iteration);
                }
                if let Some(n) = sim.state.pause_after_iteration.as_mut() {
                    ui.add(egui::DragValue::new(n).speed(1.0));
                }
            },
        );
    });

    topic(ui, codicon::CIRCLE_LARGE_FILLED, palette::PEACH, "Bodies", |ui| bodies_ui(ui, sim));

    // Named for the anchor-body picker below, and collected first because
    // that reads `bodies` while the picker holds `camera` mutably.
    let names: Vec<String> = sim.bodies.iter().map(body_name).collect();

    topic(ui, codicon::TARGET, palette::RED, "Selection", |ui| {
        selection_ui(ui, sim, c, selection_color);
        subheading(ui, "settings");
        group_selection(ui, c);
    });

    topic(ui, codicon::DEVICE_CAMERA, palette::BLUE, "Camera", |ui| {
        eye_ui(ui, &mut sim.camera, &names, false)
    });

    // Where it is, then what it does as a light. One header, because that is
    // one thing.
    topic(ui, codicon::STAR_FULL, palette::YELLOW, "Sun", |ui| {
        eye_ui(ui, &mut sim.sun, &names, true);
        subheading(ui, "light");
        group_light(ui, c);
    });

    topic(ui, codicon::PAINTCAN, palette::MAUVE, "Shading", |ui| group_shading(ui, c));
    topic(ui, codicon::COLOR_MODE, palette::LAVENDER, "Shadows", |ui| group_shadows(ui, c));
    topic(ui, codicon::LAYERS, palette::TEAL, "Wireframe", |ui| group_wireframe(ui, c));

    topic(ui, codicon::SYMBOL_COLOR, palette::PINK, "Data colouring", |ui| {
        colormap_ui(ui, c);
        group_data(ui, c);
        subheading(ui, "colour bar");
        group_colorbar(ui, c);
    });

    topic(ui, codicon::MOVE, palette::SKY, "Axes & grid", |ui| {
        group_axes(ui, c);
        subheading(ui, "grid");
        group_grid(ui, c);
    });

    topic(ui, codicon::TEXT_SIZE, palette::LAVENDER, "HUD", |ui| {
        huds_ui(ui, sim);
        subheading(ui, "settings");
        group_hud(ui, c);
    });

    // The image drawn into the window. The window itself is the app's, in the
    // side panel's app tab; namespaced all the same, since the two have
    // `width` and `height` widgets of their own.
    topic(ui, codicon::FILE_MEDIA, palette::SAPPHIRE, "Image", |ui| {
        note(ui, "0 follows the window");
        ui.push_id("image", |ui| group_image(ui, c));
    });

    topic(ui, codicon::RECORD_KEYS, palette::FLAMINGO, "Controls", |ui| group_controls(ui, c));

    topic(ui, codicon::EXPORT, palette::GREEN, "Export", |ui| {
        setting(ui, "export", "Write every frame from now on", |ui| ui.checkbox(&mut sim.export, ""));
        setting(ui, "one frame", "Write the next frame only", |ui| {
            if ui.button("export one frame").clicked() {
                sim.export_once = true;
            }
        });
        subheading(ui, "settings");
        group_export(ui, c);
    });

    // What the last frame could actually see, then the switches. The renderer
    // writes the diagnostics after fitting the frustums, and they are the
    // quickest answer to "why is my body not on screen".
    topic(ui, codicon::DEBUG, palette::MAROON, "Debug", |ui| {
        let d = &sim.diagnostics;
        row(ui, "bodies", format!("{} of {} visible", d.n_visible, d.n_bodies));
        row(ui, "clipped near", d.out_near.to_string());
        row(ui, "clipped far", d.out_far.to_string());
        row(ui, "outside sides", d.out_side.to_string());

        // The rows above answer "could the camera see it". This one answers
        // "did it appear": a body inside the frustum but wholly behind
        // another counts as visible up there and not here.
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
        subheading(ui, "switches");
        group_debug(ui, c);
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
        c.shading.srgb_mode = 2; // its list: 0 and 1
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
        sim.config.borrow_mut().shading.color_mode = 1;
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
            mesh: Some(std::rc::Rc::new(std::cell::RefCell::new(mesh))),
            ..Default::default()
        };
        assert_eq!(body_name(&body), "g_01165mm_spc_didy_v003_100k");
        assert_eq!(body_path(&body), "/data/mesh/didymos/g_01165mm_spc_didy_v003_100k.obj");

        let bare = crate::app::body::Body::default();
        assert_eq!(body_name(&bare), "built in memory");
    }
}
