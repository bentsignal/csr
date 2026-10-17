# Minimal Bevy change guards (2026-10-17)

Status: local implementation for root review and centralized validation. Base
`af8f046`; the three production files also match `c33ac15`. Root's active
release/diagnostic candidate is `cc66e5e`. No compile, test or render result is
claimed for these changes.

## Scope and parked candidate

`56ce78fe4b4ceaf2334fd722eca940fb76e6694d` remains PARKED, unaltered, on
`t3/bevy-change-detection-guards` in its existing worktree. Do not cherry-pick
it into this series. Its changed-map/new-batch/no-PVS-flag refresh behavior
and stationary-frame batch scan are separate scope. This series does not
claim new-batch, replaced-map, changed-membership or flag lifecycle correctness.

The local commits separate each guard so root can isolate their effects:

1. `d17c314` PVS visibility: only replace the original assignment with `set_if_neq`.
   Preserve `Query<(&MapClusters, &mut Visibility)>`, `Local<Option<i16>>`,
   `level.is_added()`, the cached cluster, early return, PVS decoding and flag
   sampling exactly. Unchanged-cluster frames retain the original early return
   without a batch scan. Equal results on cluster refresh frames stop marking
   `Visibility` changed; actual visible/hidden decisions remain identical.
2. `4f227b9` death mesh visibility: only replace the `update` loop's assignment with
   `set_if_neq(Visibility::Inherited)`. Initial hidden-to-inherited transitions
   remain writes. Pose readiness, corpse cooking/advancement, body limits,
   lifetime, animation, meshes and material ownership are unchanged.
3. `43c899c` joint transforms: perform the same matrix composition and
   `Transform::from_matrix` decomposition, then compare all ten output floats
   by `to_bits`. Skip only bit-identical writes. Preserve ULP changes and
   signed-zero restoration; no epsilon, sampling or update-rate changes.

No metadata, logger, geometry, animation, culling decision or rate changes
are included. The transform comparison adds work that remains unmeasured.
Each code commit touches only its named source file and can be selected
independently from the others. Apply these new commits rather than `56ce78f`:

```text
git cherry-pick d17c314 4f227b9 43c899c
```

## Fixtures and source inspection

Fixtures call the production functions through real Bevy apps, using synthetic
in-memory content and no renderer/window or installed content:

- PVS: existing batches settle, then cross a cluster boundary. Only batches
  whose enum changes get change ticks; both-cluster, empty and unknown-cluster
  batches stay visible without ticks. Check inherited visibility on a child.
- Death bodies: call the actual `death_bodies::update` with a synthetic body
  held in the existing pending-cook branch. Consume initial insertion ticks
  before revealing its hidden mesh, then check repeated settled writes and
  inherited visibility. No production test constructor/helper was added. This
  is a visibility-write fixture, not evidence about corpse physics or lifecycle.
- Transforms: production `apply_pose` plus Bevy's `TransformPlugin`; verify
  local float bits against the unchanged spawn-time `pose_transforms`, local
  and global change ticks, ULP translation, world placement, child propagation,
  signed zero, missing/new joints and an unposed extra joint. Independent
  rotation-only and scale-only tests assert the other fields' bits stay fixed.

Read installed Bevy 0.19.1 sources for change-detection traits and mutable
dereferencing, visibility components/plugin/propagation, transform components,
plugin/systems, app/world APIs, and task pool initialization/spawning. Here,
only source inspection, static source-equality assertions and `git diff --check`
were performed. No Cargo,
build/rustc, native-game/UI, shared-target, owner Steam/config writes, push or
PR operations were performed. Fixtures have not been executed.

## Root validation still pending

1. Review/apply only the new minimal commits; keep visibility and transform
   candidates independently selectable. Run the `real_bevy_` client fixtures
   (with `CSR_NO_PVS` absent), formatting checks, workspace tests, release probe
   and sim checks under root's exclusive Cargo slot. Fix fixture/API failures
   before treating the candidate as validated.
2. Use the required shared-target clean/handoff procedure when switching build
   worktrees; record the actual build cwd and copy an immutable executable.
3. Compare baseline and candidate rendered PVS crossings, poses and death-body
   visibility at matched settings. No rendered fidelity evidence exists here.
4. Pair baseline, visibility-only, transform-only and combined runs under the
   same ordinary sustained Nuke/five-bot/2560x1440 conditions. Measure guard
   comparison cost and frame behavior before accepting an improvement.

The supplied `0e9` observation (early/mid/late cadence 143/134/139, render
prepare 2.3 ms, submit 2.6 ms, bounded corpses and stable meshes) does not prove
any benefit from these guards or attribute a GPU/CPU bottleneck. PF01/PF02
remain pending root's evidence; no roadmap completion is claimed.
