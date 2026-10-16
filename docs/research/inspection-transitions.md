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

Verification is pending. Two narrow regression cases exercise repeated
interrupt continuity and independence from intermediate render frames,
including attack cancellation. Required workspace, real-data, audio and
actual render verification still apply before merge.
