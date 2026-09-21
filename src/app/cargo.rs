//! Building and launching a Rust example from the editor.
//!
//! A `.py` script runs *inside* the editor: the same process, the same
//! window, the scene appearing in the viewport. A `.rs` example cannot --
//! it is a separate program that links kalast as a library, so it has to be
//! compiled and then launched, and it opens a window of its own.
//!
//! So the editor is a different kind of thing for Rust: an editor, a build
//! button and a launcher, rather than a host. What it does keep is the log
//! -- cargo and the example both inherit the redirected stdout, so their
//! output lands in the Log panel beside everything else.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Where the generated wrapper crate lives.
///
/// Under `target/`, so it is ignored, disposable, and shares the build
/// directory with everything else -- which is what keeps a hosted build to
/// seconds instead of recompiling kalast from scratch.
pub fn wrapper_dir() -> std::path::PathBuf {
    std::path::Path::new("target").join("kalast-hosted")
}

/// The wrapper's own build directory, one per feature set.
///
/// **Not** the repo's. Sharing it was tried, to avoid compiling kalast twice,
/// and it broke `cargo run --bin kalast`: the wrapper builds kalast *with*
/// the `python` feature, those artifacts landed beside the plain ones, and
/// the next plain build linked against them -- failing on `library
/// 'python3.14' not found`, in a binary whose whole point is not needing it.
///
/// One directory per feature set rather than one overall, so switching
/// between `python -m kalast` and `cargo run --bin kalast` does not recompile
/// from scratch each way.
pub fn build_dir() -> std::path::PathBuf {
    let features = if cfg!(feature = "python") {
        "python"
    } else {
        "plain"
    };
    wrapper_dir().join(features)
}

/// Where its cdylib lands.
/// Where the library for one example lands.
pub fn dylib_path_for(name: &str, release: bool) -> std::path::PathBuf {
    let profile = if release { "release" } else { "debug" };
    build_dir().join(profile).join(format!(
        "{}{name}{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    ))
}

/// A cargo package name for an example, from its path.
///
/// The directory as well as the file, because half the examples here are
/// called `main.rs` and a library called `libmain.dylib` says nothing about
/// which one it holds -- and two of them sharing a name would each look
/// current to the other's staleness check.
pub fn package_name(example: &std::path::Path) -> String {
    let mut parts: Vec<String> = example
        .components()
        .rev()
        .take(2)
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    parts.reverse();
    if let Some(last) = parts.last_mut() {
        *last = last.trim_end_matches(".rs").to_string();
    }
    let name: String = parts
        .join("_")
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    name.trim_matches('_').to_string()
}



/// How the generated wrapper reaches kalast: the source tree, or the release.
///
/// From a clone, kalast is a **path** dependency on the working directory --
/// which is what makes the compile button a second-long rebuild rather than
/// a download, and what lets an edit to the engine show up in a hosted
/// example immediately.
///
/// A release bundle has no source tree, and this used to be the end of it:
/// cargo said "failed to read <bundle>/Cargo.toml", naming a file nobody had
/// been told to expect. It is compiled against the crates.io release of the
/// version running instead, so a `.rs` example works from a download.
///
/// `=0.5.0` rather than `0.5`, because `abi_fingerprint` hashes
/// `CARGO_PKG_VERSION`: a guest resolved one patch ahead of the host would
/// compile and then be refused at load.
///
/// **`default-features = false` in both cases.** `python` is a default
/// feature, so leaving the default on gave a guest the interpreter a
/// `--no-default-features` host does not have -- a different `Shared`
/// layout, and a load refused with "built against a different kalast". It
/// never showed up in the repository, where the host has `python` too, and
/// would have broken every hosted build from a bundle.
fn kalast_dependency(root: &std::path::Path) -> String {
    let features = if cfg!(feature = "python") {
        ", features = [\"python\"]"
    } else {
        ""
    };
    if root.join("Cargo.toml").is_file() && root.join("src").is_dir() {
        let path = root.to_string_lossy().replace('\\', "/");
        format!("kalast = {{ path = \"{path}\", default-features = false{features} }}")
    } else {
        let version = env!("CARGO_PKG_VERSION");
        format!("kalast = {{ version = \"={version}\", default-features = false{features} }}")
    }
}

/// Write a crate that wraps `example` and exports what the host calls.
///
/// The example is included as a module, so its own `fn main` becomes
/// `example::main` and nothing has to be written in the file itself. It is a
/// generated crate rather than a target in this manifest because a declared
/// target whose file is missing breaks every cargo command in the repo,
/// including for someone who never opens the editor.
pub fn write_wrapper(example: &std::path::Path) -> Result<(), String> {
    let dir = wrapper_dir();
    let src = dir.join("src");
    std::fs::create_dir_all(&src).map_err(|e| format!("cannot create {}: {e}", src.display()))?;

    let root = std::env::current_dir()
        .map_err(|e| format!("cannot read the working directory: {e}"))?;

    let dependency = kalast_dependency(&root);
    let name = package_name(example);
    let example_path = root.join(example);
    let example = example_path.to_string_lossy().replace('\\', "/");

    std::fs::write(
        dir.join("Cargo.toml"),
        format!(
            "# Generated by kalast's editor. Rewritten for each example hosted;\n\
             # nothing here is meant to be edited or committed.\n\
             [package]\n\
             name = \"{name}\"\n\
             version = \"0.0.0\"\n\
             edition = \"2024\"\n\n\
             [lib]\n\
             crate-type = [\"cdylib\"]\n\n\
             [dependencies]\n\
             {dependency}\n\n\
             [workspace]\n"
        ),
    )
    .map_err(|e| format!("cannot write the wrapper manifest: {e}"))?;

    // The example is *copied* into the wrapper rather than `include!`d or
    // declared as a module, and both of those were tried first:
    //
    // - a module puts `fn main` out of reach, because an example's main is
    //   private and privacy does not reach outwards;
    // - `include!` cannot carry a file whose header is `//!`, since inner
    //   attributes may not come from a macro expansion -- and every example
    //   here starts with one.
    //
    // Copying costs a rewrite of those header lines to `//`, and buys a file
    // where `main` is an ordinary private function of the same module as the
    // exports below.
    let source = std::fs::read_to_string(&example_path)
        .map_err(|e| format!("cannot read {}: {e}", example_path.display()))?;
    let source: String = source
        .lines()
        .map(|l| match l.strip_prefix("//!") {
            Some(rest) => format!("//{rest}\n"),
            None => format!("{l}\n"),
        })
        .collect();

    // Only when it is not already there: a duplicate import is an error, and
    // an example may well have written this one itself.
    let wgpu_import = if source.contains("use kalast::wgpu") || source.contains("use wgpu") {
        ""
    } else {
        // So that an example naming `wgpu::Color`, as one setting a colour
        // does, compiles without depending on wgpu itself.
        "use kalast::wgpu;\n\n"
    };

    std::fs::write(
        src.join("lib.rs"),
        format!(
            r#"// Generated by kalast's editor from
// {example}
//
// Rewritten every time an example is built for hosting. Nothing here is meant
// to be edited: change the file above.

{wgpu_import}{source}
/// What this was built against, checked by the host before anything is called.
#[unsafe(no_mangle)]
pub extern "C" fn kalast_abi() -> u64 {{
    kalast::app::abi_fingerprint()
}}

/// Run the example's own `main`, with the host reachable from it.
///
/// # Safety
///
/// `host` must outlive the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kalast_example(host: *const kalast::app::hosted::HostApi) {{
    unsafe {{ kalast::app::hosted::set_host(host) }};
    main();
    kalast::app::hosted::clear_host();
}}
"#
        ),
    )
    .map_err(|e| format!("cannot write the wrapper: {e}"))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// The toolchain
//
// A `.rs` example is compiled, so it needs cargo. From a clone that is a
// given. From a release bundle it is not, and telling the user to go and
// install Rust would make the executable a set of instructions rather than a
// program -- so it installs one itself, the first time one is actually
// needed.
//
// Not at first launch: most people never open a `.rs`, and a toolchain is a
// few hundred megabytes. The trigger is pressing compile on one.
// ---------------------------------------------------------------------------

/// Where a bundle keeps a toolchain it installed for itself.
///
/// In the working directory, beside `target/`, for the reason the wrapper
/// crate is there: a bundle is run from inside itself, so everything a hosted
/// build produces stays in the folder the user unpacked and goes away when
/// they delete it. Nothing is written to `$HOME` and nothing is hidden.
pub fn toolchain_dir() -> std::path::PathBuf {
    std::path::Path::new("toolchain").to_path_buf()
}

/// A cargo this program may run.
pub struct Toolchain {
    pub cargo: std::path::PathBuf,
    /// `CARGO_HOME` and `RUSTUP_HOME` for one we installed. `None` for a
    /// toolchain already on the machine, which manages its own.
    pub home: Option<std::path::PathBuf>,
}

/// The rustup name for the machine this is running on.
fn host_triple() -> Option<&'static str> {
    Some(
        match (std::env::consts::OS, std::env::consts::ARCH) {
            ("macos", "aarch64") => "aarch64-apple-darwin",
            ("macos", "x86_64") => "x86_64-apple-darwin",
            ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
            ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
            ("windows", "x86_64") => "x86_64-pc-windows-msvc",
            ("windows", "aarch64") => "aarch64-pc-windows-msvc",
            _ => return None,
        },
    )
}

fn runs(cargo: &std::path::Path, home: Option<&std::path::Path>) -> bool {
    let mut cmd = std::process::Command::new(cargo);
    cmd.arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    if let Some(home) = home {
        cmd.env("CARGO_HOME", home.join("cargo"))
            .env("RUSTUP_HOME", home.join("rustup"));
    }
    cmd.status().map(|s| s.success()).unwrap_or(false)
}

fn cargo_in(home: &std::path::Path) -> std::path::PathBuf {
    home.join("cargo")
        .join("bin")
        .join(format!("cargo{}", std::env::consts::EXE_SUFFIX))
}

/// A cargo already on this machine, if there is one.
///
/// `PATH` first, then the two places a rustup install puts it. Looking in
/// `~/.cargo/bin` is not belt and braces: a bundle double-clicked in Finder
/// inherits a `PATH` from `launchd`, not from the shell, so a machine with a
/// perfectly good toolchain looks empty from there. Downloading a second one
/// in that case would be the wrong answer to the right question.
fn existing_cargo() -> Option<Toolchain> {
    let bare = std::path::Path::new("cargo");
    if runs(bare, None) {
        return Some(Toolchain {
            cargo: bare.to_path_buf(),
            home: None,
        });
    }
    let homes = [
        std::env::var_os("CARGO_HOME").map(std::path::PathBuf::from),
        std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".cargo")),
        std::env::var_os("USERPROFILE").map(|h| std::path::PathBuf::from(h).join(".cargo")),
    ];
    for home in homes.into_iter().flatten() {
        let cargo = home
            .join("bin")
            .join(format!("cargo{}", std::env::consts::EXE_SUFFIX));
        if cargo.is_file() && runs(&cargo, None) {
            return Some(Toolchain { cargo, home: None });
        }
    }
    None
}

/// Install a minimal toolchain under `home`, the way rustup's own installer
/// does when it is not talking to a terminal.
///
/// `--profile minimal` is rustc, cargo and the standard library and nothing
/// else -- no docs, no clippy, no rustfmt -- because the only thing it is
/// here to do is build one cdylib. `--no-modify-path` because a program that
/// edits someone's shell profile behind their back has overstepped; the
/// toolchain is used by absolute path instead.
fn install_toolchain(home: &std::path::Path) -> Result<(), String> {
    let triple = host_triple().ok_or_else(|| {
        format!(
            "no rustup build for {}-{}; install Rust yourself from https://rustup.rs",
            std::env::consts::OS,
            std::env::consts::ARCH
        )
    })?;
    let name = format!("rustup-init{}", std::env::consts::EXE_SUFFIX);
    let url = format!("https://static.rust-lang.org/rustup/dist/{triple}/{name}");

    std::fs::create_dir_all(home).map_err(|e| format!("cannot create {}: {e}", home.display()))?;
    let init = home.join(&name);

    println!("no cargo found, and a .rs example has to be compiled.");
    println!("installing a Rust toolchain into {}", home.display());
    let mut curl = std::process::Command::new("curl");
    curl.args(["-sSfL", "--proto", "=https", "--tlsv1.2", "-o"])
        .arg(&init)
        .arg(&url);
    println!("$ {}", show(&curl));
    match curl.status() {
        Ok(s) if s.success() => {}
        Ok(s) => return Err(format!("downloading rustup-init failed ({s}); {url}")),
        // curl ships with macOS, every Linux desktop and Windows 10 and
        // later, so this is nearly always "no network" rather than "no
        // curl" -- but name the URL either way, so it can be done by hand.
        Err(e) => {
            return Err(format!(
                "cannot run curl to download a toolchain ({e}). Fetch {url} \
                 by hand, or install Rust from https://rustup.rs"
            ));
        }
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&init, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("cannot make {} executable: {e}", init.display()))?;
    }

    let mut rustup = std::process::Command::new(&init);
    rustup
        .args([
            "-y",
            "--profile",
            "minimal",
            "--no-modify-path",
            "--default-toolchain",
            "stable",
        ])
        .env("CARGO_HOME", home.join("cargo"))
        .env("RUSTUP_HOME", home.join("rustup"));
    println!("$ {}", show(&rustup));
    println!("(a few hundred MB; it is kept here and reused)");
    let done = rustup.status();
    let _ = std::fs::remove_file(&init);
    match done {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(format!("rustup-init exited with {s}")),
        Err(e) => Err(format!("cannot run {}: {e}", init.display())),
    }
}

/// The cargo to build a hosted example with, installing one if this machine
/// has none.
pub fn find_or_install_cargo() -> Result<Toolchain, String> {
    if let Some(found) = existing_cargo() {
        return Ok(found);
    }
    let home = toolchain_dir();
    let ours = cargo_in(&home);
    if ours.is_file() && runs(&ours, Some(&home)) {
        return Ok(Toolchain {
            cargo: ours,
            home: Some(home),
        });
    }
    install_toolchain(&home)?;
    let ours = cargo_in(&home);
    if !ours.is_file() {
        return Err(format!(
            "rustup finished but there is no cargo at {}",
            ours.display()
        ));
    }
    Ok(Toolchain {
        cargo: ours,
        home: Some(home),
    })
}

/// What a toolchain cannot supply: the system linker rustc calls at the end.
///
/// Worth its own check because the failure is otherwise a wall of `ld` output
/// after a five-minute compile, and the fix is one command the user has to be
/// told. Only macOS is checked: a Linux desktop essentially always has `cc`,
/// and on Windows rustup itself refuses to proceed without the MSVC tools and
/// says so better than this could.
fn linker_hint() -> Option<String> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let ok = std::process::Command::new("xcode-select")
        .arg("-p")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    (!ok).then(|| {
        "no command line tools, so rustc has no linker. Run `xcode-select --install`, \
         then press compile again."
            .to_string()
    })
}

/// Build that cdylib, **with this build's own feature set**.
///
/// Not optional. The `python` feature changes the layout of `Shared` and
/// `Tick`, so a guest built without it and handed an `App` from a host with
/// it reads the wrong bytes. The host is the only thing that knows which it
/// is, so it says so on the command line.
pub fn build_hosted(example: &std::path::Path, release: bool, busy: Arc<AtomicBool>) {
    let example = example.to_path_buf();
    std::thread::spawn(move || {
        // On a thread because from a bundle with no Rust on the machine the
        // first of these downloads a toolchain, and the render loop is not
        // waiting for that.
        match build_hosted_blocking(&example, release) {
            Ok(built) => println!("built {}", built.display()),
            Err(e) => println!("build failed: {e}"),
        }
        busy.store(false, Ordering::SeqCst);
    });
}

/// The same build, run here rather than on a thread, for a caller with no
/// window: `kalast --precompile`, which is how a release bundle arrives with
/// its `.rs` examples already built.
///
/// One function for both so there is no second recipe. A bundle's libraries
/// have to be produced by the same `write_wrapper`, the same feature set and
/// the same target directory the editor would look in, or they are libraries
/// the editor ignores.
pub fn build_hosted_blocking(
    example: &std::path::Path,
    release: bool,
) -> Result<std::path::PathBuf, String> {
    // The feature set the wrapper is given is this build's own, decided in
    // `kalast_dependency`: `python` changes the layout of `Shared` and
    // `Tick`, and a guest that disagrees is handed an `App` whose fields are
    // somewhere else.
    write_wrapper(example)?;

    // The linker first, because it is the one thing a toolchain cannot
    // supply and checking it costs nothing -- whereas finding out after
    // downloading a few hundred megabytes would be a poor trade.
    if let Some(hint) = linker_hint() {
        return Err(hint);
    }
    let toolchain = find_or_install_cargo()?;

    let manifest = wrapper_dir().join("Cargo.toml");
    let target_dir = std::env::current_dir().unwrap_or_default().join(build_dir());
    let mut cmd = std::process::Command::new(&toolchain.cargo);
    cmd.args(["build", "--color=never", "--manifest-path"])
        .arg(&manifest)
        .env("CARGO_TARGET_DIR", &target_dir);
    // A toolchain this program installed is reached by absolute path and has
    // nothing in the environment pointing at it, so both homes have to be
    // named or its rustup proxy finds no toolchain at all.
    if let Some(home) = &toolchain.home {
        cmd.env("CARGO_HOME", home.join("cargo"))
            .env("RUSTUP_HOME", home.join("rustup"));
    }
    if release {
        cmd.arg("--release");
    }
    println!("$ {}", show(&cmd));
    match cmd.status() {
        Ok(s) if s.success() => Ok(dylib_path_for(&package_name(example), release)),
        Ok(s) => Err(format!("cargo exited with {s}")),
        Err(e) => Err(format!("could not run cargo: {e}")),
    }
}

/// Load a built example into this process and run its `scene`.
///
/// The returned `Library` **must outlive every callback the example
/// installed**: those are function pointers into its code, and unloading it
/// while one is armed unmaps the instructions the next frame will call.
///
/// # Safety
///
/// Calls `dlopen` on a file cargo produced from this crate, and hands it a
/// pointer to the live `App`. The fingerprint check below is what makes that
/// defensible; see `kalast::app::abi_fingerprint`.
pub fn load_example(
    example: &std::path::Path,
    release: bool,
    app: &mut crate::app::App,
) -> Result<libloading::Library, String> {
    let path = dylib_path_for(&package_name(example), release);
    if !path.is_file() {
        return Err(format!(
            "{} is not built yet -- press compile first",
            path.display()
        ));
    }

    unsafe {
        let library = libloading::Library::new(&path)
            .map_err(|e| format!("could not load {}: {e}", path.display()))?;

        let abi: libloading::Symbol<extern "C" fn() -> u64> = library
            .get(b"kalast_abi")
            .map_err(|_| format!("{} exports no kalast_abi", path.display()))?;
        let (theirs, ours) = (abi(), crate::app::abi_fingerprint());
        if theirs != ours {
            return Err(format!(
                "{} was built against a different kalast ({theirs:x} against {ours:x}).\n                   rebuild it with the same features as this program -- press compile.",
                path.display()
            ));
        }

        let run: libloading::Symbol<
            unsafe extern "C" fn(*const crate::app::hosted::HostApi),
        > = library
            .get(b"kalast_example")
            .map_err(|_| format!("{} exports no kalast_example", path.display()))?;
        println!("loaded {}", path.display());

        // On this stack for the whole call, which is what `set_host` needs.
        // The example's `main` runs inside it: it builds an `App` of its own,
        // and every call that would own a loop crosses back through here.
        let api = crate::app::hosted::host::api(app as *mut _);
        run(&api as *const _);

        // Dropped by the caller, not here: the symbols are gone from scope but
        // the callbacks the example just installed are not.
        drop(run);
        drop(abi);
        Ok(library)
    }
}

/// A command as it would be typed.
fn show(cmd: &std::process::Command) -> String {
    let mut out = cmd.get_program().to_string_lossy().into_owned();
    for a in cmd.get_args() {
        out.push(' ');
        out.push_str(&a.to_string_lossy());
    }
    out
}

/// Whether the hosted library is built *and* newer than the example.
///
/// "Built" is not enough on its own: loading a library older than the file
/// shown in the panel would run code the panel is not displaying, which is a
/// worse lie than an empty viewport. The wrapper is generic, so one library
/// stands for whichever example was built last -- hence comparing against
/// the file that is open rather than against a target name.
pub fn is_current(release: bool, source: &str) -> bool {
    let modified = |p: &std::path::Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    let example = std::path::Path::new(source);
    let Some(built) = modified(&dylib_path_for(&package_name(example), release)) else {
        return false;
    };

    if let Some(edited) = modified(std::path::Path::new(source)) {
        if built < edited {
            return false;
        }
    }

    // And against the engine, which is the half the fingerprint cannot always
    // see: a `bool` added to `Shared` fit in existing padding, changed no size
    // and no offset, and a library built before it loaded anyway -- running
    // the *old* `hosted::step` against the new host. A timestamp does not
    // care whether the change was visible.
    !newest_engine_source().is_some_and(|engine| built < engine)
}

/// When the engine was last edited, over `src/` and the manifest.
///
/// A few hundred `stat`s, and only when the editor is deciding whether to
/// rebuild -- not per frame.
fn newest_engine_source() -> Option<std::time::SystemTime> {
    fn walk(dir: &std::path::Path, newest: &mut Option<std::time::SystemTime>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, newest);
            } else if let Ok(t) = entry.metadata().and_then(|m| m.modified()) {
                if newest.is_none_or(|n| t > n) {
                    *newest = Some(t);
                }
            }
        }
    }

    let mut newest = None;
    walk(std::path::Path::new("src"), &mut newest);
    walk(std::path::Path::new("shaders"), &mut newest);
    if let Ok(t) = std::fs::metadata("Cargo.toml").and_then(|m| m.modified()) {
        if newest.is_none_or(|n| t > n) {
            newest = Some(t);
        }
    }
    newest
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The engine half of the staleness check has to actually see the
    /// engine, or a library built before a change to `Shared` loads against
    /// the host that changed it.
    #[test]
    fn the_engine_s_own_sources_count_as_newer() {
        let newest = newest_engine_source().expect("src/ is walked");
        let manifest = std::fs::metadata("Cargo.toml")
            .and_then(|m| m.modified())
            .expect("the manifest is there");
        assert!(
            newest >= manifest,
            "the walk covers at least the files it is asked about"
        );
    }

    /// The wrapper is what makes an example need nothing written in it, so
    /// what it generates is worth pinning: the example included by path, its
    /// own `main` called, and the host installed around that call.
    #[test]
    fn the_wrapper_calls_the_example_s_own_main() {
        let dir = std::env::temp_dir().join("kalast-wrapper-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let example = dir.join("plain.rs");
        // With the `//!` header every example here has: it must survive the
        // copy as a plain comment, because inner docs are only legal first.
        std::fs::write(&example, "//! A plain example.\nfn main() {}\n").unwrap();

        // `write_wrapper` writes under `target/`, relative to the working
        // directory, which for a test is the crate root.
        write_wrapper(&example).unwrap();

        let lib = std::fs::read_to_string(wrapper_dir().join("src/lib.rs")).unwrap();
        assert!(lib.contains("plain.rs"), "it says where it came from");
        assert!(
            !lib.contains("//! A plain example."),
            "the inner doc header is rewritten; it is only legal first in a file"
        );
        assert!(lib.contains("// A plain example."));
        assert!(lib.contains("fn main() {}"), "the example is copied in whole");
        assert!(lib.contains("main();"), "its own main is called");
        assert!(lib.contains("set_host"), "with the host reachable from it");
        assert!(lib.contains("kalast_abi"), "and the ABI check exported");

        let manifest = std::fs::read_to_string(wrapper_dir().join("Cargo.toml")).unwrap();
        // `cfg!` and not a literal: the wrapper is given *this* build's
        // feature set, and the assertion has to follow it or the test only
        // passes in the default build. See `dependency_tests`.
        assert_eq!(
            manifest.contains("features = [\"python\"]"),
            cfg!(feature = "python"),
            "the host's features are passed on, or the layouts disagree"
        );
        assert!(manifest.contains("crate-type = [\"cdylib\"]"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod dependency_tests {
    use super::*;

    /// A clone compiles against itself, which is what makes the compile
    /// button a rebuild rather than a download.
    #[test]
    fn a_clone_is_compiled_against_itself() {
        let repo = std::env::current_dir().unwrap();
        let dep = kalast_dependency(&repo);
        assert!(dep.contains("path = "), "{dep}");
        assert!(!dep.contains("version = "), "{dep}");
    }

    /// And a bundle, which has no source tree, against the release. Reported
    /// from v0.5.0: `./kalast examples/.../step.rs` ended in "failed to read
    /// <bundle>/Cargo.toml", a file nobody had been told to expect.
    #[test]
    fn a_bundle_is_compiled_against_the_release() {
        let bundle = std::env::temp_dir().join(format!("kalast_bundle_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&bundle);
        std::fs::create_dir_all(&bundle).unwrap();
        let dep = kalast_dependency(&bundle);
        let _ = std::fs::remove_dir_all(&bundle);

        assert!(!dep.contains("path = "), "nothing to point a path at: {dep}");
        assert!(
            dep.contains(&format!("version = \"={}\"", env!("CARGO_PKG_VERSION"))),
            "pinned exactly, because abi_fingerprint hashes the version and a \
             guest one patch ahead would be refused at load: {dep}"
        );
    }

    /// The bug this would have caused, had the registry dependency been
    /// added without it: `python` is a *default* feature, so a wrapper that
    /// does not turn defaults off gives the guest an interpreter the
    /// `--no-default-features` host has not got. Different `Shared` layout,
    /// and every hosted build from a bundle refused at load with "built
    /// against a different kalast".
    #[test]
    fn defaults_are_off_and_the_host_s_features_named_explicitly() {
        let repo = std::env::current_dir().unwrap();
        for dep in [kalast_dependency(&repo), kalast_dependency(std::path::Path::new("/"))] {
            assert!(dep.contains("default-features = false"), "{dep}");
            assert_eq!(
                dep.contains("features = [\"python\"]"),
                cfg!(feature = "python"),
                "the guest gets exactly the host's feature set: {dep}"
            );
        }
    }

    /// Every platform the release builds an executable for has to be one
    /// rustup publishes an installer for, or a bundle there cannot compile a
    /// `.rs` at all.
    #[test]
    fn rustup_has_a_build_for_this_machine() {
        assert!(
            host_triple().is_some(),
            "no rustup triple for {}-{}",
            std::env::consts::OS,
            std::env::consts::ARCH
        );
    }
}
