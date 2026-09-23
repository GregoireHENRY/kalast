//! A newer release: found, offered, installed.
//!
//! **When.** Only when the UI app opens -- `App::editor_start` -- never when
//! a script runs its own window, and `app.config.check_updates = False`
//! skips it altogether. **Where.** Never on the loop's thread: `check` is a
//! blocking request to GitHub, so `editor_start` spawns it and the frame
//! reads the answer off a channel (`try_recv`, nanoseconds) until it is
//! there; installing runs on a thread of its own the same way, its progress
//! lines arriving through the same channel.
//!
//! **How**, by how this copy was installed (`Kind`): a release bundle --
//! the executable with `python/` beside it -- downloads the release's
//! archive for this platform into the bundle folder, unpacks it with `tar`
//! (`.zip` included: Windows 10's tar is bsdtar) and swaps every entry in
//! place, keeping what it replaced in `.previous` until the next start,
//! since Windows cannot delete a running executable but can rename it; a
//! pip install runs `pip install --upgrade` with the interpreter this is
//! running in; a source checkout is told to pull. Nothing restarts behind
//! the user's back: the toolbar's button becomes "restart", and that
//! relaunches this very command line.
//!
//! `kalast --update` (and `python -m kalast --update`) does check and
//! install from a terminal, printing what the log panel would show.
//! `KALAST_UPDATE_PRETEND=0.5.4` makes this copy claim that version, which
//! is how the path is tried without waiting for a release.

use std::path::{Path, PathBuf};

const REPO: &str = "GregoireHENRY/kalast";
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

/// One release as GitHub lists it.
#[derive(Debug, Clone)]
pub struct Release {
    /// Without the `v`.
    pub version: String,
    /// `YYYY-MM-DD`, or empty.
    pub date: String,
    /// The release body: the changelog section.
    pub notes: String,
    /// `(file name, download url)`.
    pub assets: Vec<(String, String)>,
}

/// The latest release, beside the version this is.
#[derive(Debug, Clone)]
pub struct Update {
    pub current: String,
    /// When this version was released, if GitHub still lists it.
    pub current_date: Option<String>,
    pub latest: Release,
}

impl Update {
    pub fn newer(&self) -> bool {
        newer(&self.latest.version, &self.current)
    }
}

/// Where the UI app is with it. `Failed` after an install that did not
/// take; a check that could not reach GitHub stays `Unchecked`, quietly.
#[derive(Debug, Clone, Default)]
pub enum State {
    #[default]
    Unchecked,
    Checking,
    UpToDate,
    Available(Update),
    Installing,
    /// Installed; the toolbar offers a restart.
    Ready,
    Failed(String),
}

/// What the threads send back.
#[derive(Debug)]
pub enum Msg {
    Checked(Result<Update, String>),
    Line(String),
    Installed(Result<(), String>),
}

/// How this copy was installed, which is how it updates.
#[derive(Debug, Clone)]
pub enum Kind {
    /// A release bundle: this folder, the executable with `python/` in it.
    Bundle(PathBuf),
    /// The extension module in a Python environment: `pip install --upgrade`.
    Pip,
    /// A checkout: nothing to install, only to pull.
    Source,
}

pub fn kind() -> Kind {
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        if crate::app::bundled_python_beside(&dir).is_some() {
            return Kind::Bundle(dir);
        }
    }
    if cfg!(feature = "ext") {
        Kind::Pip
    } else {
        Kind::Source
    }
}

/// This copy's version -- or the one `KALAST_UPDATE_PRETEND` says.
pub fn current() -> String {
    std::env::var("KALAST_UPDATE_PRETEND").unwrap_or_else(|_| CURRENT.to_string())
}

fn parse_version(v: &str) -> (u64, u64, u64) {
    let mut it = v
        .trim()
        .trim_start_matches('v')
        .split('.')
        .map(|p| p.parse::<u64>().unwrap_or(0));
    (
        it.next().unwrap_or(0),
        it.next().unwrap_or(0),
        it.next().unwrap_or(0),
    )
}

pub fn newer(candidate: &str, current: &str) -> bool {
    parse_version(candidate) > parse_version(current)
}

/// The archive a release carries for this machine.
pub fn asset_name(version: &str) -> String {
    let (os, ext) = match std::env::consts::OS {
        "macos" => ("macos", "tar.gz"),
        "windows" => ("windows", "zip"),
        _ => ("linux", "tar.gz"),
    };
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        other => other,
    };
    format!("kalast-v{version}-{os}-{arch}.{ext}")
}

fn agent() -> String {
    format!("kalast/{CURRENT}")
}

/// Ask GitHub. Blocking; run it on a thread.
pub fn check(current: &str) -> Result<Update, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases?per_page=20");
    let mut response = ureq::get(&url)
        .header("User-Agent", &agent())
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| e.to_string())?;
    let body = response.body_mut().read_to_string().map_err(|e| e.to_string())?;
    parse(&body, current)
}

/// The releases list as GitHub returns it, against `current`.
pub fn parse(json: &str, current: &str) -> Result<Update, String> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let list = value.as_array().ok_or("not a list of releases")?;
    let date = |r: &serde_json::Value| -> String {
        r["published_at"].as_str().unwrap_or("").chars().take(10).collect()
    };
    let latest = list
        .iter()
        .filter(|r| !r["draft"].as_bool().unwrap_or(false) && !r["prerelease"].as_bool().unwrap_or(false))
        .find_map(|r| {
            let tag = r["tag_name"].as_str()?;
            Some(Release {
                version: tag.trim_start_matches('v').to_string(),
                date: date(r),
                notes: r["body"].as_str().unwrap_or("").trim().to_string(),
                assets: r["assets"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| {
                                Some((
                                    x["name"].as_str()?.to_string(),
                                    x["browser_download_url"].as_str()?.to_string(),
                                ))
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            })
        })
        .ok_or("no published release")?;
    let tag = format!("v{}", current.trim_start_matches('v'));
    let current_date = list
        .iter()
        .find(|r| r["tag_name"].as_str() == Some(tag.as_str()))
        .map(date)
        .filter(|d| !d.is_empty());
    Ok(Update {
        current: current.trim_start_matches('v').to_string(),
        current_date,
        latest,
    })
}

/// What the log says, one entry per line.
pub fn message(u: &Update) -> Vec<String> {
    let this = match &u.current_date {
        Some(d) => format!("kalast v{} (released {d})", u.current),
        None => format!("kalast v{}", u.current),
    };
    if !u.newer() {
        return vec![format!("{this} is the latest release.")];
    }
    let mut lines = vec![format!(
        "{this} -> v{} available (released {}). The toolbar's \"update\" button installs it.",
        u.latest.version, u.latest.date
    )];
    lines.extend(u.latest.notes.lines().map(|l| format!("  {l}")));
    lines
}

/// Install `u.latest` the way this copy was installed. Blocking; run it on
/// a thread. `log` gets the progress lines.
pub fn install(u: &Update, kind: &Kind, log: &dyn Fn(String)) -> Result<(), String> {
    match kind {
        Kind::Bundle(dir) => install_bundle(u, dir, log),
        Kind::Pip => install_pip(u, log),
        Kind::Source => Err("this kalast is built from source: git pull and build it".to_string()),
    }
}

fn install_bundle(u: &Update, dir: &Path, log: &dyn Fn(String)) -> Result<(), String> {
    let name = asset_name(&u.latest.version);
    let (_, url) = u
        .latest
        .assets
        .iter()
        .find(|(n, _)| *n == name)
        .ok_or_else(|| format!("release v{} carries no {name}", u.latest.version))?;

    // Inside the bundle folder, so the swap below is a rename on one
    // filesystem and not a copy across two.
    let work = dir.join(".update");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| format!("{}: {e}", work.display()))?;
    let archive = work.join(&name);

    log(format!("downloading {name}"));
    let mut response = ureq::get(url)
        .header("User-Agent", &agent())
        .call()
        .map_err(|e| e.to_string())?;
    let mut file = std::fs::File::create(&archive).map_err(|e| format!("{}: {e}", archive.display()))?;
    let bytes = std::io::copy(&mut response.body_mut().as_reader(), &mut file)
        .map_err(|e| format!("download: {e}"))?;
    drop(file);
    log(format!("downloaded {name}, {} MB", bytes / 1_000_000));

    log("unpacking".to_string());
    let status = std::process::Command::new("tar")
        .arg("-xf")
        .arg(&archive)
        .arg("-C")
        .arg(&work)
        .status()
        .map_err(|e| format!("tar: {e}"))?;
    if !status.success() {
        return Err(format!("tar exited with {status}"));
    }
    let inner = work.join(name.trim_end_matches(".tar.gz").trim_end_matches(".zip"));
    if !inner.is_dir() {
        return Err(format!("{name} holds no {} folder", inner.display()));
    }

    // Every entry of the new bundle replaces its namesake here. What it
    // replaces goes to `.previous` first: on Windows the running executable
    // cannot be deleted, but it can be renamed, and `clean_previous` takes
    // the folder away at the next start.
    let previous = dir.join(".previous");
    let _ = std::fs::remove_dir_all(&previous);
    std::fs::create_dir_all(&previous).map_err(|e| format!("{}: {e}", previous.display()))?;
    for entry in std::fs::read_dir(&inner).map_err(|e| format!("{}: {e}", inner.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let target = dir.join(entry.file_name());
        if target.exists() {
            std::fs::rename(&target, previous.join(entry.file_name()))
                .map_err(|e| format!("moving {} aside: {e}", target.display()))?;
        }
        std::fs::rename(entry.path(), &target)
            .map_err(|e| format!("installing {}: {e}", target.display()))?;
    }
    let _ = std::fs::remove_dir_all(&work);
    clean_previous(dir);
    log(format!("installed v{} in {}", u.latest.version, dir.display()));
    Ok(())
}

/// What the last update left aside, gone; a running executable stays
/// until the next start, which is why this is called then too.
pub fn clean_previous(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir.join(".previous"));
}

fn install_pip(u: &Update, log: &dyn Fn(String)) -> Result<(), String> {
    let python = std::env::current_exe().map_err(|e| e.to_string())?;
    let spec = format!("kalast=={}", u.latest.version);
    log(format!("{} -m pip install --upgrade {spec}", python.display()));
    let out = std::process::Command::new(&python)
        .args(["-m", "pip", "install", "--upgrade", &spec])
        .output()
        .map_err(|e| format!("pip: {e}"))?;
    for line in String::from_utf8_lossy(&out.stdout)
        .lines()
        .chain(String::from_utf8_lossy(&out.stderr).lines())
    {
        log(format!("  {line}"));
    }
    if !out.status.success() {
        return Err(format!("pip exited with {}", out.status));
    }
    Ok(())
}

/// Start this command line again -- the executable at this path is the new
/// one -- and leave the exit to the caller.
pub fn relaunch() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    std::process::Command::new(exe)
        .args(args)
        .spawn()
        .map_err(|e| format!("relaunch: {e}"))?;
    Ok(())
}

/// `--update` from a terminal: check, say, install. The exit code.
pub fn run_now() -> i32 {
    let current = current();
    let update = match check(&current) {
        Ok(u) => u,
        Err(e) => {
            eprintln!("update check failed: {e}");
            return 1;
        }
    };
    for line in message(&update) {
        println!("{line}");
    }
    if !update.newer() {
        return 0;
    }
    let kind = kind();
    match install(&update, &kind, &|line| println!("{line}")) {
        Ok(()) => {
            println!("done: start kalast again to run v{}", update.latest.version);
            0
        }
        Err(e) => {
            eprintln!("update failed: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &str = r#"[
      {"tag_name":"v0.6.0","published_at":"2026-10-01T09:00:00Z","draft":false,"prerelease":false,
       "body":"- faster\n- fixed","assets":[{"name":"kalast-v0.6.0-macos-arm64.tar.gz","browser_download_url":"https://x/a.tar.gz"}]},
      {"tag_name":"v0.5.5","published_at":"2026-09-23T13:12:00Z","draft":false,"prerelease":false,"body":"- old","assets":[]}
    ]"#;

    #[test]
    fn versions_compare_numerically() {
        assert!(newer("0.10.0", "0.9.9"));
        assert!(newer("v1.0.0", "0.99.99"));
        assert!(!newer("0.5.5", "0.5.5"));
        assert!(!newer("0.5.4", "0.5.5"));
    }

    #[test]
    fn the_list_gives_the_latest_and_this_versions_date() {
        let u = parse(LIST, "0.5.5").unwrap();
        assert_eq!(u.latest.version, "0.6.0");
        assert_eq!(u.latest.date, "2026-10-01");
        assert_eq!(u.current_date.as_deref(), Some("2026-09-23"));
        assert!(u.newer());
        assert_eq!(u.latest.assets[0].0, "kalast-v0.6.0-macos-arm64.tar.gz");
        let lines = message(&u);
        assert_eq!(
            lines[0],
            "kalast v0.5.5 (released 2026-09-23) -> v0.6.0 available (released 2026-10-01). The toolbar's \"update\" button installs it."
        );
        assert_eq!(lines[1], "  - faster");
        assert_eq!(lines[2], "  - fixed");
    }

    #[test]
    fn up_to_date_is_one_line() {
        let u = parse(LIST, "0.6.0").unwrap();
        assert!(!u.newer());
        assert_eq!(message(&u), vec!["kalast v0.6.0 (released 2026-10-01) is the latest release."]);
    }

    #[test]
    fn a_version_github_no_longer_lists_has_no_date() {
        let u = parse(LIST, "0.1.0").unwrap();
        assert_eq!(u.current_date, None);
        assert_eq!(message(&u)[0].split(" ->").next().unwrap(), "kalast v0.1.0");
    }

    #[test]
    fn the_asset_is_named_for_this_machine() {
        let n = asset_name("0.6.0");
        assert!(n.starts_with("kalast-v0.6.0-"), "{n}");
        assert!(n.ends_with(".tar.gz") || n.ends_with(".zip"), "{n}");
    }
}
