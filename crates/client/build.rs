//! Keep desktop verification links within the shared development PC's memory.
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("TARGET").is_ok_and(|t| t.ends_with("windows-msvc"))
        && std::env::var("PROFILE").is_ok_and(|p| p == "debug")
    {
        // Bevy and Ruffle dependencies retain their debug information even
        // when this package has debug=0. Their combined final PDB can exhaust
        // memory or MSVC's PDB limit. This affects desktop tests/examples only;
        // small crates retain symbols, and playable/release links are unchanged.
        println!("cargo:rustc-link-arg=/DEBUG:NONE");
    }
}
