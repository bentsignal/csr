---
name: csr-playtest-delivery
description: Deliver CSGO Rewrite playable changes from WINSERVE to the owner's gaming PC through the installed launcher's Dev updates, or maintain the Stable and Dev release workflow.
---

# Deliver CSGO Rewrite playtests

Read `AGENTS.md` in the repository root first. The workflow is:
WINSERVE implementation and automation -> green PR and merge -> green master
CI -> published Dev update -> Shawn refreshes his installed launcher on his
gaming PC. Own the implementation, git and release operations; request owner
input for design direction and playtest feedback.

This skill is versioned with the repository and linked from `AGENTS.md` so
future agents can read it without this conversation or a tool-specific skill
installation. Paths below are relative to the repository root.

## Finish a playable batch

1. Implement on a branch on WINSERVE. Run focused checks while iterating and
   the `AGENTS.md` verification batch once for the combined playable changes.
   Keep local Cargo builds and tests from competing with the self-hosted CI
   runner. Keep automated game/audio tests off Shawn's gaming PC.
2. Push and open a PR; fix failing CI, then squash-merge and pull `master`.
   `.github/workflows/dev-release.yml` publishes automatically after successful
   **master push CI**. Passing PR checks alone does not publish a Dev build.
3. Follow the master CI and Dev publishing runs through completion. Confirm
   the Dev release is public (not a draft), is a prerelease, and its tag resolves
   to current master. Dev versions are generated from the next patch plus the
   successful CI run number and attempt; do not guess a version or reuse one
   from this skill's history. Superseded commits can be skipped intentionally;
   follow the newest master's run instead.
4. Check that the installer, portable zip, full update package and
   `releases.win.json` feed are uploaded for that version, with a delta when
   packaging produced one. Confirm the feed references the intended version.
   Release notes identify the source SHA; verify the tag's resolved commit as
   well. The launcher's build label shows the executable's source revision.
   Check the installer link resolves. When downloading an artifact, compare
   its checksum with GitHub's asset digest. Fix a failed publish before saying
   the playable batch is ready; never offer an older release as a substitute.
5. Hand back a short player-facing message: changes, relevant checks, exact
   published version/source revision, merged PR/release links, and what Shawn
   should test. For an installed copy, say **Dev -> Refresh -> Update & play**.
   For first install, provide that verified Dev release's installer link and
   tell him to select Dev once; a fresh install defaults to Stable even when
   its installer came from a Dev release. Do not require terminals, file copying,
   builds on his PC, or repeated installer downloads.

Useful inspection commands (run from the checkout; use actual run IDs and tags):

```powershell
gh api repos/bentsignal/csr/commits/master --jq .sha
gh run list --workflow ci.yml --branch master --limit 5
gh run list --workflow dev-release.yml --limit 5
gh release list --limit 10
```

For a selected release, inspect `gh release view` for assets/draft/prerelease
state and `gh api repos/bentsignal/csr/commits/<tag> --jq .sha` for the resolved
commit. If the environment supplies PR linking/watching tools, link the PR and
use its watcher while PR checks are pending. Monitor the separate master CI
and release workflows after merge; a merged PR does not prove publishing
succeeded. Report an external blocker honestly if it prevents completion.

## Preserve the two tracks

- **Dev is Shawn's normal playtest path:** latest published, tested master.
  Check for a newer master before handoff; follow its publish when superseded.
- **Stable is the default for friends:** deliberately bump workspace version,
  merge green, then tag `v<version>` on master as `AGENTS.md` requires.
  Routine Dev delivery needs no extra Stable release. Dev prereleases must
  not replace GitHub's latest Stable download.
- Installer identity `CSGORewrite` and saved data must survive updates and
  track changes. Preserve `%LocalAppData%\CSGO-Rewrite\userdata`, including
  settings and private packs. Source/portable copies do not self-update.
- Check real launcher download/apply behavior when changing the updater or
  publishing mechanics. Use isolated test data; do not reset the owner's
  installation to prove an update. Normal gameplay batches do not need a new
  updater test framework or exhaustive repeat of updater tests.
- Documentation-only work does not need a player playtest, local game build,
  manual package, or new Stable tag. Normal automated workflows may still run.

Read `docs/CI.md` for runner/build and publishing details, `docs/PLAYERS.md`
for the player flow, and `docs/OWNER-PRIORITIES.md` for active and paused work.
Continue honoring paused scopes; this delivery workflow does not resume them.
