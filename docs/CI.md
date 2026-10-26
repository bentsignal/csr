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
| `dev-release.yml` after successful master-push CI | `[self-hosted, csr]` | package the tested source SHA and publish a Dev prerelease; skip superseded commits |

Stable releases are explicitly tagged. Dev versions use the next patch plus
`-dev.<CI run number>.<attempt>`; they never become GitHub's latest Stable
release. Publishing starts with a draft and exposes it only after the installer,
packages and feed are uploaded. The launcher's Stable track reads the latest
Stable download feed directly, so Dev release volume cannot hide Stable.

The release job needs the .NET SDK (8 or newer) on the runner for `vpk`; the
workflow installs or updates the pinned `vpk` version itself
(`dotnet tool update -g vpk`).

Every build needs a Java runtime (`java` on `PATH` or `JAVA_HOME`): Ruffle's
`ruffle_core` build script compiles its ActionScript `playerglobal` library
with Java.

Jobs queue while the PC is off and start when it is back.

## The runner

- Runs on the development machine, WINSERVE (since 2026-10-24; it replaced
  `GAMERBOX-csr` on the owner's gaming PC). Installed at
  `S:\tools\csr-actions-runner`, registered against `bentsignal/csr` as
  `WINSERVE-csr` with labels `csr`, `gpu`, `csgo-legacy`.
- Runs as the owner's user (no admin): the per-user scheduled task
  `CSGO Rewrite CI runner` runs `start-runner.vbs` at logon, which starts
  `run.cmd` hidden. `Start-ScheduledTask 'CSGO Rewrite CI runner'` starts it
  by hand.
- Toolchain on WINSERVE (all per-user except Build Tools): rustup stable,
  Visual Studio 2022 Build Tools (C++ workload, Windows 11 SDK), .NET 8 SDK in
  `%USERPROFILE%\.dotnet`, Temurin JDK 21 in `%LocalAppData%\Programs\jdk-21`,
  Git and `gh` in `%LocalAppData%\Programs`. Workflow steps run through
  Windows PowerShell, so the user's execution policy must allow scripts
  (`Set-ExecutionPolicy -Scope CurrentUser RemoteSigned`); Windows' default
  fails every step.
- Build cache: `S:\tools\csr-actions-runner\cargo-target` (`CARGO_TARGET_DIR`
  in the workflows). Delete it to force a clean build.
- Re-register (for example after moving it):
  `gh api -X POST repos/bentsignal/csr/actions/runners/registration-token --jq .token`,
  then `config.cmd --unattended --url https://github.com/bentsignal/csr --token <token> --name WINSERVE-csr --labels csr,gpu,csgo-legacy --work _work --replace`.

## Safety

The repository is public and the runner executes repository code on the
owner's PC. So:

- The self-hosted job only runs for pushes and for PRs whose branch lives in
  this repository (`head.repo.full_name == github.repository`).
- The repository requires approval before workflows run for any outside
  contributor's PR (`fork-pr-contributor-approval: all_external_contributors`).
- Never relax either of these.
