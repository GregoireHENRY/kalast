//! The colours the UI app's panels are drawn in: `app.config.theme`.
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
        UiTheme::CatppuccinMocha => mocha(),
        UiTheme::Dark => Visuals::dark(),
    };
    ctx.set_visuals_of(egui::Theme::Dark, visuals.clone());
    ctx.set_visuals_of(egui::Theme::Light, visuals);
}

/// What a drag handle is lit in: Catppuccin's accent, mauve, as its VS Code
/// port lights an edge; VS Code's own blue in dark.
pub fn accent(theme: UiTheme) -> Color32 {
    match theme {
        UiTheme::CatppuccinMocha => palette::MAUVE,
        UiTheme::Dark => Color32::from_rgb(0x00, 0x7f, 0xd4),
    }
}

/// The side panel's card: a shade under the others, as VS Code's sidebar
/// is under its editor -- mantle against base in Mocha.
pub fn side_fill(theme: UiTheme) -> Color32 {
    match theme {
        UiTheme::CatppuccinMocha => palette::MANTLE,
        UiTheme::Dark => Color32::from_gray(20),
    }
}

/// A card's outline: barely lighter than the card, as VS Code's are.
pub fn outline(theme: UiTheme) -> Color32 {
    match theme {
        UiTheme::CatppuccinMocha => palette::SURFACE0,
        UiTheme::Dark => Color32::from_gray(44),
    }
}

/// Catppuccin's Mocha palette, as much of it as the UI uses.
/// <https://catppuccin.com/palette>
mod palette {
    use egui::Color32;

    pub const ROSEWATER: Color32 = Color32::from_rgb(0xf5, 0xe0, 0xdc);
    pub const MAUVE: Color32 = Color32::from_rgb(0xcb, 0xa6, 0xf7);
    pub const MAROON: Color32 = Color32::from_rgb(0xeb, 0xa0, 0xac);
    pub const PEACH: Color32 = Color32::from_rgb(0xfa, 0xb3, 0x87);
    pub const BLUE: Color32 = Color32::from_rgb(0x89, 0xb4, 0xfa);
    pub const TEXT: Color32 = Color32::from_rgb(0xcd, 0xd6, 0xf4);
    pub const OVERLAY1: Color32 = Color32::from_rgb(0x7f, 0x84, 0x9c);
    pub const SURFACE2: Color32 = Color32::from_rgb(0x58, 0x5b, 0x70);
    pub const SURFACE1: Color32 = Color32::from_rgb(0x45, 0x47, 0x5a);
    pub const SURFACE0: Color32 = Color32::from_rgb(0x31, 0x32, 0x44);
    pub const BASE: Color32 = Color32::from_rgb(0x1e, 0x1e, 0x2e);
    pub const MANTLE: Color32 = Color32::from_rgb(0x18, 0x18, 0x25);
    pub const CRUST: Color32 = Color32::from_rgb(0x11, 0x11, 0x1b);
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
        apply(&ctx, UiTheme::Dark);
        for theme in [egui::Theme::Dark, egui::Theme::Light] {
            assert_eq!(ctx.style_of(theme).visuals, Visuals::dark(), "{theme:?}");
        }
    }

    #[test]
    fn names_parse_back() {
        for theme in [UiTheme::CatppuccinMocha, UiTheme::Dark] {
            assert_eq!(UiTheme::parse(theme.name()), Some(theme));
        }
        assert_eq!(UiTheme::parse("Catppuccin Mocha"), Some(UiTheme::CatppuccinMocha));
        assert_eq!(UiTheme::parse("latte"), None);
    }
}
