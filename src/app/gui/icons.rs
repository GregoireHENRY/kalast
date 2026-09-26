//! Icons: VS Code's own for buttons, Catppuccin's for files.
//!
//! Buttons use the Codicons font (microsoft/vscode-codicons, CC BY 4.0,
//! `assets/LICENSE-codicons`; `codicon.ttf` from the `@vscode/codicons` npm
//! package, 0.0.46-24, unchanged) -- the icons VS Code draws its run, debug
//! and explorer actions with. It is a fallback in both of egui's font families,
//! so a codepoint below draws inside any text, beside a label or alone.
//!
//! Files use Catppuccin's icon theme for VS Code (catppuccin/vscode-icons,
//! MIT, `assets/icons/LICENSE`), Mocha: SVGs drawn through egui_extras'
//! loader, picked by `icon_table.rs`, which `tools/gen_icons.py` generates
//! from the extension's own associations.

use super::icon_table;

/// Codicon code points, by their names in VS Code -- the ones kalast draws;
/// the rest are in `codicon.csv` of the `@vscode/codicons` package.
pub mod codicon {
    pub const PLAY: &str = "\u{eb2c}";
    pub const PAUSE: &str = "\u{ead1}";
    pub const RESTART: &str = "\u{ead2}";
    pub const STEP: &str = "\u{ead6}";
    pub const SAVE: &str = "\u{eb4b}";
    pub const CLEAR_ALL: &str = "\u{eabf}";
    pub const CHEVRON_RIGHT: &str = "\u{eab6}";
    pub const CHEVRON_DOWN: &str = "\u{eab4}";
    pub const CLOUD_DOWNLOAD: &str = "\u{eac2}";
    pub const REFRESH: &str = "\u{eb37}";
    // The side panel's tabs.
    pub const SETTINGS_GEAR: &str = "\u{eb51}";
    pub const GLOBE: &str = "\u{eb01}";
    pub const FILES: &str = "\u{eaf0}";
    // The simulation tab's sections.
    pub const PLAY_CIRCLE: &str = "\u{eba6}";
    pub const CIRCLE_LARGE_FILLED: &str = "\u{ebb4}";
    pub const TARGET: &str = "\u{ebf8}";
    pub const DEVICE_CAMERA: &str = "\u{eada}";
    pub const STAR_FULL: &str = "\u{eb59}";
    pub const PAINTCAN: &str = "\u{eb2a}";
    pub const COLOR_MODE: &str = "\u{eac6}";
    pub const LAYERS: &str = "\u{ebd2}";
    pub const SYMBOL_COLOR: &str = "\u{eb5c}";
    pub const MOVE: &str = "\u{eb22}";
    pub const TEXT_SIZE: &str = "\u{eb69}";
    pub const FILE_MEDIA: &str = "\u{eaea}";
    pub const RECORD_KEYS: &str = "\u{ea65}";
    pub const EXPORT: &str = "\u{ebac}";
    pub const DEBUG: &str = "\u{ead8}";
    // The app tab's sections.
    pub const LAYOUT: &str = "\u{ebeb}";
    pub const WINDOW: &str = "\u{eaae}";
    pub const CODE: &str = "\u{eac4}";
    // A code block's copy button in the documentation, and what it turns
    // into once it has copied.
    pub const COPY: &str = "\u{ebcc}";
    pub const CHECK: &str = "\u{eab2}";
}

/// The Codicons font behind every family, DejaVu Sans behind that, and the
/// SVG loader for the file icons. Once per egui context.
pub fn install(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "codicon".to_owned(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!("assets/codicon.ttf"))),
    );
    fonts.font_data.insert(
        "dejavu".to_owned(),
        std::sync::Arc::new(egui::FontData::from_static(crate::app::window::DEJAVU_SANS)),
    );
    // Last in each family: the Codicons' glyphs sit in the private-use area,
    // which no other font there has, so it is reached only for those; and
    // DejaVu after it, for what none of the others draws -- the arrows,
    // `⌥` and `●` the documentation is written with drew as boxes, since
    // egui's proportional font has none of them.
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        let fallbacks = fonts.families.entry(family).or_default();
        fallbacks.push("codicon".to_owned());
        fallbacks.push("dejavu".to_owned());
    }
    ctx.set_fonts(fonts);
    egui_extras::install_image_loaders(ctx);
}

/// The icon for a file named `name`, as VS Code with Catppuccin's theme shows
/// it: by its whole name first -- `Cargo.toml`, `README.md` -- then by its
/// longest extension, then the plain file.
pub fn for_file(name: &str) -> egui::ImageSource<'static> {
    let name = name.to_lowercase();
    if let Some(icon) = find(icon_table::BY_NAME, &name) {
        return icon_table::source(icon);
    }
    // `a.tar.gz`: `tar.gz`, then `gz`.
    let mut rest = name.as_str();
    while let Some(dot) = rest.find('.') {
        rest = &rest[dot + 1..];
        if let Some(icon) = find(icon_table::BY_EXT, rest) {
            return icon_table::source(icon);
        }
    }
    icon_table::source("_file")
}

/// The icon for a folder named `name`, open or shut.
pub fn for_folder(name: &str, open: bool) -> egui::ImageSource<'static> {
    let open = if open { "_open" } else { "" };
    match find(icon_table::BY_FOLDER, &name.to_lowercase()) {
        Some(icon) => icon_table::source(&format!("folder_{icon}{open}")),
        None => icon_table::source(&format!("_folder{open}")),
    }
}

fn find(table: &'static [(&'static str, &'static str)], key: &str) -> Option<&'static str> {
    table.binary_search_by(|(k, _)| (*k).cmp(key)).ok().map(|i| table[i].1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uri(source: egui::ImageSource<'static>) -> String {
        match source {
            egui::ImageSource::Bytes { uri, .. } => uri.into_owned(),
            other => panic!("not compiled in: {other:?}"),
        }
    }

    /// The tables are searched by bisection, so each has to be sorted, and
    /// a name, the longest extension and a folder each find what VS Code
    /// would show.
    #[test]
    fn files_and_folders_find_their_icons() {
        for table in [icon_table::BY_NAME, icon_table::BY_EXT, icon_table::BY_FOLDER] {
            assert!(table.windows(2).all(|w| w[0].0 < w[1].0), "sorted");
        }
        assert!(uri(for_file("main.py")).ends_with("/python.svg"));
        assert!(uri(for_file("Cargo.toml")).ends_with("/cargo.svg"));
        assert!(uri(for_file("stubs.pyi")).ends_with("/python.svg"), "kalast's own addition");
        assert!(uri(for_file("didymos.obj")).ends_with("/3d.svg"));
        assert!(uri(for_file("frames.tar.gz")).ends_with("/zip.svg"));
        assert!(uri(for_file("no_extension")).ends_with("/_file.svg"));
        assert!(uri(for_folder("src", false)).ends_with("/folder_src.svg"));
        assert!(uri(for_folder("examples", true)).ends_with("/folder_examples_open.svg"));
        assert!(uri(for_folder("anything", true)).ends_with("/_folder_open.svg"));
    }

    /// Every icon the table names was fetched, so none falls back to the
    /// plain file by accident.
    #[test]
    fn every_icon_named_is_compiled_in() {
        for (_, icon) in icon_table::BY_NAME.iter().chain(icon_table::BY_EXT) {
            if *icon != "_file" {
                assert!(!uri(icon_table::source(icon)).ends_with("/_file.svg"), "{icon} missing");
            }
        }
        for (_, icon) in icon_table::BY_FOLDER {
            for open in ["", "_open"] {
                let name = format!("folder_{icon}{open}");
                assert!(!uri(icon_table::source(&name)).ends_with("/_file.svg"), "{name} missing");
            }
        }
    }
}
