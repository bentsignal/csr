# World skinned culling source candidate

Status: authored, **not compiled or executed**. Based on local
`perf/gameplay-diagnostics` commit `6d79cae4b800ef377e2ba23a664e18571f6c52fc`.
This implementation worktree did not run Cargo, rustc, games, UI, GPU work,
benchmarks, Steam/config writes, pushes or PRs. Root owns those operations.
Only source inspection, source/doc edits, git diff checks and local commits
were performed. No performance improvement is established.

The read-only input audit is
`C:/Users/shawn/csr-fidelity/post-playtest-20261016/skinned-culling-audit-r1.md`.
The current observation supplied with this task is approximately 149 FPS on
ordinary Nuke versus 153 FPS with **all** classic HUD disabled. This does not
isolate a rendering bottleneck or justify treating the HUD as a large gain.

## Implementation boundary

- `skinning_bounds::certify` checks the final emitted attributes, complete
  parallel vertex counts, triangle indices, all four shader joint lanes,
  nonnegative finite unit-sum weights (within four f32 epsilon), finite affine
  bind matrices, and absence of morph displacement. Every vertex must have a
  positive influence. The generated influencing-joint set must match exactly.
- It calls Bevy 0.19.1 `Mesh::generate_skinned_mesh_bounds`, before adding the
  render-only mesh. Only the generated bounds, joint count and scalar vertex
  magnitude are retained as certification metadata, shared across instances.
- Only `spawn_world` callers for character bodies, held world weapons and
  corpses opt in. `spawn`, `spawn_posed`, viewmodels, arms, decorations (including
  explicit world layer 0), menu previews and all other callers remain exempt.
  Eligibility does not depend on render layers.
- Eligible validated parts start with an explicit `Aabb`, `NoAutoAabb` and
  `NoFrustumCulling`. Invalid-at-conversion parts keep the old uncullable path.
- After transform propagation and `CalculateBounds`, the guard checks current
  assets, every joint/global/bind matrix (including zero-weight shader lanes),
  composed matrix finiteness, inverse conditioning and current bounds. It calls
  Bevy's public `entity_aabb_from_skinned_mesh_bounds` exactly once per valid
  part/frame. There is no custom skinning bounds algorithm and no second
  `DynamicSkinnedMeshBounds` calculation. The latter component is deliberately
  absent because its updater silently retains old bounds on failure.
- A fresh, finite padded box removes `NoFrustumCulling`; any failed lookup,
  malformed data, unsupported numeric envelope or calculation failure restores
  it in the same frame. A missing `Aabb` is repaired while remaining uncullable
  that frame. The explicit deferred flush precedes **both** camera and light
  visibility sets. Hidden/off-camera skins still update.
- Pose evaluation, joints, mesh transforms, sim, hitboxes, interpolation,
  ragdolls and lighting are untouched. Dynamic shadows are not enabled.

## Numerical enclosure

The built-in positive-influence joint boxes form a conservative convex enclosure
for nonnegative unit-sum blending. The remaining margin covers rounding and the
allowed small stored weight-sum error. It uses f64 absolute row-sum magnitudes
of the actual f32 matrices and the largest absolute emitted vertex coordinate.
Absolute magnitudes retain large terms lost to cancellation in a small final
box. The runtime budget includes world, inverse/entity-local and visibility
reconstruction magnitudes. The mesh matrix/inverse row-norm product must be at
most 16; inverse residuals are also checked. Singular transforms fall back.

The outward local-space margin is `512 * f32::EPSILON * work + 1/64 inch`.
The factor exceeds the dependent rounding count of matrix multiplication,
four-lane matrix blending, point multiplication, joint-box transformation,
world/local conversion and final center/half-extents reconstruction, including
the allowed four-epsilon weight-sum error and checked inverse residuals. The
absolute floor covers tiny arithmetic. `work` must be finite and at most 2^24;
the resulting box must be finite and enclose the unpadded result. Outside this
envelope culling stays disabled. This is deliberately generous: tighter
padding requires additional proof, not visual tuning.

Authored CPU fixtures compare both the shader's f32 matrix-blend order and
independent f64 point blending against the final box, including cancellation
near 2^17 inches. They do **not** establish target-GPU arithmetic acceptance
until root compiles/runs the candidate and inspects actual edge cases.

## Asset mutation contract

Geometry writes must use tracked `Assets<Mesh>` APIs and finish before
`AssetEventSystems`. All existing source geometry writers follow that path.
Modified/removed/replaced meshes trigger re-certification once per event;
unchanged poses do not rescan vertices. The retained built-in bounds are also
compared with the certificate each frame. A changed mesh whose CPU data has
already been extracted cannot re-certify and stays uncullable until a fresh
valid geometry replacement arrives. Bind matrices are validated every frame.

Review R3 identified edit-cache retention on normal mesh disposal: Bevy queues
`Unused` before `Removed`, so immediate cleanup was undone by the latter event,
including for unrelated meshes. The cache handler now applies all `Unused`
cleanup after the complete event batch. Explicit removal with live handles
retains its tombstone until replacement re-certification or last-handle disposal.
This narrow source fix and its real-asset lifetime fixture remain unexecuted.

Bevy's render extractor uses `get_mut_untracked` for `take_gpu_data`, which
retains the certified bounds and emits no modification event. That is the only
permitted untracked geometry mutation. A future untracked vertex edit, vertex
shader displacement, morph path, or new mesh writer after `AssetEventSystems`
must extend the proof/guard before using this eligibility API. There is no
claim of safety for silently bypassing asset change tracking.

## Root-owned admission work

Eight synthetic tests are authored in `skinning_bounds/tests.rs`, not run:

1. Final attribute coverage, malformed weights/indices and nonfinite rejection.
2. Blended CPU/skin-shader enclosure, Source axis conversion and inverse binds,
   merged long-weapon bones, ragdoll world poses and a nonidentity mesh parent.
3. Actual Bevy transform/visibility schedule: initialization, changed pose,
   teleport, off-camera and hidden-to-visible transitions, missing mesh/bind
   assets and joint restoration, nonfinite pose and same-frame consumer flags.
4. `RENDER_WORLD` GPU-data extraction retaining certification and bounds.
5. Runtime malformed geometry/binds and conservative missing-AABB repair.
6. Large cancellation, outward margin and unsupported numeric-envelope fallback.
7. Actual deferred Update spawn and explicit exemptions for generic/default,
   world-decoration, layer-2 and layer-3 callers.
8. Real `Assets<Mesh>` last-handle disposal/event order, bounded edit-cache
   lifetime across unrelated mesh churn and scene teardown, and explicit
   removal with live handles followed by malformed/valid tracked restoration.
   A test observer uses the production event-batch handler to inspect cache
   entries; the installed guard independently verifies fallback and recovery.

Root should first compile/run the focused tests, then the required workspace,
probe and sim checks. Use the exclusive Cargo slot and the documented explicit
profile/shared-target cleanup when moving between worktrees. Record the real
build cwd and verify an immutable copied executable, not only a source receipt.

Rendered admission must cover raised limbs, reloads, extended rifles, camera
rotation and frustum edges, teleports, death/ragdoll motion and attachment
visibility. Compare layer-2 gun/arms/decorations and layer-3 previews with the
baseline. Test GPU extraction/reappearance with genuinely hidden skins. Record
fallback rates and verify the numeric envelope admits ordinary installed data.

**Animated-shadow admission remains open.** The light-set fixture is a
scheduling sentinel, not a real off-camera caster/cascade shadow fixture.
Ordinary gameplay's disabled animated shadows cannot prove that case. Do not
enable dynamic shadows or expand shadow eligibility on this evidence.

Paired ordinary Nuke/Dust II runs must measure guard/validation CPU time,
visible skinned parts, joint upload work, draw/submission counts and
prepare/submit timings with matching camera/local-alive/bot state and stable
presentation. Include an ordinary long-session case. Animation and transform
propagation still run; the new serial matrix validation and conservative boxes
may cost more than rendering they save. Do not claim an FPS gain without that
evidence. No release/production acceptance follows from this source candidate.
