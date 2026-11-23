//! `E`: Blender's menu search, for what kalast's UI app does, every setting of
//! its panels, the bodies it ships and the files it opens. A field over the
//! window, what is typed matched against every name (`fuzzy`): "pl" finds
//! Play, "sun pos" the Sun's position -- a setting chosen is shown in the side
//! panel, its field taking what is typed next, and one that is on or off
//! switched --, "ico" the ico spheres to
//! add, "sph tpm" `examples/sphere/tpm.py`, typos forgiven. The
//! arrows move through what matched, `Enter` or a click does it, `Escape` or a
//! click elsewhere shuts it. What is chosen is done by the frame after, as the
//! toolbar's buttons and the scripts tab's rows do it.
//!
//! `E` because the user asked for it over Blender's `F3`, which a Mac's
//! keyboard gives to the system; free in kalast, and only while nothing is
//! typed into.

use super::fuzzy;
use crate::app::config::{Quality, UiTheme};

/// What the search can do: the toolbar's buttons, the keys', the middle's
/// tabs, the side panel's, and switches of the scene.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Play,
    Step,
    Restart,
    Reset,
    Save,
    Open,
    ExportFrame,
    Renderer,
    Editor,
    Docs,
    AppTab,
    SimulationTab,
    ScriptsTab,
    Settings,
    FoldPanels,
    Fullscreen,
    Focus,
    PrintCamera,
    Grid,
    Colorbar,
    ClearLog,
    CheckUpdates,
    GitHub,
}

impl Action {
    const ALL: [Action; 23] = [
        Action::Play,
        Action::Step,
        Action::Restart,
        Action::Reset,
        Action::Save,
        Action::Open,
        Action::ExportFrame,
        Action::Renderer,
        Action::Editor,
        Action::Docs,
        Action::AppTab,
        Action::SimulationTab,
        Action::ScriptsTab,
        Action::Settings,
        Action::FoldPanels,
        Action::Fullscreen,
        Action::Focus,
        Action::PrintCamera,
        Action::Grid,
        Action::Colorbar,
        Action::ClearLog,
        Action::CheckUpdates,
        Action::GitHub,
    ];

    /// Its name in the list, and its keys where it has some.
    fn label(self) -> (&'static str, &'static str) {
        match self {
            Action::Play => ("Play or pause", "P"),
            Action::Step => ("Step one iteration", "K"),
            Action::Restart => ("Restart the script", ""),
            Action::Reset => ("Reset to a new app", ""),
            Action::Save => ("Save the file in the editor", "Ctrl S"),
            Action::Open => ("Open a file...", ""),
            Action::ExportFrame => ("Export one frame", ""),
            Action::Renderer => ("Show the renderer", ""),
            Action::Editor => ("Show the editor", ""),
            Action::Docs => ("Show the documentation", ""),
            Action::AppTab => ("Show the app's settings", ""),
            Action::SimulationTab => ("Show the simulation's settings", ""),
            Action::ScriptsTab => ("Show the scripts", ""),
            Action::Settings => ("Search the settings", "Ctrl F"),
            Action::FoldPanels => ("Fold or unfold the panels", "N"),
            Action::Fullscreen => ("Fullscreen", "F"),
            Action::Focus => ("Focus mode: the window to the scene", "Shift F"),
            Action::PrintCamera => ("Print the camera's state", "H"),
            Action::Grid => ("Show or hide the ground grid", ""),
            Action::Colorbar => ("Show or hide the colour bar", ""),
            Action::ClearLog => ("Clear the log's tab", ""),
            Action::CheckUpdates => ("Check for updates", ""),
            Action::GitHub => ("kalast on GitHub", ""),
        }
    }
}

/// What was chosen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Found {
    Action(Action),
    /// A file, to open in the editor.
    File(String),
    /// A shape kalast ships, `res/...`, to add as a body.
    Body(String),
    Shadows(Quality),
    Theme(UiTheme),
    /// A setting of the panels, to show (`widgets::reveal`).
    Setting(super::widgets::Place),
}

impl Found {
    pub fn action(&self) -> Option<Action> {
        match self {
            Found::Action(a) => Some(*a),
            _ => None,
        }
    }
}

/// The search, open or not.
#[derive(Default)]
pub struct Search {
    open: bool,
    query: String,
    /// The row the arrows are on.
    at: usize,
    /// What it lists beyond the actions, read when it opens: `(found, name)`.
    things: Vec<(Found, String)>,
    files: Vec<(Found, String)>,
}

/// How many rows it shows.
const SHOWN: usize = 14;

impl Search {
    /// Opened and shut by `E` while nothing is typed into; drawn over
    /// everything while open. What was chosen, in the frame it was.
    pub fn show(&mut self, ctx: &egui::Context) -> Option<Found> {
        let free = ctx.memory(|m| m.focused().is_none());
        if free && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::E)) {
            // The letter the key also typed, kept from the field.
            ctx.input_mut(|i| i.events.retain(|e| !matches!(e, egui::Event::Text(t) if t.eq_ignore_ascii_case("e"))));
            self.open = !self.open;
            self.query.clear();
            self.at = 0;
            if self.open {
                self.read(ctx);
            }
        }
        if !self.open {
            return None;
        }
        let (up, down, enter, escape) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                i.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
                i.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
            )
        });
        if escape {
            self.open = false;
            return None;
        }
        // What kalast does and the shapes it ships before the files: a short
        // word is more likely one of those -- "stp" is Step, though it
        // spells the initials of `sphere/tpm.py`.
        let actions = Action::ALL.iter().map(|a| (Found::Action(*a), a.label().0.to_string()));
        // Every setting, as the catalogue drawn the frame it opened found
        // them, by the sections they are in: "Sun › pos X".
        let settings = super::widgets::catalogued(ctx).into_iter().map(|place| {
            let label = place.label();
            (Found::Setting(place), label)
        });
        let named: Vec<(Found, String)> = actions.chain(settings).chain(self.things.iter().cloned()).collect();
        let first = fuzzy::rank(&self.query, named.iter().map(|(f, name)| (f.clone(), name.as_str())));
        let files = fuzzy::rank(&self.query, self.files.iter().map(|(f, name)| (f.clone(), name.as_str())));
        let matched: Vec<Found> = first.into_iter().chain(files).take(SHOWN).collect();
        let name_of = |found: &Found| -> String {
            match found {
                Found::Action(a) => a.label().0.to_string(),
                _ => named.iter().chain(&self.files).find(|(f, _)| f == found).map(|(_, n)| n.clone()).unwrap_or_default(),
            }
        };
        if down {
            self.at += 1;
        }
        if up {
            self.at = self.at.saturating_sub(1);
        }
        self.at = self.at.min(matched.len().saturating_sub(1));
        let mut chosen = enter.then(|| matched.get(self.at).cloned()).flatten();
        let area = egui::Area::new(egui::Id::new("kalast search"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 48.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(460.0);
                    let field = super::widgets::text_field_as(
                        ui,
                        egui::TextEdit::singleline(&mut self.query)
                            .hint_text(format!("{}  search: what kalast does, its settings, its bodies, its files", super::icons::codicon::SEARCH))
                            .desired_width(f32::INFINITY),
                        egui::Id::new("kalast search field"),
                    );
                    field.request_focus();
                    if field.changed() {
                        self.at = 0;
                    }
                    ui.add_space(4.0);
                    if matched.is_empty() {
                        ui.label(egui::RichText::new("nothing matches").weak());
                    }
                    for (row, found) in matched.iter().enumerate() {
                        // A setting that is on or off says which, where an
                        // action's keys go: choosing it switches it.
                        let keys = match found {
                            Found::Setting(place) => match place.on {
                                Some(true) => "on",
                                Some(false) => "off",
                                None => "",
                            },
                            _ => found.action().map_or("", |a| a.label().1),
                        };
                        let icon = match found {
                            Found::Action(_) => super::icons::codicon::PLAY,
                            Found::File(_) => super::icons::codicon::FILE_CODE,
                            Found::Body(_) => super::icons::codicon::ADD,
                            Found::Shadows(_) | Found::Theme(_) | Found::Setting(_) => super::icons::codicon::SETTINGS_GEAR,
                        };
                        let name = name_of(found);
                        let (rect, response) =
                            ui.allocate_exact_size(egui::vec2(ui.available_width(), 22.0), egui::Sense::click());
                        let lit = row == self.at || response.hovered();
                        if lit {
                            ui.painter().rect_filled(rect, 3.0, super::theme::extra(ui.ctx()).fill);
                        }
                        let color = if lit { ui.visuals().strong_text_color() } else { ui.visuals().text_color() };
                        let y = rect.center().y;
                        let weak = ui.visuals().weak_text_color();
                        ui.painter().text(egui::pos2(rect.left() + 12.0, y), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(13.0), weak);
                        let font = egui::TextStyle::Body.resolve(ui.style());
                        let keys_width = if keys.is_empty() {
                            0.0
                        } else {
                            ui.painter().text(egui::pos2(rect.right() - 8.0, y), egui::Align2::RIGHT_CENTER, keys, font.clone(), weak).width() + 16.0
                        };
                        // Cut short, with an ellipsis, rather than past the
                        // field's edge: a path, a body's long name.
                        let mut job = egui::text::LayoutJob::single_section(
                            name,
                            egui::TextFormat { font_id: font, color, ..Default::default() },
                        );
                        job.wrap = egui::text::TextWrapping::truncate_at_width((rect.width() - 26.0 - 8.0 - keys_width).max(20.0));
                        let galley = ui.painter().layout_job(job);
                        ui.painter().galley(egui::pos2(rect.left() + 26.0, y - galley.size().y / 2.0), galley, color);
                        if response.clicked() {
                            chosen = Some(found.clone());
                        }
                    }
                });
            });
        // A click elsewhere shuts it, as one shuts a menu.
        let elsewhere = ctx.input(|i| i.pointer.any_click())
            && ctx.pointer_interact_pos().is_some_and(|p| !area.response.rect.contains(p));
        if chosen.is_some() || elsewhere {
            self.open = false;
        }
        chosen
    }

    /// What the search lists beyond the actions, as things stand when it
    /// opens: the shapes to add, the shadows' presets, the themes, and the
    /// files.
    fn read(&mut self, ctx: &egui::Context) {
        super::widgets::request_catalogue(ctx);
        let bodies = super::simulation_panel::shapes().into_iter().map(|(name, path)| (Found::Body(path), format!("Add body: {name}")));
        let shadows = Quality::ALL.iter().map(|q| (Found::Shadows(*q), format!("Shadows: {}", q.name())));
        let themes = [(UiTheme::Blender, "Blender"), (UiTheme::CatppuccinMocha, "Catppuccin Mocha"), (UiTheme::Dark, "Dark")]
            .into_iter()
            .map(|(t, name)| (Found::Theme(t), format!("Theme: {name}")));
        self.things = bodies.chain(shadows).chain(themes).collect();
        self.files = files().into_iter().map(|f| (Found::File(f.clone()), f)).collect();
    }
}

/// The files of `examples/` -- not `examples/old/`, which is superseded -- and
/// of `scripts/`, as the scripts tab names them.
fn files() -> Vec<String> {
    let mut found = Vec::new();
    let mut folders: Vec<std::path::PathBuf> = vec!["examples".into(), "scripts".into()];
    while let Some(dir) = folders.pop() {
        let Ok(entries) = crate::fs::list(&dir) else { continue };
        for (name, is_dir) in entries {
            if name.starts_with('.') || name == "__pycache__" {
                continue;
            }
            let path = dir.join(&name);
            if is_dir {
                if path != std::path::Path::new("examples/old") {
                    folders.push(path);
                }
            } else {
                found.push(path.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    found.sort();
    found
}
