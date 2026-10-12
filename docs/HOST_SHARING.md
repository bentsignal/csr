# Host address sharing

Integration pending. This leaf preserves the server's actual socket bind and
allocated port separately from the loopback address used for private owner
admission. A wildcard bind is a listening scope, not an endpoint to copy.
`ServerInfo` and the network protocol are unchanged.

The client leaf `host_sharing.rs` provides a generation-stamped receipt with
at most 32 deterministic address candidates, labeled `On this network` or
`This PC`. Specific binds produce their exact endpoint. Wildcard binds use one
owned Windows adapter-enumeration worker, matching the socket's address family
and actual port. It includes active preferred unicast interface addresses;
adapter enumeration does not establish remote or Internet reachability. No
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
pointer/key gestures before native movie or gameplay input. Root UI wiring is
separate; preserve authored controls and use measured unoccupied space.

`copy(intent, &RawHandleWrapper)` must execute on the game window's owning UI
thread. The wrapper retains Bevy's owned window, and Win32 checks reject stale,
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
names at128 characters, and copied endpoint text at256 UTF-16 units.

Tests use generated interface metadata, owned ephemeral UDP hosts and fake
clipboard sinks. They never overwrite the system clipboard. Full local and
exact-head CI results will be recorded before this leaf is ready to merge;
rendered sharing UI remains integration work.

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
