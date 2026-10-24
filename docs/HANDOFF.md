# Handoff

> **Status: HANDED OFF on 2026-10-24 to Claude Code / T3 Code session on WINSERVE.**
>
> Development moved from the owner's gaming PC (GAMERBOX) to WINSERVE on
> 2026-10-24; the environment facts below that name GAMERBOX are historical.
> See `docs/CI.md` for the current machine and runner.
>
> If you are the agent picking this up: **before doing anything else**, change
> the line above to `Status: HANDED OFF on <date> to <thread or session
> name>`, then commit and merge that change (a one-line PR is fine). That
> tells later agents this handoff is consumed, so they don't redo or undo
> your work. After that, the source of truth is the repo, the open PRs and
> docs/ROADMAP.md, not this file.

Written 2026-10-11 by the orchestrating session, which was stopped by the
owner's usage limit. Everything is committed and pushed; nothing exists only
on this machine except build caches.

## Read first

1. `AGENTS.md`: rules, including **agents own git** (branch, PR, CI, squash-merge, tag releases; the owner never does).
2. `docs/VISION.md`: what the owner wants, in their words.
3. `docs/ROADMAP.md`: phases and tracks; decisions 1 to 7 at the end.
4. `docs/CI.md`: CI runs on the owner's PC (self-hosted runner `GAMERBOX-csr`).

## State of master (9fd35da)

- **Phase 0 is done:** CI, asset guard, Minecraft code removed, CS:GO Legacy detection (app 4465480) with a Steam install prompt, 128 tick.
- **Released: v0.3.0.** The installer (Velopack, `CSGORewrite-win-Setup.exe`) self-updates from GitHub Releases and offers "Add to Steam". Permanent link: https://github.com/bentsignal/global-offensive/releases/latest/download/CSGORewrite-win-Setup.exe
- **Fidelity harness** (`tools/fidelity/`, `docs/FIDELITY.md`): drives the real CS:GO Legacy and our game from matched viewpoints. Baseline on Dust II: MAE 22.7, SSIM 0.669.
- **Scaleform spike** (`tools/scaleform-spike/`, `docs/research/scaleform-spike.md`): Ruffle plays the install's own `mainmenu.swf` plus 25 movies. MAE is 6.0 against the reference with non-Scaleform content masked. Decision: integrate Ruffle.

## Open work (all pushed; each has a PR)

| Track | PR | Branch | State | Next step |
|---|---|---|---|---|
| A1 sim authority | #9 (ready) | `track-a/sim-authority` | Complete per its commits: bots and loadouts in `sim`, `World: Clone`, serde `WorldState`/`Snapshot` (postcard), runtime tick rate (128 default), VR `TrackedPose` + `AimRay` in `TickInput`, determinism tests. Interrupted before opening its own PR. | Review the diff, wait for CI, run the full AGENTS.md verification plus a `csr play` screenshot, then merge. Then start **A2 networking** (ROADMAP Track A: transport, `csr server`, `csr connect`, prediction, lag compensation, two-client proof). |
| C menu integration | #10 (draft) | `track-c/scaleform-menu` | The spike is merged (#8). The new `crates/scaleform` integration crate exists; the last commit is **WIP and unbuilt** (it was writing the shim's key table and key callback). | Read `docs/research/scaleform-spike.md` (integration plan), finish the crate, embed it as the client's menu layer behind an app state machine, replace `crates/client/src/menu.rs`. Ruffle is pinned to the owner's fork `bentsignal/ruffle`, branch `csgo-scaleform` (three small core fixes, upstreamable). Ruffle uses wgpu 30 and Bevy wgpu 29: render offscreen and copy. |
| D sound and fidelity | #11 (draft) | `track-d/sound` | Commit `ff6e28e` is complete (measured mixing, distance curves, distant gunfire layer, footsteps per surface, MDL AE_CL_PLAYSOUND reload/deploy events, surface impacts, `CSR_SOUND_LOG=1`). The top commit, a core audio rewrite, is **WIP and may not build**. | Get it building, verify with `tools/fidelity` (`sounds` mode) against the real game, and drop the WIP commit if it doesn't pay off. Then materials and lighting, worst first. Baseline gaps: low contrast (lifted shadows, compressed highlights), props far too bright, some mis-lit lightmap faces, soft ground textures, no detail sprites. |
| E old maps and packs | #12 (draft) | `track-e/old-maps` | Commit `ab59dbc` is complete (workshop crate: table of Valve "Compatibility Version" maps verified via the API, fetch from Steam library or CDN with resume and SHA-1, `<map>_<year>.bsp`, `csr maps`/`fetch-map`/`remove-map`, `level::maps` used by the menu). The top commit (`crates/packs`, MODDING.md, OLD-MAPS.md) is **WIP**. | Finish and verify (probe/sim on each old map), consider splitting the old-maps part into its own PR to land it sooner, then the `.csrpack` format, drag-and-drop and file association (ROADMAP Track E). |

Track B (installer) is merged and done. Its branch `track-b/installer` can be deleted.

## Conflicts to expect

A1 (#9) touches `sim` and the client's gameplay plumbing (bots, loadouts, `skin_finish`, `inventory::apply`). E (#12) changed `level::maps` and the current menu. C will replace the menu entirely. Merge order that minimises pain: **#9 first**, then E's old-maps part, then D, then C's integration (rebasing each on master).

## Environment facts (owner's PC, GAMERBOX)

- Ryzen 9 9900X (24 threads), 62 GB RAM, RTX 5070 Ti. Rust stable via rustup in `%USERPROFILE%\.cargo\bin`. MSVC Build Tools and the .NET SDK are present; `vpk` is a dotnet global tool.
- CS:GO Legacy: `C:\Program Files (x86)\Steam\steamapps\common\csgo legacy\csgo` (1.38.8.1). MW2 is also installed under `common\Call of Duty Modern Warfare 2` (for the future Intervention work).
- `gh` is logged in as `bentsignal`, with the default repo set to the fork `bentsignal/global-offensive`. **Never** open PRs against upstream `FrosttysBots/CS-Craft`.
- CI runner: `D:\tools\csr-actions-runner`, which starts hidden at logon from the Startup folder. Build cache: `D:\tools\csr-actions-runner\cargo-target`.
- Old agent worktrees under `.claude/worktrees/` (ignored by git) may still exist. Everything in them is pushed; delete them with `git worktree remove --force <path>` and `git worktree prune`.

## Lessons from this session

- **Usage limits stopped all parallel agents twice.** Run fewer agents at once (two is safe), and tell each to commit and push work-in-progress often. Two worktrees were lost on the first interruption.
- Agents interrupted mid-task leave WIP commits; label them `WIP:` and say so in the PR.
- Self-hosted CI makes a warm build about 5 minutes and a cold one about 11. CI also runs `probe` and `sim` against the real install.
- Track D's harness opens CS:GO Legacy on the owner's desktop for short capture sessions. It only deletes files the game created during the session.
