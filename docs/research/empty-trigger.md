# Fully depleted trigger feedback

The current rewrite silently ignores primary fire with an empty magazine and
no reserve. This work restores the installed game's dry-trigger feedback
through authoritative simulation events and runtime soundscript lookup.
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

Implementation and rewrite runtime verification are pending. The native
session finished successfully, restored all 12 preference entries exactly,
and left no game process. Physical mouse input was not disabled. Native
evidence is private; only the derived observations above are committed.
