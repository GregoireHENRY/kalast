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
}
