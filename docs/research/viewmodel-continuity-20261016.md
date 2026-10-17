# October 16 viewmodel continuity track

Status: implementation candidate; Rust, installed-data regressions and render
validation are pending root's coordinated build/native slots. This child ran
no Cargo/rustc, graphical or native game, soundcheck or shared-target mutation.
The frozen owner candidate, player preferences, maps and assets were untouched.

## Bounded causes and changes

AN16-01 has two concrete visibility gaps in the production system:

- Replacement weapon/arms entities were spawned with default visible meshes,
  then the system returned before applying joint transforms. They now start
  hidden and become visible on the next update with a sampled pose. This also
  covers switching to a dead spectator target.
- A missing selected player returned before hiding the cached instances. The
  cached meshes now hide, and playback is invalidated. If the same player and
  life return after a snapshot gap, the existing model instances restart Draw
  from a fresh pose. Ordinary death/respawn still uses the existing life epoch.

VM16-03/AN16-01 also expose an absolute-animation decoding/playback mismatch.
The decoder already fills compressed-out absolute tracks with the skeleton's
rest pose. `Clip::sample_layered`, however, excludes every `animated == false`
bone. Using it for a whole absolute viewmodel sequence retains those bones from
the preceding reload/fire/inspect pose, even when the sequence weight is 1.
This conflates omitted compressed tracks with an explicit zero-weight mask.

A viewmodel-local absolute sequence sampler now applies the decoded values
according to the sequence's actual weights. Explicit zero masks still preserve
their preceding pose. The existing inspection transition continues to supply
its fixed starting pose. The shared character/MDL sampler and existing delta
clip path are unchanged. Allocation count remains the prior one pose-vector
clone per sample; no new per-frame asset work or timing filter is introduced.

Read-only metadata was inspected directly from the installed Legacy VPK using
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

VM16-01 remains open for all-weapon look/run/walk motion. The new absolute-clip
regression checks 42 ordinary gun/knife/taser/grenade entries, including all six
grenades, but that is animation coverage, not evidence to generalize M4 motion
or its image landmark regions to other geometry/content.

## Root validation and native harness requirements

Only `git diff --check` and read-only source/installed metadata inspection were
run by this child. Root must run the coordinated package-clean build checks,
workspace tests and required release probe/sim, then these focused cases:

```text
cargo test -p client viewmodel::sequence_pose::tests
cargo test -p client viewmodel::life_tests:: -- --include-ignored --test-threads=1
```

The new synthetic tests distinguish absolute compressed rest tracks from masks,
verify full-weight pose independence at 30/144 samples, retain fractional masks,
and preserve the previous delta path. Installed tests check 42 loaded models'
absolute activities at start/intermediate/end; the production Galil system
interrupts its actual mid-reload pose with idle and all three fire variants,
without changing authoritative state. The visibility test covers initial spawn,
team/target switch, missing target, same-life target return, dead target and
return to the local owner. Existing respawn/inspect/knife regressions remain
required. These tests have not yet compiled or executed.

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
