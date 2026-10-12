# Owned server address catalog

`remote_servers::Backend` is an independent leaf. Root declares it and adapts
its numeric `ServerKey`, generation-bound intents and row snapshot to the
authored browser. Construct it with the absolute launcher-selected userdata
root, poll once per render update and retain it through `busy()==false` after
cancellation before replacing the resource or exiting. Construction, intents,
snapshots and polling perform no filesystem, DNS or socket I/O on the render
thread. Joining, browser rendering and root lifetime/generation wiring remain
separate integration work.

Refresh accepts Internet/Favorites. Internet explicitly has no directory and
shows only manually probed addresses; Favorites shows saved addresses. Literal
IPv4/IPv6 and ordinary ASCII hostname:port inputs are normalized, never treated
as URLs or paths. Keys remain stable for retained normalized inputs across
refresh and ordering changes. Connect and RemoveFavorite require the rendered
generation. Connect additionally rejects busy, undiscovered and expired rows,
and returns an owned resolved address plus the exact validated `ServerInfo`.
Root re-discovers and checks full metadata before installed-content admission.

One worker owns storage, OS DNS and bounded Inspect-only discovery. There are
at most 32 favorites, 32 manual addresses, eight concurrent probe sockets and
eight DNS results per address. An address probe has a five-second wall budget,
a batch thirty seconds; checks occur after OS DNS returns because the OS call
cannot be interrupted. Cancellation invalidates tickets immediately but keeps
the worker busy until its owned thread exits. A second worker cannot start
while cancelled DNS is still outstanding. Close cancels the catalog;
CancelAddress cancels only an address-probe/AddFavorite job, preserving an
unrelated Favorites refresh. Root must retain ownership while waiting.

RemoteSession sends only Inspect, validates advertised metadata, releases its
pending transport and stops at AwaitingMap. No Hello, gameplay admission,
content mounting/fetching, Steam lookup or master-server request occurs.
Transport polling already caps incoming datagrams at 256 and control messages
at 16 per update. Server name, counts and ping remain unknown. The separate
discovery elapsed value measures resolution plus the Inspect handshake and
retries; it is not an RTT ping.

Favorites live in `server-favorites.txt` under userdata. A bounded versioned
UTF-8 format accepts canonical unique addresses only. The worker takes an OS
file lock, re-reads current favorites and publishes a synchronized temporary
file by atomic rename. Malformed/oversized data is preserved, links are refused
and cancelled temporary writes are cleaned up. Cancellation cannot undo a
favorite update already committed; a later refresh resolves persisted truth.
No player's Legacy or Steam configuration is touched.

Synthetic tests include actual loopback hostname discovery with zero admitted
players, unknown metadata, stable keys, stale/busy/undiscovered tickets,
corruption and cancellation rollback, atomic replacement, bounded lists,
held-lock cleanup, late uninterruptible DNS results and address-only cancellation.
Actual browser/Internet-directory integration remains outside this leaf.
