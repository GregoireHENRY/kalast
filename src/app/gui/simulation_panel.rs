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
fn dir_row(ui: &mut egui::Ui, path: Option<&std::path::Path>) {
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

/// The bodies in the scene, and the means to change which ones they are.
fn bodies_ui(ui: &mut egui::Ui, sim: &mut Simulation) {
    if sim.bodies.is_empty() {
        ui.label(egui::RichText::new("none loaded").weak());
    }

    // Both are applied after the loop: removing a body while iterating over
    // them shifts every index behind it.
    let mut remove = None;
    let mut dirty = false;

    for i in 0..sim.bodies.len() {
        let name = body_name(&sim.bodies[i]);
        let mut drop_it = false;
        egui::CollapsingHeader::new(format!("body {i}  {name}"))
            .id_salt(i)
            .default_open(true)
            .show(ui, |ui| {
                dirty |= body_ui(ui, i, &mut sim.bodies[i]);
                if ui
                    .button("remove")
                    .on_hover_text("Take this body out of the scene")
                    .clicked()
                {
                    drop_it = true;
                }
            });
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
    ui.add_space(6.0);
    let id = ui.id().with("add");
    let mut path = remembered(ui, id, String::new);
    let mut flat = ui.data_mut(|d| d.get_temp::<bool>(id.with("flat"))).unwrap_or(true);
    ui.add(
        egui::TextEdit::singleline(&mut path)
            .hint_text("path to an .obj")
            .desired_width(f32::INFINITY),
    );
    ui.horizontal(|ui| {
        ui.checkbox(&mut flat, "flat")
            .on_hover_text("Give every facet its own vertices, as `flatten=True` does");
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
        ui.colored_label(egui::Color32::from_rgb(220, 120, 120), error);
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
        ui.label(egui::RichText::new("no mesh").weak());
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
    ui.add(
        egui::TextEdit::singleline(&mut path)
            .desired_width(f32::INFINITY)
            .font(egui::TextStyle::Monospace),
    );
    ui.horizontal(|ui| {
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
        ui.colored_label(egui::Color32::from_rgb(220, 120, 120), error);
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

    if let Some(shadow) = body.shadow_mesh.as_ref() {
        let mut shadow = shadow.borrow_mut();
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
        dir_row(ui, shadow.path.as_deref());
        dirty |= mesh_ui(ui, &mut shadow);
    }

    // Model to world, editable. Usually written by a script every frame, in
    // which case an edit here lasts one frame -- but for a scene that is
    // placed once, this is where to place it.
    ui.add_space(4.0);
    ui.label(egui::RichText::new("mat").weak())
        .on_hover_text("Model to world. A script that sets `body.mat` every iteration wins over this.");
    for r in 0..4 {
        ui.horizontal(|ui| {
            for c in 0..4 {
                ui.add(egui::DragValue::new(&mut body.mat.col_mut(c)[r]).speed(0.01));
            }
        });
    }

    dirty
}

/// Counts, shading and extent for one mesh. Returns whether it was changed
/// in a way the GPU buffers have to follow.
fn mesh_ui(ui: &mut egui::Ui, mesh: &mut crate::mesh::Mesh) -> bool {
    // One number per row, for the same reason the bounds are split: the
    // widest row in a side panel is the panel's width.
    row(ui, "facets", mesh.facets.len().to_string());
    row(ui, "vertices", mesh.vertices.len().to_string());
    row(ui, "indices", mesh.indices.len().to_string());

    let was = mesh.is_flat();
    let mut flat = was;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("shading").weak())
            .on_hover_text(
                "Flat gives every facet its own three vertices instead of sharing \
                 corners with its neighbours, so each shades as a plate and a \
                 per-facet value colours exactly one triangle. What flatten=True asks for.",
            );
        ui.selectable_value(&mut flat, true, "flat");
        ui.selectable_value(&mut flat, false, "smooth");
    });
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

/// The text overlays: what each says, where it sits, how it looks.
///
/// `text` is the *template*, not the line on screen -- `{it}`, `{fps}` and
/// the rest are filled in every frame. A script that assigns `hud.text`
/// each iteration owns it, and an edit here lasts until its next
/// assignment; the expanded result is shown underneath so both are visible.
fn huds_ui(ui: &mut egui::Ui, sim: &mut Simulation) {
    use crate::app::config::{HAlign, Hud, HudAnchor, VAlign};

    if sim.huds.is_empty() {
        ui.label(egui::RichText::new("none").weak());
    }

    let mut remove = None;
    for (i, handle) in sim.huds.iter().enumerate() {
        let mut hud = handle.borrow_mut();
        egui::CollapsingHeader::new(format!("hud {i}"))
            .id_salt(i)
            .default_open(true)
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut hud.text)
                        .desired_rows(1)
                        .desired_width(f32::INFINITY)
                        .font(egui::TextStyle::Monospace),
                )
                .on_hover_text(
                    "Template. {it} {drawn} {nit} {its} {fps} {ms} {bodies} {paused} {warn}, \
                     with an optional precision as {fps:.1}.",
                );

                // What that template comes out as this frame. Worth showing
                // even when it is the same string: a HUD a script rewrites
                // every iteration has no placeholders left by the time it
                // gets here, and seeing the line on screen next to the field
                // is what says so.
                let shown = crate::app::expand_hud(
                    &hud.text,
                    &sim.state,
                    0.0,
                    &sim.diagnostics,
                    sim.state.iteration,
                );
                ui.label(egui::RichText::new(shown).weak().small());

                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("anchor").weak());
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
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("inset").weak());
                    ui.add(egui::DragValue::new(&mut hud.x).speed(1.0));
                    ui.add(egui::DragValue::new(&mut hud.y).speed(1.0));
                })
                .response
                .on_hover_text("Pixels from the anchor");
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("size").weak());
                    ui.add(egui::DragValue::new(&mut hud.size).speed(0.5).range(1.0..=200.0));
                    ui.color_edit_button_rgba_unmultiplied(&mut hud.color);
                });

                // `None` means "follow the anchor", which is what a HUD
                // wants unless it is being pinned somewhere unusual -- so
                // the default stays reachable rather than being overwritten
                // the moment the picker is touched.
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("align").weak());
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

                if ui.button("remove").clicked() {
                    remove = Some(i);
                }
            });
    }

    if let Some(i) = remove {
        sim.huds.remove(i);
    }

    if ui.button("add HUD").clicked() {
        // A template rather than empty text, so a new HUD says something the
        // moment it appears and shows what a placeholder looks like.
        sim.huds.push(std::rc::Rc::new(std::cell::RefCell::new(Hud::new(
            "it={drawn}  {fps} fps",
        ))));
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

    group(ui, "Bodies", true, |ui| bodies_ui(ui, sim));

    // Named for the anchor-body picker below, and collected first because
    // that reads `bodies` while the picker holds `camera` mutably.
    let names: Vec<String> = sim.bodies.iter().map(body_name).collect();
    group(ui, "Camera", false, |ui| {
        eye_ui(ui, &mut sim.camera, &names, false)
    });
    group(ui, "Sun", false, |ui| eye_ui(ui, &mut sim.sun, &names, true));

    group(ui, "HUDs", true, |ui| huds_ui(ui, sim));

    group(ui, "Export", false, |ui| {
        ui.checkbox(&mut sim.export, "export")
            .on_hover_text("Write every frame from now on");
        if ui
            .button("export one frame")
            .on_hover_text("Write the next frame only")
            .clicked()
        {
            sim.export_once = true;
        }
        ui.label(
            egui::RichText::new("Where they land is under Config > Export")
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The shading toggle is `flatten`/`smoothen` and nothing else, so it
    /// has to survive being pressed more than once: `flatten` stashes the
    /// vertices it is about to replace, and a second call with nothing
    /// restored would stash the flattened ones and lose the originals.
    #[test]
    fn shading_toggles_back_and_forth() {
        let mut mesh = crate::mesh::Mesh::load("res/ico3.obj", |v| v);
        let smooth = mesh.vertices.len();
        let facets = mesh.facets.len();
        assert!(!mesh.is_flat());

        for _ in 0..3 {
            mesh.flatten();
            assert!(mesh.is_flat());
            assert_eq!(mesh.vertices.len(), facets * 3);

            mesh.smoothen();
            assert!(!mesh.is_flat());
            assert_eq!(mesh.vertices.len(), smooth);
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

    #[test]
    fn loading_flat_gives_every_facet_its_own_vertices() {
        let mesh = try_load("res/ico3.obj", true).unwrap();
        assert!(mesh.is_flat());
        assert_eq!(mesh.vertices.len(), mesh.facets.len() * 3);

        let mesh = try_load("res/ico3.obj", false).unwrap();
        assert!(!mesh.is_flat());
    }
}
