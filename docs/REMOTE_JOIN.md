# Rendered remote joining

The frontend owns one `RemoteClient`, one bounded map worker and one render
preparation generation. `JoinRequest` selects an already resolved endpoint;
the network driver discovers its validated `ServerInfo` before the loader
resolves installed local content. The existing local map resolver checks the
exact map/catalog manifest and creates a map-only world once. No advertised
filesystem path, URL, content download or offline fallback participates.

Remote loading shares the offline loader's single worker. Cancellation and a
changed complete server policy invalidate its serial. Late worker results
cannot publish a level for a different policy. The prepared map-only world is
cloned for renderer publication, without local players, bots, loadouts or a
second `load_map`. Server snapshots subsequently own gameplay configuration.

Scene spawning occurs in Match behind an opaque Loading presentation. Input,
HUD, effects and offline ticks remain gated until current remote status is
Ready. Real-time discovery/transport continues while menus pause virtual time.
The render ticket is consumed after that Update's network drive, so a previous
expensive frame cannot count against the new synchronization deadline.

On policy rediscovery, the previous gameplay publications and scene ownership
are discarded before deferred state teardown. Home cancels both render ticket
and gate, removes the remote session and invalidates loader work. Offline
matches then use their normal fixed-tick simulation.

This integration is in progress. Scene-owned asset capture, browser endpoint
selection and rendered loopback validation are pending. Until scene capture
completes, admission fails closed in AwaitingMap. The draft does not expose a
finished player-facing network join flow.
