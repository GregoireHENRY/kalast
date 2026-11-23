//! The side panel's building blocks, drawn after Blender's (the user: "yes
//! restyle the panels like blender"). A section is Blender's panel: a rounded
//! box, its header a disclosure triangle, an icon and a title, what it holds
//! inside under the header; one inside another is a sub-panel, a shade darker
//! within it. A setting is Blender's split row: its name right-aligned in a
//! column of its own, against its control on the right. The controls are
//! Blender's too: a checkbox blue when ticked, a slider filled in blue as far
//! as its value, a menu's button darker than the other buttons, and tabs on a
//! dark strip, the chosen one in the colour of what it opens.
//!
//! One layout for every setting, generated (`config_panel.rs`) or not
//! (`simulation_panel.rs`). The generated rows used to each lay themselves
//! out -- a checkbox's name after the box, a slider's to its right, a drag
//! field's as a prefix -- so no two lined up. Rows are 22 points, Blender's
//! 20-point widgets with its 2 between them.

use super::{fuzzy, icons, ROW};

/// Blender's split row (`use_property_split`): the label column is 0.4 of
/// what the row leaves after the decorators' column, 20 -- empty here, kalast
/// having no keyframes, a margin as in Blender -- and the 8 between label
/// and field (`columnspace`); the field has the rest.
const SPLIT: f32 = 0.4;
const LABEL_GAP: f32 = 8.0;
const DECORATOR: f32 = 20.0;

/// `(label width, the field's x from the row's left, field width)`, in a row
/// `width` wide.
fn split(width: f32) -> (f32, f32, f32) {
    let ws = (width - DECORATOR).max(0.0);
    let label = ((ws - LABEL_GAP).max(0.0) * SPLIT).floor();
    (label, label + LABEL_GAP, (ws - label - LABEL_GAP).max(0.0))
}

/// The settings' search, as Blender's properties editor has it: what is typed in
/// the field atop the side panel (`search_field`). While it holds words, a
/// setting shows only if its name matches them (`fuzzy`), and a section only
/// if something in it matches -- open then, whatever it was -- or its title
/// does, which shows all of it.
fn search_id() -> egui::Id {
    egui::Id::new("kalast settings search")
}

/// The words searched for, if any.
fn searched(ui: &egui::Ui) -> Option<String> {
    ui.data(|d| d.get_temp::<String>(search_id())).filter(|q| !q.trim().is_empty())
}

/// The sections being drawn while searching, innermost last: how many of the
/// settings in each matched, and whether its title did, or one around it.
fn search_stack() -> egui::Id {
    egui::Id::new("kalast settings search stack")
}

/// Whether a section around the row being drawn matched by its title.
fn inside_a_match(ui: &egui::Ui) -> bool {
    ui.data(|d| d.get_temp::<Vec<(u32, bool)>>(search_stack())).and_then(|s| s.last().copied()).is_some_and(|(_, t)| t)
}

/// A setting matched: counted in the section it is in.
fn count_match(ui: &egui::Ui) {
    ui.data_mut(|d| {
        if let Some(top) = d.get_temp_mut_or_default::<Vec<(u32, bool)>>(search_stack()).last_mut() {
            top.0 += 1;
        }
    });
}

/// Where a row that does not match is drawn while searching: a child the
/// parent gives no room, drawn invisibly, so that what draws it still runs
/// and returns what it returns.
fn hidden(ui: &mut egui::Ui) -> egui::Ui {
    let rect = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(ui.available_width().max(1.0), ROW));
    ui.new_child(egui::UiBuilder::new().max_rect(rect).invisible())
}

/// A response for a row not shown, named `what`: no room taken, nothing
/// hovered.
fn nothing(ui: &mut egui::Ui, what: &str) -> egui::Response {
    ui.interact(egui::Rect::NOTHING, ui.id().with(("kalast not shown", what)), egui::Sense::hover())
}

/// A text field as Blender's: on its darkest grey, darker still while typed
/// into, its outline the same either way -- egui rings a field with focus in
/// white.
pub fn text_field(ui: &mut egui::Ui, edit: egui::TextEdit<'_>) -> egui::Response {
    let id = ui.auto_id_with("kalast text field");
    text_field_as(ui, edit, id)
}

/// `text_field`, under `id`: for a field given the focus from elsewhere.
pub fn text_field_as(ui: &mut egui::Ui, edit: egui::TextEdit<'_>, id: egui::Id) -> egui::Response {
    let editing = ui.memory(|m| m.has_focus(id));
    ui.scope(|ui| {
        let editing_fill = super::theme::extra(ui.ctx()).editing;
        let v = ui.visuals_mut();
        v.selection.stroke = v.widgets.inactive.bg_stroke;
        if editing {
            v.text_edit_bg_color = Some(editing_fill);
        }
        ui.add(edit.id(id))
    })
    .inner
}

/// The search field atop the side panel, `Ctrl+F` (`Cmd+F`) while the
/// pointer is over `panel`; `Escape` empties it.
pub fn search_field(ui: &mut egui::Ui, panel: egui::Rect) {
    let id = egui::Id::new("kalast settings search field");
    let mut query = ui.data(|d| d.get_temp::<String>(search_id())).unwrap_or_default();
    let field = text_field_as(
        ui,
        egui::TextEdit::singleline(&mut query)
            .hint_text(format!("{}  search the settings", icons::codicon::SEARCH))
            .desired_width(f32::INFINITY),
        id,
    );
    let pointed = ui.ctx().pointer_hover_pos().is_some_and(|p| panel.contains(p));
    if pointed && ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::F)) {
        field.request_focus();
    }
    if field.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        query.clear();
    }
    ui.data_mut(|d| d.insert_temp(search_id(), query));
}

/// The titles of the sections around what is being drawn, outermost first:
/// a setting is found by them too ("sun pos" is the Sun's `pos X`), and
/// listed with them (`Place`).
fn path_id() -> egui::Id {
    egui::Id::new("kalast section path")
}

fn path(ui: &egui::Ui) -> Vec<String> {
    ui.data(|d| d.get_temp::<Vec<String>>(path_id())).unwrap_or_default()
}

/// Where a setting is: on the app tab or the simulation's, in which
/// sections, under which name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Place {
    pub app: bool,
    pub path: Vec<String>,
    pub name: String,
    /// A setting that is on or off: whether it was, as catalogued.
    pub on: Option<bool>,
}

impl Place {
    /// As the search lists it: "Sun › Light › exposure"; a name the path
    /// repeats, a tab and its panel of the same name, once: "Sun › pos X".
    pub fn label(&self) -> String {
        let mut parts: Vec<&str> = Vec::new();
        for part in self.path.iter().chain(std::iter::once(&self.name)) {
            if parts.last() != Some(&part.as_str()) {
                parts.push(part);
            }
        }
        parts.join(" \u{203a} ")
    }
}

fn catalogue_id() -> egui::Id {
    egui::Id::new("kalast catalogue")
}

/// While the catalogue is being drawn: which tab's.
fn cataloguing(ui: &egui::Ui) -> Option<bool> {
    ui.data(|d| d.get_temp::<bool>(egui::Id::new("kalast cataloguing")))
}

/// Ask for every setting's `Place`, drawn by the next frame (`catalogue`).
pub fn request_catalogue(ctx: &egui::Context) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("kalast catalogue wanted"), true));
}

/// Every setting's `Place`, as the last catalogue found them.
pub fn catalogued(ctx: &egui::Context) -> Vec<Place> {
    ctx.data(|d| d.get_temp::<Vec<Place>>(catalogue_id())).unwrap_or_default()
}

/// When asked (`request_catalogue`): the app tab and the simulation's drawn
/// once more, invisibly, every section open, each setting noting its `Place`
/// as it is drawn -- the hand-written as well as the generated. Under ids of
/// their own, so that nothing of the panels shown is touched.
pub fn catalogue(ui: &mut egui::Ui, app: &mut dyn FnMut(&mut egui::Ui), simulation: &mut dyn FnMut(&mut egui::Ui)) {
    let wanted = egui::Id::new("kalast catalogue wanted");
    if !ui.data(|d| d.get_temp::<bool>(wanted)).unwrap_or(false) {
        return;
    }
    ui.data_mut(|d| {
        d.remove::<bool>(wanted);
        d.insert_temp(catalogue_id(), Vec::<Place>::new());
    });
    fn pass(ui: &mut egui::Ui, is_app: bool, draw: &mut dyn FnMut(&mut egui::Ui)) {
        ui.data_mut(|d| d.insert_temp(egui::Id::new("kalast cataloguing"), is_app));
        ui.push_id(("kalast catalogue", is_app), |ui| {
            let rect = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(ui.available_width().max(1.0), 1.0));
            draw(&mut ui.new_child(egui::UiBuilder::new().max_rect(rect).invisible()));
        });
        ui.data_mut(|d| d.remove::<bool>(egui::Id::new("kalast cataloguing")));
    }
    pass(ui, true, app);
    pass(ui, false, simulation);
}

fn reveal_id() -> egui::Id {
    egui::Id::new("kalast reveal")
}

/// Show the setting at `place`: its sections opened, the panel scrolled to
/// it, its field taking typed text -- what the search does with a setting
/// chosen in it. The settings' own search is emptied, so that it is not
/// hidden.
pub fn reveal(ctx: &egui::Context, place: Place) {
    ctx.data_mut(|d| {
        d.insert_temp(reveal_id(), place);
        d.insert_temp(search_id(), String::new());
    });
}

/// The setting being revealed, until its row is drawn: for the tab it is in
/// to be chosen first.
pub fn revealing(ctx: &egui::Context) -> Option<Place> {
    ctx.data(|d| d.get_temp::<Place>(reveal_id()))
}

/// The field of a revealed setting takes the focus, the first that asks.
fn take_focus(ui: &egui::Ui) -> bool {
    ui.data_mut(|d| d.remove_temp::<bool>(egui::Id::new("kalast focus field"))).unwrap_or(false)
}

/// A field given the focus (`focused`, the frame it is), its text all
/// selected the frame after, once egui keeps a state for it: so that what is
/// typed replaces the value rather than going after it -- "5" made 0.00 into
/// 0.005. egui selects it itself only for a focus taken before the field is
/// drawn.
fn select_all_after(ui: &egui::Ui, response: &egui::Response, focused: bool) {
    let key = egui::Id::new("kalast select all");
    if focused {
        ui.data_mut(|d| d.insert_temp(key, response.id));
        return;
    }
    if ui.data(|d| d.get_temp::<egui::Id>(key)) != Some(response.id) {
        return;
    }
    if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), response.id) {
        let all = egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(4096));
        state.cursor.set_char_range(Some(all));
        state.store(ui.ctx(), response.id);
        ui.data_mut(|d| d.remove::<egui::Id>(key));
    }
}

/// A panel's header, as tall as Blender's (`PNL_HEADER`, 25 at 1x though its
/// comment says 24).
const HEADER: f32 = 25.0;

/// How deep in sections the one being drawn is, while it draws what it holds.
fn depth_id() -> egui::Id {
    egui::Id::new("kalast section depth")
}

/// Where the content of the section being drawn begins, under its padding.
fn section_top_id() -> egui::Id {
    egui::Id::new("kalast section top")
}

/// A tree folder's disclosure, as Blender's outliner draws it: its thin
/// chevron, centred on `c`.
pub fn twistie(painter: &egui::Painter, c: egui::Pos2, open: bool, color: egui::Color32) {
    chevron(painter, c, open, color.gamma_multiply(0.8));
}

/// A section of the panel, closed until clicked: Blender's panel -- a rounded
/// box a shade lighter than the region, its header a disclosure triangle, the
/// icon and the title, a click anywhere on it opening or shutting it -- and
/// under the header, inside the box, what it holds. One inside another is
/// Blender's sub-panel, a shade darker within its panel. What is open is kept
/// for the session, by `key`. Returns the header's response, for a hover on
/// it.
pub fn section(
    ui: &mut egui::Ui,
    title: &str,
    key: impl std::hash::Hash + std::fmt::Debug,
    add: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    section_opened(ui, title, key, false, add)
}

/// A panel of a tab's page (`tab_page`): a `section`, open until its header
/// is clicked, as most of Blender's panels are -- the tabs are the table of
/// contents now, where a closed section each was.
pub fn panel(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) -> egui::Response {
    section_opened(ui, title, title, true, add)
}

fn section_opened(
    ui: &mut egui::Ui,
    title: &str,
    key: impl std::hash::Hash + std::fmt::Debug,
    open_at_first: bool,
    add: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    let id = ui.make_persistent_id(("section", key));
    let depth = ui.data(|d| d.get_temp::<u32>(depth_id())).unwrap_or(0);
    ui.data_mut(|d| d.get_temp_mut_or_default::<Vec<String>>(path_id()).push(title.to_owned()));
    let response = section_in(ui, title, id, depth, open_at_first, add);
    ui.data_mut(|d| d.get_temp_mut_or_default::<Vec<String>>(path_id()).pop());
    response
}

/// `section`, its title on the path.
fn section_in(
    ui: &mut egui::Ui,
    title: &str,
    id: egui::Id,
    depth: u32,
    open_at_first: bool,
    add: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    // Catalogued: drawn open, whatever it is.
    if cataloguing(ui).is_some() {
        return draw_section(ui, title, id, depth, &mut true, true, add);
    }
    let mut open = ui.data_mut(|d| *d.get_persisted_mut_or(id, open_at_first));
    // On the way to a setting revealed: opened, and left open.
    let here = path(ui);
    if ui.data(|d| d.get_temp::<Place>(reveal_id())).is_some_and(|t| t.path.starts_with(&here)) && !open {
        open = true;
        ui.data_mut(|d| d.insert_persisted(id, true));
    }
    // Searching: shown, and open, if its title or something in it matches.
    // What matched is counted as it is drawn, so a section is shown from the
    // count of the pass before; one hidden is drawn invisibly to count.
    let search = searched(ui).map(|q| {
        let titled = inside_a_match(ui) || fuzzy::score(&q, title).is_some();
        let before = ui.data(|d| d.get_temp::<u32>(id.with("matched"))).unwrap_or(0);
        ui.data_mut(|d| d.get_temp_mut_or_default::<Vec<(u32, bool)>>(search_stack()).push((0, titled)));
        (titled, before)
    });
    if let Some((titled, before)) = search {
        let response = if titled || before > 0 {
            let mut open = true;
            draw_section(ui, title, id, depth, &mut open, true, add)
        } else {
            add(&mut hidden(ui));
            nothing(ui, title)
        };
        let (matched, _) = ui
            .data_mut(|d| d.get_temp_mut_or_default::<Vec<(u32, bool)>>(search_stack()).pop())
            .unwrap_or_default();
        // What matched here matched in the section around too.
        ui.data_mut(|d| {
            if let Some(top) = d.get_temp_mut_or_default::<Vec<(u32, bool)>>(search_stack()).last_mut() {
                top.0 += matched;
            }
            d.insert_temp(id.with("matched"), matched);
        });
        if (matched > 0) != (before > 0) && !titled {
            ui.ctx().request_discard("a section's search matches changed");
        }
        return response;
    }
    draw_section(ui, title, id, depth, &mut open, false, add)
}

/// One tab's page of panels -- a topic of the simulation tab -- its title on
/// the path, so that its settings are found and listed by it ("Sun › Light
/// › exposure"). Drawn when `selected`; otherwise only when catalogued, and,
/// while the settings are searched, invisibly, to count what in it matches,
/// for its tab (`tab_matches`). A page not shown costs nothing.
pub fn tab_page(ui: &mut egui::Ui, title: &str, selected: bool, add: impl FnOnce(&mut egui::Ui)) {
    let cataloguing = cataloguing(ui).is_some();
    let search = searched(ui).filter(|_| !cataloguing);
    if !selected && !cataloguing && search.is_none() {
        return;
    }
    ui.data_mut(|d| d.get_temp_mut_or_default::<Vec<String>>(path_id()).push(title.to_owned()));
    match search {
        Some(q) => {
            let titled = fuzzy::score(&q, title).is_some();
            ui.data_mut(|d| d.get_temp_mut_or_default::<Vec<(u32, bool)>>(search_stack()).push((0, titled)));
            if selected {
                add(ui);
            } else {
                add(&mut hidden(ui));
            }
            let (matched, _) = ui
                .data_mut(|d| d.get_temp_mut_or_default::<Vec<(u32, bool)>>(search_stack()).pop())
                .unwrap_or_default();
            let matched = if titled { matched.max(1) } else { matched };
            let id = tab_matches_id(title);
            let before = ui.data(|d| d.get_temp::<u32>(id));
            ui.data_mut(|d| d.insert_temp(id, matched));
            if before.is_none_or(|b| (b > 0) != (matched > 0)) {
                ui.ctx().request_discard("a tab's search matches changed");
            }
        }
        None => add(ui),
    }
    ui.data_mut(|d| d.get_temp_mut_or_default::<Vec<String>>(path_id()).pop());
}

fn tab_matches_id(title: &str) -> egui::Id {
    egui::Id::new(("kalast tab matches", title))
}

/// While the settings are searched, how many of a tab's settings matched,
/// as its page counted them the pass before; `None` while nothing is.
pub fn tab_matches(ctx: &egui::Context, title: &str) -> Option<u32> {
    let searching = ctx.data(|d| d.get_temp::<String>(search_id())).is_some_and(|q| !q.trim().is_empty());
    searching.then(|| ctx.data(|d| d.get_temp::<u32>(tab_matches_id(title))).unwrap_or(0))
}

/// A section drawn, as Blender's panel, by the numbers its source gives at
/// 1x: a header 25 high, the body padded 8 on every side, corners of 4 -- its
/// panel roundness, 0.4, of half its 20-pixel unit -- no outline, which 4.x
/// does not draw, and 2 between panels. A thin chevron at 80 % of the title's
/// colour, the title at 22, Regular, with Blender's shadow under it, and on a
/// top-level panel its grip, four dots by two, at the right. A sub-panel
/// spans its parent's width, its header bare, its chevron and title 14
/// further in, its body a shade darker -- `#0000001f` over the panel -- with
/// round bottom corners, and sub-panels follow each other with no gap.
/// `open` is toggled by a click on the header, and kept, unless a search
/// holds it open.
#[allow(clippy::too_many_arguments)]
fn draw_section(
    ui: &mut egui::Ui,
    title: &str,
    id: egui::Id,
    depth: u32,
    open: &mut bool,
    searching: bool,
    add: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    let sub = depth > 0;
    let back = ui.painter().add(egui::Shape::Noop);
    let top = ui.cursor().top();
    ui.spacing_mut().item_spacing.y = 0.0;
    let (header, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), HEADER), egui::Sense::click());
    // A sub-panel's edges are its parent's, over the parent's padding.
    let reach = if sub { PANEL_PAD } else { 0.0 };
    let edge = header.x_range().expand(reach);
    let painter = ui.painter();
    let y = header.center().y;
    let text = ui.visuals().text_color();
    let indent = if sub { 14.0 } else { 0.0 };
    chevron(painter, egui::pos2(edge.min + 5.0 + indent + 8.0, y), *open, text.gamma_multiply(0.8));
    let font = egui::TextStyle::Body.resolve(ui.style());
    let title_x = edge.min + 22.0 + indent;
    let mut job = egui::text::LayoutJob::single_section(
        title.to_owned(),
        egui::TextFormat { font_id: font, color: text, ..Default::default() },
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width((edge.max - title_x - 28.0).max(0.0));
    let galley = painter.layout_job(job);
    shadowed(painter, egui::pos2(title_x, y - galley.size().y / 2.0), galley, text);
    if !sub {
        grip(painter, egui::pos2(edge.max - 17.0, y));
    }
    if response.clicked() && !searching {
        *open = !*open;
        ui.data_mut(|d| d.insert_persisted(id, *open));
    }
    if *open {
        ui.data_mut(|d| d.insert_temp(depth_id(), depth + 1));
        let pad = PANEL_PAD as i8;
        egui::Frame::NONE.inner_margin(egui::Margin { left: if sub { 0 } else { pad }, right: if sub { 0 } else { pad }, top: pad, bottom: pad }).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.data_mut(|d| d.insert_temp(section_top_id(), ui.cursor().top()));
            add(ui);
        });
        ui.data_mut(|d| d.insert_temp(depth_id(), depth));
    }
    // The backgrounds, under what was drawn, now that its height is known.
    let bottom = ui.cursor().top();
    let whole = egui::Rect::from_x_y_ranges(edge, top..=bottom);
    let shape = if !sub {
        egui::Shape::rect_filled(whole, egui::CornerRadius::same(4), super::theme::extra(ui.ctx()).panel)
    } else if *open {
        let body = egui::Rect::from_x_y_ranges(edge, header.bottom()..=bottom);
        egui::Shape::rect_filled(body, egui::CornerRadius { nw: 0, ne: 0, sw: 4, se: 4 }, egui::Color32::from_black_alpha(0x1f))
    } else {
        egui::Shape::Noop
    };
    ui.painter().set(back, shape);
    // Blender's gap between panels: the region shows between them.
    if !sub {
        ui.add_space(2.0);
    }
    response
}

/// A panel's padding, all round: Blender's `panelspace`.
const PANEL_PAD: f32 = 8.0;

/// Blender's collapse chevron, thin, centred on `c`: `>` shut, `v` open.
fn chevron(painter: &egui::Painter, c: egui::Pos2, open: bool, color: egui::Color32) {
    let stroke = egui::Stroke::new(1.0, color);
    let points = if open {
        vec![c + egui::vec2(-4.5, -2.0), c + egui::vec2(0.0, 2.5), c + egui::vec2(4.5, -2.0)]
    } else {
        vec![c + egui::vec2(-2.0, -4.5), c + egui::vec2(2.5, 0.0), c + egui::vec2(-2.0, 4.5)]
    };
    painter.add(egui::Shape::line(points, stroke));
}

/// Blender's panel grip: four dots by two at a pitch of 3, each with a black
/// dot under it, from `at` rightwards, centred on its height. A mesh of
/// squares on whole pixels: egui draws a rectangle this small as a line, which
/// came out as nothing.
fn grip(painter: &egui::Painter, at: egui::Pos2) {
    use egui::emath::GuiRounding as _;
    let at = at.round_to_pixels(painter.pixels_per_point());
    let mut mesh = egui::Mesh::default();
    for row in [-2.0_f32, 1.0] {
        for col in 0..4 {
            let p = egui::pos2(at.x + col as f32 * 3.0, at.y + row);
            mesh.add_colored_rect(egui::Rect::from_min_size(p + egui::vec2(0.0, 1.0), egui::vec2(1.0, 1.0)), egui::Color32::BLACK);
            mesh.add_colored_rect(egui::Rect::from_min_size(p, egui::vec2(1.0, 1.0)), egui::Color32::from_gray(0x91));
        }
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// Text with Blender's shadow under it: the same, a point lower, black at a
/// third -- Blender's is a half, blurred over three pixels, which egui does not
/// blur.
fn shadowed(painter: &egui::Painter, at: egui::Pos2, galley: std::sync::Arc<egui::Galley>, color: egui::Color32) {
    painter.galley_with_override_text_color(at + egui::vec2(0.0, 1.0), galley.clone(), egui::Color32::from_black_alpha(85));
    painter.galley(at, galley, color);
}

/// One setting, as Blender's split layout has it: its name right-aligned
/// against its control, with Blender's text shadow, cut short with an
/// ellipsis when its column is too narrow -- and then whole on hover, before
/// its doc; `add` draws the control, filling the field column.
pub fn setting<R>(ui: &mut egui::Ui, name: &str, hover: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    row(ui, name, name, hover, true, ROW, None, add)
}

/// One of a vector's rows, found by the searches and listed as `known_as` --
/// its "Y" as "pos Y" -- and 20 high, so that its fields meet as Blender
/// stacks a vector's (`column(align=True)`). The searches match a setting
/// with the titles of the sections it is in before it, so that "sun pos"
/// finds the Sun's position.
pub fn stacked<R>(ui: &mut egui::Ui, name: &str, known_as: &str, hover: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    row(ui, name, known_as, hover, true, 20.0, None, add)
}

/// A setting that is on or off, as Blender lays one out: the label column
/// empty, the box and the name after it in the field column, the doc on hover.
/// Chosen in the `E` search (`reveal`), it is switched, as Blender's menu
/// search switches what it finds, rather than only shown -- the user: "pressing
/// render back face on E menu doesnt toggle render back face button".
pub fn toggle(ui: &mut egui::Ui, name: &str, hover: &str, value: &mut bool) -> egui::Response {
    row(ui, name, name, hover, false, ROW, Some(*value), |ui| {
        let switched = take_focus(ui);
        if switched {
            *value = !*value;
        }
        let mut response = checkbox(ui, value, name);
        if switched {
            response.mark_changed();
        }
        if hover.is_empty() { response } else { response.on_hover_text(hover) }
    })
}

/// A setting's row, `height` high: found by the searches, listed in the
/// catalogue -- with whether it is `on`, for one that is on or off -- shown by
/// `reveal`; its name in the label column if `labelled`.
#[allow(clippy::too_many_arguments)]
fn row<R>(
    ui: &mut egui::Ui,
    name: &str,
    known_as: &str,
    hover: &str,
    labelled: bool,
    height: f32,
    on: Option<bool>,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let here = path(ui);
    if let Some(app) = cataloguing(ui) {
        let place = Place { app, path: here.clone(), name: known_as.to_owned(), on };
        ui.data_mut(|d| d.get_temp_mut_or_default::<Vec<Place>>(catalogue_id()).push(place));
    } else if let Some(q) = searched(ui) {
        let full = format!("{} {known_as}", here.join(" "));
        if !inside_a_match(ui) && fuzzy::score(&q, &full).is_none() {
            return add(&mut hidden(ui));
        }
        count_match(ui);
    }
    let revealed = ui.data(|d| d.get_temp::<Place>(reveal_id())).is_some_and(|t| t.path == here && t.name == known_as);
    let width = ui.available_width();
    let (label_width, field_x, field_width) = split(width);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    if revealed {
        ui.scroll_to_rect(rect, Some(egui::Align::Center));
        ui.data_mut(|d| {
            d.remove::<Place>(reveal_id());
            d.insert_temp(egui::Id::new("kalast focus field"), true);
        });
    }
    if labelled {
        let font = egui::TextStyle::Body.resolve(ui.style());
        let natural = ui.fonts_mut(|f| f.layout_no_wrap(name.to_owned(), font.clone(), egui::Color32::PLACEHOLDER).size().x);
        let color = ui.visuals().text_color();
        let mut job = egui::text::LayoutJob::single_section(
            name.to_owned(),
            egui::TextFormat { font_id: font, color, ..Default::default() },
        );
        job.wrap = egui::text::TextWrapping::truncate_at_width(label_width);
        let galley = ui.painter().layout_job(job);
        // Right-aligned, a point short of the column's edge, as Blender's.
        let at = egui::pos2(rect.left() + label_width - 1.0 - galley.size().x, rect.center().y - galley.size().y / 2.0);
        shadowed(ui.painter(), at, galley, color);
        // A name cut short is whole on hover, before its doc.
        let hover = match (natural > label_width, hover.is_empty()) {
            (true, true) => name.to_owned(),
            (true, false) => format!("{name}\n\n{hover}"),
            (false, _) => hover.to_owned(),
        };
        if !hover.is_empty() {
            let label = egui::Rect::from_min_size(rect.min, egui::vec2(label_width, height));
            let id = ui.auto_id_with(("setting", name));
            ui.interact(label, id, egui::Sense::hover()).on_hover_text(hover);
        }
    }
    let control = egui::Rect::from_min_size(egui::pos2(rect.left() + field_x, rect.top()), egui::vec2(field_width, height));
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(control)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    // A slider, a list and a text field fill the field column -- a text
    // field took egui's 280 points, past the panel's edge.
    child.spacing_mut().slider_width = (field_width - 64.0).max(40.0);
    child.spacing_mut().combo_width = field_width;
    child.spacing_mut().text_edit_width = field_width.max(40.0);
    let inner = add(&mut child);
    // Revealed, with no field that took the focus: let it go.
    if revealed {
        ui.data_mut(|d| d.remove::<bool>(egui::Id::new("kalast focus field")));
    }
    inner
}

/// A row of the grid holding whatever `add` puts in it, left to right --
/// buttons, a list of values -- for what is not a named setting: across the
/// panel's content, as Blender lays a label or an operator's button.
pub fn line<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    if searched(ui).is_some() && !inside_a_match(ui) {
        return add(&mut hidden(ui));
    }
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, ROW), egui::Sense::hover());
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    add(&mut child)
}

/// A button as Blender's operator buttons in a panel: as wide as what is left
/// of its row, its label centred, blue while pressed, its emboss under it.
pub fn button(ui: &mut egui::Ui, label: impl Into<egui::WidgetText>) -> egui::Response {
    let size = egui::vec2(ui.available_width().max(40.0), ui.spacing().interact_size.y);
    let response = ui.add(egui::Button::new(label).min_size(size));
    let radius = ui.visuals().widgets.inactive.corner_radius.sw;
    ui.painter().add(emboss(response.rect, radius));
    response
}

/// A choice among a few, as Blender expands an enum: a block of buttons as
/// wide as the field, joined by one line between them and round at its outer
/// corners only, the chosen one in blue with white text, the others grey and
/// lit under the pointer. Changed when another is clicked.
pub fn segmented<V: PartialEq + Clone>(ui: &mut egui::Ui, current: &mut V, options: &[(V, &str)]) -> egui::Response {
    let size = egui::vec2(ui.available_width().max(40.0), ui.spacing().interact_size.y);
    let (rect, mut response) = ui.allocate_exact_size(size, egui::Sense::hover());
    let visuals = ui.visuals().widgets.clone();
    let (blue, white) = (ui.visuals().selection.bg_fill, ui.visuals().strong_text_color());
    let r = visuals.inactive.corner_radius.nw;
    let width = rect.width() / options.len().max(1) as f32;
    let font = egui::TextStyle::Button.resolve(ui.style());
    let last = options.len().saturating_sub(1);
    for (i, (value, label)) in options.iter().enumerate() {
        let cell = egui::Rect::from_min_size(rect.min + egui::vec2(i as f32 * width, 0.0), egui::vec2(width, rect.height()));
        let clicked = ui.interact(cell, response.id.with(i), egui::Sense::click());
        if clicked.clicked() && *current != *value {
            *current = value.clone();
            response.mark_changed();
        }
        let chosen = *current == *value;
        let fill = if chosen {
            blue
        } else if clicked.hovered() {
            visuals.hovered.bg_fill
        } else {
            visuals.inactive.bg_fill
        };
        let (left, right) = (if i == 0 { r } else { 0 }, if i == last { r } else { 0 });
        let corners = egui::CornerRadius { nw: left, sw: left, ne: right, se: right };
        // A point to the left over its neighbour's edge, so that the two
        // outlines are one line, as Blender stitches a row's buttons.
        let drawn = if i == 0 { cell } else { cell.with_min_x(cell.left() - 1.0) };
        ui.painter().rect(drawn, corners, fill, visuals.inactive.bg_stroke, egui::StrokeKind::Inside);
        let color = if chosen { white } else { visuals.inactive.text_color() };
        let mut job = egui::text::LayoutJob::single_section(label.to_string(), egui::TextFormat { font_id: font.clone(), color, ..Default::default() });
        job.wrap = egui::text::TextWrapping::truncate_at_width((width - 8.0).max(0.0));
        job.halign = egui::Align::Center;
        let galley = ui.painter().layout_job(job);
        ui.painter().galley(egui::pos2(cell.center().x, cell.center().y - galley.size().y / 2.0), galley, color);
    }
    ui.painter().add(emboss(rect, r));
    response
}

/// A dim remark on a row of its own: "none loaded", what a click does.
pub fn note(ui: &mut egui::Ui, text: &str) {
    line(ui, |ui| {
        ui.add(egui::Label::new(egui::RichText::new(text).weak().small()).truncate());
    });
}

/// A quiet heading inside a section, between an entity and its settings: a
/// dim label on a row of its own, as Blender heads the groups of a panel.
pub fn subheading(ui: &mut egui::Ui, label: &str) -> egui::Response {
    if searched(ui).is_some() && !inside_a_match(ui) {
        return nothing(ui, label);
    }
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), ROW), egui::Sense::hover());
    let mut chars = label.chars();
    let label: String = chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default();
    ui.painter().text(
        egui::pos2(rect.left(), rect.center().y + 1.0),
        egui::Align2::LEFT_CENTER,
        label,
        egui::TextStyle::Body.resolve(ui.style()),
        ui.visuals().weak_text_color(),
    );
    response
}

/// Blender's properties editor's navigation bar: its width, and its tabs'
/// height, 1.4 of its 20-point rows.
const NAV: f32 = 29.0;
const NAV_TAB: f32 = 28.0;

/// Blender's navigation bar down the left of what is left of the panel, on the
/// theme's darkest grey (`theme::Extra::nav`), under its header and to its
/// bottom edge, its tabs (`nav_tab`) from 8 below its top, which `add` draws
/// -- scrolled when the panel is too short for them. Returns the region
/// beside it, for the panels, 8 in from the bar and from the right edge, as
/// Blender's region has them.
pub fn nav_bar(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) -> egui::Ui {
    let whole = ui.available_rect_before_wrap();
    ui.advance_cursor_after_rect(whole);
    let bar = egui::Rect::from_min_max(whole.min, egui::pos2(whole.left() + NAV, whole.bottom()));
    // Round where the panel's corner is.
    let round = egui::CornerRadius { nw: 0, sw: 5, ne: 0, se: 0 };
    ui.painter().rect_filled(bar, round, super::theme::extra(ui.ctx()).nav);
    let tabs = egui::Rect::from_min_max(bar.min + egui::vec2(0.0, 8.0), bar.max);
    let mut nav = ui.new_child(egui::UiBuilder::new().max_rect(tabs).layout(egui::Layout::top_down(egui::Align::Min)));
    egui::ScrollArea::vertical()
        .id_salt("kalast navigation bar")
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .show(&mut nav, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            add(ui);
        });
    let region = egui::Rect::from_min_max(
        egui::pos2(bar.right() + PANEL_PAD, whole.top()),
        egui::pos2(whole.right() - PANEL_PAD, whole.bottom()),
    );
    ui.new_child(egui::UiBuilder::new().max_rect(region).layout(egui::Layout::top_down(egui::Align::Min)))
}

/// One of the navigation bar's tabs, an icon named on hover by the caller:
/// across the bar, 28 high; the chosen one in the region's colour, its left
/// corners round, so that it runs into the region as a tab; the others bare,
/// a shade lighter under the pointer. The icon in its `color`, as Blender
/// draws its tabs' coloured icons, at 80 % unless chosen or pointed at, and
/// faint when `dimmed` -- a search finding nothing in it.
pub fn nav_tab(ui: &mut egui::Ui, selected: bool, icon: &str, color: egui::Color32, dimmed: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(NAV, NAV_TAB), egui::Sense::click());
    let round = egui::CornerRadius { nw: 4, sw: 4, ne: 0, se: 0 };
    let lit = selected || response.hovered();
    if selected {
        ui.painter().rect_filled(rect, round, ui.visuals().panel_fill);
    } else if response.hovered() {
        ui.painter().rect_filled(rect, round, egui::Color32::from_white_alpha(6));
    }
    let color = match (dimmed, lit) {
        (true, _) => color.gamma_multiply(0.3),
        (false, true) => color,
        (false, false) => color.gamma_multiply(0.8),
    };
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(16.0), color);
    response
}

/// A tab that is an icon alone, named on hover by the caller, on a strip of
/// tabs (`tab_strip`): the side panel's app, simulation and scripts.
pub fn icon_tab(ui: &mut egui::Ui, selected: bool, icon: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(32.0, TAB), egui::Sense::click());
    let color = tab_back(ui, rect, selected, response.hovered());
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(17.0), color);
    response
}

/// A tab's height on its strip.
const TAB: f32 = 26.0;

/// A tab's back, as Blender draws its tabs: the chosen one in the colour of
/// the panel it opens, its top corners round, so that it reads as part of
/// what is under it; the others bare on the strip, lit a little under the
/// pointer. The label's colour: lit when chosen or pointed at, dim otherwise.
fn tab_back(ui: &egui::Ui, rect: egui::Rect, selected: bool, hovered: bool) -> egui::Color32 {
    let top = egui::CornerRadius { nw: 4, ne: 4, sw: 0, se: 0 };
    if selected {
        ui.painter().rect_filled(rect, top, ui.visuals().panel_fill);
    } else if hovered {
        ui.painter().rect_filled(rect, top, egui::Color32::from_white_alpha(8));
    }
    if selected || hovered {
        ui.visuals().strong_text_color()
    } else {
        ui.visuals().weak_text_color()
    }
}

/// A tab that is a word: Blender's workspace tabs, over its top bar.
pub fn tab(ui: &mut egui::Ui, selected: bool, label: &str) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(label.to_owned(), egui::FontId::proportional(13.0), egui::Color32::PLACEHOLDER);
    let width = galley.size().x + 18.0;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, TAB), egui::Sense::click());
    let color = tab_back(ui, rect, selected, response.hovered());
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, color);
    response
}

/// A row of tabs on Blender's dark strip, the width of what holds it: the
/// chosen tab, in the colour of what it opens, stands out of it. `add` lays
/// out the tabs, and whatever goes on the strip beside them.
pub fn tab_strip<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let strip = ui.painter().add(egui::Shape::Noop);
    let (left, right) = (ui.cursor().left(), ui.max_rect().right());
    let row = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 1.0;
        add(ui)
    });
    let rect = egui::Rect::from_x_y_ranges(left..=right, row.response.rect.y_range());
    ui.painter().set(strip, egui::Shape::rect_filled(rect, egui::CornerRadius::same(4), ui.visuals().extreme_bg_color));
    row.inner
}

/// A checkbox as Blender's: a rounded box, Blender's blue with a white tick
/// when ticked, its label after it. Changed when clicked.
pub fn checkbox(ui: &mut egui::Ui, checked: &mut bool, text: impl Into<egui::WidgetText>) -> egui::Response {
    const SIDE: f32 = 14.0;
    let text: egui::WidgetText = text.into();
    let spacing = ui.spacing().icon_spacing + 2.0;
    let galley = (!text.is_empty()).then(|| {
        let wrap = ui.available_width() - SIDE - spacing;
        text.into_galley(ui, Some(egui::TextWrapMode::Truncate), wrap, egui::TextStyle::Body)
    });
    let width = SIDE + galley.as_ref().map_or(0.0, |g| spacing + g.size().x);
    let size = egui::vec2(width, ui.spacing().interact_size.y);
    let (rect, mut response) = ui.allocate_exact_size(size, egui::Sense::click());
    if response.clicked() {
        *checked = !*checked;
        response.mark_changed();
    }
    let label = galley.as_ref().map_or(String::new(), |g| g.text().to_owned());
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), *checked, &label));
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&response);
        let square = egui::Rect::from_center_size(egui::pos2(rect.left() + SIDE / 2.0, rect.center().y), egui::vec2(SIDE, SIDE));
        let extra = super::theme::extra(ui.ctx());
        let fill = match (*checked, response.hovered()) {
            (true, true) => lighten_by(extra.check, 17),
            (true, false) => extra.check,
            (false, _) => visuals.bg_fill,
        };
        ui.painter().rect(square, egui::CornerRadius::same(3), fill, visuals.bg_stroke, egui::StrokeKind::Inside);
        if *checked {
            let at = |x: f32, y: f32| square.min + egui::vec2(x, y) * SIDE;
            ui.painter().line(
                vec![at(0.24, 0.52), at(0.42, 0.7), at(0.77, 0.31)],
                egui::Stroke::new(1.7, extra.tick),
            );
        }
        if let Some(galley) = galley {
            let at = egui::pos2(square.right() + spacing, rect.center().y - galley.size().y / 2.0);
            ui.painter().galley(at, galley, visuals.text_color());
        }
    }
    response
}


/// A colour, as Blender's field: a swatch as wide as its row -- the colour
/// opaque on the left and with its alpha over a checkerboard on the right, as
/// both egui and Blender draw one -- a click opening the picker.
pub fn color_rgba(ui: &mut egui::Ui, rgba: &mut [f32; 4]) -> egui::Response {
    ui.scope(|ui| {
        ui.spacing_mut().interact_size.x = ui.available_width().max(40.0);
        ui.color_edit_button_rgba_unmultiplied(rgba)
    })
    .inner
}

/// `color_rgba`, with no alpha.
pub fn color_rgb(ui: &mut egui::Ui, rgb: &mut [f32; 3]) -> egui::Response {
    ui.scope(|ui| {
        ui.spacing_mut().interact_size.x = ui.available_width().max(40.0);
        ui.color_edit_button_rgb(rgb)
    })
    .inner
}

/// A vector's components, as Blender stacks them (`column(align=True)`): a
/// row each, the first named "pos X" and the others "Y", "Z", their fields
/// meeting in one block with round outer corners. Each is found by the
/// searches and listed by its whole name, "pos Y". Whether one changed.
pub fn components<Num: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    label: &str,
    hover: &str,
    values: Vec<(&str, &mut Num)>,
    style: impl for<'b> Fn(Field<'b>) -> Field<'b>,
) -> bool {
    // Blender's 5 between a vector and the rows round it (its panels'
    // `templatespace`), the rows' own 1 counted: none more after another,
    // nor at the top of a panel, where its padding is the gap.
    let after_one = egui::Id::new("kalast components end");
    let top = ui.cursor().top();
    let first = ui.data(|d| d.get_temp::<f32>(section_top_id())) == Some(top);
    if ui.data(|d| d.get_temp::<f32>(after_one)) != Some(top) && !first {
        ui.add_space(4.0);
    }
    let last = values.len().saturating_sub(1);
    let mut changed = false;
    for (i, (axis, value)) in values.into_iter().enumerate() {
        let known_as = format!("{label} {axis}");
        let name = if i == 0 { known_as.clone() } else { axis.to_owned() };
        changed |= stacked(ui, &name, &known_as, hover, |ui| {
            let r = ui.visuals().widgets.inactive.corner_radius.nw;
            let (top, bottom) = (if i == 0 { r } else { 0 }, if i == last { r } else { 0 });
            let corners = egui::CornerRadius { nw: top, ne: top, sw: bottom, se: bottom };
            let w = &mut ui.visuals_mut().widgets;
            for state in [&mut w.inactive, &mut w.hovered, &mut w.active] {
                state.corner_radius = corners;
            }
            number(ui, value, &style).changed()
        });
    }
    ui.add_space(4.0);
    ui.data_mut(|d| d.insert_temp(after_one, ui.cursor().top()));
    changed
}

/// egui's `DragValue` as `number` sets it up, with the calls the panels make
/// of it, its speed kept for the arrows' step (`arrow_step`).
pub struct Field<'a> {
    drag: egui::DragValue<'a>,
    speed: f64,
}

impl<'a> Field<'a> {
    /// How much a point dragged changes it.
    pub fn speed(self, speed: impl Into<f64>) -> Self {
        let speed = speed.into();
        Self { drag: self.drag.speed(speed), speed }
    }

    pub fn range<Num: egui::emath::Numeric>(self, range: std::ops::RangeInclusive<Num>) -> Self {
        let Self { drag, speed } = self;
        Self { drag: drag.range(range), speed }
    }

    pub fn clamp_existing_to_range(self, clamp: bool) -> Self {
        let Self { drag, speed } = self;
        Self { drag: drag.clamp_existing_to_range(clamp), speed }
    }

    pub fn max_decimals(self, decimals: usize) -> Self {
        let Self { drag, speed } = self;
        Self { drag: drag.max_decimals(decimals), speed }
    }

    /// Written after the value: a unit.
    pub fn suffix(self, suffix: impl egui::IntoAtoms<'a>) -> Self {
        let Self { drag, speed } = self;
        Self { drag: drag.suffix(suffix), speed }
    }
}

/// A number, as Blender's field: as wide as its row, the value centred
/// across it. Dragged, it changes -- slower with Shift, as egui's does, and held
/// to round steps with Ctrl (`snap`); clicked, it takes typed text, maths
/// included: "2*pi", "90/4", "*2" for twice what it holds (`expr`). Under the
/// pointer, Blender's arrows at either end, a click on one stepping the value
/// (`arrow_step`). `style` sets the field up: its speed, range, decimals, unit
/// (`Field`, egui's `DragValue`'s calls).
pub fn number<Num: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    value: &mut Num,
    style: impl for<'b> FnOnce(Field<'b>) -> Field<'b>,
) -> egui::Response {
    let slot = ui.next_auto_id();
    let key = slot.with("typed");
    // An arrow clicked the frame before: the value stepped before the field
    // reads it, so that egui's `DragValue` holds it to its range.
    let stepped = ui.data_mut(|d| d.remove_temp::<f64>(slot.with("step")));
    if let Some(step) = stepped {
        *value = Num::from_f64(tidy(value.to_f64() + step));
    }
    let current = value.to_f64();
    let base = typing_base(ui, key, current);
    let back = ui.painter().add(egui::Shape::Noop);
    let size = egui::vec2(ui.available_width().max(40.0), ui.spacing().interact_size.y);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let mut field = field_ui(ui, slot, rect);
    let Field { drag, speed } = style(Field { drag: egui::DragValue::new(&mut *value), speed: 1.0 });
    // Typed text is taken on Enter, as Blender takes it.
    let mut drag = drag.custom_parser(move |text| typed(text, base)).update_while_editing(false);
    if !Num::INTEGRAL {
        drag = drag.custom_formatter(compact);
    }
    let mut response = field.add(drag);
    if stepped.is_some() {
        response.mark_changed();
    }
    let focused = take_focus(ui);
    if focused {
        response.request_focus();
    }
    select_all_after(ui, &response, focused);
    // Asked of the memory: the response says no focus while the text is
    // being typed.
    let typing = ui.memory(|m| m.focused()) == Some(response.id);
    typing_kept(ui, key, current, typing);
    if response.dragged() && ui.input(|i| i.modifiers.ctrl || i.modifiers.command) {
        let v = value.to_f64();
        let step = snap(v.abs(), Num::INTEGRAL);
        *value = Num::from_f64((v / step).round() * step);
    }
    // Blender's arrows: a zone at either end, 14 wide or a third of a
    // narrow field, shown under the pointer, the one pointed at lighter.
    let zone = (rect.width() / 3.0).min(0.7 * rect.height());
    let side_of = |p: egui::Pos2| {
        if p.x < rect.left() + zone {
            -1.0
        } else if p.x > rect.right() - zone {
            1.0
        } else {
            0.0
        }
    };
    // A click on one steps the value, the frame after, rather than taking
    // typed text -- the focus the click gave the field taken back.
    let clicked_side = response.clicked().then(|| response.interact_pointer_pos().map(side_of)).flatten();
    let mut typing = typing;
    if let Some(side) = clicked_side.filter(|side| *side != 0.0) {
        ui.memory_mut(|m| m.surrender_focus(response.id));
        typing = false;
        typing_kept(ui, key, current, false);
        ui.data_mut(|d| d.insert_temp(slot.with("step"), side * arrow_step(speed, Num::INTEGRAL)));
        ui.ctx().request_repaint();
    }
    let pointed = response.hover_pos().filter(|_| !typing && !response.dragged()).map(side_of);
    let mut shapes = field_back(ui, &response, rect, typing, None);
    if let Some(side) = pointed {
        let visuals = &ui.visuals().widgets;
        let radius = visuals.inactive.corner_radius;
        if side != 0.0 {
            let (lit, corners) = if side < 0.0 {
                (egui::Rect::from_min_max(rect.min, egui::pos2(rect.left() + zone, rect.bottom())), egui::CornerRadius { ne: 0, se: 0, ..radius })
            } else {
                (egui::Rect::from_min_max(egui::pos2(rect.right() - zone, rect.top()), rect.max), egui::CornerRadius { nw: 0, sw: 0, ..radius })
            };
            shapes.insert(1, egui::Shape::rect_filled(lit, corners, lighten_by(visuals.hovered.bg_fill, 20)));
        }
        let color = visuals.hovered.fg_stroke.color;
        let stroke = egui::Stroke::new(1.0, color);
        let y = rect.center().y;
        for (x, dx) in [(rect.left() + 8.0, 1.5_f32), (rect.right() - 8.0, -1.5)] {
            shapes.push(egui::Shape::line(
                vec![egui::pos2(x + dx, y - 3.0), egui::pos2(x - dx, y), egui::pos2(x + dx, y + 3.0)],
                stroke,
            ));
        }
    }
    ui.painter().set(back, egui::Shape::Vec(shapes));
    response
}

/// What a click on a field's arrow adds or takes away, the same each time as
/// Blender's: 1 to a whole number, as Blender steps one; to a fraction ten
/// points' drag at its `speed` -- 0.1 to a position dragged at 0.01, 0.01 to an
/// albedo at 0.001, a degree to an angle at 0.1 -- as Blender steps a
/// hundredth of a property's step, which is ten times what a pixel drags.
fn arrow_step(speed: f64, integer: bool) -> f64 {
    if integer { 1.0 } else { 10.0 * speed.abs() }
}

/// `v` to 12 significant digits: no 0.30000000000000004 from steps of 0.1.
fn tidy(v: f64) -> f64 {
    format!("{v:.11e}").parse().unwrap_or(v)
}

/// The child a field's `DragValue` is drawn in over `rect`, bare -- its back
/// painted by `field_back` -- its value centred, or at the left while typed
/// into, as Blender's. Whether it is, is asked of the id the field had the
/// frame before, kept under `slot`.
fn field_ui(ui: &mut egui::Ui, slot: egui::Id, rect: egui::Rect) -> egui::Ui {
    let editing = ui.data(|d| d.get_temp::<egui::Id>(slot.with("field"))).is_some_and(|id| ui.memory(|m| m.has_focus(id)));
    let layout = if editing {
        // egui's `left_to_right` centres along the row unless told.
        egui::Layout::left_to_right(egui::Align::Center).with_main_align(egui::Align::Min).with_main_justify(true)
    } else {
        egui::Layout::centered_and_justified(egui::Direction::LeftToRight)
    };
    let mut field = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(layout));
    ui.data_mut(|d| d.insert_temp(slot.with("field"), field.next_auto_id()));
    let v = field.visuals_mut();
    // Its text white under the pointer and while held, as Blender's.
    v.override_text_color = None;
    v.text_edit_bg_color = Some(egui::Color32::TRANSPARENT);
    v.selection.stroke = egui::Stroke::NONE;
    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active] {
        w.bg_fill = egui::Color32::TRANSPARENT;
        w.weak_bg_fill = egui::Color32::TRANSPARENT;
        w.bg_stroke = egui::Stroke::NONE;
        w.expansion = 0.0;
    }
    field
}

/// A field's back, as Blender paints a number field or a slider: its fill,
/// lighter under the pointer and dark while held or typed into; a slider's
/// share of its range in the theme's fill; its outline; and its emboss, a line
/// of shadow under its bottom edge, when that edge is its own -- in a stack,
/// the last field's.
fn field_back(ui: &egui::Ui, response: &egui::Response, rect: egui::Rect, typing: bool, share: Option<f32>) -> Vec<egui::Shape> {
    let visuals = &ui.visuals().widgets;
    let extra = super::theme::extra(ui.ctx());
    let held = typing || response.dragged() || response.is_pointer_button_down_on();
    let fill = if held {
        extra.active_field
    } else if response.hovered() {
        visuals.hovered.bg_fill
    } else {
        visuals.inactive.bg_fill
    };
    let radius = visuals.inactive.corner_radius;
    // Under another in a stack, a point higher, so that the two outlines are
    // one line between them, as Blender stitches a vector's fields.
    let rect = if radius.nw == 0 && radius.ne == 0 { rect.with_min_y(rect.top() - 1.0) } else { rect };
    let mut shapes = vec![egui::Shape::rect_filled(rect, radius, fill)];
    if let Some(share) = share.filter(|s| *s > 0.0 && !typing) {
        let filled = egui::Rect::from_min_max(rect.min, egui::pos2(rect.left() + rect.width() * share, rect.bottom()));
        let right = if share < 1.0 { 0 } else { radius.ne };
        shapes.push(egui::Shape::rect_filled(
            filled,
            egui::CornerRadius { nw: radius.nw, sw: radius.sw, ne: right, se: right },
            extra.fill,
        ));
    }
    shapes.push(egui::Shape::rect_stroke(rect, radius, visuals.inactive.bg_stroke, egui::StrokeKind::Inside));
    if radius.sw > 0 {
        shapes.push(emboss(rect, radius.sw));
    }
    shapes
}

/// Blender's emboss: a line of black at 15 % just under a widget's bottom
/// edge, short of its rounded corners.
fn emboss(rect: egui::Rect, radius: u8) -> egui::Shape {
    let inset = radius as f32 * 0.5;
    let line = egui::Rect::from_min_max(egui::pos2(rect.left() + inset, rect.bottom()), egui::pos2(rect.right() - inset, rect.bottom() + 1.0));
    egui::Shape::rect_filled(line, 0.0, egui::Color32::from_black_alpha(0x26))
}

/// A fraction as a field writes it: as egui does, with the decimals its speed
/// asks for, unless it is very large or very small, then in powers of ten to
/// four digits -- the Sun at 1 au, -30627055616.00, ran past its field.
/// Typed back, `expr` reads either.
fn compact(v: f64, decimals: std::ops::RangeInclusive<usize>) -> String {
    if v != 0.0 && v.is_finite() && !(1e-4..1e7).contains(&v.abs()) {
        format!("{v:.3e}")
    } else {
        egui::emath::format_with_decimals_in_range(v, decimals)
    }
}

/// What typed text stands for in a field holding `current`: its maths, a
/// degree sign after it allowed.
fn typed(text: &str, current: f64) -> Option<f64> {
    super::expr::eval(text.trim().trim_end_matches('\u{b0}'), current)
}

/// What a field's typed text is read against: the value it held when the
/// typing began, kept under `key` (`typing_kept`). egui reads the text twice,
/// on Enter and the frame after, so "*2" read against the value it had just
/// made doubled it twice.
fn typing_base(ui: &egui::Ui, key: egui::Id, current: f64) -> f64 {
    ui.data(|d| d.get_temp::<(f64, bool)>(key)).map_or(current, |(base, _)| base)
}

/// Keeps the value a field held as typing began, `(value, still typing)`,
/// until the frame after the text is taken, when egui reads it once more.
fn typing_kept(ui: &egui::Ui, key: egui::Id, current: f64, typing: bool) {
    ui.data_mut(|d| match (d.get_temp::<(f64, bool)>(key), typing) {
        (None, true) | (Some((_, false)), true) => {
            d.insert_temp(key, (current, true));
        }
        (Some((base, true)), false) => {
            d.insert_temp(key, (base, false));
        }
        (Some((_, false)), false) => d.remove::<(f64, bool)>(key),
        (Some((_, true)), true) | (None, false) => {}
    });
}

/// The step Ctrl holds a dragged value to, from its size, `span` -- a
/// slider's range, or a field's value: two significant digits, 13.47 held to
/// 13 and 0.385 to 0.39, and a whole one for an integer.
fn snap(span: f64, integer: bool) -> f64 {
    let step = if span > 0.0 && span.is_finite() { 10f64.powi(span.log10().floor() as i32 - 1) } else { 1.0 };
    if integer { step.max(1.0) } else { step }
}

/// A number in a range, as Blender's slider: a field filled in Blender's blue
/// as far as the value goes, the value written across it. It is egui's
/// `DragValue` on that field, so it behaves as Blender's does: dragged, it
/// moves the width's share of the range for each point -- the same share of
/// its decades for a logarithmic one -- slower with Shift; clicked, it takes
/// typed text. Edits are clamped to the range; a value already outside it is
/// kept, as `SliderClamping::Edits` kept it.
pub struct Slider<'a> {
    get_set: Box<dyn 'a + FnMut(Option<f64>) -> f64>,
    range: std::ops::RangeInclusive<f64>,
    logarithmic: bool,
    integer: bool,
    suffix: String,
}

impl<'a> Slider<'a> {
    pub fn new<Num: egui::emath::Numeric>(value: &'a mut Num, range: std::ops::RangeInclusive<Num>) -> Self {
        Self {
            get_set: Box::new(move |v: Option<f64>| {
                if let Some(v) = v {
                    *value = Num::from_f64(v);
                }
                value.to_f64()
            }),
            range: range.start().to_f64()..=range.end().to_f64(),
            logarithmic: false,
            integer: Num::INTEGRAL,
            suffix: String::new(),
        }
    }

    /// Filled, and dragged, by the share of the range's decades.
    pub fn logarithmic(mut self, logarithmic: bool) -> Self {
        self.logarithmic = logarithmic;
        self
    }

    /// Written after the value: a unit.
    pub fn suffix(mut self, suffix: impl Into<String>) -> Self {
        self.suffix = suffix.into();
        self
    }
}

impl egui::Widget for Slider<'_> {
    fn ui(mut self, ui: &mut egui::Ui) -> egui::Response {
        let (lo, hi) = (*self.range.start(), *self.range.end());
        let log = self.logarithmic && lo > 0.0 && hi > lo;
        let slot = ui.next_auto_id();
        let key = slot.with("typed");
        let back = ui.painter().add(egui::Shape::Noop);
        let size = egui::vec2(ui.available_width().max(40.0), ui.spacing().interact_size.y);
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        let value = (self.get_set)(None);
        let base = typing_base(ui, key, value);
        let share = if log { (value.ln() - lo.ln()) / (hi.ln() - lo.ln()) } else { (value - lo) / (hi - lo) };
        let share = if share.is_finite() { share.clamp(0.0, 1.0) as f32 } else { 0.0 };
        let width = rect.width() as f64;
        let speed = if log { value.abs().max(lo) * (hi / lo).ln() / width } else { (hi - lo) / width };
        let mut field = field_ui(ui, slot, rect);
        let get_set = &mut self.get_set;
        let mut drag = egui::DragValue::from_get_set(|v| get_set(v))
            .range(lo..=hi)
            .clamp_existing_to_range(false)
            .speed(speed)
            .suffix(&self.suffix)
            .custom_parser(move |text| typed(text, base))
            .update_while_editing(false);
        if self.integer {
            drag = drag.fixed_decimals(0);
        }
        let response = field.add(drag);
        let focused = take_focus(ui);
        if focused {
            response.request_focus();
        }
        select_all_after(ui, &response, focused);
        let typing = ui.memory(|m| m.focused()) == Some(response.id);
        typing_kept(ui, key, value, typing);
        // Ctrl, as on a number: steps of a tenth of the range, or two
        // significant digits of a logarithmic one's value.
        if response.dragged() && ui.input(|i| i.modifiers.ctrl || i.modifiers.command) {
            let v = (self.get_set)(None);
            let step = snap(if log { v.abs() } else { hi - lo }, self.integer);
            (self.get_set)(Some(((v / step).round() * step).clamp(lo, hi)));
        }
        // Held, dark under its blue, as Blender's: not egui's active blue,
        // which filled the whole field.
        let shapes = field_back(ui, &response, rect, typing, Some(share));
        ui.painter().set(back, egui::Shape::Vec(shapes));
        response
    }
}

/// A list of values to choose from, as Blender's menu button: darker than the
/// other buttons, and its menu Blender's, the item under the pointer in the
/// blue. And as Blender's menus, typed into: a search field atop it holds the
/// focus, and the items (`choice`, `choice_label`) are those whose labels
/// match what is typed (`fuzzy`); `Enter` takes the first. egui's `ComboBox`,
/// with the same calls.
pub struct ComboBox(egui::ComboBox, egui::Id);

/// What the menu open is searched for, as its items are drawn.
#[derive(Clone, Default)]
struct MenuSearch {
    query: String,
    /// `Enter` pressed: the first item that matches is taken.
    take_first: bool,
    taken: bool,
}

fn menu_search_id() -> egui::Id {
    egui::Id::new("kalast menu search")
}

impl ComboBox {
    pub fn from_id_salt(id_salt: impl std::hash::Hash + std::fmt::Debug) -> Self {
        let key = egui::Id::new(("kalast menu", &id_salt));
        // What is chosen cut short in the button, with an ellipsis, rather
        // than past the panel's edge: "Crater in a plane, 1024-5000_h=0.437".
        let menu = egui::ComboBox::from_id_salt(id_salt)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .wrap_mode(egui::TextWrapMode::Truncate);
        Self(menu, key)
    }

    pub fn selected_text(self, text: impl Into<egui::WidgetText>) -> Self {
        Self(self.0.selected_text(text), self.1)
    }

    pub fn width(self, width: f32) -> Self {
        Self(self.0.width(width), self.1)
    }

    pub fn height(self, height: f32) -> Self {
        Self(self.0.height(height), self.1)
    }

    pub fn show_ui<R>(self, ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> egui::InnerResponse<Option<R>> {
        let menu = lighten_by(ui.visuals().text_edit_bg_color(), 11);
        let blue = ui.visuals().selection.bg_fill;
        let key = self.1;
        let shown = ui
            .scope(|ui| {
                let w = &mut ui.visuals_mut().widgets;
                w.inactive.weak_bg_fill = menu;
                w.hovered.weak_bg_fill = lighten_by(menu, 8);
                // Blender's blue at 70 % while its menu is open.
                w.open.weak_bg_fill = blue.linear_multiply(0.7);
                self.0.icon(menu_chevron).show_ui(ui, |ui| {
                    // The item chosen and the one under the pointer in the
                    // theme's solid fill, their text bright: Mocha's
                    // selection, a tint behind overlay-grey text, was hard
                    // to read.
                    let fill = super::theme::extra(ui.ctx()).fill;
                    let bright = ui.visuals().strong_text_color();
                    let v = ui.visuals_mut();
                    v.selection.bg_fill = fill;
                    v.selection.stroke.color = bright;
                    let mut query = ui.data(|d| d.get_temp::<String>(key)).unwrap_or_default();
                    let field = text_field_as(
                        ui,
                        egui::TextEdit::singleline(&mut query)
                            .hint_text(format!("{}  search", icons::codicon::SEARCH))
                            .desired_width(f32::INFINITY),
                        key.with("field"),
                    );
                    // Asked before the field is given the focus back: given
                    // it, it had not lost it, and `Enter` took nothing.
                    let take_first = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if !take_first && ui.memory(|m| m.focused().is_none()) {
                        field.request_focus();
                    }
                    ui.data_mut(|d| {
                        d.insert_temp(key, query.clone());
                        d.insert_temp(menu_search_id(), MenuSearch { query, take_first, taken: false });
                    });
                    ui.visuals_mut().widgets.hovered.weak_bg_fill = fill;
                    let inner = add(ui);
                    ui.data_mut(|d| d.remove::<MenuSearch>(menu_search_id()));
                    inner
                })
            })
            .inner;
        // Shut: what was typed goes, for the next time it opens.
        if shown.inner.is_none() {
            ui.data_mut(|d| d.remove::<String>(key));
        }
        let radius = ui.visuals().widgets.inactive.corner_radius.sw;
        ui.painter().add(emboss(shown.response.rect, radius));
        shown
    }
}

/// A menu button's one thin chevron, Blender's: pointing down, 7 wide and 4
/// high, 6.4 from the button's right edge -- `rect` is egui's place for its
/// icon, inside the button's padding of 8.
fn menu_chevron(ui: &egui::Ui, rect: egui::Rect, visuals: &egui::style::WidgetVisuals, _open: bool) {
    let c = egui::pos2(rect.right() + 8.0 - 6.4, rect.center().y + 0.5);
    let points = vec![c + egui::vec2(-3.5, -2.0), c + egui::vec2(0.0, 2.0), c + egui::vec2(3.5, -2.0)];
    ui.painter().add(egui::Shape::line(points, egui::Stroke::new(1.0, visuals.fg_stroke.color)));
}

/// Whether a menu's item labelled `label` is shown: always, unless a menu is
/// being searched and the label does not match; and whether `Enter` takes it,
/// the first that matches.
fn menu_item(ui: &mut egui::Ui, label: &str) -> (bool, bool) {
    let Some(mut search) = ui.data(|d| d.get_temp::<MenuSearch>(menu_search_id())) else { return (true, false) };
    if !search.query.trim().is_empty() && fuzzy::score(&search.query, label).is_none() {
        return (false, false);
    }
    let take = search.take_first && !search.taken;
    search.taken |= take;
    ui.data_mut(|d| d.insert_temp(menu_search_id(), search));
    (true, take)
}

/// Whether a menu's row labelled `label` is shown, for a row drawn by hand
/// rather than as one `choice`: unless the menu is searched for something
/// the label does not match.
pub fn in_menu_search(ui: &egui::Ui, label: &str) -> bool {
    ui.data(|d| d.get_temp::<MenuSearch>(menu_search_id()))
        .is_none_or(|search| search.query.trim().is_empty() || fuzzy::score(&search.query, label).is_some())
}

/// A menu's item setting `current` to `value`: egui's `selectable_value`,
/// hidden while the menu is searched for something its label does not match,
/// and shutting the menu when chosen.
pub fn choice<V: PartialEq>(
    ui: &mut egui::Ui,
    current: &mut V,
    value: V,
    label: impl Into<egui::WidgetText>,
) -> egui::Response {
    let label: egui::WidgetText = label.into();
    let (shown, take) = menu_item(ui, label.text());
    if !shown {
        return nothing(ui, label.text());
    }
    let mut response = ui.selectable_label(*current == value, label);
    if response.clicked() || take {
        if *current != value {
            *current = value;
            response.mark_changed();
        }
        ui.close();
    }
    response
}

/// A menu's item that is a label, chosen by a click: egui's
/// `selectable_label`, hidden as `choice` is, shutting the menu when clicked.
pub fn choice_label(ui: &mut egui::Ui, selected: bool, label: impl Into<egui::WidgetText>) -> egui::Response {
    let label: egui::WidgetText = label.into();
    if !menu_item(ui, label.text()).0 {
        return nothing(ui, label.text());
    }
    let response = ui.selectable_label(selected, label);
    if response.clicked() {
        ui.close();
    }
    response
}

/// `c` lighter by `step` on each channel: Blender lights a widget under the
/// pointer by 17.
fn lighten_by(c: egui::Color32, step: u8) -> egui::Color32 {
    let up = |v: u8| v.saturating_add(step);
    egui::Color32::from_rgba_premultiplied(up(c.r()), up(c.g()), up(c.b()), c.a())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Blender's split: the label column 0.4 of the row less the decorators'
    /// 20 and the 8 between, the field the rest, as its properties editor lays
    /// a 311-pixel region's rows out.
    #[test]
    fn a_row_is_split_as_blenders() {
        assert_eq!(split(300.0), (108.0, 116.0, 164.0));
        // Blender's, measured: content 280 wide, label 100, field 152 from
        // the content's left.
        let (label, field_x, field) = split(280.0);
        assert_eq!((label, field_x), (100.0, 108.0));
        assert_eq!(field_x + field, 280.0 - 20.0, "the decorators' column left at the right");
        assert_eq!(split(10.0), (0.0, 8.0, 0.0), "too narrow: nothing but the gap");
    }

    /// A setting that is on or off, chosen in the `E` search, is switched, and
    /// once: the frame after leaves it be.
    #[test]
    fn a_toggle_chosen_in_the_search_is_switched() {
        let ctx = egui::Context::default();
        let mut on = false;
        let mut frame = |on: &mut bool| {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                toggle(ui, "render_back_face", "", on);
            });
            out.textures_delta.clear();
        };
        frame(&mut on);
        assert!(!on, "drawn, left as it was");
        reveal(&ctx, Place { app: false, path: Vec::new(), name: "render_back_face".into(), on: Some(false) });
        frame(&mut on);
        assert!(on, "chosen, switched on");
        frame(&mut on);
        assert!(on, "and only once");
    }

    /// A value too large or too small for its field in powers of ten, which
    /// the field reads back when typed.
    #[test]
    fn a_far_value_is_written_short() {
        assert_eq!(compact(-30627055616.0, 2..=2), "-3.063e10");
        assert_eq!(compact(0.00002, 2..=4), "2.000e-5");
        assert_eq!(compact(5.0, 2..=2), "5.00");
        assert_eq!(compact(0.0, 2..=2), "0.00");
        assert_eq!(typed("-3.063e10", 0.0), Some(-3.063e10));
        assert_eq!(arrow_step(0.01, false), 0.1);
        assert_eq!(arrow_step(0.5, true), 1.0);
        assert_eq!(tidy(0.1 + 0.2), 0.3);
    }
}
