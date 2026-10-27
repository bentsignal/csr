//! Embeds CSGO Rewrite's icon (drawn in `src/art.rs`) in `csr.exe`, so the
//! installer's Start menu and desktop shortcuts show it.

#[path = "src/art.rs"]
mod art;

fn main() {
    println!("cargo:rerun-if-env-changed=CSR_BUILD_SHA");
    let supplied_revision = std::env::var("CSR_BUILD_SHA")
        .ok()
        .filter(|s| !s.is_empty());
    // CI provides the immutable source SHA. Watching checkout timestamps too
    // would relink the same executable when Stable and Dev use that same SHA.
    if supplied_revision.is_none() {
        // Record the executable's real source identity, including worktree refs.
        for reference in ["HEAD", "packed-refs"] {
            if let Ok(out) = std::process::Command::new("git")
                .args(["rev-parse", "--git-path", reference])
                .output()
            {
                if out.status.success() {
                    println!(
                        "cargo:rerun-if-changed={}",
                        String::from_utf8_lossy(&out.stdout).trim()
                    );
                }
            }
        }
        if let Ok(out) = std::process::Command::new("git")
            .args(["symbolic-ref", "-q", "HEAD"])
            .output()
        {
            if out.status.success() {
                if let Ok(path) = std::process::Command::new("git")
                    .args([
                        "rev-parse",
                        "--git-path",
                        String::from_utf8_lossy(&out.stdout).trim(),
                    ])
                    .output()
                {
                    println!(
                        "cargo:rerun-if-changed={}",
                        String::from_utf8_lossy(&path.stdout).trim()
                    );
                }
            }
        }
    }
    let revision = supplied_revision
        .or_else(|| {
            std::process::Command::new("git")
                .args(["rev-parse", "--short=8", "HEAD"])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        })
        .unwrap_or_else(|| "source".into());
    println!("cargo:rustc-env=CSR_BUILD_SHA={revision}");
    println!("cargo:rerun-if-changed=src/art.rs");
    println!("cargo:rerun-if-changed=build.rs");
    // The MSVC linker takes a compiled resource file as an input.
    if std::env::var("TARGET").is_ok_and(|t| t.ends_with("windows-msvc")) {
        let res = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap())
            .join("csgo-rewrite-icon.res");
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
