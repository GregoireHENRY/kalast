//! What the UI app remembers between sessions: `app.config.theme`,
//! `app.config.fullscreen`, and the script editor's settings -- Neovim, the
//! ruler, the language servers.
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remembered {
    pub theme: UiTheme,
    pub fullscreen: bool,
    pub neovim: bool,
    pub neovim_path: String,
    pub ruler: u32,
    pub language_servers: bool,
    pub python_language_server: String,
    pub rust_language_server: String,
}

impl Remembered {
    pub fn of(config: &AppConfig) -> Self {
        Self {
            theme: config.theme,
            fullscreen: config.fullscreen,
            neovim: config.neovim,
            neovim_path: config.neovim_path.clone(),
            ruler: config.ruler,
            language_servers: config.language_servers,
            python_language_server: config.python_language_server.clone(),
            rust_language_server: config.rust_language_server.clone(),
        }
    }

    pub fn apply(&self, config: &mut AppConfig) {
        config.theme = self.theme;
        config.fullscreen = self.fullscreen;
        config.neovim = self.neovim;
        config.neovim_path = self.neovim_path.clone();
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
                "neovim" => flag(&mut r.neovim),
                "language_servers" => flag(&mut r.language_servers),
                "ruler" => {
                    if let Ok(n) = value.parse() {
                        r.ruler = n;
                    }
                }
                "neovim_path" => r.neovim_path = unquote(value),
                "python_language_server" => r.python_language_server = unquote(value),
                "rust_language_server" => r.rust_language_server = unquote(value),
                _ => {}
            }
        }
        r
    }

    fn to_text(&self) -> String {
        format!(
            "# What the kalast UI app remembers; written by the app when these change in it.\n\
             theme = \"{}\"\n\
             fullscreen = {}\n\
             neovim = {}\n\
             neovim_path = {}\n\
             ruler = {}\n\
             language_servers = {}\n\
             python_language_server = {}\n\
             rust_language_server = {}\n",
            self.theme.name(),
            self.fullscreen,
            self.neovim,
            quote(&self.neovim_path),
            self.ruler,
            self.language_servers,
            quote(&self.python_language_server),
            quote(&self.rust_language_server),
        )
    }
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
    let text = std::fs::read_to_string(path()?).ok()?;
    Some(Remembered::parse(&text))
}

/// Remember `r`. A failure is reported and otherwise ignored: forgetting a
/// theme is not worth interrupting anyone over.
pub fn save(r: &Remembered) {
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
        let default = Remembered::of(&AppConfig::default());
        for r in [
            Remembered { theme: UiTheme::Dark, fullscreen: true, ..default.clone() },
            Remembered {
                theme: UiTheme::CatppuccinMocha,
                fullscreen: false,
                neovim: true,
                neovim_path: r"C:\Program Files\Neovim\bin\nvim.exe".to_string(),
                ruler: 100,
                language_servers: false,
                python_language_server: "pyright-langserver --stdio".to_string(),
                rust_language_server: r#""C:\tools\rust analyzer.exe""#.to_string(),
            },
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
