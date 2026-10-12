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
Ready. Remote pause sends neutral controls while transport and visual timing
continue; offline pause still stops virtual time.
The render ticket is consumed after that Update's network drive, so a previous
expensive frame cannot count against the new synchronization deadline.

On policy rediscovery, the previous gameplay publications and scene ownership
are discarded before deferred state teardown. Home cancels both render ticket
and gate, removes the remote session and invalidates loader work. Offline
matches then use their normal fixed-tick simulation.

The installed Community menu opens the local server browser. Favorites and
hostname resolution use one owned worker; stale selections never become joins.
Map preparation gathers current match geometry/material dependencies and
retained character, weapon, glove, arm and ground-item assets. Two consecutive
render preparation cycles acknowledge that exact generation before admission.
This acknowledges CPU render preparation, not GPU idle or future shader variants.

Browser input owns its primary window and balances movie releases. Closing wins
over a connect captured earlier in the same input batch; closing an address
dialog supersedes an undispatched favorite write. Browser and pack exit owners
remain closed until all retained workers finish, even when another owner holds
the exit message. Missing installed browser definitions return Home with an error.

The opt-in loopback engine proof (`CSR_TEST_REMOTE=127.0.0.1:port`, with
`--screenshot`) exercises discovery, exact local preparation, authority, neutral
pause, Home teardown and offline reload. Each scene transition waits for its
actual screenshot callback. Private captures and isolated player data stay
outside the repository. The browser interaction proof and final graphical
checks are still being completed on this draft.

Public directory discovery, LAN browsing, menu hosting and automatic server
content acquisition remain open. The current browser exposes local address
entry and favorites; unknown server name/player count/ping remain blank.
