// use egui::Checkbox;
use egui_backend::egui;
use egui_sdl2_gl::{self as egui_backend, egui::Ui};

pub struct GUI {
    pub quit: bool,
    pub test_str: String,
    pub slider: f64,
}

impl GUI {
    pub fn new() -> Self {
        let quit = false;
        let test_str: String =
            "A text box to write in. Cut, copy, paste commands are available.".to_owned();
        let slider = 0.0;
        Self {
            quit,
            test_str,
            slider,
        }
    }

    pub fn central_panel(&mut self, ui: &mut Ui) {
        ui.label(" ");
        ui.text_edit_multiline(&mut self.test_str);
        ui.label(" ");
        ui.add(egui::Slider::new(&mut self.slider, 0.0..=50.0).text("Slider"));
        ui.separator();
        if ui.button("Quit?").clicked() {
            self.quit = true;
        }
    }
}
