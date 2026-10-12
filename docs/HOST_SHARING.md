# Host address sharing

The host's original Pause menu now includes a collapsed `Invite friends`
control. Expanding it shows up to three address candidates per page, their
`On this network` / `This PC` labels, explicit Copy buttons and Refresh.
Leaving the card collapses it before returning input to the native movie.
Native Options suspends the card; its observed Back/Cancel restores Pause only
after fresh owned visibility proof. Rendered verification is still pending.

The backend preserves the server's actual socket bind and
allocated port separately from the loopback address used for private owner
admission. A wildcard bind is a listening scope, not an endpoint to copy.
`ServerInfo` and the network protocol are unchanged.

The client leaf `host_sharing.rs` provides a generation-stamped receipt with
at most 32 deterministic address candidates, labeled `On this network` or
`This PC`. Specific binds produce their exact endpoint. Wildcard binds use one
owned Windows adapter-enumeration worker, matching the socket's address family
and actual port. It includes active preferred unicast interface addresses;
IPv6 link-local addresses are excluded because their scope index belongs only
to the host and cannot be copied as the friend's outgoing interface index.
Adapter enumeration does not establish remote or Internet reachability. No
public-IP lookup, network probe, firewall modification or directory request is
made. LAN discovery remains separate unchecked work.

## Caller contract

Retain one `Sharing` owner across scene teardown. `set_host(host_generation,
bound_address)` consumes only actual Preparing/Running host metadata, with a
fresh host lifetime generation. Repeating the same host is a no-op. `refresh`
invalidates the receipt and rescans. `clear` immediately removes all copyable
addresses on Home/cancel/replacement. Continue `poll` and retain the resource
until `busy()` is false before completing app exit. An OS adapter enumeration
call is synchronous and cannot be interrupted: old results are discarded, and
only the latest pending request can start after the one worker returns.

Render an immutable `Receipt`; stamp Copy actions with its opaque `generation`
and candidate `key`. `copy` rechecks current lifetime, revision, busy state and
membership before writing a literal endpoint. UI ownership must consume its
pointer/key gestures before native movie or gameplay input. `host_sharing_ui`
updates before `frontend::input`, which uses the existing input-ownership
transition to balance releases outside the movie stage. The card occupies the
measured right-hand space beside Pause and auto-collapses when the pointer
leaves. Display-only adapter labels are capped at24 characters; copied endpoint
and receipt identity stay exact. See
[Pause/Options bookkeeping](research/pause-options-visibility.md) for measured
callback evidence and the distinction from authored internal animations.

The passive tracker owner is the movie-worker generation, not the host
generation. Scene/scope transitions keep cached root paths, while replacement
workers clear them. Options opening immediately invalidates proof; its
`OnCancel` clears the wanted lifecycle and hides the exact cached root. Late
Options `OnReady` and Pause load callbacks cannot override a newer selection.
The sampler allows one outstanding request and at most one per presented movie
frame. All Copy/Refresh/paging/toggle actions carry the rendered receipt
generation, and hidden/collapsed controls cannot act.

`copy(intent, &RawHandleWrapper)` must execute on the game window's owning UI
thread (`NonSendMarker` enforces the UI system's thread). The wrapper retains
Bevy's owned window, and Win32 checks reject stale,
foreign or wrong-thread HWNDs. Only an explicit Copy action opens and replaces
the clipboard. Success is returned only after native `SetClipboardData`
succeeds. `copy_with` is an injectable sink for honest in-memory proof fixtures;
it has the same receipt gates and does not claim OS clipboard evidence.

## Bounds and evidence

The worker uses a 15KB initial native buffer, at most three allocation attempts,
a 1MiB memory cap, 64 adapter nodes and 512 unicast nodes. Pointer traversal,
string reads and C structure reads stay inside the owned initialized buffer;
cycles, invalid lengths and limit excesses fail with bounded errors. Native
storage is released between scans. Returned interface records are capped at256,
stored names at128 characters, and copied endpoint text at256 UTF-16 units. Global
IPv6 and loopback literals omit unnecessary local scope/flow metadata and must
round-trip through the existing browser address normalizer unchanged.

Tests use generated interface metadata, owned ephemeral UDP hosts and fake
clipboard sinks. They never overwrite the system clipboard. The actual-client
focused leaf check passed23 tests (13 sharing/native-buffer cases and10 existing
browser backend cases). Ten netcode host cases passed, including exact
wildcard/ephemeral-port Preparing/Running identity and unchanged owner admission.
Seven tracker regressions and the generated five-case actual AVM1/EI sampler
integration regression passed, alongside ten Scaleform library cases.

The combined UI checkpoint `8c2f73d24981b467c17253fa39bd4428fd27b076`
passed509 workspace tests (5 ignored), the focused collapsed-Invite checks and
the play build. That checkpoint includes a separately reviewed capabilities
slice whose PR is still being integrated; final results must be bound to the
combined published source before merge.

The first private Deathmatch128 two-client fixture rendered Invite, enumerated
an actual local interface, and delivered its exact bound-port endpoint through
the real Copy button to a fake clipboard sink and browser parser. The guest
completed its lifecycle with0 failures, but the host then timed out with1
failure because its Audio helper sent the unsupported
`GameInterface.ExecuteCommand`. This is partial Copy evidence, **not** a
successful Options/Back or complete host lifecycle proof. The corrected
current-source rendered run, mandatory release Dust II probe/sim/audio and
exact-head CI remain merge gates. There is no OS clipboard, remote reachability
or LAN-discovery completion claim.

Primary references read for this implementation:

- [Microsoft GetAdaptersAddresses](https://learn.microsoft.com/en-us/windows/win32/api/iphlpapi/nf-iphlpapi-getadaptersaddresses):
  synchronous local enumeration, buffer sizing and changing adapter records.
- [Microsoft unicast address structure](https://learn.microsoft.com/en-us/windows/win32/api/iptypes/ns-iptypes-ip_adapter_unicast_address_lh):
  native length, socket address and address state.
- [Microsoft SetClipboardData](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setclipboarddata):
  owned HWND, moveable memory and successful ownership transfer.
- [Microsoft GetWindowThreadProcessId](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getwindowthreadprocessid):
  clipboard window ownership check.

The actual published `bevy_window-0.19.1/src/raw_handle.rs`,
`raw-window-handle-0.6.2/src/windows.rs`, and `windows-sys-0.61.2` bindings were
read locally. No game function bodies, game assets or private SDK source were
used.
