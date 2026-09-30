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
//! since Windows cannot delete a running executable but can rename it. The
//! user's `scripts` and `examples` folders are never among them: an archive
//! carries its examples in `res/examples`, and the new version, at its first
//! start, puts them in place of the old, keeping in `scripts/` whatever
//! example the user changed or added (`install_examples`); a
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

/// Which commit this build is, and when that commit was made, in UTC as
/// GitHub writes its dates. Set by the release workflow for the bundle's
/// executable, absent from any other build. It is what tells a beta from the
/// release of the same version, which both call themselves v<version>.
const BUILD_COMMIT: Option<&str> = option_env!("KALAST_COMMIT");
const BUILD_DATE: Option<&str> = option_env!("KALAST_COMMIT_DATE");

/// One release as GitHub lists it.
#[derive(Debug, Clone)]
pub struct Release {
    /// Without the `v`, and without a beta's `-beta`: what its archives are
    /// named for.
    pub version: String,
    /// As GitHub has it: `v0.5.10`, or `v0.5.10-beta`.
    pub tag: String,
    /// `YYYY-MM-DD`, or empty.
    pub date: String,
    /// `YYYY-MM-DDTHH:MM:SSZ`, or empty.
    pub published: String,
    /// The commit it was made from, when GitHub names one.
    pub commit: String,
    /// The release body: the changelog section.
    pub notes: String,
    /// `(file name, download url)`.
    pub assets: Vec<(String, String)>,
}

/// The release to offer, beside the version this is.
#[derive(Debug, Clone)]
pub struct Update {
    pub current: String,
    /// When this version was released, if GitHub still lists it.
    pub current_date: Option<String>,
    pub latest: Release,
    /// This build is a beta: it knows its commit, and its version has no
    /// release yet.
    pub beta: bool,
    /// When this build's commit was made, for a beta.
    pub built: Option<String>,
    /// `latest` is a newer build of this same version: a newer beta, or the
    /// release this beta's version became.
    pub rebuilt: bool,
}

impl Update {
    /// Whether `latest` is worth installing: a newer version, or a newer
    /// build of this one.
    pub fn newer(&self) -> bool {
        self.rebuilt || newer(&self.latest.version, &self.current)
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

/// The releases list as GitHub returns it, against `current` and this build.
pub fn parse(json: &str, current: &str) -> Result<Update, String> {
    parse_for(json, current, BUILD_COMMIT.zip(BUILD_DATE))
}

/// `parse`, for a build that is `(commit, commit date)` -- or, `None`, for
/// one that does not know, which is every build but a release bundle's.
///
/// A newer version is offered as it always was. Beyond that, a build that
/// knows its commit is offered a newer build of its own version, which is
/// how a beta is kept current: while the version has no release, the beta
/// on GitHub, `v<version>-beta`, when it was built from another commit and
/// published after this one's commit; once it has one, that release, when it
/// was made from another commit -- this one being an older beta of it.
pub fn parse_for(json: &str, current: &str, build: Option<(&str, &str)>) -> Result<Update, String> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let list = value.as_array().ok_or("not a list of releases")?;
    let release = |r: &serde_json::Value| -> Option<Release> {
        let tag = r["tag_name"].as_str()?;
        let published = r["published_at"].as_str().unwrap_or("").to_string();
        Some(Release {
            version: tag.trim_start_matches('v').trim_end_matches("-beta").to_string(),
            tag: tag.to_string(),
            date: published.chars().take(10).collect(),
            published,
            commit: r["target_commitish"].as_str().unwrap_or("").to_string(),
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
    };
    // Newest first, as GitHub lists them; `true` for a pre-release.
    let releases: Vec<(bool, Release)> = list
        .iter()
        .filter(|r| !r["draft"].as_bool().unwrap_or(false))
        .filter_map(|r| Some((r["prerelease"].as_bool().unwrap_or(false), release(r)?)))
        .collect();
    let stable = releases
        .iter()
        .find(|(pre, _)| !pre)
        .map(|(_, r)| r.clone())
        .ok_or("no published release")?;

    let current = current.trim_start_matches('v').to_string();
    let released = releases.iter().find(|(pre, r)| !pre && r.version == current).map(|(_, r)| r);
    let beta_tag = format!("v{current}-beta");
    let beta = releases.iter().find(|(pre, r)| *pre && r.tag == beta_tag).map(|(_, r)| r);

    let mut latest = stable.clone();
    let mut rebuilt = false;
    let mut is_beta = false;
    if let Some((commit, built)) = build {
        match released {
            // A release names its commit only since betas existed; before,
            // it named the branch, and nothing can be said.
            Some(r) if is_commit(&r.commit) && r.commit != commit => {
                latest = r.clone();
                rebuilt = true;
                is_beta = true;
            }
            Some(_) => {}
            None => {
                is_beta = true;
                if let Some(b) = beta.filter(|b| b.commit != commit && b.published.as_str() > built) {
                    latest = b.clone();
                    rebuilt = true;
                }
            }
        }
    }
    // A newer version wins over any build of this one.
    if newer(&stable.version, &current) {
        latest = stable;
        rebuilt = false;
    }

    Ok(Update {
        current_date: released.map(|r| r.date.clone()).filter(|d| !d.is_empty()),
        current,
        latest,
        beta: is_beta,
        built: build.filter(|_| is_beta).map(|(_, d)| d.chars().take(10).collect()),
        rebuilt,
    })
}

/// A full commit hash, which is what a release made since betas names.
fn is_commit(s: &str) -> bool {
    s.len() == 40 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// What the log says, one entry per line.
pub fn message(u: &Update) -> Vec<String> {
    let this = match (&u.built, &u.current_date) {
        (Some(d), _) => format!("kalast v{} beta (built {d})", u.current),
        (None, Some(d)) => format!("kalast v{} (released {d})", u.current),
        (None, None) if u.beta => format!("kalast v{} beta", u.current),
        (None, None) => format!("kalast v{}", u.current),
    };
    if !u.newer() {
        let what = if u.beta { "beta" } else { "release" };
        return vec![format!("{this} is the latest {what}.")];
    }
    let available = if u.latest.tag.ends_with("-beta") {
        format!("a newer beta of v{} (published {})", u.latest.version, u.latest.date)
    } else {
        format!("v{} available (released {})", u.latest.version, u.latest.date)
    };
    let mut lines = vec![format!(
        "{this} -> {available}. The toolbar's \"update\" button installs it."
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
    swap_in(&inner, dir, &previous)?;
    let _ = std::fs::remove_dir_all(&work);
    clean_previous(dir);
    log(format!("installed v{} in {}", u.latest.version, dir.display()));
    Ok(())
}

/// Every entry of `inner`, an archive unpacked, in place of its namesake in
/// `dir`, which goes to `previous` -- all but two. The user's `scripts`, and
/// the examples they may have changed: no archive is to replace either,
/// whatever it holds. The new examples come in `res/examples`, for
/// `install_examples` to put in place.
fn swap_in(inner: &Path, dir: &Path, previous: &Path) -> Result<(), String> {
    for entry in std::fs::read_dir(inner).map_err(|e| format!("{}: {e}", inner.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_name() == "scripts" || entry.file_name() == "examples" {
            continue;
        }
        let target = dir.join(entry.file_name());
        if target.exists() {
            std::fs::rename(&target, previous.join(entry.file_name()))
                .map_err(|e| format!("moving {} aside: {e}", target.display()))?;
        }
        std::fs::rename(entry.path(), &target)
            .map_err(|e| format!("installing {}: {e}", target.display()))?;
    }
    Ok(())
}

/// What the last update left aside, gone; a running executable stays
/// until the next start, which is why this is called then too.
pub fn clean_previous(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir.join(".previous"));
}

/// Every example file kalast has shipped, a line each: the fingerprint of its
/// text and its path under `examples/` (`tools/gen_examples_shipped.py`).
const SHIPPED_EXAMPLES: &str = include_str!("../../res/examples-shipped.txt");

/// FNV-1a, 64 bits, of `data` with each `\r\n` read as `\n` -- a checkout on
/// Windows writes the one and the tags hold the other: the fingerprint
/// `SHIPPED_EXAMPLES` lists, which the tool computes the same way.
pub fn example_fingerprint(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for (i, &b) in data.iter().enumerate() {
        if b == b'\r' && data.get(i + 1) == Some(&b'\n') {
            continue;
        }
        h = (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// `SHIPPED_EXAMPLES`, read: the fingerprints each file was shipped with,
/// and every folder it was shipped in.
struct Shipped {
    files: std::collections::HashMap<String, Vec<u64>>,
    folders: std::collections::HashSet<String>,
}

impl Shipped {
    fn new() -> Self {
        Self::parse(SHIPPED_EXAMPLES)
    }

    fn parse(text: &str) -> Self {
        let mut shipped = Self { files: Default::default(), folders: Default::default() };
        for line in text.lines().filter(|l| !l.is_empty() && !l.starts_with('#')) {
            let Some((hex, path)) = line.split_once(' ') else { continue };
            let Ok(value) = u64::from_str_radix(hex, 16) else { continue };
            shipped.files.entry(path.to_string()).or_default().push(value);
            shipped.folders.extend(path.match_indices('/').map(|(i, _)| path[..i].to_string()));
        }
        shipped
    }

    /// Whether `path`, `name` under `examples/`, is as some release shipped
    /// it: a file with a text some release gave that path, or a folder some
    /// release had whose every entry is. A link is the user's, never
    /// followed.
    fn has(&self, path: &Path, name: &str) -> bool {
        let Ok(meta) = std::fs::symlink_metadata(path) else {
            return false;
        };
        if meta.is_file() {
            return self.files.get(name).is_some_and(|known| {
                std::fs::read(path).is_ok_and(|data| known.contains(&example_fingerprint(&data)))
            });
        }
        if !meta.is_dir() || !self.folders.contains(name) {
            return false;
        }
        let Ok(entries) = std::fs::read_dir(path) else {
            return false;
        };
        entries.into_iter().all(|entry| {
            entry.is_ok_and(|entry| {
                let file = entry.file_name();
                is_litter(&file) || file.to_str().is_some_and(|f| self.has(&entry.path(), &format!("{name}/{f}")))
            })
        })
    }
}

/// What a folder holds that nobody wrote: the Finder's and Explorer's
/// records of it, and Python's bytecode.
fn is_litter(name: &std::ffi::OsStr) -> bool {
    matches!(name.to_str(), Some(".DS_Store" | "Thumbs.db" | "desktop.ini" | "__pycache__"))
}

/// What `install_examples` did.
#[derive(Debug, PartialEq)]
pub struct ExamplesInstalled {
    /// The folder the examples changed or added went to, and their names
    /// there, when there were any.
    pub kept: Option<(PathBuf, Vec<String>)>,
}

/// The examples this version ships, put in place: `res/examples`, where a
/// bundle carries them, becomes `examples`.
///
/// Never `examples` itself in the archive, because an update replaces every
/// entry its archive holds, and v0.5.10's deleted what it replaced -- the
/// examples a user had changed or added with the rest. So they arrive beside
/// the old, and here, at the first start of the version that brought them,
/// take their place: each example, a folder of `examples/` or a file there,
/// in which anything is not exactly as some release shipped it -- a file
/// edited, added, a link -- is moved whole into
/// `scripts/examples-before-v<version>/`, where it still runs beside
/// whatever it reads; the others, as shipped to the byte, go. A rename, both
/// ways, so every file keeps its time -- and the precompiled Rust examples,
/// checked against it, stay current.
///
/// `None` when there is nothing new to install. An error leaves
/// `res/examples` where it is, for the next start to try again; what was
/// moved by then is in `scripts/`, and the error says where.
pub fn install_examples(dir: &Path, version: &str) -> Result<Option<ExamplesInstalled>, String> {
    install_examples_against(dir, version, &Shipped::new())
}

/// `install_examples`, against a list of its own for the tests.
fn install_examples_against(dir: &Path, version: &str, shipped: &Shipped) -> Result<Option<ExamplesInstalled>, String> {
    let staged = dir.join("res").join("examples");
    if !staged.is_dir() {
        return Ok(None);
    }
    let show = |p: &Path| p.strip_prefix(dir).unwrap_or(p).display().to_string();
    let installed = dir.join("examples");
    let kept = keep_changed_examples(dir, &installed, version, shipped)?;
    let swapped = (|| {
        // What is left is kalast's own, as it shipped it: aside, and gone
        // once the new ones are in -- or back, if they could not be.
        let aside = dir.join(".previous").join("examples");
        let old = std::fs::symlink_metadata(&installed).is_ok();
        if old {
            let _ = std::fs::remove_dir_all(&aside);
            std::fs::create_dir_all(dir.join(".previous")).map_err(|e| format!(".previous: {e}"))?;
            std::fs::rename(&installed, &aside).map_err(|e| format!("moving {} aside: {e}", show(&installed)))?;
        }
        if let Err(e) = std::fs::rename(&staged, &installed) {
            if old {
                let _ = std::fs::rename(&aside, &installed);
            }
            return Err(format!("installing {}: {e}", show(&staged)));
        }
        let _ = std::fs::remove_dir_all(&aside);
        Ok(())
    })();
    match (swapped, &kept) {
        (Ok(()), _) => Ok(Some(ExamplesInstalled { kept })),
        (Err(e), None) => Err(e),
        (Err(e), Some((backup, _))) => Err(format!("{e}; the ones you had changed are in {} already", show(backup))),
    }
}

/// The examples of `installed` that are not as kalast shipped them, moved
/// into a new folder of `scripts/`: that folder and their names, if any.
/// Whole examples, by name; a link or a file where the folder was is the
/// user's, whole.
fn keep_changed_examples(
    dir: &Path,
    installed: &Path,
    version: &str,
    shipped: &Shipped,
) -> Result<Option<(PathBuf, Vec<String>)>, String> {
    let show = |p: &Path| p.strip_prefix(dir).unwrap_or(p).display().to_string();
    let Ok(meta) = std::fs::symlink_metadata(installed) else {
        return Ok(None);
    };
    let changed: Vec<std::ffi::OsString> = if meta.is_dir() {
        let mut changed = vec![];
        for entry in std::fs::read_dir(installed).map_err(|e| format!("{}: {e}", show(installed)))? {
            let entry = entry.map_err(|e| format!("{}: {e}", show(installed)))?;
            let name = entry.file_name();
            if !is_litter(&name) && !name.to_str().is_some_and(|n| shipped.has(&entry.path(), n)) {
                changed.push(name);
            }
        }
        changed.sort();
        changed
    } else {
        vec!["examples".into()]
    };
    if changed.is_empty() {
        return Ok(None);
    }
    let scripts = dir.join("scripts");
    let base = format!("examples-before-v{version}");
    let backup = (1..)
        .map(|n| scripts.join(if n == 1 { base.clone() } else { format!("{base}-{n}") }))
        .find(|p| std::fs::symlink_metadata(p).is_err())
        .expect("a free name");
    std::fs::create_dir_all(&backup).map_err(|e| format!("{}: {e}", show(&backup)))?;
    for name in &changed {
        let from = if meta.is_dir() { installed.join(name) } else { installed.to_path_buf() };
        std::fs::rename(&from, backup.join(name))
            .map_err(|e| format!("moving {} to {}: {e}", show(&from), show(&backup)))?;
    }
    Ok(Some((backup, changed.iter().map(|n| n.to_string_lossy().into_owned()).collect())))
}

/// What the kalast tab says of `install_examples`'s outcome, and whether it
/// is news -- anything kept, or a failure -- rather than a line to read.
pub fn examples_message(outcome: &Result<ExamplesInstalled, String>, dir: &Path, version: &str) -> (String, bool) {
    match outcome {
        Ok(ExamplesInstalled { kept: None }) => (format!("installed the examples of v{version}"), false),
        Ok(ExamplesInstalled { kept: Some((backup, names)) }) => {
            let backup = backup.strip_prefix(dir).unwrap_or(backup).display();
            let what = match names.len() {
                1 => "the example you had changed or added is".to_string(),
                n => format!("the {n} examples you had changed or added are"),
            };
            (format!("installed the examples of v{version}; {what} in {backup}: {}", names.join(", ")), true)
        }
        Err(e) => (format!("the examples of v{version} could not be installed, kalast tries again at its next start: {e}"), true),
    }
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

    const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    /// The beta of 0.5.10 built from B and published on the 26th; 0.5.9, made
    /// before betas, the latest release and naming its branch.
    const BETA: &str = r#"[
      {"tag_name":"v0.5.10-beta","published_at":"2026-09-26T10:00:00Z","draft":false,"prerelease":true,
       "target_commitish":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","body":"Pre-release of v0.5.10.",
       "assets":[{"name":"kalast-v0.5.10-macos-arm64.tar.gz","browser_download_url":"https://x/b.tar.gz"}]},
      {"tag_name":"v0.5.9","published_at":"2026-09-24T16:39:00Z","draft":false,"prerelease":false,
       "target_commitish":"main","body":"- older","assets":[]}
    ]"#;

    /// 0.5.10 released from B on the 28th -- what the beta became.
    const RELEASED: &str = r#"[
      {"tag_name":"v0.5.10","published_at":"2026-09-28T09:00:00Z","draft":false,"prerelease":false,
       "target_commitish":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","body":"- new","assets":[]},
      {"tag_name":"v0.5.9","published_at":"2026-09-24T16:39:00Z","draft":false,"prerelease":false,
       "target_commitish":"main","body":"- older","assets":[]}
    ]"#;

    #[test]
    fn a_beta_is_offered_the_newer_beta_of_its_version() {
        let u = parse_for(BETA, "0.5.10", Some((A, "2026-09-25T14:00:00Z"))).unwrap();
        assert!(u.beta && u.rebuilt && u.newer());
        assert_eq!(u.latest.tag, "v0.5.10-beta");
        assert_eq!(u.latest.version, "0.5.10", "its archives are named for the version");
        assert_eq!(
            message(&u)[0],
            "kalast v0.5.10 beta (built 2026-09-25) -> a newer beta of v0.5.10 (published 2026-09-26). The toolbar's \"update\" button installs it."
        );
    }

    #[test]
    fn the_beta_it_is_is_not_offered() {
        let u = parse_for(BETA, "0.5.10", Some((B, "2026-09-26T09:40:00Z"))).unwrap();
        assert!(u.beta && !u.newer());
        assert_eq!(message(&u), vec!["kalast v0.5.10 beta (built 2026-09-26) is the latest beta."]);
    }

    /// Another commit, but made after that beta was published: not newer.
    #[test]
    fn a_beta_published_before_this_build_is_not_offered() {
        let u = parse_for(BETA, "0.5.10", Some((A, "2026-09-27T08:00:00Z"))).unwrap();
        assert!(u.beta && !u.newer());
    }

    #[test]
    fn a_beta_is_offered_the_release_its_version_became() {
        let u = parse_for(RELEASED, "0.5.10", Some((A, "2026-09-25T14:00:00Z"))).unwrap();
        assert!(u.beta && u.rebuilt && u.newer());
        assert_eq!(u.latest.tag, "v0.5.10");
        assert_eq!(
            message(&u)[0],
            "kalast v0.5.10 beta (built 2026-09-25) -> v0.5.10 available (released 2026-09-28). The toolbar's \"update\" button installs it."
        );
    }

    /// The last beta's bundle is the release's: same commit, nothing to do.
    #[test]
    fn the_release_is_not_offered_to_the_beta_it_was_made_from() {
        let u = parse_for(RELEASED, "0.5.10", Some((B, "2026-09-26T09:40:00Z"))).unwrap();
        assert!(!u.beta && !u.newer());
        assert_eq!(message(&u), vec!["kalast v0.5.10 (released 2026-09-28) is the latest release."]);
    }

    #[test]
    fn a_newer_version_wins_over_a_newer_beta() {
        let list = BETA.replacen(
            "[",
            r#"[{"tag_name":"v0.5.11","published_at":"2026-10-01T09:00:00Z","draft":false,"prerelease":false,
                "target_commitish":"cccccccccccccccccccccccccccccccccccccccc","body":"- newer","assets":[]},"#,
            1,
        );
        let u = parse_for(&list, "0.5.10", Some((A, "2026-09-25T14:00:00Z"))).unwrap();
        assert!(u.newer() && !u.rebuilt);
        assert_eq!(u.latest.tag, "v0.5.11");
    }

    /// Every build but a release bundle's, and every other version: the
    /// betas are not there for them.
    #[test]
    fn betas_are_offered_to_no_one_else() {
        assert!(!parse_for(BETA, "0.5.10", None).unwrap().newer(), "a build that knows no commit");
        let old = parse_for(BETA, "0.5.9", Some((A, "2026-09-20T00:00:00Z"))).unwrap();
        assert!(!old.newer() && !old.beta, "0.5.9 is its release, named for a branch");
        assert_eq!(old.latest.tag, "v0.5.9");
    }

    #[test]
    fn the_asset_is_named_for_this_machine() {
        let n = asset_name("0.6.0");
        assert!(n.starts_with("kalast-v0.6.0-"), "{n}");
        assert!(n.ends_with(".tar.gz") || n.ends_with(".zip"), "{n}");
    }

    /// FNV-1a's published values, a line ending either way the same, and a
    /// carriage return on its own the file's -- what
    /// `tests/test_examples_shipped.py` checks the tool gives.
    #[test]
    fn the_fingerprint_is_the_tools() {
        let f = |d: &[u8]| format!("{:016x}", example_fingerprint(d));
        assert_eq!(f(b""), "cbf29ce484222325");
        assert_eq!(f(b"a"), "af63dc4c8601ec8c");
        assert_eq!(f(b"foobar"), "85944171f73967e8");
        assert_eq!(f(b"x = 1\r\ny = 2\r\n"), "5e4216b9c8c23ae7");
        assert_eq!(f(b"x = 1\ny = 2\n"), "5e4216b9c8c23ae7");
        assert_eq!(f(b"\r\r\n"), "083cb407b4f40f36");
    }

    /// The list compiled in is the one the tool writes, and knows the
    /// examples by the paths they have under `examples/`.
    #[test]
    fn the_shipped_examples_are_compiled_in() {
        let shipped = Shipped::new();
        assert!(shipped.files.len() > 100, "{} files", shipped.files.len());
        assert!(shipped.files.contains_key("README.md"));
        assert!(shipped.files.contains_key("cube/color_map.py"));
        assert!(shipped.folders.contains("cube") && shipped.folders.contains("hera_mars_swingby/cosmographia"));
        assert!(!shipped.folders.contains("README.md"));
    }

    /// A bundle folder of its own, gone when dropped.
    struct Bundle(PathBuf);

    impl Bundle {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("kalast-examples-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn write(&self, path: &str, text: &str) {
            let path = self.0.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }

        fn read(&self, path: &str) -> Option<String> {
            std::fs::read_to_string(self.0.join(path)).ok()
        }

        fn exists(&self, path: &str) -> bool {
            std::fs::symlink_metadata(self.0.join(path)).is_ok()
        }
    }

    impl Drop for Bundle {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// What some release shipped, for these tests: two versions of
    /// `cube/main.py`, and the rest once.
    fn shipped() -> Shipped {
        let line = |text: &str, path: &str| format!("{:016x} {path}\n", example_fingerprint(text.as_bytes()));
        Shipped::parse(
            &[
                line("# the README\n", "README.md"),
                line("cube = 1\n", "cube/main.py"),
                line("cube = 2\n", "cube/main.py"),
                line("v 0 0 0\n", "cube/cube.obj"),
                line("sphere = 1\n", "sphere/main.py"),
                line("orbit = 1\n", "mars/swingby/main.py"),
            ]
            .concat(),
        )
    }

    /// A first start, from an archive just unpacked: the examples are put
    /// in place, and a second start finds nothing more to do.
    #[test]
    fn a_new_bundles_examples_are_put_in_place() {
        let b = Bundle::new("fresh");
        b.write("res/examples/cube/main.py", "cube = 3\n");
        let done = install_examples_against(&b.0, "9.9.9", &shipped()).unwrap();
        assert_eq!(done, Some(ExamplesInstalled { kept: None }));
        assert_eq!(b.read("examples/cube/main.py").as_deref(), Some("cube = 3\n"));
        assert!(!b.exists("res/examples") && !b.exists("scripts"));
        assert_eq!(install_examples_against(&b.0, "9.9.9", &shipped()).unwrap(), None);
    }

    /// An update: every example exactly as some release shipped it -- in
    /// Windows' line endings too, and with the litter a folder collects --
    /// goes; one edited, one with a file added, one of the user's own and
    /// an edited README are kept whole in `scripts/`, and the new examples
    /// take the place of the old.
    #[test]
    fn the_examples_a_user_changed_are_kept_in_scripts() {
        let b = Bundle::new("update");
        b.write("examples/README.md", "# the README\r\n");
        b.write("examples/cube/main.py", "cube = 1\r\n");
        b.write("examples/cube/cube.obj", "v 0 0 0\n");
        b.write("examples/cube/__pycache__/main.cpython-314.pyc", "bytes");
        b.write("examples/.DS_Store", "finder");
        b.write("examples/sphere/main.py", "sphere = 1, edited\n");
        b.write("examples/mars/swingby/main.py", "orbit = 1\n");
        b.write("examples/mars/swingby/out.csv", "t, x\n");
        b.write("examples/mine/main.py", "mine = 1\n");
        b.write("scripts/test/main.py", "the user's\n");
        b.write("res/examples/README.md", "# the new README\n");
        b.write("res/examples/cube/main.py", "cube = 3\n");
        b.write("res/examples/sphere/main.py", "sphere = 2\n");

        let done = install_examples_against(&b.0, "9.9.9", &shipped()).unwrap().unwrap();
        let (backup, names) = done.kept.unwrap();
        assert_eq!(backup, b.0.join("scripts/examples-before-v9.9.9"));
        assert_eq!(names, ["mars", "mine", "sphere"]);
        assert_eq!(b.read("scripts/examples-before-v9.9.9/sphere/main.py").as_deref(), Some("sphere = 1, edited\n"));
        assert_eq!(b.read("scripts/examples-before-v9.9.9/mars/swingby/out.csv").as_deref(), Some("t, x\n"));
        assert_eq!(b.read("scripts/examples-before-v9.9.9/mars/swingby/main.py").as_deref(), Some("orbit = 1\n"));
        assert_eq!(b.read("scripts/examples-before-v9.9.9/mine/main.py").as_deref(), Some("mine = 1\n"));
        assert!(!b.exists("scripts/examples-before-v9.9.9/cube") && !b.exists("scripts/examples-before-v9.9.9/README.md"));
        assert_eq!(b.read("scripts/test/main.py").as_deref(), Some("the user's\n"));

        assert_eq!(b.read("examples/README.md").as_deref(), Some("# the new README\n"));
        assert_eq!(b.read("examples/cube/main.py").as_deref(), Some("cube = 3\n"));
        assert_eq!(b.read("examples/sphere/main.py").as_deref(), Some("sphere = 2\n"));
        assert!(!b.exists("examples/cube/cube.obj") && !b.exists("examples/mine") && !b.exists("examples/.DS_Store"));
        assert!(!b.exists("res/examples") && !b.exists(".previous/examples"));

        let (line, news) = examples_message(&Ok(ExamplesInstalled { kept: Some((backup, names)) }), &b.0, "9.9.9");
        assert!(news);
        assert_eq!(
            line,
            format!(
                "installed the examples of v9.9.9; the 3 examples you had changed or added are in {}: mars, mine, sphere",
                std::path::Path::new("scripts").join("examples-before-v9.9.9").display()
            )
        );
    }

    /// Every example as shipped: nothing is kept, and nothing said but that
    /// the examples are in.
    #[test]
    fn examples_as_shipped_are_replaced_quietly() {
        let b = Bundle::new("quiet");
        b.write("examples/cube/main.py", "cube = 2\n");
        b.write("examples/README.md", "# the README\n");
        b.write("res/examples/cube/main.py", "cube = 3\n");
        let done = install_examples_against(&b.0, "9.9.9", &shipped()).unwrap();
        assert_eq!(done, Some(ExamplesInstalled { kept: None }));
        assert!(!b.exists("scripts"));
        assert_eq!(b.read("examples/cube/main.py").as_deref(), Some("cube = 3\n"));
        assert_eq!(examples_message(&Ok(done.unwrap()), &b.0, "9.9.9"), ("installed the examples of v9.9.9".to_string(), false));
    }

    /// A second update of one version -- a beta, then its release -- keeps
    /// the first's folder and makes another beside it.
    #[test]
    fn a_folder_kept_before_is_not_written_into() {
        let b = Bundle::new("again");
        b.write("scripts/examples-before-v9.9.9/mine/main.py", "first\n");
        b.write("examples/mine/main.py", "second\n");
        b.write("res/examples/cube/main.py", "cube = 3\n");
        let done = install_examples_against(&b.0, "9.9.9", &shipped()).unwrap().unwrap();
        assert_eq!(done.kept.unwrap().0, b.0.join("scripts/examples-before-v9.9.9-2"));
        assert_eq!(b.read("scripts/examples-before-v9.9.9/mine/main.py").as_deref(), Some("first\n"));
        assert_eq!(b.read("scripts/examples-before-v9.9.9-2/mine/main.py").as_deref(), Some("second\n"));
    }

    /// The new examples keep their files' times through the move: the
    /// precompiled Rust examples are current only while their libraries are
    /// newer.
    #[test]
    fn the_examples_keep_their_times() {
        let b = Bundle::new("times");
        b.write("examples/cube/main.py", "cube = 1\n");
        b.write("res/examples/cube/main.rs", "fn main() {}\n");
        let then = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        std::fs::File::options()
            .write(true)
            .open(b.0.join("res/examples/cube/main.rs"))
            .unwrap()
            .set_modified(then)
            .unwrap();
        install_examples_against(&b.0, "9.9.9", &shipped()).unwrap();
        let modified = std::fs::metadata(b.0.join("examples/cube/main.rs")).unwrap().modified().unwrap();
        assert_eq!(modified, then);
    }

    /// A link is the user's, whole, and never followed: whatever it points
    /// at is left as it is, and the link itself is what moves.
    #[cfg(unix)]
    #[test]
    fn a_link_is_kept_and_not_followed() {
        let b = Bundle::new("links");
        let elsewhere = Bundle::new("elsewhere");
        elsewhere.write("theirs/main.py", "cube = 1\n");
        std::fs::create_dir_all(b.0.join("examples")).unwrap();
        std::os::unix::fs::symlink(elsewhere.0.join("theirs"), b.0.join("examples/cube")).unwrap();
        b.write("res/examples/cube/main.py", "cube = 3\n");
        let done = install_examples_against(&b.0, "9.9.9", &shipped()).unwrap().unwrap();
        assert_eq!(done.kept.unwrap().1, ["cube"]);
        let kept = b.0.join("scripts/examples-before-v9.9.9/cube");
        assert!(std::fs::symlink_metadata(&kept).unwrap().file_type().is_symlink());
        assert_eq!(elsewhere.read("theirs/main.py").as_deref(), Some("cube = 1\n"));
        assert_eq!(b.read("examples/cube/main.py").as_deref(), Some("cube = 3\n"));

        // `examples` itself a link, to a checkout's, say.
        let c = Bundle::new("linked");
        std::os::unix::fs::symlink(&elsewhere.0, c.0.join("examples")).unwrap();
        c.write("res/examples/cube/main.py", "cube = 3\n");
        let done = install_examples_against(&c.0, "9.9.9", &shipped()).unwrap().unwrap();
        assert_eq!(done.kept.unwrap().1, ["examples"]);
        assert!(std::fs::symlink_metadata(c.0.join("scripts/examples-before-v9.9.9/examples")).unwrap().file_type().is_symlink());
        assert_eq!(elsewhere.read("theirs/main.py").as_deref(), Some("cube = 1\n"));
        assert_eq!(c.read("examples/cube/main.py").as_deref(), Some("cube = 3\n"));
    }

    /// The swap replaces what the archive holds and leaves the user's own:
    /// an archive holding `examples` or `scripts` all the same is not let
    /// near theirs.
    #[test]
    fn an_update_never_replaces_the_examples_or_the_scripts() {
        let b = Bundle::new("swap");
        b.write("kalast", "old");
        b.write("res/sph1.obj", "old");
        b.write("examples/mine/main.py", "mine\n");
        b.write("scripts/test/main.py", "the user's\n");
        b.write(".update/kalast-v9.9.9/kalast", "new");
        b.write(".update/kalast-v9.9.9/res/examples/cube/main.py", "cube = 3\n");
        b.write(".update/kalast-v9.9.9/examples/cube/main.py", "cube = 3\n");
        b.write(".update/kalast-v9.9.9/scripts/test/main.py", "shipped\n");
        std::fs::create_dir_all(b.0.join(".previous")).unwrap();
        swap_in(&b.0.join(".update/kalast-v9.9.9"), &b.0, &b.0.join(".previous")).unwrap();
        assert_eq!(b.read("kalast").as_deref(), Some("new"));
        assert_eq!(b.read(".previous/kalast").as_deref(), Some("old"));
        assert!(!b.exists("res/sph1.obj") && b.exists("res/examples/cube/main.py"));
        assert_eq!(b.read("examples/mine/main.py").as_deref(), Some("mine\n"));
        assert!(!b.exists("examples/cube"));
        assert_eq!(b.read("scripts/test/main.py").as_deref(), Some("the user's\n"));
    }
}
