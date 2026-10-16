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
Outgoing fade policy, immediate stab-to-inspect admission, R/F cancellation,
rare inspect selection and continuous karambit loops remain open. Gameplay
attack readiness and the current inspect eligibility checks are unchanged.

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
input timing or native interpolation parity. Required combined RELEASE
verification and final CI remain pending.
