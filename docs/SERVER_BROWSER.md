# Native server-browser adapter

The installed classic main menu emits `MainMenu.BasePanelRunCommand("OpenServerBrowser", "bHidePanel")` after its multiplayer privilege query succeeds. It does not request a separate browser SWF. The address dialog belongs to the installed native VGUI resources, not the lobby movie's lobby-specific JoinPlayer callback.

`frontend_servers_data` reads browser, game-page, Add Server and game-info RES files and English localization from the detected Legacy installation's `platform/servers` directory. It reads the source scheme from the game VFS. No layout, font or game asset is embedded or copied into the repository. The caller mounts that platform directory read-only and supplies runtime font sources.

`frontend_servers` owns only its Bevy UI tree. The caller supplies a camera, viewport, definition, fonts, bounded model and backend snapshot, gates input by window and browser ownership, and tears down the tree on transitions. A failed build removes all allocated descendants. The view emits explicit refresh, address probe/add/cancel, close and generation-stamped connect intents; it never resolves hosts, writes favorites, joins sessions or fetches assets. The backend must validate the stable key and generation before connecting. Unknown name, map, player-count and latency remain blank. A successful address discovery does not imply admission.

The Add Server field accepts bounded UTF-8 editing, selection, Backspace/Delete and cursor navigation. Ctrl+V reads Windows Unicode clipboard data only on an explicit nonrepeat paste with owned address focus; invalid/oversized/control-character text is rejected atomically. Enter emits one AddFavorite request from the address dialog; selecting a validated favorite and pressing Connect emits its current key/generation. Escape/Close cancels an open address dialog explicitly before closing the browser. Busy snapshots prevent refresh/tab/address edits and joining; address cancellation remains available. Up/Down and wheel scrolling reach all filtered rows without changing row identity.

This is a functional adapter of declared resource bounds/styles. Native engine sizing overrides, column widths, row padding, title-bar controls and tab widths have not been measured against the original application. Those details are not claimed as completed 1:1 fidelity. The declared resource frame and child extents disagree; the adapter preserves the declared bounds rather than asserting an invented original size. Address horizontal scrolling, native browser filtering and measured context-menu geometry remain gaps. A localized favorite-removal context action is provided through the caller-owned gesture; only its localization is observed, and its bounds/activation are explicitly unmeasured. The Internet projection is manually probed endpoints, not an invented Steam directory; the backend status must state that scope.

Integrated 1920x1080 engine-fixture captures show the list and Connect control
extending approximately 28 pixels beyond the declared frame's right edge. This
remains unresolved native-sizing fidelity work; the context readability fix does
not alter those bounds or invent a native sizing override.

The context action uses the installed `Menu.BgColor` and `Menu.TextColor` roles.
The actual Legacy `resource/sourcescheme.res` sets an opaque menu background;
its `Button.BgColor` instead aliases a fully transparent color, which let the
underlying map-cell text show through the removal action. Menu roles prevent
that bleed while preserving the context geometry and input ownership. The
installed `MenuBorder` aliases `RaisedBorder`; native border geometry remains
unmeasured and this change does not invent one. These observations were verified
read-only against `csgo/pak01_dir.vpk` / `pak01_203.vpk`, member
`resource/sourcescheme.res`, SHA256
`203a3d4dc559bb6e44d03e86532ef00fd0c1f6ce327a5dced8898992de919f81`.

For an offscreen private render, `cargo run -p client --example server_browser -- <private-capture-directory>` mounts the installed resources and supplies local system font files at runtime. It renders an explicitly synthetic localhost favorite with unknown metadata and types an address through the actual leaf keyboard path. It opens no window and performs no network requests. Captures must stay outside the repository.

## LAN discovery

The installed `ServerBrowser_LanTab` token supplies the LAN tab's label. Its
refresh collects local endpoint hints, verifies them with the existing Inspect
exchange, and offers the existing generation-checked Connect flow. A hint
contains no host address, map, player count, content manifest or invite. The
packet's source IPv4 address and declared nonzero game port identify a candidate;
only a successful Inspect supplies metadata and a connectable row. This is
discovery over the rewrite's existing unauthenticated transport, not server
identity authentication. Direct addresses and Favorites remain separate.

Running authorities send a 14-byte, exact-version hint once per second to
`239.255.73.82:42690`, with multicast TTL 1. Preparing hosts do not advertise.
The authority owns a separate ephemeral UDP socket; it never shares reads with
the game's netcode socket or starts an advertisement thread. Send failure
disables this optional advertisement without stopping the game. Shutdown/drop
releases the socket. No firewall rules, administrator prompts, external service
or game-install writes are involved.

The browser's one retained worker opens a multicast receiver with reuse enabled
before binding, so multiple local browsers can listen to the same group/port.
Multiple hosts advertise their actual game ports. Collection ends after 2.5
seconds, 256 received datagrams or 32 distinct candidate endpoints; malformed,
oversized and incompatible packets are ignored. The existing bounded Inspect
batch then checks candidates with at most eight concurrent probes. Cancel/Close
invalidates the generation immediately; the worker observes cancellation and
drops its socket. Refresh replaces LAN membership with fresh verified results;
it does not save discoveries as favorites or admit gameplay players.

This initial scope uses IPv4 and the OS-selected multicast interface. IPv6,
multi-interface enumeration, routed discovery and exact native tab sizing are
not implemented. Multicast filtering, a VPN/default-route choice or a blocked
port may hide a server; an empty list is not proof that no host exists. The UI
keeps direct address/Favorites access available. The socket ownership/reuse
follows [Microsoft's multicast reuse documentation](https://learn.microsoft.com/windows/win32/winsock/using-so-reuseaddr-and-so-exclusiveaddruse)
and the [socket2 API](https://docs.rs/socket2/0.6.3/socket2/struct.Socket.html).
Focused netcode tests pass, including two simultaneous loopback multicast
receivers seeing both server ports, socket release, rate limits and malformed
hint rejection. Browser compilation/integration and rendered joining remain
pending central verification; no completed cross-machine LAN or UI fidelity
claim is made.

The existing browser engine proof has an optional LAN path: retain
`CSR_TEST_BROWSER=localhost:<owned-port>` and set
`CSR_TEST_BROWSER_LAN=<owned-authority-IPv4>:<same-port>` in isolated userdata.
An independently owned headless authority must already be running and
advertising. The proof clicks the real LAN tab, waits for that exact endpoint's
successful discovery, selects its row and clicks Connect. It checks Ready,
content identity and advancing authoritative ticks, then returns Home and
checks teardown. Three fresh ScreenshotCaptured saves are required:
`browser-lan.png`, `browser-ready.png` and `browser-home.png`. It does not inject
rows, enter addresses or bypass the join flow. Without the optional variable,
the existing seven-capture Favorites proof is unchanged. These are engine
input fixtures, not operating-system mouse input or cross-machine evidence.
