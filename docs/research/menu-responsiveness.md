# Menu response and Pause presentation

The October 13 owner playtest reported a cached logo/menu flashing, disappearing,
and reappearing on Escape, followed by late hover animations.

## Pause transition

The frontend previously sent each root hide, the Pause root show, and its
`showPanel` call as separate worker inputs. The worker's finite input budget
allows a tick and frame publication between inputs. Showing the Bevy image
immediately also exposes the raster retained from the previous open cycle.

Pause now sends one `Present` input containing the entire visibility transition.
It cannot publish between these commands. Frames carry the completed transition
revision, and the frontend waits for the current revision before revealing its
image. A frame from the preceding open cannot satisfy a close/reopen cycle.
The authored `showPanel` animation still plays; it is not replaced or skipped.

## Pointer queue pressure

`CSR_UI_WORKER_TRACE=1` now records each consumed pointer's queue age and the
remaining queue length. Coalescing updates that timestamp along with the latest
position. This distinguishes stale queued positions from expensive processing
or a slow authored animation; an Inspect acknowledgement does not measure that.

`CSR_TEST_PAUSE_HOVER=1` extends the existing opt-in bot play proof with five
seconds of continuous pointer movement over Pause's options at the game update
cadence. It uses the production frontend input bridge and the Bevy window's
cursor state, without moving the OS pointer. Normal gameplay never runs it.

Adjacent pointer positions were already coalesced. Pause's passive visibility
query formerly interrupted that coalescing. It now has an explicit observation
input: newer motion can replace the preceding position across this query alone,
so the query sees current hover state rather than retaining obsolete positions.
Every authored call, click, key, focus, release and lifecycle command retains its
existing FIFO boundary. No Ruffle
hit-testing semantics, ActionScript method caches, menu movies, or content are
modified here. In particular, `call_internal_interface` does not itself perform
the additional mouse pick; native property calls are not the claimed hot path.

An attempted frame/timer-deadline scheduler did not improve the first paired
fullscreen comparison (both `play` profile at 2560x1440, CI activity recorded):
baseline 464 presentations/20s versus candidate 435, input receipt p95 50.5ms
versus 49.0ms. It was removed. No menu performance improvement is claimed from
that experiment. The atomic Pause correction remains separate from unresolved
hover latency and original-game timing fidelity.

The first five-second Pause pressure run with the observation still acting as a
barrier consumed 55 pointer positions: queue age p95 167.2ms, maximum 208.0ms,
maximum five pending inputs. Only positive-coordinate pressure positions were
included; inactive/startup ownership-reset positions were excluded. Other CPU
build activity was recorded. The coalescing correction requires a follow-up
pressure comparison; these numbers are not a claim of one-second FIFO backlog.
