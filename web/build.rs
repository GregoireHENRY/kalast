//! The page's files and its Rust examples, written into the build's output
//! for `src/lib.rs` to include: the repository's `examples/` as git has it --
//! what a clone carries, without a working copy's caches, and without
//! `examples/old/`, which is superseded -- and the resources those examples
//! and the documentation tab read.

use std::fmt::Write as _;
use std::path::PathBuf;

/// Files bigger than this stay out of the page: a few Python examples' data,
/// megabytes each, for scripts a browser cannot run.
const MAX: u64 = 512 * 1024;

/// What the page carries of `res/`, whatever its size: the meshes its
/// examples load, Rust's and Python's, and the picture the documentation tab
/// shows.
const RES: &[&str] = &[
    "res/didymos_v003_10k.obj",
    "res/dimorphos_v004_10k.obj",
    "res/plane_crater_1024-5000_h=0.437.obj",
    "res/plane_crater_1024-512_h=0.437.obj",
    "res/cube.obj",
    "res/ico2.obj",
    "res/ico3.obj",
    "res/ico4.obj",
    "res/ico5.obj",
    "res/sph1.obj",
    "res/kalast-ui.png",
];

fn main() {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("..");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let listed = std::process::Command::new("git")
        .args(["ls-files", "-z", "examples"])
        .current_dir(&root)
        .output()
        .expect("git, to list the examples");
    let mut paths: Vec<String> = String::from_utf8(listed.stdout)
        .unwrap()
        .split('\0')
        // Not Hera's cameras at Didymos either: the mission's data there is not public yet.
        .filter(|p| !p.is_empty() && !p.starts_with("examples/old/") && !p.starts_with("examples/hera_didymos/"))
        .map(str::to_string)
        .collect();
    paths.extend(RES.iter().map(|p| p.to_string()));

    let mut files = String::from("/// The page's files: each one's path and its bytes.\npub static FILES: &[(&str, &[u8])] = &[\n");
    let mut examples = String::new();
    let mut register = String::from("/// The Rust examples, made ones the page runs.\npub fn add_examples() {\n");
    for (i, path) in paths.iter().enumerate() {
        let full = root.join(path);
        let Ok(meta) = std::fs::metadata(&full) else { continue };
        println!("cargo:rerun-if-changed={}", full.display());
        if meta.len() > MAX && !RES.contains(&path.as_str()) {
            continue;
        }
        let full = full.canonicalize().unwrap();
        let full = full.to_str().unwrap();
        writeln!(files, "    ({path:?}, include_bytes!({full:?})),").unwrap();
        if path.starts_with("examples/") && path.ends_with(".rs") {
            // A copy, its `//!` lines made `//`: `include!` cannot carry inner
            // attributes, as the editor's wrapper finds (`app::cargo`).
            let text: String = std::fs::read_to_string(full)
                .unwrap()
                .lines()
                .map(|l| match l.strip_prefix("//!") {
                    Some(rest) => format!("//{rest}\n"),
                    None => format!("{l}\n"),
                })
                .collect();
            let copy = out.join(format!("example_{i}.rs"));
            std::fs::write(&copy, text).unwrap();
            let copy = copy.to_str().unwrap();
            writeln!(examples, "// {path}\n#[allow(warnings)]\nmod example_{i} {{\n    include!({copy:?});\n    pub fn run() {{\n        main()\n    }}\n}}").unwrap();
            writeln!(register, "    kalast::app::cargo::add_example({path:?}, include_str!({full:?}), example_{i}::run);").unwrap();
        }
    }
    files.push_str("];\n\n");
    register.push_str("}\n");
    std::fs::write(out.join("files.rs"), files + &examples + "\n" + &register).unwrap();
    // A file added or taken away: git's index changes.
    println!("cargo:rerun-if-changed={}", root.join(".git/index").display());
}
