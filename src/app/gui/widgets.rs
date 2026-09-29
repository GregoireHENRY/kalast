//! The side panel's building blocks, on the files tab's grid: every row is
//! the explorer's -- 22 points, no gap to the next, lit under the pointer --
//! so the three tabs read as one list. A section is a folder's row: a
//! chevron, its icon, its title; what it holds is indented under it with a
//! guide, as a folder's files are. A setting is its name in a column of its
//! own on the left and its control filling the right, as VS Code lays out
//! its settings.
//!
//! One layout for every setting, generated (`config_panel.rs`) or not
//! (`simulation_panel.rs`). The generated rows used to each lay themselves
//! out -- a checkbox's name after the box, a slider's to its right, a drag
//! field's as a prefix -- so no two lined up; and sections of 26 points with
//! egui's gap under each stood 29 apart where the explorer's rows are 22.

use super::icons;
use super::{INDENT, ROW, TWISTIE};

/// The least a setting's control is left, beside its name: a checkbox, a
/// short value. A name is cut short only to leave this -- 120 points, room
/// for a slider, cut `simulation_folded` beside a checkbox in a panel with
/// room to spare; a slider makes do with less, the panel there to be widened.
const CONTROL_MIN: f32 = 48.0;

/// The column of the names of the settings drawn in one `Ui` -- a section's
/// -- as wide as the widest of them, so each shows whole where the panel has
/// room and the controls beside them line up. Measured as they are drawn and
/// used from the next pass on: one that finds it wider asks egui to draw the
/// frame again before showing it, so the first frame is not seen askew.
fn names_column(ui: &egui::Ui, wanted: f32) -> f32 {
    let key = ui.id().with("setting names");
    let pass = ui.ctx().cumulative_pass_nr();
    // The widest of the pass before, the widest of this one so far, and
    // which pass this is.
    let (mut before, mut now, at) = ui.data(|d| d.get_temp::<(f32, f32, u64)>(key)).unwrap_or((0.0, 0.0, pass));
    if at != pass {
        before = now;
        now = 0.0;
    }
    now = now.max(wanted);
    ui.data_mut(|d| d.insert_temp(key, (before, now, pass)));
    if wanted > before {
        ui.ctx().request_discard("a setting's name wider than its column");
    }
    before.max(wanted)
}

/// What a row is lit in under the pointer: the explorer's hover, from the
/// theme's faint background.
fn hover_fill(ui: &egui::Ui) -> egui::Color32 {
    ui.visuals().faint_bg_color.gamma_multiply(0.6)
}

/// What a section's row shows before its title.
pub enum Icon {
    /// A Codicon, in its colour.
    Codicon(&'static str, egui::Color32),
    /// A file's icon, as the files tab shows it.
    Image(egui::ImageSource<'static>),
}

/// A section of the panel, closed until clicked: the explorer's folder row --
/// a chevron, the icon, the title, lit under the pointer, a click anywhere on
/// it opening or shutting it -- and under it, indented with a guide, what it
/// holds. What is open is kept for the session, by `key`. Returns the row's
/// response, for a hover on it.
pub fn section(
    ui: &mut egui::Ui,
    icon: Icon,
    title: &str,
    key: impl std::hash::Hash + std::fmt::Debug,
    add: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    let id = ui.make_persistent_id(("section", key));
    let mut open = ui.data_mut(|d| *d.get_persisted_mut_or(id, false));
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), ROW), egui::Sense::click());
    let painter = ui.painter_at(rect);
    if response.hovered() {
        painter.rect_filled(rect, 0.0, hover_fill(ui));
    }
    let y = rect.center().y;
    let text = ui.visuals().text_color();
    painter.text(
        egui::pos2(rect.left() + TWISTIE / 2.0, y),
        egui::Align2::CENTER_CENTER,
        if open { icons::codicon::CHEVRON_DOWN } else { icons::codicon::CHEVRON_RIGHT },
        egui::FontId::proportional(14.0),
        text,
    );
    // Where the explorer puts a file's icon and its name.
    let icon_x = rect.left() + TWISTIE + 2.0 + 8.0;
    match icon {
        Icon::Codicon(glyph, color) => {
            painter.text(egui::pos2(icon_x, y), egui::Align2::CENTER_CENTER, glyph, egui::FontId::proportional(16.0), color);
        }
        Icon::Image(source) => {
            egui::Image::new(source).paint_at(ui, egui::Rect::from_center_size(egui::pos2(icon_x, y), egui::vec2(16.0, 16.0)));
        }
    }
    let mut job = egui::text::LayoutJob::single_section(
        title.to_owned(),
        egui::TextFormat { font_id: egui::TextStyle::Body.resolve(ui.style()), color: text, ..Default::default() },
    );
    let name_x = rect.left() + TWISTIE + 2.0 + 16.0 + 6.0;
    job.wrap = egui::text::TextWrapping::truncate_at_width((rect.right() - name_x - 4.0).max(0.0));
    let galley = painter.layout_job(job);
    painter.galley(egui::pos2(name_x, y - galley.size().y / 2.0), galley, text);
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    if response.clicked() {
        open = !open;
        ui.data_mut(|d| d.insert_persisted(id, open));
    }
    if open {
        let top = ui.cursor().top();
        let inner = egui::Frame::NONE
            .inner_margin(egui::Margin { left: INDENT as i8, right: 0, top: 0, bottom: 0 })
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 0.0;
                add(ui);
            });
        // The guide, through the chevron's column, as the explorer joins a
        // folder to its files.
        let bottom = inner.response.rect.bottom();
        let guide = ui.visuals().widgets.noninteractive.bg_stroke.color;
        ui.painter().vline(rect.left() + TWISTIE / 2.0, egui::Rangef::new(top, bottom), egui::Stroke::new(1.0, guide));
    }
    response
}

/// The names' column in a row `width` wide: all it asks for, as long as the
/// control keeps `CONTROL_MIN` -- and never under 72 points, which a name
/// needs to be told apart from the next.
fn name_width(column: f32, width: f32) -> f32 {
    column.min(width - CONTROL_MIN).max(72.0_f32.min(width))
}

/// One setting: its name dim on the left, whole unless the panel is too
/// narrow for it and its control both -- then cut short with an ellipsis --
/// and its doc on hover; `add` draws the control, filling the rest of the
/// row. The names of a section share one column (`names_column`). The row is
/// lit under the pointer, as a list row is.
pub fn setting<R>(ui: &mut egui::Ui, name: &str, hover: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let width = ui.available_width();
    let font = egui::TextStyle::Body.resolve(ui.style());
    // Its own width, and the 6 points before it and 8 after.
    let natural = ui.fonts_mut(|f| f.layout_no_wrap(name.to_owned(), font.clone(), egui::Color32::PLACEHOLDER).size().x);
    let name_width = name_width(names_column(ui, natural + 14.0), width);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, ROW), egui::Sense::hover());
    if ui.rect_contains_pointer(rect) {
        ui.painter().rect_filled(rect, 0.0, hover_fill(ui));
    }
    let name_rect = egui::Rect::from_min_size(rect.min + egui::vec2(6.0, 0.0), egui::vec2(name_width - 6.0, ROW));
    let mut job = egui::text::LayoutJob::single_section(
        name.to_owned(),
        egui::TextFormat { font_id: font, color: ui.visuals().weak_text_color(), ..Default::default() },
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width(name_rect.width() - 8.0);
    let galley = ui.painter().layout_job(job);
    ui.painter().galley(
        egui::pos2(name_rect.left(), name_rect.center().y - galley.size().y / 2.0),
        galley,
        egui::Color32::PLACEHOLDER,
    );
    if !hover.is_empty() {
        let id = ui.auto_id_with(("setting", name));
        ui.interact(name_rect, id, egui::Sense::hover()).on_hover_text(hover);
    }
    let control = egui::Rect::from_min_max(egui::pos2(rect.left() + name_width, rect.top()), rect.max);
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(control)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    // A slider fills what its value box leaves; a list and a text field
    // what there is -- a text field took egui's 280 points, past the
    // panel's edge.
    child.spacing_mut().slider_width = (control.width() - 64.0).max(40.0);
    child.spacing_mut().combo_width = control.width() - 4.0;
    child.spacing_mut().text_edit_width = (control.width() - 8.0).max(40.0);
    add(&mut child)
}

/// A row of the grid holding whatever `add` puts in it, left to right --
/// buttons, a list of values -- for what is not a named setting.
pub fn line<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, ROW), egui::Sense::hover());
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(egui::vec2(6.0, 0.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    add(&mut child)
}

/// A dim remark on a row of its own: "none loaded", what a click does.
pub fn note(ui: &mut egui::Ui, text: &str) {
    line(ui, |ui| {
        ui.add(egui::Label::new(egui::RichText::new(text).weak().small()).truncate());
    });
}

/// A quiet heading inside a section, between an entity and its settings:
/// small capitals, dim, on a row of the grid, as VS Code heads the groups in
/// its side bar.
pub fn subheading(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), ROW), egui::Sense::hover());
    ui.painter().text(
        egui::pos2(rect.left() + 6.0, rect.center().y + 2.0),
        egui::Align2::LEFT_CENTER,
        label.to_uppercase(),
        egui::FontId::proportional(10.5),
        ui.visuals().weak_text_color(),
    );
    response
}

/// A tab that is an icon alone, as VS Code's activity bar is: dim until it is
/// chosen or under the pointer, the accent under the chosen one.
pub fn icon_tab(ui: &mut egui::Ui, selected: bool, icon: &str, accent: egui::Color32) -> egui::Response {
    let size = egui::vec2(32.0, 28.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let color = if selected || response.hovered() {
        ui.visuals().strong_text_color()
    } else {
        ui.visuals().weak_text_color()
    };
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(18.0), color);
    if selected {
        ui.painter().hline(rect.x_range().shrink(6.0), rect.bottom() - 1.0, egui::Stroke::new(2.0, accent));
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A section's names share a column as wide as the widest of them, from
    /// the first frame shown: the pass that finds it is drawn again before
    /// anything is shown, and a steady panel is drawn once.
    #[test]
    fn a_sections_names_share_a_column_as_wide_as_the_widest() {
        let ctx = egui::Context::default();
        let names = ["focus", "panels_folded", "simulation_folded", "toolbar text"];
        let frame = || {
            let mut column = 0.0;
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(600.0, 400.0))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(raw, |ui| {
                for name in names {
                    setting(ui, name, "", |ui| ui.label("x"));
                }
                column = ui.data(|d| d.get_temp::<(f32, f32, u64)>(ui.id().with("setting names"))).unwrap().0;
            });
            output.textures_delta.clear();
            (column, output.platform_output.num_completed_passes)
        };
        let (first, passes) = frame();
        let font = egui::TextStyle::Body.resolve(&ctx.global_style());
        let widest = ctx.fonts_mut(|f| {
            names.iter().map(|n| f.layout_no_wrap(n.to_string(), font.clone(), egui::Color32::WHITE).size().x).fold(0.0, f32::max)
        });
        assert_eq!(first, widest + 14.0, "the widest name, whole");
        assert_eq!(passes, 2, "drawn again before it was shown");
        assert_eq!(frame(), (first, 1), "steady: once");
    }

    /// Whole where there is room; cut only to leave the control its least.
    #[test]
    fn a_name_is_cut_only_to_leave_its_control_room() {
        assert_eq!(name_width(134.0, 400.0), 134.0);
        assert_eq!(name_width(134.0, 235.0), 134.0, "a side panel of 235: whole, beside a checkbox");
        assert_eq!(name_width(134.0, 160.0), 112.0, "a narrow one: the control keeps 48");
        assert_eq!(name_width(134.0, 110.0), 72.0, "never under 72");
        assert_eq!(name_width(134.0, 60.0), 60.0, "nor over the row");
    }
}
