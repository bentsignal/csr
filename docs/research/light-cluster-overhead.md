# Unused clustered-light work

The client supplies map lightmaps, prop ambient cubes, sun lighting and model
lighting through `WorldMaterial`. Its cameras do not sample Bevy's clustered
point/spot-light lists, and the client does not spawn those light types.
The match sky, world, viewmodel and isolated studio cameras can omit those
clusters without changing their material lighting inputs.

## Installed Bevy API and rejected attempt

Bevy 0.19.1 publicly documents `ClusterConfig::None` as disabling a camera's
cluster calculations. The initial actual Nuke attempt at source `5eb5b02`
instead exposed the GPU implementation allocating a zero-width `clustering
dummy texture`. Its validation error and early exit are retained privately;
that attempt is rejected and contributes no performance result.

The documented `GlobalClusterSettings.gpu_clustering = None` selects the CPU
fallback. Combined with disabled camera grids, it avoids that invalid GPU
allocation and unnecessary clustering work. Other renderers can still use
explicitly configured CPU clusters. No dependency or engine implementation
was modified. Only the installed public Bevy API and source were inspected.

## Actual same-binary comparison

The corrected diagnostic source `cc99bc7` was built after all 23 workspace
packages were cleaned from the shared play profile. An immutable executable,
source/tree identities and unique compiled-source marker were verified.
Both modes used that identical executable, Nuke, CT spawn, five bots, ordinary
deathmatch, a 1920x1080 window, saved VSync off and the same camera position.
The only mode difference was the experimental light-cluster setting.

Each owned run waited for scene readiness, activated its own window, warmed
up and measured a bounded interval. Analysis includes only frame timestamps
inside that interval. Actual screenshots were retained, exits and raw error
logs checked, and the owner's configuration was unchanged. Local Cargo work
was held for the following comparisons; the separate self-hosted CI runner
continued, so these remain measurements on a shared desktop.

| Order | Camera clusters | Mean frame ms | FPS from mean | p95 ms | p99 ms |
|---|---|---:|---:|---:|---:|
| Forward, first | Default | 9.912 | 100.9 | 12.2 | 13.5 |
| Forward, second | Disabled + CPU fallback | 9.115 | 109.7 | 12.0 | 13.7 |
| Reverse, first | Disabled + CPU fallback | 9.009 | 111.0 | 11.4 | 12.7 |
| Reverse, second | Default | 9.663 | 103.5 | 12.4 | 15.1 |

Mean frame time improved by about 7-8% in both orders. Render-stage wall time
fell from 6.80-6.91 ms to 6.15-6.19 ms. These nested wall scopes include waits;
they are not additive CPU or GPU execution times. The first forward p99
slightly worsened, so uniformly improved tail latency is not established.
An earlier pair overlapped a new local compilation and is excluded from the
causal comparison, rather than presented as a larger improvement.

This is a bounded Nuke improvement, not a 155 Hz or original-game FPS parity
claim. Other viewpoints, moving play, maps, menu pressure and the remaining
main/render costs still need work. Geometry, material shading and lighting
bugs are not fixed by skipping unused clustered-light work.

## Production policy

The merged production policy configures only the client's own custom-lit
cameras at spawn and selects the CPU fallback once at startup. It removes
the experimental per-frame camera query. `CSR_TEST_LIGHT_CLUSTERS=1` retains
the previous default camera/GPU path for bounded comparisons. This is a
renderer diagnostic, not a player-facing option or an invented fidelity mode.
Production source `d39ae8c` passed an all-workspace-package release-profile
clean, launcher build and installed Dust II `probe`/`sim` with zero failures.
Its immutable release executable's actual default-path Nuke capture was
independently inspected: scene, props, hands, weapon and HUD remained intact,
the startup policy marker was present, the process exited successfully without
renderer errors, and the owner's configuration was unchanged. This render is
a correctness check, not a cross-profile performance comparison.

Full workspace/installed-data CI and the asset guard passed before
[PR #149](https://github.com/bentsignal/global-offensive/pull/149) merged as
`fbe2481`. The source failure and rejected GPU allocation attempt remain
recorded separately from successful production verification.

Raw captures, account data and installed assets stay private. Numerical
measurements and independently written renderer policy are the public output.
