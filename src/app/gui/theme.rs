//! The colours the UI app's panels are drawn in: `app.config.theme`. Blender's
//! own by default, as the panels are drawn after Blender's (`widgets.rs`).
//!
//! The panels only. The scene is drawn by the renderer and shown as an image
//! with no tint, over a pass cleared to black, so no theme reaches a render,
//! its background, or an exported frame.

use crate::app::config::UiTheme;
use egui::style::{Selection, WidgetVisuals, Widgets};
use egui::{Color32, Stroke, Visuals};

/// Put `theme` in both of egui's slots, dark and light: egui follows the
/// system between the two, and switching the system's appearance would
/// otherwise take the theme away -- `Dark` included, which is dark whatever
/// the system says.
pub fn apply(ctx: &egui::Context, theme: UiTheme) {
    let visuals = match theme {
        UiTheme::Blender => blender(),
        UiTheme::CatppuccinMocha => mocha(),
        UiTheme::Dark => Visuals::dark(),
    };
    ctx.set_visuals_of(egui::Theme::Dark, visuals.clone());
    ctx.set_visuals_of(egui::Theme::Light, visuals);
    ctx.data_mut(|d| d.insert_temp(extra_id(), extras(theme)));
    metrics(ctx);
}

/// Blender's sizes, for every theme: its text at 11 points, Regular -- its
/// every style is, titles included -- and its widgets 20 high, the unit its
/// whole layout is counted in. The script and the log keep 12-point
/// monospace, as Blender's text editor has it.
fn metrics(ctx: &egui::Context) {
    use egui::{FontFamily::Proportional, FontId, TextStyle};
    ctx.all_styles_mut(|style| {
        style.text_styles.insert(TextStyle::Body, FontId::new(11.0, Proportional));
        style.text_styles.insert(TextStyle::Button, FontId::new(11.0, Proportional));
        style.text_styles.insert(TextStyle::Small, FontId::new(9.5, Proportional));
        style.spacing.interact_size.y = 20.0;
        style.spacing.button_padding = egui::vec2(8.0, 2.0);
    });
}

/// What the panels need beyond egui's visuals, by theme (`extra`): the box a
/// section is drawn in, which must stand apart from the region behind it and
/// from the fields in it -- Mocha's surface0 was both a section and a field,
/// and its fields vanished; and solid colours where egui's selection is a
/// tint: a ticked box and its tick, and a slider's fill, which its value is
/// written across and must be read on; a field held -- dragged, pressed or
/// typed into -- and a text field typed into, which Blender darkens; and the
/// side panel's navigation bar, the darkest of its greys.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Extra {
    pub panel: Color32,
    pub check: Color32,
    pub tick: Color32,
    pub fill: Color32,
    pub active_field: Color32,
    pub editing: Color32,
    pub nav: Color32,
}

fn extra_id() -> egui::Id {
    egui::Id::new("kalast theme extra")
}

/// The theme's `Extra`, as `apply` left it.
pub fn extra(ctx: &egui::Context) -> Extra {
    ctx.data(|d| d.get_temp(extra_id())).unwrap_or_else(|| extras(UiTheme::Blender))
}

fn extras(theme: UiTheme) -> Extra {
    match theme {
        UiTheme::Blender => Extra {
            panel: blender::PANEL,
            check: blender::BLUE,
            tick: Color32::WHITE,
            fill: blender::BLUE,
            active_field: blender::FIELD_HELD,
            editing: blender::POPUP,
            nav: blender::FIELD,
        },
        // Sections on base over the side panel's mantle, fields on surface0
        // in them; ticked in blue with crust, as Catppuccin's checkboxes are;
        // a slider filled with blue at 45% over base, under its text.
        UiTheme::CatppuccinMocha => Extra {
            panel: palette::BASE,
            check: palette::BLUE,
            tick: palette::CRUST,
            fill: Color32::from_rgb(0x4e, 0x61, 0x8a),
            active_field: palette::CRUST,
            editing: palette::CRUST,
            nav: palette::CRUST,
        },
        UiTheme::Dark => Extra {
            panel: Color32::from_gray(32),
            check: Color32::from_rgb(0, 92, 128),
            tick: Color32::WHITE,
            fill: Color32::from_rgb(0, 92, 128),
            active_field: Color32::from_gray(16),
            editing: Color32::from_gray(5),
            nav: Color32::from_gray(12),
        },
    }
}

/// What a drag handle is lit in: Blender's blue, what it lights the active
/// and the chosen in; Catppuccin's accent, mauve, as its VS Code port lights
/// an edge; VS Code's own blue in dark.
pub fn accent(theme: UiTheme) -> Color32 {
    match theme {
        UiTheme::Blender => blender::BLUE,
        UiTheme::CatppuccinMocha => palette::MAUVE,
        UiTheme::Dark => Color32::from_rgb(0x00, 0x7f, 0xd4),
    }
}

/// The side panel's card: Blender's properties editor; in the others a shade
/// under the other cards, as VS Code's sidebar is under its editor -- mantle
/// against base in Mocha.
pub fn side_fill(theme: UiTheme) -> Color32 {
    match theme {
        UiTheme::Blender => blender::REGION,
        UiTheme::CatppuccinMocha => palette::MANTLE,
        UiTheme::Dark => Color32::from_gray(20),
    }
}

/// A card's outline: barely lighter than the card, as Blender's editors and
/// VS Code's cards are.
pub fn outline(theme: UiTheme) -> Color32 {
    match theme {
        UiTheme::Blender => blender::EDITOR_OUTLINE,
        UiTheme::CatppuccinMocha => palette::SURFACE0,
        UiTheme::Dark => Color32::from_gray(44),
    }
}

/// The colours VS Code gives its debug toolbar's icons: run and restart
/// green, step and pause blue, stop red -- Blender's console's green, blue
/// and red in Blender's, Catppuccin's shades of them in Mocha, VS Code's own
/// in dark.
pub struct Actions {
    pub run: Color32,
    pub step: Color32,
    pub stop: Color32,
}

pub fn actions(theme: UiTheme) -> Actions {
    match theme {
        UiTheme::Blender => Actions { run: blender::INFO, step: blender::BLUE_LIGHT, stop: blender::ERROR },
        UiTheme::CatppuccinMocha => Actions { run: palette::GREEN, step: palette::BLUE, stop: palette::RED },
        UiTheme::Dark => Actions {
            run: Color32::from_rgb(0x89, 0xd1, 0x85),
            step: Color32::from_rgb(0x75, 0xbe, 0xff),
            stop: Color32::from_rgb(0xf4, 0x87, 0x71),
        },
    }
}

/// A tree row's colours, as VS Code's lists have them: `(hover, selected,
/// indent guide)`. Blender's outliner's active row in Blender's; Catppuccin's
/// surfaces on the side panel's mantle in Mocha.
pub fn list(theme: UiTheme) -> (Color32, Color32, Color32) {
    match theme {
        UiTheme::Blender => (Color32::from_white_alpha(10), blender::OUTLINER_ACTIVE, blender::PANEL),
        UiTheme::CatppuccinMocha => (
            Color32::from_rgba_unmultiplied(0x31, 0x32, 0x44, 150),
            palette::SURFACE0,
            palette::SURFACE1,
        ),
        UiTheme::Dark => (Color32::from_gray(42), Color32::from_rgb(0x04, 0x39, 0x5e), Color32::from_gray(58)),
    }
}

/// VS Code's primary button -- "Update", "Install", "Restart to update" --
/// the accent filled in, its label in the darkest shade: `(fill, label)`.
/// Blender's blue, its label white, as Blender's own chosen buttons.
pub fn primary(theme: UiTheme) -> (Color32, Color32) {
    match theme {
        UiTheme::Blender => (blender::BLUE, blender::TEXT_HI),
        UiTheme::CatppuccinMocha => (palette::MAUVE, palette::CRUST),
        UiTheme::Dark => (Color32::from_rgb(0x00, 0x78, 0xd4), Color32::WHITE),
    }
}

/// Catppuccin's Mocha palette, as much of it as the UI uses.
/// <https://catppuccin.com/palette>
pub(super) mod palette {
    use egui::Color32;

    pub const ROSEWATER: Color32 = Color32::from_rgb(0xf5, 0xe0, 0xdc);
    pub const FLAMINGO: Color32 = Color32::from_rgb(0xf2, 0xcd, 0xcd);
    pub const PINK: Color32 = Color32::from_rgb(0xf5, 0xc2, 0xe7);
    pub const MAUVE: Color32 = Color32::from_rgb(0xcb, 0xa6, 0xf7);
    pub const MAROON: Color32 = Color32::from_rgb(0xeb, 0xa0, 0xac);
    pub const PEACH: Color32 = Color32::from_rgb(0xfa, 0xb3, 0x87);
    pub const YELLOW: Color32 = Color32::from_rgb(0xf9, 0xe2, 0xaf);
    pub const TEAL: Color32 = Color32::from_rgb(0x94, 0xe2, 0xd5);
    pub const SKY: Color32 = Color32::from_rgb(0x89, 0xdc, 0xeb);
    pub const SAPPHIRE: Color32 = Color32::from_rgb(0x74, 0xc7, 0xec);
    pub const BLUE: Color32 = Color32::from_rgb(0x89, 0xb4, 0xfa);
    pub const LAVENDER: Color32 = Color32::from_rgb(0xb4, 0xbe, 0xfe);
    pub const GREEN: Color32 = Color32::from_rgb(0xa6, 0xe3, 0xa1);
    pub const RED: Color32 = Color32::from_rgb(0xf3, 0x8b, 0xa8);
    pub const TEXT: Color32 = Color32::from_rgb(0xcd, 0xd6, 0xf4);
    pub const OVERLAY1: Color32 = Color32::from_rgb(0x7f, 0x84, 0x9c);
    pub const SURFACE2: Color32 = Color32::from_rgb(0x58, 0x5b, 0x70);
    pub const SURFACE1: Color32 = Color32::from_rgb(0x45, 0x47, 0x5a);
    pub const SURFACE0: Color32 = Color32::from_rgb(0x31, 0x32, 0x44);
    pub const BASE: Color32 = Color32::from_rgb(0x1e, 0x1e, 0x2e);
    pub const MANTLE: Color32 = Color32::from_rgb(0x18, 0x18, 0x25);
    pub const CRUST: Color32 = Color32::from_rgb(0x11, 0x11, 0x1b);
}

/// Blender's default theme, as much of it as the UI uses: its values in
/// Blender's `userdef_default_theme.c`, the colours alone.
pub(super) mod blender {
    use egui::Color32;

    /// The properties editor's region, behind its panels.
    pub const REGION: Color32 = Color32::from_rgb(0x30, 0x30, 0x30);
    /// A panel: its header and its back, one colour since Blender 4.
    pub const PANEL: Color32 = Color32::from_rgb(0x3d, 0x3d, 0x3d);
    /// A button's, a number field's, a checkbox's inner; under the pointer,
    /// lighter by the step Blender lights it.
    pub const WIDGET: Color32 = Color32::from_rgb(0x54, 0x54, 0x54);
    pub const WIDGET_HOVER: Color32 = Color32::from_rgb(0x65, 0x65, 0x65);
    pub const WIDGET_OUTLINE: Color32 = Color32::from_rgb(0x3d, 0x3d, 0x3d);
    /// A number field or a slider held: dragged, pressed, typed into.
    pub const FIELD_HELD: Color32 = Color32::from_rgb(0x22, 0x22, 0x22);
    /// A menu, a tooltip: what pops up.
    pub const POPUP: Color32 = Color32::from_rgb(0x18, 0x18, 0x18);
    pub const POPUP_OUTLINE: Color32 = Color32::from_rgb(0x24, 0x24, 0x24);
    /// A text field's inner; the strip a row of tabs sits on.
    pub const FIELD: Color32 = Color32::from_rgb(0x1d, 0x1d, 0x1d);
    /// The gap between editors, and the outline each has.
    pub const BORDER: Color32 = Color32::from_rgb(0x16, 0x16, 0x16);
    pub const EDITOR_OUTLINE: Color32 = Color32::from_rgba_premultiplied(0x15, 0x15, 0x15, 0x15);
    /// The active, the chosen, the checked: Blender's blue.
    pub const BLUE: Color32 = Color32::from_rgb(0x47, 0x72, 0xb3);
    /// The text cursor's, the console's output.
    pub const BLUE_LIGHT: Color32 = Color32::from_rgb(0x71, 0xa8, 0xff);
    pub const OUTLINER_ACTIVE: Color32 = Color32::from_rgb(0x33, 0x4d, 0x80);
    pub const TEXT: Color32 = Color32::from_rgb(0xe6, 0xe6, 0xe6);
    pub const TEXT_HI: Color32 = Color32::WHITE;
    /// A tab's label when it is not the chosen one.
    pub const TEXT_DIM: Color32 = Color32::from_rgb(0x98, 0x98, 0x98);
    pub const LINK: Color32 = Color32::from_rgb(0x6f, 0xa9, 0xe6);
    /// The console's error and info, and a warning report's hue, lit to be
    /// read as text.
    pub const ERROR: Color32 = Color32::from_rgb(0xff, 0x4d, 0x84);
    pub const INFO: Color32 = Color32::from_rgb(0x95, 0xd6, 0x00);
    pub const WARNING: Color32 = Color32::from_rgb(0xe0, 0xae, 0x48);
    /// The text editor's back.
    pub const TEXT_EDITOR: Color32 = Color32::from_rgb(0x23, 0x23, 0x23);
}

/// Blender's theme over egui's dark visuals: the region grey behind, widgets
/// a lighter grey with a dark outline and Blender's corners, the active and
/// the chosen in its blue, text fields and popups near black.
fn blender() -> Visuals {
    use blender::*;

    let dark = Visuals::dark();
    let widget = |old: WidgetVisuals, fill: Color32, text: Color32| WidgetVisuals {
        bg_fill: fill,
        weak_bg_fill: fill,
        bg_stroke: Stroke::new(1.0, WIDGET_OUTLINE),
        fg_stroke: Stroke { color: text, ..old.fg_stroke },
        // Blender's roundness, 0.2 of its 20-pixel unit.
        corner_radius: egui::CornerRadius::same(4),
        // Blender does not grow a widget under the pointer.
        expansion: 0.0,
    };
    Visuals {
        dark_mode: true,
        override_text_color: Some(TEXT),
        weak_text_color: Some(TEXT_DIM),
        hyperlink_color: LINK,
        faint_bg_color: PANEL,
        // The gap between editors, which the cards stand on.
        extreme_bg_color: BORDER,
        text_edit_bg_color: Some(FIELD),
        code_bg_color: FIELD,
        warn_fg_color: WARNING,
        error_fg_color: ERROR,
        window_fill: POPUP,
        window_stroke: Stroke::new(1.0, POPUP_OUTLINE),
        window_corner_radius: egui::CornerRadius::same(6),
        menu_corner_radius: egui::CornerRadius::same(4),
        panel_fill: REGION,
        widgets: Widgets {
            noninteractive: WidgetVisuals {
                bg_stroke: Stroke::new(1.0, PANEL),
                ..widget(dark.widgets.noninteractive, REGION, TEXT)
            },
            inactive: widget(dark.widgets.inactive, WIDGET, TEXT),
            hovered: widget(dark.widgets.hovered, WIDGET_HOVER, TEXT_HI),
            active: widget(dark.widgets.active, BLUE, TEXT_HI),
            open: widget(dark.widgets.open, BLUE, TEXT_HI),
        },
        selection: Selection { bg_fill: BLUE, stroke: Stroke::new(1.0, TEXT_HI) },
        text_cursor: egui::style::TextCursorStyle { stroke: Stroke::new(2.0, BLUE_LIGHT), ..dark.text_cursor },
        // Blender's menus' shadow: a fifth of black, six pixels wide.
        window_shadow: egui::epaint::Shadow { offset: [0, 2], blur: 8, spread: 0, color: Color32::from_black_alpha(64) },
        popup_shadow: egui::epaint::Shadow { offset: [0, 2], blur: 6, spread: 0, color: Color32::from_black_alpha(64) },
        slider_trailing_fill: true,
        ..dark
    }
}

/// Mocha over egui's dark visuals, with the roles Catppuccin's own egui
/// port gives each colour: panels on base, fields on crust, widgets on the
/// surfaces, text in text, links in rosewater.
fn mocha() -> Visuals {
    use palette::*;

    let dark = Visuals::dark();
    let widget = |old: WidgetVisuals, fill: Color32| WidgetVisuals {
        bg_fill: fill,
        weak_bg_fill: fill,
        bg_stroke: Stroke { color: OVERLAY1, ..old.bg_stroke },
        fg_stroke: Stroke { color: TEXT, ..old.fg_stroke },
        ..old
    };
    Visuals {
        dark_mode: true,
        override_text_color: Some(TEXT),
        hyperlink_color: ROSEWATER,
        faint_bg_color: SURFACE0,
        extreme_bg_color: CRUST,
        // Fields a shade under the panels, as VS Code's are, rather than on
        // crust -- which is the backdrop between the cards, and would have
        // made the script editor a hole in its own.
        text_edit_bg_color: Some(MANTLE),
        code_bg_color: MANTLE,
        warn_fg_color: PEACH,
        error_fg_color: MAROON,
        window_fill: BASE,
        panel_fill: BASE,
        window_stroke: Stroke { color: OVERLAY1, ..dark.window_stroke },
        widgets: Widgets {
            noninteractive: widget(dark.widgets.noninteractive, BASE),
            inactive: widget(dark.widgets.inactive, SURFACE0),
            hovered: widget(dark.widgets.hovered, SURFACE2),
            active: widget(dark.widgets.active, SURFACE1),
            open: widget(dark.widgets.open, SURFACE0),
        },
        selection: Selection {
            bg_fill: BLUE.linear_multiply(0.2),
            stroke: Stroke { color: OVERLAY1, ..dark.selection.stroke },
        },
        window_shadow: egui::epaint::Shadow { color: BASE, ..dark.window_shadow },
        popup_shadow: egui::epaint::Shadow { color: BASE, ..dark.popup_shadow },
        ..dark
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Either theme holds in both slots, so the system's appearance cannot
    /// swap it out; and dark is egui's dark exactly.
    #[test]
    fn a_theme_holds_in_both_slots() {
        let ctx = egui::Context::default();
        apply(&ctx, UiTheme::CatppuccinMocha);
        for theme in [egui::Theme::Dark, egui::Theme::Light] {
            assert_eq!(ctx.style_of(theme).visuals.panel_fill, palette::BASE, "{theme:?}");
        }
        apply(&ctx, UiTheme::Blender);
        for theme in [egui::Theme::Dark, egui::Theme::Light] {
            assert_eq!(ctx.style_of(theme).visuals.panel_fill, blender::REGION, "{theme:?}");
        }
        apply(&ctx, UiTheme::Dark);
        for theme in [egui::Theme::Dark, egui::Theme::Light] {
            assert_eq!(ctx.style_of(theme).visuals, Visuals::dark(), "{theme:?}");
        }
    }

    #[test]
    fn names_parse_back() {
        for theme in [UiTheme::Blender, UiTheme::CatppuccinMocha, UiTheme::Dark] {
            assert_eq!(UiTheme::parse(theme.name()), Some(theme));
        }
        assert_eq!(UiTheme::parse("Blender"), Some(UiTheme::Blender));
        assert_eq!(UiTheme::parse("Catppuccin Mocha"), Some(UiTheme::CatppuccinMocha));
        assert_eq!(UiTheme::parse("latte"), None);
    }
}
