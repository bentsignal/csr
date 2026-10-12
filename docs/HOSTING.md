# Owned rendered hosting

The frontend host controller consumes a typed `HostSelection` retaining the
chosen `MapRequest`, supported tick rate, connection slots and local identity.
The existing serial map worker loads one Level, derives its retained content
manifest and constructs one map-only simulation World. An authority clone
receives the selected rules, nav and preconfigured bots. Installed Legacy maps
and immutable private-pack selections use the same ownership seam.

The authority binds and polls discovery/networking while Preparing, with no
simulation or bot steps. Its passive one-use owner admission creates no client
or synchronization deadline. The frontend publishes the retained level and
map-only World, then uses the existing current-scene render preparation gate
before `provide_map`. It never launches another map worker for that offer.

Once the local session has a validated authoritative snapshot, the private
Begin command acknowledges/discards all previously submitted inputs. The
frontend keeps fresh movement/attacks neutral until Running is observed.
After that the independent server worker advances even while the host pauses
or loses focus; gameplay remains in sim and changes through TickInput.

Home revokes pending host/join/offline requests, render admission, map selection
and the running local session. Cancelled host workers remain owned and are
reaped before replacement. The first process-exit barrier stops authority and
revokes pending work immediately, even if a later browser/pack barrier retains
exit while another worker finishes. Host/join/offline requests and deferred
Loading entry cannot reopen work during that retained exit.

## Verification and current scope

The controller checkpoint `b291d9f212effd86f0a0aa9ebecfa29ef25a36d1`
passes the client library tests, including passive admission, neutral controls
before Running, socket release, terminal errors, retained exit, stale map
replacement, and pending jobs/launches during Quit. The preceding controller
checkpoint also passed the full locked workspace suite.

Fresh owned engine fixtures on Dust II at 64 and 128 tick both completed with
`failures: 0` and recorded process exit code 0. Each captured rendered Ready,
Pause, Home and offline reload. The authoritative tick advanced during Pause;
Home retained no remote/match admission and waited for host worker cleanup.
The copied executable SHA256 was
`EB5FA7BFD6C9422C37189263383E283A9A6B0ABBFAB0E07222362F3C5CC0DA6B`.
Private provenance/logs/captures remain under
`C:\Users\shawn\csr-fidelity\rendered-host-controller-64` and
`rendered-host-controller-128`. These test the typed request and engine inputs,
not operating-system input or the original lobby's complete player flow.

The original lobby bridge is being integrated separately using observed
ChooseGameSettings, checked query, UpdatedSelections and zero-argument
LaunchGame callbacks. Its standalone mouse-driven probe completes mode/map,
Accept, bot options and the original lobby Go. The integrated authored host
flow and simultaneous guest proof must pass before this draft is complete.
Dynamic bot slot filling and original competitive warmup remain open.

This is the existing unauthenticated address-based transport. It does not
establish Steam identity, friends-only access or public server discovery.
Original menu geometry and broader fidelity remain measurement work; the
inherited Home News body still has visible left-edge clipping.
