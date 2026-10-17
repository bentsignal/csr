# October 16 viewmodel continuity track

Status: implementation candidate; Rust, installed-data regressions and render
validation are pending root's coordinated build/native slots. This child ran
no Cargo/rustc, graphical or native game, soundcheck or shared-target mutation.
The frozen owner candidate, player preferences, maps and assets were untouched.

## Bounded causes and changes

AN16-01 investigation identified visibility paths in the production system;
these source changes are candidates, not verified fixes for the owner's report:

- Replacement weapon/arms entities were spawned with default visible meshes,
  then the system returned before applying joint transforms. They now start
  hidden and become visible on the next update with a sampled pose. This also
  covers the replacement frame when switching directly to an already dead
  spectator target; a pending fixture now exercises that switch explicitly.
- A missing selected player returned before hiding the cached instances. The
  cached meshes now hide, and playback is invalidated. If the same player and
  life return after a snapshot gap, the existing model instances restart Draw
  from a fresh pose. Ordinary death/respawn still uses the existing life epoch.
- Cached instances were unhidden before clip selection. A retained model with
  no selected or idle clips could return without applying any joint transforms.
  Reveal now follows gun and merged-arm joint updates. Empty activity lists and
  selected clips with no frames also leave the meshes hidden. The pending
  installed fixture retains an actual Galil asset, removes its clips, checks
  hidden identity joints across repeated updates, and restores its clips to
  check that gun and arm transforms change before the meshes become visible.

The absolute-animation change makes whole-sequence playback apply the decoded
pose representation. The decoder fills compressed-out absolute tracks with the
skeleton's rest pose. `Clip::sample_layered`, however, excludes every `animated == false`
bone. Using it for a whole absolute viewmodel sequence retains those bones from
the preceding reload/fire/inspect pose, even when the sequence weight is 1.
The candidate distinguishes these decoded values from an explicit zero-weight
mask. This is a representation-consistency choice; native sequence blending and
bone-preservation policy have not been independently established here.

A viewmodel-local absolute sequence sampler now applies the decoded values
according to the sequence's actual weights. Explicit zero masks still preserve
their preceding pose. The existing inspection transition continues to supply
its fixed starting pose. The shared character/MDL sampler and existing delta
clip path are unchanged. Allocation count remains the prior one pose-vector
clone per sample; no new per-frame allocation, asset work or timing filter is
introduced by these changes. The reveal follow-up adds a visibility-only hide
pass over the existing cached mesh list before possible early returns.

This also changes include-model retargeting: `mdl::anim::retarget` supplies the
target rest pose for target-only bones and marks them unanimated, while
`retarget_weights` defaults those bones to weight 1. Absolute sampling now
applies those values too. The synthetic include-model fixture explicitly checks
that reset, a mapped zero-weight bone's preserved pose, and numeric world-space
transforms after real gun/merged-arm ECS joint updates. It exercises the existing
retarget functions with authored synthetic data, not binary include-model
loading or observed native fidelity. Whether native playback instead preserves
any such target-only bone remains a validation question.

The original implementation recorded read-only metadata inspected directly
from the installed Legacy VPK using
the repository's documented VPK/MDL field layouts, without extraction to the
repo or inspection of implementation sources:

| Model | SHA1 of installed MDL | Relevant declaration |
| --- | --- | --- |
| `v_rif_galilar.mdl` | `00898e1962d42092e904a59bacd3fd6d1550b778` | 59 bones; absolute idle/reload/draw and three shoot clips; every sequence bone weight is 1 |
| `v_rif_m4a1.mdl` | `f892a6e9bff09f3ac2a621da7a6c0328133ce94e` | 57 bones; absolute idle/reload/draw and three shoot clips; every sequence bone weight is 1 |

The Galil idle omits the right-hand track; all three shoot clips omit magazine,
latch and multiple finger tracks. Its firing clips are 25 frames at 30 FPS,
without animation sections. The observed flags establish the compressed-track
case; they do not prove that it explains every reported firing glitch. Native
firing onset/retrigger transition behavior remains to be compared before adding
any new blend law. No authored fade constant was repurposed as a guessed fix.

## Motion admission remains bounded

The #150 finite-history look response and #157 speed-settling coefficients,
raw schema gate, canonical M4 identity, camera/aim composition and grounded
admission are unchanged. There is no new coefficient or all-weapon motion claim.

VM16-02 has an identifiable source discontinuity: `Settling::sample` returns
identity as soon as `on_ground` becomes false, whereas a forward ground run
returns the nonzero measured pose. The reverse occurs on landing. That alone
can cause a pose jump. The magnitude and any additional animation/camera cause
must be measured; the owner's front/center teleport is not declared resolved.
Dropping the ground gate, holding the last pose through air, or adding easing
would each invent an unobserved motion policy, so none is admitted here.

VM16-01 remains open for all-weapon look/run/walk motion. The pending
absolute-clip regression enumerates 42 ordinary gun/knife/taser/grenade entries,
including all six grenades. It compares against the same `Clip::sample` used by
the candidate and establishes only whether full-weight output clears an
arbitrary prior pose to that decoded representation. It cannot independently
validate decoding, native blending or motion, and is not evidence to generalize
M4 motion or its image landmark regions to other geometry/content.

## Root validation and native harness requirements

The original pass recorded read-only source/installed metadata inspection.
The review follow-up ran only source inspection, `git diff --check` and local
commits; it did not repeat that metadata measurement. Root must run the coordinated package-clean build checks,
workspace tests and required release probe/sim, then these focused cases:

```text
cargo test -p client viewmodel::sequence_pose::tests
cargo test -p client viewmodel::life_tests:: -- --include-ignored --test-threads=1
```

Pending synthetic tests distinguish decoded absolute rest values from masks,
retain fractional masks, and preserve the previous delta path. The original
constant-frame 30/144 test was too narrow to support a timing claim; its
replacement samples a synthetic varying translation/rotation curve at both
rates, common off-grid timestamps, a backwards seek and an end clamp, with
independent piecewise numeric expectations. It tests the sampler, not native
motion. The retarget fixture described above checks missing target bones,
zero masks and actual merged-arm joint transforms.

Pending installed tests compare 42 models' decoded absolute activities at
start/intermediate/end to the same sampler; the production Galil test interrupts
its decoded mid-reload pose with idle and all three fire variants, without
changing authoritative state. These do not independently establish decoder
correctness or native animation fidelity. The visibility fixture covers initial
spawn, team/target switch, missing target, same-life target return, death of the
selected target, a direct switch to an already dead target, and return to the
local owner. It now reads actual gun and arm joint transforms, including their
unchanged hidden identity transforms on the direct dead switch and their fresh
baseline transforms on return. The no-clips fixture checks missing activities,
empty activity lists, a selected frameless clip, and recovery when clips return.
Existing respawn/inspect/knife regressions remain required. None of these tests
have been compiled or executed by this child.

For private visual acceptance, compare immutable root-built executable captures
against the accepted grounded M4 baseline, then Galil taps/held fire and reload
interruptions. Include all six grenade pin/hold/release poses: their explicit
weight masks must retain the intended pin/part state. Inspect complete frames,
hands, muzzle/wick attachments and reload/inspect transitions, not only logs.

For native VM16-01/02 expansion, root must use the native-slot wrapper and the
existing prefs/process guards. Require positively identified Legacy weapon,
schema/model and actual viewmodel FOV/offsets, local owner/life, view angles,
horizontal/vertical velocity and ground/duck state. Capture matching baseline,
ground run/walk, takeoff, apex, precontact and settled landing with actual frame
times; do not infer timing from command receipt. Retain the established M4
landmarks only for its matching geometry; each additional weapon/grenade needs
its own visible landmark/attachment admission before reusing a motion fit.
Death-during-reload is one case, not the presumed cause: also cover idle/fire/
inspect/grenade death, target changes during reload, snapshot absence and respawn
with both unchanged and changed weapon/team. No native helper was launched here.
