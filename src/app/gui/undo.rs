//! `Ctrl+Z` (`Cmd+Z`): undo for what the panels change, as Blender undoes a
//! property's change; `Ctrl+Shift+Z` does it again.
//!
//! What it covers: the simulation's config, `app.simulation.config` -- the
//! shading, the light, the shadows, the colours, the grid, the HUDs' style,
//! the export and the rest. Not the camera nor the Sun, which turning the
//! view moves and which Blender does not undo either, nor the bodies, whose
//! meshes are far too big to keep copies of, nor the app's own settings: its
//! panels and its theme are the window's, not the work's.
//!
//! A step is a change made while the frame draws -- by a widget, since a
//! script never runs then -- and lasts while something is held: a drag is one
//! step, not one a frame, and so is typing into a field until it lets go.
//! The keys are left alone while a field is typed into, so the editor and the
//! fields keep their own undo.

use crate::app::config::Config;

/// How many steps are kept, the oldest dropped first.
const STEPS: usize = 100;

#[derive(Default)]
pub struct Undo {
    /// The config before each step, the latest last.
    done: Vec<Config>,
    /// The config each undo replaced, for `Ctrl+Shift+Z`.
    undone: Vec<Config>,
    /// The config before the step being made, while it is.
    making: Option<Config>,
}

impl Undo {
    /// The keys among the frame's input, taken out of it and done on `config`
    /// before the frame draws: an undo puts back the config before the last
    /// step, a redo the one the undo replaced. Left in the input while
    /// `typing`, a field having the focus.
    pub fn keys(&mut self, events: &mut Vec<egui::Event>, typing: bool, config: &mut Config) {
        if typing {
            return;
        }
        events.retain(|event| match event {
            egui::Event::Key { key: egui::Key::Z, pressed: true, modifiers, .. } if modifiers.command => {
                let (from, to) = if modifiers.shift {
                    (&mut self.undone, &mut self.done)
                } else {
                    (&mut self.done, &mut self.undone)
                };
                if let Some(config_then) = from.pop() {
                    to.push(std::mem::replace(config, config_then));
                }
                false
            }
            _ => true,
        });
    }

    /// A step made all at once, outside the frame's drawing -- the search's
    /// switches -- from `before`.
    pub fn push(&mut self, before: Config) {
        self.undone.clear();
        self.done.push(before);
        if self.done.len() > STEPS {
            self.done.remove(0);
        }
    }

    /// After the frame drew: a change made while it did, from `before`, is a
    /// step, ended once nothing is `holding` it -- the pointer let go, no
    /// field typed into. A new step drops what was undone.
    pub fn record(&mut self, before: Config, config: &Config, holding: bool) {
        if *config != before {
            self.making.get_or_insert(before);
            self.undone.clear();
        }
        if holding {
            return;
        }
        if let Some(before) = self.making.take().filter(|before| before != config) {
            self.done.push(before);
            if self.done.len() > STEPS {
                self.done.remove(0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn z(shift: bool) -> egui::Event {
        let modifiers = egui::Modifiers { command: true, shift, ..Default::default() };
        egui::Event::Key { key: egui::Key::Z, physical_key: None, pressed: true, repeat: false, modifiers }
    }

    /// A drag is one step: undone in one, done again in one.
    #[test]
    fn a_drag_is_one_step() {
        let (mut undo, mut config) = (Undo::default(), Config::default());
        let start = config.light.ambient;
        for frame in 1..=5 {
            let before = config.clone();
            config.light.ambient = start + frame as f32 * 0.01;
            undo.record(before, &config, true);
        }
        undo.record(config.clone(), &config, false);
        let dragged = config.light.ambient;
        undo.keys(&mut vec![z(false)], false, &mut config);
        assert_eq!(config.light.ambient, start, "back to before the drag");
        undo.keys(&mut vec![z(true)], false, &mut config);
        assert_eq!(config.light.ambient, dragged, "and done again");
    }

    /// The keys belong to a field while one is typed into, and the events
    /// stay for it.
    #[test]
    fn typing_keeps_the_keys() {
        let (mut undo, mut config) = (Undo::default(), Config::default());
        let before = config.clone();
        config.light.ambient += 0.1;
        undo.record(before, &config, false);
        let mut events = vec![z(false)];
        undo.keys(&mut events, true, &mut config);
        assert_eq!(events.len(), 1, "left for the field");
        assert_ne!(config.light.ambient, Config::default().light.ambient, "nothing undone");
    }

    /// A change after an undo starts a new history: there is nothing to redo.
    #[test]
    fn a_new_step_drops_what_was_undone() {
        let (mut undo, mut config) = (Undo::default(), Config::default());
        let before = config.clone();
        config.light.ambient += 0.1;
        undo.record(before, &config, false);
        undo.keys(&mut vec![z(false)], false, &mut config);
        let before = config.clone();
        config.light.exposure += 0.1;
        undo.record(before, &config, false);
        let now = config.clone();
        undo.keys(&mut vec![z(true)], false, &mut config);
        assert_eq!(config, now, "no redo past a new step");
    }
}
