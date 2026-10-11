# CI

Builds and tests run on the owner's PC through a self-hosted GitHub Actions
runner, not on GitHub's machines. Cold builds there took 30 to 50 minutes;
here the build cache persists, and CS:GO Legacy is installed, so CI can also
run the real-data checks.

## What runs

| Workflow | Where | Steps |
|---|---|---|
| `ci.yml` on every PR and push to `master` | `[self-hosted, csr]` | `cargo test`, `play` build, `csr probe de_dust2`, `csr sim de_dust2` |
| `ci.yml` asset guard | GitHub (`ubuntu-latest`, seconds) | `tools/check-no-assets.sh` |
| `release.yml` on a `v*` tag | `[self-hosted, csr]` | tests, `tools/package-release.ps1`, publish the zip to GitHub Releases |

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
