//! What the UI app remembers between sessions: `app.config.theme` and
//! `app.config.fullscreen`.
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
//! so a script dressing a figure does not change the app for next time.

use crate::app::config::{AppConfig, UiTheme};

/// The remembered fields, as they stand in `config`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Remembered {
    pub theme: UiTheme,
    pub fullscreen: bool,
}

impl Remembered {
    pub fn of(config: &AppConfig) -> Self {
        Self {
            theme: config.theme,
            fullscreen: config.fullscreen,
        }
    }

    pub fn apply(&self, config: &mut AppConfig) {
        config.theme = self.theme;
        config.fullscreen = self.fullscreen;
    }

    /// `key = value` lines, the TOML this needs and nothing more. A key it
    /// does not know, or a value it cannot read, is skipped and that field
    /// keeps its default, so a file from a newer or older kalast still loads.
    fn parse(text: &str) -> Self {
        let mut r = Self::of(&AppConfig::default());
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "theme" => {
                    if let Some(t) = UiTheme::parse(value.trim_matches('"')) {
                        r.theme = t;
                    }
                }
                "fullscreen" => match value {
                    "true" => r.fullscreen = true,
                    "false" => r.fullscreen = false,
                    _ => {}
                },
                _ => {}
            }
        }
        r
    }

    fn to_text(self) -> String {
        format!(
            "# What the kalast UI app remembers; written by the app when these change in it.\n\
             theme = \"{}\"\n\
             fullscreen = {}\n",
            self.theme.name(),
            self.fullscreen,
        )
    }
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
    let text = std::fs::read_to_string(path()?).ok()?;
    Some(Remembered::parse(&text))
}

/// Remember `r`. A failure is reported and otherwise ignored: forgetting a
/// theme is not worth interrupting anyone over.
pub fn save(r: Remembered) {
    let Some(path) = path() else {
        return;
    };
    let written = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(&path, r.to_text()));
    if let Err(e) = written {
        eprintln!("cannot remember the app's settings in {}: {e}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_written_reads_back() {
        for r in [
            Remembered { theme: UiTheme::Dark, fullscreen: true },
            Remembered { theme: UiTheme::CatppuccinMocha, fullscreen: false },
        ] {
            assert_eq!(Remembered::parse(&r.to_text()), r);
        }
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
