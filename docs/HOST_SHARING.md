# Host address sharing

The host's original Pause menu now includes a collapsed `Invite friends`
control. Expanding it shows up to three address candidates per page, their
`On this network` / `This PC` labels, explicit Copy buttons and Refresh.
Leaving the card collapses it before returning input to the native movie.
Native Options suspends the card; its observed Back/Cancel restores Pause only
after fresh owned visibility proof. Both Deathmatch and Competitive two-player
fixtures verified this flow through Home and an offline reload.

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
leaves. Display-only adapter labels are capped at 24 characters; copied endpoint
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

The corrected combined source `66709f2bc65dff49969c004d927e1bcc52b76236`
passed 509 workspace tests (5 ignored) and the play build. After merging the
capabilities slice from master, the published PR source matches that checkpoint
in `crates`, `Cargo.toml` and `Cargo.lock`; only this evidence document differs.
The preceding `8c2f73d24981b467c17253fa39bd4428fd27b076` release checks passed
Dust II probe/sim with 0 failures and soundcheck with 1,425 decoded sounds,
0 unsupported and 0 crashes. Corrected-source release checks and exact-head CI
are tracked separately before merge.

Two private 128Hz two-client fixtures at the corrected source each produced 14
PNG captures, host/guest exit 0 and both failure counters 0. All 28 images were
independently inspected. Deathmatch asserted two active human actors;
Competitive asserted two humans with one active and the late guest spectating.
Both verified original picker cancellation/cached reopen, the selected mode and
Dust II in Lobby, original GO, Ready, Pause, Home and offline reload.

The real rendered Invite/Copy controls delivered the enumerated endpoint
`10.0.0.250:27015` through a fake clipboard sink and the existing browser parser.
The registered Pause root's inventoried public
`Panel.MainMenuNav.OnOpenAudioSettingsDialog()` callback emitted the measured
`PauseMenu.BasePanelRunCommand('OpenAudioSettingsDialog', 'bHideMenu')`.
This was a public controller invocation, not a physical Audio-menu selection.
The actual Back pointer at (380, 972) emitted `OptionsMenu.OnCancel()`;
fresh sampler proof restored the collapsed Invite control, and Home cleared
its receipt. The Audio view contained no Invite overlay.

These establish functional visibility restoration, not settled native-animation
or full Settings fidelity. The Deathmatch Audio/restored-Pause captures were
unsettled: `#SFUI_Control_0` remained and Pause lacked the full logo/highlight.
The same artifact's Competitive captures showed Master Volume, its slider and
0.5 value, then the full Pause logo/highlight; the Deathmatch frame does not
establish a persistent settings defect. The black lower Audio preview and
clipped Home News placeholder remain separate known gaps. No OS input,
system clipboard write, remote reachability or LAN-discovery completion was
proved by these engine-input fixtures.

Private provenance (never committed):

- `C:/Users/shawn/csr-fidelity/rendered-host-sharing-deathmatch-128-2`
- `C:/Users/shawn/csr-fidelity/rendered-host-sharing-competitive-128-1`
- Copied EXE SHA256: `F1677C4B0A4838497596C2CA4EBCBBC273E1B19E04EDEE0317F4EB3825AC8158`.
- Both source-diff patches were empty; SHA256:
  `E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855`.
- Both launch scripts SHA256:
  `52AB59CBB5E4E51D466A854FB020AA760F3AC1C57A0BE8490B7EE5CA8CD24543`.

The source, copied EXE, diff and invocation hashes were independently checked.
Both fixtures' owned copied processes were absent after completion. Automated
Copy evidence used only the fake sink; the native clipboard was untouched.

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
