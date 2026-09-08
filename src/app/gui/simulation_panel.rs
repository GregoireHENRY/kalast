//! What the simulation currently *is*, beside what it is configured to be.
//!
//! Hand-written, unlike `config_panel`, because this is runtime state rather
//! than a struct of settings: a generator reading field names would have
//! nothing sensible to say about a `Mat4` or an `Option<Rc<RefCell<Mesh>>>`,
//! and the useful thing to show for a body is its facet count, not its
//! transform's sixteen numbers.

use crate::app::simulation::Simulation;
use crate::Float;

/// A labelled row, so every readout lines up the same way. Returns the
/// value's response, for the rows that want a hover on it.
fn row(ui: &mut egui::Ui, label: &str, value: impl Into<String>) -> egui::Response {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).weak());
        ui.label(egui::RichText::new(value.into()).monospace())
    })
    .inner
}

/// Three drag fields for a vector, returning whether any changed.
fn vec3(ui: &mut egui::Ui, label: &str, v: &mut crate::Vec3, speed: f64) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).weak());
        for c in [&mut v.x, &mut v.y, &mut v.z] {
            changed |= ui.add(egui::DragValue::new(c).speed(speed)).changed();
        }
    });
    changed
}

/// Where the file is, without the file: its name is already the header just
/// above, and a shape model's full path is routinely wider than the panel,
/// so spelling it out again both repeats itself and sets the panel's width
/// for every other row. The whole path is one hover away.
fn path_row(ui: &mut egui::Ui, path: Option<&std::path::Path>) {
    let Some(path) = path else { return };
    let dir = path.parent().unwrap_or(std::path::Path::new("."));
    let dir = dir.to_string_lossy();
    row(ui, "in", if dir.is_empty() { ".".into() } else { dir })
        .on_hover_text(path.to_string_lossy());
}

/// A body's file name, or a stand-in for one built in memory.
fn body_name(body: &crate::app::body::Body) -> String {
    body.mesh
        .as_ref()
        .and_then(|m| {
            m.borrow()
                .path
                .as_ref()
                .and_then(|p| p.file_name().map(|f| f.to_string_lossy().into_owned()))
        })
        .unwrap_or_else(|| "built in memory".to_string())
}

/// One body: which file it is, and the shape of what was loaded. Counts and
/// bounds rather than the arrays themselves -- 3.1M facets is not something
/// to put in a side panel, and the questions actually asked of a mesh here
/// are "did it flatten", "how big is it" and "does it carry values".
fn body_ui(ui: &mut egui::Ui, i: usize, body: &crate::app::body::Body) {
    let mesh = body.mesh.as_ref();
    // The file name alone in the header, since a full path is usually long
    // enough to widen the panel on its own; the whole thing goes below.
    let file = body_name(body);

    egui::CollapsingHeader::new(format!("body {i}  {file}"))
        .id_salt(i)
        .default_open(true)
        .show(ui, |ui| {
            let Some(mesh) = mesh else {
                ui.label(egui::RichText::new("no mesh").weak());
                return;
            };
            let mesh = mesh.borrow();

            path_row(ui, mesh.path.as_deref());
            mesh_ui(ui, &mesh);

            // The values a colormap reads, if a script has written any. Their
            // range is the useful part: an all-zero column and a missing one
            // look identical in the render.
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

            if let Some(shadow) = body.shadow_mesh.as_ref() {
                let shadow = shadow.borrow();
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(format!(
                        "shadow mesh  {}",
                        shadow
                            .path
                            .as_ref()
                            .and_then(|p| p.file_name())
                            .map(|f| f.to_string_lossy().into_owned())
                            .unwrap_or_default()
                    ))
                    .weak(),
                );
                path_row(ui, shadow.path.as_deref());
                mesh_ui(ui, &shadow);
            }

            // Model to world. Read-only: bodies are usually placed by a
            // script every frame, so an edit here would last one frame.
            ui.add_space(4.0);
            ui.label(egui::RichText::new("mat").weak());
            let m = body.mat.to_cols_array_2d();
            for r in 0..4 {
                row(
                    ui,
                    "",
                    format!(
                        "{:>7.3} {:>7.3} {:>7.3} {:>7.3}",
                        m[0][r], m[1][r], m[2][r], m[3][r]
                    ),
                );
            }
        });
}

/// Counts, winding and extent for one mesh.
fn mesh_ui(ui: &mut egui::Ui, mesh: &crate::mesh::Mesh) {
    // One number per row, for the same reason the bounds are split: the
    // widest row in a side panel is the panel's width.
    row(ui, "facets", mesh.facets.len().to_string());
    row(ui, "vertices", mesh.vertices.len().to_string());
    row(ui, "indices", mesh.indices.len().to_string());
    // Flattened means every facet owns its three vertices instead of
    // sharing corners with its neighbours: each shades as a flat plate,
    // and a per-facet value colours exactly one triangle. What
    // `flatten=True` asks for, and what per-facet science data needs.
    //
    // Not "winding", which is the order the three are listed in and
    // decides which way the normal points -- a different property, and the
    // one `flip_facets` repairs.
    row(ui, "shading", if mesh.is_flat() { "flat" } else { "smooth" })
        .on_hover_text(
            "Flat means every facet owns its three vertices instead of sharing \
             corners with its neighbours, so each shades as a plate and a \
             per-facet value colours exactly one triangle. What flatten=True asks for.",
        );
    // Three rows rather than one long one: a side panel is narrow, and a
    // row wide enough to hold six numbers makes the whole panel that wide.
    let b = &mesh.bounds;
    row(ui, "min", format!("{:.3} {:.3} {:.3}", b.min.x, b.min.y, b.min.z));
    row(ui, "max", format!("{:.3} {:.3} {:.3}", b.max.x, b.max.y, b.max.z));
    let e = b.max - b.min;
    row(ui, "extent", format!("{:.3} {:.3} {:.3}", e.x, e.y, e.z));
    if let Some(id) = mesh.material_id {
        row(ui, "material", id.to_string());
    }
}

fn eye_ui(ui: &mut egui::Ui, eye: &mut crate::app::frame::Eye, bodies: &[String], is_sun: bool) {
    vec3(ui, "pos", &mut eye.pos, 0.05);

    // The Sun's frame is not a camera anyone reframes: it looks at the scene
    // from wherever `pos` puts it, orthographically, and its anchor follows
    // the geometry. Showing those as settings would only invite changing
    // something that is not a choice.
    if !is_sun {
        // Direction and up must stay unit vectors -- a short one panics the
        // renderer, inside a callback that cannot unwind -- so they are
        // renormalised on every edit rather than trusted.
        if vec3(ui, "dir", &mut eye.dir, 0.01) {
            eye.dir = eye.dir.normalize_or_zero();
        }
        if vec3(ui, "up", &mut eye.up, 0.01) {
            eye.up = eye.up.normalize_or_zero();
        }

        vec3(ui, "anchor", &mut eye.anchor, 0.05);

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("anchor body").weak());
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
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("mode").weak());
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
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("fovy").weak());
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
    // frame. The tick is which of the two, and the value beside it is what
    // is actually in the matrix either way -- untick and it goes back to
    // following the geometry, from the number it was last fitted to.
    let fitted = p.resolved();
    for (name, field, value) in [
        ("near", &mut p.near, fitted.near),
        ("far", &mut p.far, fitted.far),
        ("side", &mut p.side, fitted.side),
    ] {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(name).weak());
            let mut pinned = field.is_some();
            if ui
                .checkbox(&mut pinned, "")
                .on_hover_text("Pin this plane. Unticked, it is fitted to the scene every frame.")
                .changed()
            {
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

/// A collapsing section, opened by default or not.
fn group(ui: &mut egui::Ui, title: &str, open: bool, add: impl FnOnce(&mut egui::Ui)) {
    egui::CollapsingHeader::new(title)
        .default_open(open)
        .show(ui, add);
}

/// The simulation's live state: what is loaded, where it is, what it can see.
pub fn simulation_panel(ui: &mut egui::Ui, sim: &mut Simulation) {
    // The config panel below has its own "Export" header, and egui derives a
    // widget's id from its label -- so without a namespace the two collide
    // and egui paints a red "first/second use of widget ID" warning over
    // both.
    ui.push_id("simulation", |ui| panel(ui, sim));
}

fn panel(ui: &mut egui::Ui, sim: &mut Simulation) {
    group(ui, "State", true, |ui| {
        // Named as the field is, because that is what a script writes --
        // and it is one ahead of the toolbar's counter on purpose: the
        // toolbar says which frame you are looking at, this says how many
        // have been started.
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("iteration").weak());
            ui.label(egui::RichText::new(sim.state.iteration.to_string()).monospace())
                .on_hover_text("sim.state.iteration -- iterations begun, so one ahead of the frame on screen");
        });
        ui.checkbox(&mut sim.state.is_paused, "is_paused");
        ui.horizontal(|ui| {
            let mut on = sim.state.pause_at.is_some();
            if ui.checkbox(&mut on, "pause_at").changed() {
                sim.state.pause_at = on.then_some(sim.state.iteration + 1);
            }
            if let Some(n) = sim.state.pause_at.as_mut() {
                ui.add(egui::DragValue::new(n).speed(1.0));
            }
        });
    });

    group(ui, "Bodies", true, |ui| {
        if sim.bodies.is_empty() {
            ui.label(egui::RichText::new("none loaded").weak());
        }
        for (i, body) in sim.bodies.iter().enumerate() {
            body_ui(ui, i, body);
        }
    });

    // Named for the anchor-body picker below, and collected first because
    // that reads `bodies` while the picker holds `camera` mutably.
    let names: Vec<String> = sim.bodies.iter().map(body_name).collect();
    group(ui, "Camera", false, |ui| {
        eye_ui(ui, &mut sim.camera, &names, false)
    });
    group(ui, "Sun", false, |ui| eye_ui(ui, &mut sim.sun, &names, true));

    group(ui, "HUDs", true, |ui| {
        if sim.huds.is_empty() {
            ui.label(egui::RichText::new("none").weak());
        }
        for (i, hud) in sim.huds.iter().enumerate() {
            let mut hud = hud.borrow_mut();
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(format!("{i}")).weak());
                // The expanded text, as drawn -- a callback usually rewrites
                // this every frame, so editing it here is a preview at best.
                ui.add(
                    egui::TextEdit::singleline(&mut hud.text)
                        .desired_width(f32::INFINITY)
                        .font(egui::TextStyle::Monospace),
                );
            });
        }
    });

    group(ui, "Export", false, |ui| {
        ui.checkbox(&mut sim.export, "exporting every frame");
        if ui.button("export one frame").clicked() {
            sim.export_once = true;
        }
        ui.label(
            egui::RichText::new("Frames land in simulation.config.export_dir")
                .weak()
                .small(),
        );
    });

    // What the last frame could actually see. The renderer writes this after
    // fitting the frustums, and it is the quickest answer to "why is my body
    // not on screen".
    group(ui, "Visibility", false, |ui| {
        let d = &sim.diagnostics;
        row(ui, "bodies", format!("{} of {} visible", d.n_visible, d.n_bodies));
        row(ui, "clipped near", d.out_near.to_string());
        row(ui, "clipped far", d.out_far.to_string());
        row(ui, "outside sides", d.out_side.to_string());
        if d.light_cube_clipped {
            ui.label(
                egui::RichText::new("light cube is outside the camera's far plane")
                    .weak()
                    .small(),
            );
        }
    });
}
