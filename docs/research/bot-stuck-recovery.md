# Failed bot crossings

The owner reported bots repeatedly jumping into the Dust II B-window box and
corners (BO01). The existing two-second stuck handler discarded a route, but
the next tick could rebuild the identical deterministic shortest path to the
same objective. Its regression stopped at the cleared route and did not check
that next replan. Large sideways escape motion also counted as progress even
when the bot never approached its current portal.

Bot brains now retain up to eight recently failed directed crossings for an
eight-second retry window. The existing two-second lack-of-progress trigger
records only the attempted connected current-to-next edge. A* excludes that
crossing while preserving tactical area costs; another route may still enter
the same destination. If no alternate path exists, the existing wander
fallback remains available. Expiry permits retrying temporary obstructions;
round reset, death and respawn discard the old brain or its failed crossings.
Navigation interruptions, such as combat, reset the progress baseline rather
than treating the time spent elsewhere as failed crossing attempts.

Progress toward the same portal requires approaching it by the existing
48-unit threshold, rather than sideways displacement. Advancing a route area
resets the baseline; completed grounded height changes retain the prior
recovery behavior. A same-area exact target never fabricates a self-edge.
Human movement, animation, networking and render-frame gameplay are unchanged.

Two synthetic regressions cover the next replan after jump/sideways
oscillation, a valid alternate approach, directed-edge scope, a disconnected
fallback, expiry and same-area handling at 64 and 128 Hz. Compilation and
actual bot-motion rendering are pending. This is bounded recovery policy for
this implementation, not a measurement or claim of original CS:GO bot AI
parity. The reported B-window case and any independent airborne animation or
interpolation defect remain unverified until actual rendered checks.
