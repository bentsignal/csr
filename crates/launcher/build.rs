//! Embeds CSGO Rewrite's icon (drawn in `src/art.rs`) in `csr.exe`, so the
//! installer's Start menu and desktop shortcuts show it.

#[path = "src/art.rs"]
mod art;

fn main() {
    println!("cargo:rerun-if-changed=src/art.rs");
    println!("cargo:rerun-if-changed=build.rs");
    // The MSVC linker takes a compiled resource file as an input.
    if std::env::var("TARGET").is_ok_and(|t| t.ends_with("windows-msvc")) {
        let res = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("csgo-rewrite-icon.res");
        std::fs::write(&res, art::icon_res()).expect("write icon resource");
        println!("cargo:rustc-link-arg-bin=csr={}", res.display());
        // Package debug=0 removes our own symbols, but linked dependencies
        // still carry debug information. Their combined PDB can exceed MSVC's
        // LNK1140 limit. Suppress only the examples' final PDB, preserving
        // dependency artifacts and normal executable/test debug behavior.
        println!("cargo:rustc-link-arg-examples=/PDB:NONE");
    }
}
