# Inspection transition continuity

Owner item WP07 reports hard restarts when repeatedly inspecting or moving
between knife attacks and inspection. The viewmodel loader retained the
animation frames and bone weights but discarded the MDL sequence's parsed
`fade_in` and `fade_out` fields. Each accepted inspect request immediately
sampled the new clip over the previous pose.

Read-only installed Legacy 1.38.8.1 data on 2026-10-16 shows `lookat01` with
`fade_in` and `fade_out` both approximately 0.3 seconds in the CT/T default,
bayonet, flip, gut, M9 and karambit animation models. These are authored
sequence fields, separate from animation duration. Included animation models
use the existing bone-name retargeting path. The private observation receipt
is `knife-interruption-20261016/authored-knife-sequence-metadata.json`; no model
bytes are included here.

The first correction retains the incoming authored fade-in and uses the
already displayed bone-local pose as the source of an inspect transition.
Repeated inspection captures the partially blended pose that was actually
visible. The interpolation depends on elapsed time from the request, rather
than recursively blending against each rendered frame, so render frequency
does not change the transition. Missing, nonfinite or nonpositive fade-in
keeps direct sampling. Life changes clear the retained pose, and leaving
inspection clears the transition. Grenade hold and suppressor transitions
retain their existing playback paths.

This restores inspection continuity with authored durations. The linear
interpolation policy is not an independently measured native easing law.
The subsequent [#174 knife-input correction](knife-inspect-input.md) adds
bounded stab-to-inspect admission and rebound reload cancellation without
changing gameplay attack readiness. Outgoing fade policy, native repeated
R/F timing, rare inspect selection and continuous karambit loops remain open.

Final source `f423376` passed the orchestrator's complete DEV and PLAY checks
in frozen union `7746327eec9f891996cb95a69ab7860489352b2b`, crates tree
`c65cece5a3246fe6fc0b7365cb8b7ea7e77be0ad`. Two narrow regression cases exercise
repeated interrupt continuity and independence from intermediate render
frames through the actual fractionally masked clip sampler, including attack
cancellation. The earlier DEV run's exact zero-weight quaternion one-ULP
failure is preserved; the production endpoint now copies the source pose
directly, without weakening the continuity assertion.

The sealed PLAY executable SHA-256 is
`270AC4BBE11D87E914D6831585E55652A0503D664084995A4AB1CF54C1C55DA8`.
Two ordinary `CSR_INSPECT` captures used its existing InspectRequest path and
selected the actual default CT knife and AK-47. Both processes exited zero,
logged the requested INSPECT activity and zero shots, and produced fresh
1600x900 images. Original images were personally inspected for intact weapon
and hand geometry: `render-7746327-ct-knife-inspect/shot.png` and
`render-7746327-ak47-inspect/shot.png` under the private
`knife-interruption-20261016` packet. The knife authority time was 1.63281
seconds; the AK time was 1.60938 seconds with 30 rounds remaining. Separate
visual review receipts preserve the raw capture flags. Private configurations
were fresh, and exact artifact/process guards, a finite per-child bound and
the canonical native mutex were used; no owner userdata was changed.

These images admit model integrity during ordinary inspection, not repeated
input timing or native interpolation parity. The [final combined RELEASE
verification and CI](overnight-fidelity-verification.md) passed; no separate
feature-branch RELEASE build is claimed.

## Inspect interrupted by a magazine reload

The inspection-entry and knife-cancel transitions above did not cover a gun
reload interrupting a displayed inspect pose: entering `ACT_VM_RELOAD` cleared
the transition source and snapped to the reload's first sampled pose. Installed
AK `ak47_reload` and M4A1-S `reload` sequences both declare a 0.2-second incoming
fade. Their header SHA1 identities are respectively
`c63dfcc6acaafeb4c5e5b5369b48886cb7dc6959` and
`c1d36194ca0c483c310fd130552e8950fb796ff9`.

An incoming magazine reload now captures the displayed inspection pose once,
then uses that reload sequence's own fade-in and fixed masked sample source.
Fire interruptions remain immediate. Shotgun stages, grenades, suppressor
transitions and new-life resets retain their existing boundaries. This changes
presentation only; normal TickInput still starts and controls the actual reload.

One installed production-system regression starts inspection, sends a real
TickInput reload, checks its exact first displayed pose and subsequent fade
progress, and verifies that rendering changes no authoritative reload/ammo
state. This requires installed Legacy but no GPU. Compilation and rendered
verification are pending coordinated checks; exact native interruption easing,
rapid repeated input, general reload timing and historical parity remain open.
