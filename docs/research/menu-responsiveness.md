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

## Worker deadlines

The pinned public Ruffle revision (`5b20c6507c51657dabdad25e172cc81c8587d898`)
exposes `Player::time_til_next_frame`, which includes the next ActionScript timer
deadline. `Player::tick` also performs display-list mouse picking even when no
timeline frame runs, through its stream-manager update. The host previously
called it on every worker turn, including idle turns separated by only 4 ms.

The worker now accumulates elapsed time from the last actual tick and honors
that deadline. Input handling and rendering do not wait for the deadline:
pointer input updates Flash hover immediately, and dirty frames can publish on
the same turn. Loads continue pumping between ticks. Input alone never resets
the elapsed clock, so continuous motion cannot starve frames or timers. Inactive
movies retain the previous paused-clock policy.

Adjacent pointer positions were already coalesced. Every click, key, focus,
release and lifecycle command retains its existing FIFO boundary. No Ruffle
hit-testing semantics, ActionScript method caches, menu movies, or content are
modified here. In particular, `call_internal_interface` does not itself perform
the additional mouse pick; native property calls are not the claimed hot path.

These changes address concrete presentation and redundant scheduling behavior.
They do not establish exact original-game latency or prove that all delayed
hover animation is eliminated. Rendered fullscreen comparisons and normal
Pause/Options/reopen checks remain necessary.
