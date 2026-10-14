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
        // LNK1140 limit. Desktop test/binary links also exhaust memory while
        // merging these symbols. Preserve dependency artifacts and the small
        // crates' debuggers; suppress desktop final PDBs in the dev/test profile.
        // /PDB:NONE names a shared file on current MSVC; /DEBUG:NONE actually
        // disables PDB generation for concurrently linked examples.
        if std::env::var("PROFILE").is_ok_and(|p| p == "debug") {
            println!("cargo:rustc-link-arg=/DEBUG:NONE");
        } else {
            println!("cargo:rustc-link-arg-examples=/DEBUG:NONE");
        }
    }
}
