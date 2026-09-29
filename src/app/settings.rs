//! What the UI app remembers between sessions: `app.config.theme`,
//! `app.config.fullscreen`, the script editor's settings -- Neovim and its
//! config, the ruler, the language servers -- and the window: where it was
//! left -- on which screen, where on it, how big -- or, with
//! `remember_window` off, where it is to open every time.
//!
//! In `settings.toml` in the user's configuration folder -- never in the
//! project or the bundle -- or wherever `KALAST_SETTINGS` names:
//!
//! | | |
//! |---|---|
//! | macOS | `~/Library/Application Support/kalast/settings.toml` |
//! | Linux | `$XDG_CONFIG_HOME/kalast/settings.toml`, else `~/.config/kalast/settings.toml` |
//! | Windows | `%APPDATA%\kalast\settings.toml` |
//!
//! Read when the UI app starts, before a script runs, so a script that sets
//! either still has the last word; written when one is changed *in the app*
//! -- its widget, `F`, the green button -- and never when a script sets it,
//! so a script dressing a figure does not change the app for next time. The
//! window's place is written when the app closes, from the window itself.

use crate::app::config::{AppConfig, UiTheme};

/// Where the window was left.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    /// The screen, by the name the system gives it and its origin on the
    /// desktop, in physical pixels: two screens of one model share a name.
    pub monitor: String,
    pub origin: (i32, i32),
    /// The window's top-left corner from the screen's, in physical pixels,
    /// where the system says -- Wayland does not.
    pub position: Option<(i32, i32)>,
    /// Its inner size, in physical pixels: not known when it was only ever
    /// fullscreen or maximised.
    pub size: Option<(u32, u32)>,
    pub maximized: bool,
}

/// The remembered fields, as they stand in `config`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remembered {
    pub theme: UiTheme,
    pub fullscreen: bool,
    /// Where the window opens: where it was left, or -- off -- the six
    /// below, which `apply` then gives the config.
    pub remember_window: bool,
    pub monitor: String,
    pub window_x: i32,
    pub window_y: i32,
    pub width: u32,
    pub height: u32,
    pub start_fullscreen: bool,
    pub neovim: bool,
    pub neovim_path: String,
    pub neovim_config: String,
    pub ruler: u32,
    pub language_servers: bool,
    pub python_language_server: String,
    pub rust_language_server: String,
    /// Not the config's: written when the app closes (`save_place`), and
    /// kept by `save`.
    pub window: Option<Place>,
}

impl Remembered {
    pub fn of(config: &AppConfig) -> Self {
        Self {
            theme: config.theme,
            fullscreen: config.fullscreen,
            remember_window: config.remember_window,
            monitor: config.monitor.clone(),
            window_x: config.window_x,
            window_y: config.window_y,
            width: config.width,
            height: config.height,
            start_fullscreen: config.start_fullscreen,
            neovim: config.neovim,
            neovim_path: config.neovim_path.clone(),
            neovim_config: config.neovim_config.clone(),
            ruler: config.ruler,
            language_servers: config.language_servers,
            python_language_server: config.python_language_server.clone(),
            rust_language_server: config.rust_language_server.clone(),
            window: None,
        }
    }

    pub fn apply(&self, config: &mut AppConfig) {
        config.theme = self.theme;
        config.remember_window = self.remember_window;
        // Remembering, the window opens as it was left: fullscreen as it
        // was, and the place is `window`'s, the six at their defaults so as
        // not to force anything. Not, as the six say. Unticking the box
        // fills them from the window as it then stands (`App::realise`).
        config.monitor = self.monitor.clone();
        config.window_x = self.window_x;
        config.window_y = self.window_y;
        config.width = self.width;
        config.height = self.height;
        config.start_fullscreen = self.start_fullscreen;
        if self.remember_window {
            config.fullscreen = self.fullscreen;
            config.monitor.clear();
            (config.window_x, config.window_y, config.width, config.height) = (-1, -1, 0, 0);
        } else {
            config.fullscreen = self.start_fullscreen;
        }
        config.neovim = self.neovim;
        config.neovim_path = self.neovim_path.clone();
        config.neovim_config = self.neovim_config.clone();
        config.ruler = self.ruler;
        config.language_servers = self.language_servers;
        config.python_language_server = self.python_language_server.clone();
        config.rust_language_server = self.rust_language_server.clone();
    }

    /// `key = value` lines, the TOML this needs and nothing more. A key it
    /// does not know, or a value it cannot read, is skipped and that field
    /// keeps its default, so a file from a newer or older kalast still loads.
    fn parse(text: &str) -> Self {
        let mut r = Self::of(&AppConfig::default());
        let (mut monitor, mut origin, mut position, mut size, mut maximized) =
            (None, (None, None), (None, None), (None, None), false);
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            let flag = |b: &mut bool| match value {
                "true" => *b = true,
                "false" => *b = false,
                _ => {}
            };
            match key.trim() {
                "theme" => {
                    if let Some(t) = UiTheme::parse(value.trim_matches('"')) {
                        r.theme = t;
                    }
                }
                "fullscreen" => flag(&mut r.fullscreen),
                "remember_window" => flag(&mut r.remember_window),
                "start_fullscreen" => flag(&mut r.start_fullscreen),
                "monitor" => r.monitor = unquote(value),
                "window_x" => {
                    if let Ok(n) = value.parse() {
                        r.window_x = n;
                    }
                }
                "window_y" => {
                    if let Ok(n) = value.parse() {
                        r.window_y = n;
                    }
                }
                "width" => {
                    if let Ok(n) = value.parse() {
                        r.width = n;
                    }
                }
                "height" => {
                    if let Ok(n) = value.parse() {
                        r.height = n;
                    }
                }
                "neovim" => flag(&mut r.neovim),
                "language_servers" => flag(&mut r.language_servers),
                "ruler" => {
                    if let Ok(n) = value.parse() {
                        r.ruler = n;
                    }
                }
                "neovim_path" => r.neovim_path = unquote(value),
                "neovim_config" => r.neovim_config = unquote(value),
                "python_language_server" => r.python_language_server = unquote(value),
                "rust_language_server" => r.rust_language_server = unquote(value),
                "left_monitor" => monitor = Some(unquote(value)),
                "left_monitor_x" => origin.0 = value.parse().ok(),
                "left_monitor_y" => origin.1 = value.parse().ok(),
                "left_x" => position.0 = value.parse().ok(),
                "left_y" => position.1 = value.parse().ok(),
                "left_width" => size.0 = value.parse().ok(),
                "left_height" => size.1 = value.parse().ok(),
                "left_maximized" => flag(&mut maximized),
                _ => {}
            }
        }
        r.window = monitor.map(|monitor| Place {
            monitor,
            origin: origin.0.zip(origin.1).unwrap_or((0, 0)),
            position: position.0.zip(position.1),
            size: size.0.zip(size.1),
            maximized,
        });
        r
    }

    fn to_text(&self) -> String {
        format!(
            "# What the kalast UI app remembers; written by the app when these change in it.\n\
             theme = \"{}\"\n\
             fullscreen = {}\n\
             remember_window = {}\n\
             monitor = {}\n\
             window_x = {}\n\
             window_y = {}\n\
             width = {}\n\
             height = {}\n\
             start_fullscreen = {}\n\
             neovim = {}\n\
             neovim_path = {}\n\
             neovim_config = {}\n\
             ruler = {}\n\
             language_servers = {}\n\
             python_language_server = {}\n\
             rust_language_server = {}\n",
            self.theme.name(),
            self.fullscreen,
            self.remember_window,
            quote(&self.monitor),
            self.window_x,
            self.window_y,
            self.width,
            self.height,
            self.start_fullscreen,
            self.neovim,
            quote(&self.neovim_path),
            quote(&self.neovim_config),
            self.ruler,
            self.language_servers,
            quote(&self.python_language_server),
            quote(&self.rust_language_server),
        ) + &self.window.as_ref().map(place_text).unwrap_or_default()
    }
}

/// The window's lines, after the settings'.
fn place_text(p: &Place) -> String {
    let mut text = format!(
        "# Where the window was left, written when the app closes.\n\
         left_monitor = {}\n\
         left_monitor_x = {}\n\
         left_monitor_y = {}\n",
        quote(&p.monitor),
        p.origin.0,
        p.origin.1,
    );
    if let Some((x, y)) = p.position {
        text += &format!("left_x = {x}\nleft_y = {y}\n");
    }
    if let Some((w, h)) = p.size {
        text += &format!("left_width = {w}\nleft_height = {h}\n");
    }
    text + &format!("left_maximized = {}\n", p.maximized)
}

/// A TOML basic string: a Windows path's backslashes and a command's quotes
/// escaped, so `C:\Program Files\Neovim` reads back as written.
fn quote(s: &str) -> String {
    let mut out = String::from('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `quote` undone. Anything not in quotes is taken as it stands.
fn unquote(s: &str) -> String {
    let Some(inner) = s.strip_prefix('"').and_then(|s| s.strip_suffix('"')) else {
        return s.to_string();
    };
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Where the settings live, if there is a home to put them in.
pub fn path() -> Option<std::path::PathBuf> {
    if let Some(p) = std::env::var_os("KALAST_SETTINGS") {
        return Some(p.into());
    }
    let home = || std::env::var_os("HOME").map(std::path::PathBuf::from);
    let dir = if cfg!(target_os = "macos") {
        home()?.join("Library/Application Support")
    } else if cfg!(windows) {
        std::env::var_os("APPDATA")?.into()
    } else {
        match std::env::var_os("XDG_CONFIG_HOME") {
            Some(d) if !d.is_empty() => d.into(),
            _ => home()?.join(".config"),
        }
    };
    Some(dir.join("kalast").join("settings.toml"))
}

/// The remembered settings, or `None` when nothing has been saved yet.
pub fn load() -> Option<Remembered> {
    load_from(&path()?)
}

fn load_from(path: &std::path::Path) -> Option<Remembered> {
    let text = std::fs::read_to_string(path).ok()?;
    Some(Remembered::parse(&text))
}

/// Remember `r`, and the window's place already remembered unless `r` has
/// one. A failure is reported and otherwise ignored: forgetting a theme is
/// not worth interrupting anyone over.
pub fn save(r: &Remembered) {
    if let Some(path) = path() {
        save_to(&path, r);
    }
}

fn save_to(path: &std::path::Path, r: &Remembered) {
    let kept;
    let r = if r.window.is_none() {
        kept = Remembered { window: load_from(path).and_then(|old| old.window), ..r.clone() };
        &kept
    } else {
        r
    };
    let written = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(path, r.to_text()));
    if let Err(e) = written {
        eprintln!("cannot remember the app's settings in {}: {e}", path.display());
    }
}

/// Remember where the window was left, and the rest as it was remembered --
/// not as a script may have set it.
pub fn save_place(place: Place) {
    if let Some(path) = path() {
        save_place_to(&path, place);
    }
}

fn save_place_to(path: &std::path::Path, place: Place) {
    let old = load_from(path).unwrap_or_else(|| Remembered::of(&AppConfig::default()));
    save_to(path, &Remembered { window: Some(place), ..old });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_written_reads_back() {
        let default = Remembered::of(&AppConfig::default());
        for r in [
            Remembered { theme: UiTheme::Dark, fullscreen: true, ..default.clone() },
            Remembered {
                theme: UiTheme::CatppuccinMocha,
                fullscreen: false,
                neovim: true,
                neovim_path: r"C:\Program Files\Neovim\bin\nvim.exe".to_string(),
                neovim_config: r"C:\Users\me\AppData\Local\nvim".to_string(),
                ruler: 100,
                language_servers: false,
                remember_window: false,
                monitor: "PHL 279P1".to_string(),
                window_x: 40,
                window_y: -1,
                width: 1600,
                height: 900,
                start_fullscreen: true,
                python_language_server: "pyright-langserver --stdio".to_string(),
                rust_language_server: r#""C:\tools\rust analyzer.exe""#.to_string(),
                window: Some(Place {
                    monitor: "DELL U2720Q".to_string(),
                    origin: (-2560, 0),
                    position: Some((120, -40)),
                    size: Some((2400, 1300)),
                    maximized: true,
                }),
            },
            Remembered {
                window: Some(Place { monitor: String::new(), origin: (0, 0), position: None, size: None, maximized: false }),
                ..default.clone()
            },
        ] {
            assert_eq!(Remembered::parse(&r.to_text()), r);
        }
    }

    /// The window's place, written as the app closes, and the settings,
    /// written as they change in the app, each keep the other.
    #[test]
    fn the_window_and_the_settings_keep_each_other() {
        let dir = std::env::temp_dir().join(format!("kalast-settings-{}", std::process::id()));
        let path = dir.join("settings.toml");
        let place = Place {
            monitor: "Built-in Retina Display".to_string(),
            origin: (0, 0),
            position: Some((80, 60)),
            size: Some((2000, 1200)),
            maximized: false,
        };
        save_place_to(&path, place.clone());
        let first = load_from(&path).unwrap();
        assert_eq!(first.window.as_ref(), Some(&place));
        assert_eq!(first.theme, AppConfig::default().theme, "defaults, not a script's settings");

        // The theme changed in the app: the window's place stays.
        save_to(&path, &Remembered { theme: UiTheme::Dark, ..Remembered::of(&AppConfig::default()) });
        let second = load_from(&path).unwrap();
        assert_eq!((second.theme, second.window.as_ref()), (UiTheme::Dark, Some(&place)));

        // The window moved: the theme stays.
        let moved = Place { position: Some((10, 10)), ..place };
        save_place_to(&path, moved.clone());
        let third = load_from(&path).unwrap();
        assert_eq!((third.theme, third.window), (UiTheme::Dark, Some(moved)));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Remembering, the window opens as it was left and the forced values
    /// wait; not, they are the config's, fullscreen included.
    #[test]
    fn the_window_opens_as_left_or_as_set() {
        let set = Remembered {
            fullscreen: true,
            remember_window: false,
            monitor: "2".to_string(),
            window_x: 10,
            window_y: 20,
            width: 1200,
            height: 800,
            start_fullscreen: false,
            ..Remembered::of(&AppConfig::default())
        };
        let mut config = AppConfig::default();
        set.apply(&mut config);
        assert_eq!(
            (config.monitor.as_str(), config.window_x, config.window_y, config.width, config.height, config.fullscreen),
            ("2", 10, 20, 1200, 800, false),
            "as set, fullscreen as set rather than as last left"
        );
        let mut config = AppConfig::default();
        Remembered { remember_window: true, ..set.clone() }.apply(&mut config);
        assert_eq!(
            (config.monitor.as_str(), config.window_x, config.width, config.fullscreen, config.start_fullscreen),
            ("", -1, 0, true, false),
            "as left, the forced values kept for later"
        );
    }

    /// A file from another version -- a key this one does not know, a value
    /// it cannot read -- still loads, keeping the defaults where it must.
    #[test]
    fn an_unknown_key_or_value_keeps_the_default() {
        let r = Remembered::parse("theme = \"latte\"\nfullscreen = true\nzoom = 2\n");
        assert_eq!(r.theme, AppConfig::default().theme);
        assert!(r.fullscreen);
    }
}
