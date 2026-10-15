# Notes for agents working on CSGO Rewrite

**If [docs/HANDOFF.md](docs/HANDOFF.md) says `Status: OPEN`, start there.**
**Read [docs/VISION.md](docs/VISION.md) first.** It is the owner's intent.
[docs/ROADMAP.md](docs/ROADMAP.md) is the plan; tick items off as they land.

## Ownership and git

- **Owner:** shawn (GitHub `bentsignal`). This is a fork of a project started
  by hawk; keep the original attribution and the license notices.
- **Agents own git end to end.** The owner never commits, pushes, opens or
  merges PRs, or tags releases. Do it without being asked:
  - Never commit straight to `master`. Work on a branch named for the work
    (`phase-0`, `track-a/netcode`, `fix/install-detection`); parallel agents
    each use their own branch, in a git worktree when they share a machine.
  - Commit in small, verified steps with messages that say what changed and
    why. Push the branch and open a PR early (`gh pr create`).
  - Wait for CI to pass, then squash-merge (`gh pr merge --squash
    --delete-branch`) and pull `master`. If CI fails, fix it; do not merge red.
  - Releases: bump `version` in the root `Cargo.toml`, merge, then tag
    `v<version>` on `master` and push the tag. The Release workflow builds
    and publishes it to GitHub Releases.
  - Never force-push `master`, rewrite published history or skip hooks.
- **Report** what was merged or released (PR links) at the end of each task.

## Hard rules

- **No DMCA exposure.** The repo and public releases contain no third-party
  game assets (Valve, Activision, Mojang, ...) and no code ported from leaked
  or decompiled sources. Content is read at runtime from the player's own
  installs, or comes from private content packs that never touch the repo.
  `tools/check-no-assets.sh` enforces this in CI.
- **Clean-room.** Work from public format documentation, the installed
  game's data and observed behaviour (screenshots, footage). Never use the
  leaked CS:GO/TF2 source or paste Source SDK code.
- **Fidelity is 1:1.** Match the original exactly (2018-era look, sounds and
  feel); measure against references, never invent.
- **CS:GO, not CS2.** We rebuild the original CS:GO. Never use or imitate
  Counter-Strike 2 content, behaviour, UI or the CS2 install (`game\csgo`,
  `.vpk`s under app 730's CS2 layout); the only source is CS:GO Legacy.
- **Friction is a bug.** Players may not be technical. Setup must feel like
  magic: install CS:GO Legacy on Steam, run our installer, play. No
  terminals, config files, manual copying or long setup. Anything a player
  must do by hand is a bug unless there is no other way.
- **Estimates:** never give human-scale durations. Plan in phases,
  dependencies and parallel tracks.

## Design seams to protect

- Gameplay state lives in `sim` and changes only through per-player
  `TickInput`; that becomes the network protocol. No gameplay in Bevy
  render-frame systems.
- Do not assume a player is one camera: leave room for tracked head and
  hand poses and a separate aim ray (VR cross-play).
- New content gets a `ContentId` and must be loadable from a content pack,
  not only from the CS:GO install.

## Game data

CS:GO Legacy is its own Steam app, **4465480**, installed at
`C:\Program Files (x86)\Steam\steamapps\common\csgo legacy\csgo` on the dev
machine (CS:GO 1.38.8.1, Oct 2023). App 730 is now Counter-Strike 2. Older
installs may still use the 730 `csgo_legacy` beta in
`common\Counter-Strike Global Offensive\csgo`; both are supported. Read only.
`CSR_GAME_DIR` overrides the path.

## Toolchain and builds

- Rust stable via rustup; cargo is in `%USERPROFILE%\.cargo\bin`. In Git
  Bash: `export PATH="$HOME/.cargo/bin:$PATH"`.
- Bevy 0.19: read the sources in `~/.cargo/registry/src/*/bevy_*-0.19.*`
  before using an API. Do not write Bevy APIs from memory.
- The first `client` build takes several minutes. Use `--profile play` for
  day-to-day runs; `release` uses thin LTO.
- Editing files with non-ASCII (`×`, `µ`, `—`) from Python: always pass
  `encoding='utf-8'`, or use the Edit tool.

Shared-target handoffs: Cargo was observed reusing an executable from another
worktree when that worktree's artifacts were newer than the current sources.
Under the exclusive Cargo slot, run `tools/clean-workspace-profile.ps1` with
the explicit profile and absolute target directory before building in a
different worktree.
It cleans every workspace package for that profile while retaining external
Bevy/Ruffle dependencies. Record the real build cwd and verify new behavior
in the immutable copied executable; a source HEAD receipt alone is insufficient.

Native Legacy observations and CI's userdata-preservation tests share
`tools/fidelity/with-native-slot.ps1`. Run owned native helpers through it with
`-Script <absolute helper> -NativeSlotGranted`; retain their process/prefs guards.
Never overlap native runs with an unwrapped CI fidelity-session check.

## Verify every change

- `cargo test --workspace`
- `cargo run --release -p launcher -- probe de_dust2` (expects `failures: 0`)
- `cargo run --release -p launcher -- sim de_dust2` (expects `failures: 0`)
- Visual changes: `csr play de_dust2 --screenshot shot.png --camera x y z pitch yaw`,
  then look at the image. Menu work: compare against `docs/reference/` at
  the same resolution (1920x1080 for the home screen).
- Audio: `cargo run -p launcher --example soundcheck` decodes every weapon
  sound (Bevy panics on undecodable audio).
- Fidelity against the real game (screenshot pairs, sound logs):
  `tools/fidelity/fidelity.ps1`, see [docs/FIDELITY.md](docs/FIDELITY.md).

Debug switches: `CSR_WATCH_BOT=n` (camera on bot n), `CSR_AUTOFIRE=1`,
`CSR_NO_PROPS`, `CSR_NO_HUD`, `CSR_NO_SELF` (hide own body from the noclip camera),
`CSR_NO_CULL=world|props|all`,
`CSR_PRESENT=immediate|mailbox|fifo`, `CSR_FRAME_LOG=ms` (log slow frames),
`CSR_TEST_WEAPON=weapon_x` with `CSR_TEST_RELOAD=frame`, `CSR_WEAPON_TRACE=1`.

## Player data and releases

- The launcher (`crates/launcher/src/setup.rs`) runs inside the data folder:
  the repository root for source builds, `%LocalAppData%\CSGO-Rewrite\userdata`
  for installed copies (outside the app folder that updates replace), or
  `userdata\` beside the exe when a `csgo-rewrite.portable` file sits next
  to it (the portable zip). Save paths are relative to it; never use
  `CARGO_MANIFEST_DIR` for runtime files. `CSR_DATA_DIR` overrides.
- Releases are built by `.github/workflows/release.yml` on a `v*` tag, using
  `tools/package-release.ps1` (`Package Release.cmd` runs it locally). It
  makes the portable zip and, with Velopack's `vpk`, the installer and the
  update packages. Installed copies update themselves from GitHub Releases
  (`crates/launcher/src/update.rs`); the Velopack pack id `CSGORewrite` must
  never change. `CSR_NO_UPDATE` disables the check; `CSR_UPDATE_URL` (a
  GitHub repo, URL or local `vpk` output folder) and
  `CSR_UPDATE_PRERELEASE=1` test a release before publishing it.
- "Add to Steam" (`crates/launcher/src/steam.rs`) edits every Steam
  account's `shortcuts.vdf`. Tests use a fake Steam folder via
  `CSR_STEAM_DIR`; never test against the real one without a backup.
- Artwork (icon, Steam library images, installer splash) is drawn in code
  (`crates/launcher/src/art.rs`, `csr art <folder>`); no image files.

CI runs on the owner's PC (self-hosted runner, real-data checks included): `docs/CI.md`.
Architecture: `docs/ARCHITECTURE.md`. Multi-game plans: `docs/STITCHING.md`.
