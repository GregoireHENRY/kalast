use std::env::var;

fn main() {
    let _manifest_dir = var("CARGO_MANIFEST_DIR").unwrap();
    
    println!("cargo:rustc-link-search=C:/softwares/SDL2-2.28.1-devel-msvc/lib/x64/");
}
