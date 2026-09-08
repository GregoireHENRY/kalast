//! What the simulation currently *is*, beside what it is configured to be.
//!
//! Hand-written, unlike `config_panel`, because this is runtime state rather
//! than a struct of settings: a generator reading field names would have
//! nothing sensible to say about a `Mat4` or an `Option<Rc<RefCell<Mesh>>>`,
//! and the useful thing to show for a body is its facet count, not its
//! transform's sixteen numbers.

use crate::app::simulation::Simulation;

/// A labelled row, so every readout lines up the same way.
fn row(ui: &mut egui::Ui, label: &str, value: impl Into<String>) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).weak());
        ui.label(egui::RichText::new(value.into()).monospace());
    });
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

fn eye_ui(ui: &mut egui::Ui, eye: &mut crate::app::frame::Eye, is_sun: bool) {
    vec3(ui, "pos", &mut eye.pos, 0.05);

    if !is_sun {
        // Direction and up must stay unit vectors -- a short one panics the
        // renderer, inside a callback that cannot unwind -- so they are shown
        // and renormalised rather than edited raw.
        if vec3(ui, "dir", &mut eye.dir, 0.01) {
            eye.dir = eye.dir.normalize_or_zero();
        }
        if vec3(ui, "up", &mut eye.up, 0.01) {
            eye.up = eye.up.normalize_or_zero();
        }
    }

    vec3(ui, "anchor", &mut eye.anchor, 0.05);
    row(
        ui,
        "anchor body",
        match eye.anchor_body {
            Some(i) => i.to_string(),
            None => "none".to_string(),
        },
    );

    let p = &mut eye.projection;
    row(ui, "mode", format!("{:?}", p.mode));
    // An orthographic frustum has no field of view; `side` is its width, and
    // showing a dead angle beside it only invites editing it.
    if p.mode == crate::app::frame::ProjectionMode::Perspective {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("fovy").weak());
            let mut deg = p.fovy.to_degrees();
            if ui
                .add(egui::DragValue::new(&mut deg).speed(0.2).suffix("°"))
                .changed()
            {
                p.fovy = deg.to_radians();
            }
        });
    }

    // The resolved values rather than the requests: an unset one is fitted
    // to the scene every frame, and the fitted number is what explains the
    // picture. Which of the two it is matters as much as the value.
    let r = p.resolved();
    for (name, set, value) in [
        ("near", p.near.is_some(), r.near),
        ("far", p.far.is_some(), r.far),
        ("side", p.side.is_some(), r.side),
    ] {
        row(
            ui,
            name,
            format!(
                "{value:.4} {}",
                if set { "(pinned)" } else { "(fitted)" }
            ),
        );
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
            let facets = body
                .mesh
                .as_ref()
                .map(|m| m.borrow().facets.len())
                .unwrap_or(0);
            let shadow = body.shadow_mesh.as_ref().map(|m| m.borrow().facets.len());
            // Where it is, which is the part of a 4x4 anyone reads.
            let p = body.mat.w_axis;
            row(
                ui,
                &format!("body {i}"),
                match shadow {
                    Some(n) => format!("{facets} facets, {n} shadow"),
                    None => format!("{facets} facets"),
                },
            );
            row(ui, "  at", format!("{:.3} {:.3} {:.3}", p.x, p.y, p.z));
        }
    });

    group(ui, "Camera", false, |ui| eye_ui(ui, &mut sim.camera, false));
    group(ui, "Sun", false, |ui| eye_ui(ui, &mut sim.sun, true));

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
