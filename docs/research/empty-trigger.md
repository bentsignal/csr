# Fully depleted trigger feedback

The rewrite originally ignored primary fire with an empty magazine and no
reserve. Observed content policies restore depleted-trigger feedback through
authoritative simulation events and runtime soundscript lookup.
An empty magazine with reserve is a separate reload case, covered by the
automatic-fire lifecycle work; it must not inherit an extra invented click.

## Clean-room observation

An owned CS:GO Legacy 1.38.8.1 session on 2026-10-15 selected the AK-47 on
the Terrorist team. Its actual rendered HUD started at 30/90. Finite ordinary
primary-input pulses emitted exactly 120 identified AK near-shot waves.
The selected AK then rendered 0/0 before and after both subsequent cases:

| Input | Actual console hold | Observed empty cues | Normal AK shots |
|---|---:|---:|---:|
| Short press, release | 0.191 seconds | 1 | 0 |
| Hold, release | 1.525 seconds | 8 | 0 |

The cue was `weapons/clipempty_rifle.wav`, emitted as a dynamic weapon-channel
sound. Its sound source differed from the player-source near-shot entries.
The initial capture helper incorrectly filtered empty cues to the near-shot
source number, so its raw cue arrays are empty. Those receipts remain
unchanged; a separate review uses the actual raw dynamic-wave entries and
the rendered AK identity/ammo. No second game run was needed to correct the
analysis.

The held case's engine sound-time gaps were 0.188, 0.207, 0.205, 0.208,
0.203, 0.204 and 0.204 seconds: a mean of 0.2027 seconds. This establishes
repeated feedback at approximately 0.2 seconds. These are sound-log and
input-receipt observations, not exact server attack deadlines. They do not
establish rapid re-press behavior or another weapon's empty-trigger policy.

The installed soundscript declares `Default.ClipEmpty_Rifle` for that wave.
The installed AK item definition does not explicitly declare `sound_empty`.
The wave observation therefore supports an AK-specific binding; existence of
the rifle/pistol soundscript entries alone does not prove defaults for all
weapons or content packs. No asset bytes or native implementation are copied
into this repository.

## Implementation boundary

Gameplay admission and repeat scheduling belong to `sim`, driven by
`TickInput`. A distinct appended event must carry the firing weapon and
origin so the client resolves the runtime sound without polling render-frame
mouse input or using a later active weapon. Empty feedback consumes no ammo
and produces no live-fire event, tracer, damage, or guessed attack animation.

The content definition must opt into observed feedback. Unsupported content
keeps its existing behavior rather than inheriting a guessed rifle default.
The new authoritative event requires a protocol compatibility bump, including
payload validation and rejection of older peers.

The implementation uses a 0.2-second interval as a candidate inferred from
that aggregate. It shares the weapon's existing attack deadline, including
deployment/reload/frozen/muzzle admission. It does not change ammunition,
shot counts or recoil impulses. The initial policy attached only to the exact observed installed AK;
its identity contributes to the catalog digest independently of recoil and
animation imports. Protocol23 rejects older peers.

Other content may explicitly provide `WeaponDefinition.empty_trigger`.
The item-schema importer also accepts both `visuals/sound_empty` and the
engine extension attribute `empty trigger interval`; neither field alone
invents a default. Invalid or absent policies retain existing behavior.

The initial AK runtime path was verified in PR #140; the independently
added M4 runtime checks are recorded below. The native session finished
successfully, restored all 12 preference entries exactly, and left no game
process. The root reviewer independently viewed the loaded/depleted AK and
held-case before/after images, including the dynamic empty-wave overlay.
Physical mouse input was not disabled. Native evidence is private; only the
derived observations above are committed.


## Independently observed M4A4 depletion

A separate owned Legacy 1.38.8.1 session identified the actual M4A4 model and
30/90 HUD, consumed exactly 120 local M4A4 near-shot waves, and showed the
same weapon at 0/0 before dry input. Depletion used acknowledged timescale 4
with finite ordinary attack/release inputs and normal released-trigger
reloads. Timescale was restored and positively reported as 1 before both
empty-trigger cases. Accelerated depletion is not dry-cadence evidence.

A short press held for 0.1916 console-receipt seconds emitted one dynamic
`weapons/clipempty_rifle.wav` cue. A held press lasting 1.6585 receipt seconds
emitted nine cues with engine sound-time gaps 0.204, 0.203, 0.205, 0.192,
0.206, 0.203, 0.205 and 0.204 seconds (mean 0.20275). Neither case emitted
a local live-shot wave. Actual before/after images retained M4A4 at 0/0.
The 0.2-second policy is inferred from this aggregate; it is not an exact
native attack deadline or independently established 2015 behavior.

Only the matching installed schema and canonical M4A4 identity/statistics
opt in. Explicit pack policies are preserved. AK policy identity and its
existing simulation/audio behavior are retained; the accumulated catalog
digest includes each independently attached observation. No new protocol
event or guessed dry-fire animation is introduced.

The same session attempted an M4A1-S afterward. Ordinary `give`/`use` did
not replace the depleted M4A4: its actual model/HUD remained M4A4 at 0/0,
and no M4A1-S near-shot wave was observed. That case is rejected, not evidence
for another weapon. The original failing receipt and all raw captures stay
private. All 12 native preference entries restored exactly and no native
process remained. Rust and rendered verification is recorded below.


## Independent M4A1-S retry

A fresh CT session started with a free primary slot and positively admitted
the M4A1-S model/HUD at 20/80, then 0/0 after exactly 100 local suppressed
near-shot waves. The same accelerated-depletion/normal-speed-dry boundary
was acknowledged. Attached short input emitted one empty cue during a
0.1909-second receipt interval. Attached held input emitted nine during
1.6947 seconds, with a mean engine sound-time gap 0.202375 seconds.

A secondary press began removing the suppressor. The capture three seconds
later still showed that animation in progress; the immediately following
0.1982-second short primary press emitted no empty cue. That negative is
retained as a readiness boundary, not claimed as a ready detached tap.
The later detached held case emitted eight rifle-empty cues during 1.5197
receipt seconds, mean sound-time gap 0.202857 seconds. Its final actual
image shows the bare muzzle and 0/0. No dry case emitted a local live-shot
wave. Existing suppressor-transition and attack-readiness guards remain.

The independent rifle policy uses the observed 0.2-second aggregate in
both suppressor states without changing shot/recoil behavior or inventing
an exact transition duration. All 12 preference entries restored exactly;
the owned game exited and no native game remained. A prior retry safely
failed preflight on a TIME_WAIT NetCon port before launching anything;
that receipt is preserved separately from this successful new-port run.
Other guns, rapid re-press behavior, detached ready taps and historical
2015 parity remain unestablished.


## Rewritten-client verification

Code head `6019a49` (crates tree `15e44dd77fcd081cb4b1aa37e7969a761ec69b4d`)
passed full workspace/real-data CI. Under the exclusive local Cargo slot,
all 23 workspace packages were cleaned for play; focused level, sim and
client checks passed. The soundcheck decoded all 1,425 weapon/feedback/foley
sounds without unsupported/crashing entries. Play-profile probe and sim both
reported zero failures. The immutable copied executable contains the unique
M4A1-S policy reference and has a recorded source/cwd/hash receipt. Combined
release verification is separate; no individual release pass is claimed here.

Three private-profile clients used ordinary screenshot-only `TickInput`
cycles to exhaust actual starting ammunition, then send a dedicated short
and held depleted press. They exited normally, emitted no ERROR/panic, and
kept the owner's config unchanged. Actual authoritative logs admit AK 120
shots, M4A4 120 and M4A1-S 100, each ending at 0/0. Each dedicated tap/held
segment produced nine authoritative events and nine decoded original empty
cues. Faster/shorter magazines also reached 0/0 before the final scheduled
hold ended: M4A4 emitted two extra depleted held cues, M4A1-S five. Those are
separate from the nine dedicated cues, not additional bullets or reload clicks.

The inherited private receipt helper first expected the outdated attack bit
1, while current `movement::buttons::ATTACK` is 8 and actual inputs logged 8.
Its later total-only AK count also rejected the legitimate extra M4A4 cues.
Both original negative receipts remain untouched. Separate posthoc admissions
check the actual raw input, ammunition, event/cue segments and successful
process exits. The final M4A1-S helper validates the dedicated segment and
full-stream event/audio agreement directly. No source behavior was changed
to make these receipt expectations pass, and valid cases were not rerun.

Both the implementing and coordinating agents independently viewed all three
actual 1920x1080 screenshots: correct AK/M4A4/attached-M4A1-S identity, intact
hands and visible 0/0 ammunition. This establishes empty-state integrity and
the bounded event/runtime decode path, not complete pose, audio mix, native
cadence at exact ticks, detached-ready-tap or historical fidelity. Raw captures,
logs and account-bearing receipts remain private.
