# CI

Builds and tests run on the owner's PC through a self-hosted GitHub Actions
runner, not on GitHub's machines. Cold builds there took 30 to 50 minutes;
here the build cache persists, and CS:GO Legacy is installed, so CI can also
run the real-data checks.

## What runs

| Workflow | Where | Steps |
|---|---|---|
| `ci.yml` on every PR and push to `master` | `[self-hosted, csr]` | `cargo test`, `play` build, `csr probe de_dust2`, `csr sim de_dust2`, two-client UDP Dust II netcheck and owned-session checks at 128/64 tick |
| `ci.yml` asset guard | GitHub (`ubuntu-latest`, seconds) | `tools/check-no-assets.sh` |
| `release.yml` on a `v*` tag | `[self-hosted, csr]` | tests, `tools/package-release.ps1 -DeltaFromGitHub` (zip, Velopack installer, full and delta update packages), publish all of it to the GitHub Release (`gh release create`, then `vpk upload github --merge`) |

The release job needs the .NET SDK (8 or newer) on the runner for `vpk`; the
workflow installs or updates the pinned `vpk` version itself
(`dotnet tool update -g vpk`).

Before packaging and again before publication, `tools/verify-release-version.ps1`
resolves the common workspace-member version through actual Cargo metadata and
requires the tag to equal `v<version>`. A manual `-Version` packaging override
must also match that version. Publication selects the exact versioned ZIP.
`tools/test-release-version.ps1` verifies real valid/mismatched tags, prerelease
identity, mixed workspace versions and rejection before staging mutation; both
CI workflows run it without compiling targets or fetching dependencies.
Cargo's metadata schema and `--no-deps` behavior are documented in the
[Cargo Book](https://doc.rust-lang.org/cargo/commands/cargo-metadata.html).

`workflow_dispatch` produces unpublished installer/ZIP artifacts. A pushed
version tag publishes immediately; it is not a private candidate channel.
Packaged static-CRT executable and installed-update checks are separate from
the ordinary source `play`/release proofs. Player data and app ID must remain
unchanged across an update.

Jobs queue while the PC is off and start when it is back.

## The runner

- Installed at `D:\tools\csr-actions-runner`, registered as
  `GAMERBOX-csr` with labels `csr`, `gpu`, `csgo-legacy`.
- Runs as the owner's user (no admin): `CSGO Rewrite CI runner.vbs` in the
  Windows Startup folder starts `run.cmd` hidden at logon.
- Build cache: `D:\tools\csr-actions-runner\cargo-target` (`CARGO_TARGET_DIR`
  in the workflows). Delete it to force a clean build.
- Re-register (for example after moving it):
  `gh api -X POST repos/bentsignal/global-offensive/actions/runners/registration-token --jq .token`,
  then `config.cmd --unattended --url https://github.com/bentsignal/global-offensive --token <token> --name GAMERBOX-csr --labels csr,gpu,csgo-legacy --work _work --replace`.

## Safety

The repository is public and the runner executes repository code on the
owner's PC. So:

- The self-hosted job only runs for pushes and for PRs whose branch lives in
  this repository (`head.repo.full_name == github.repository`).
- The repository requires approval before workflows run for any outside
  contributor's PR (`fork-pr-contributor-approval: all_external_contributors`).
- Never relax either of these.
