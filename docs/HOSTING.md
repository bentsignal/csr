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

Coordinated host and guest fixtures subsequently passed at both rates using
executable SHA256
`E9687E75490D6D770002D7F94C671F8A01A06E3937C0617D9602104AC01CE04A`.
Both processes exited with code 0 and `failures: 0` in each run. The host
observed two authoritative humans and one bot while the guest was connected,
then one human after the guest returned Home and loaded offline. Both clients
captured Ready, Pause, Home and offline reload; all sixteen images were
reviewed. Private provenance, logs and captures remain under
`rendered-host-guest-controller-64` and `rendered-host-guest-controller-128`
in the same private evidence folder. The 64-tick source is `25e4796`; the
128-tick source is its parent `e1fcdfb` plus the recorded fixture diff that
became that commit. These still exercise typed hosting rather than the
complete authored lobby path.

The original lobby bridge uses observed ChooseGameSettings, checked query,
UpdatedSelections and zero-argument LaunchGame callbacks. An owned scene epoch
guards cached API receivers; raw text only selects immutable catalog requests.
The checked query must match before UpdatedSelections stages a selection, and
the original Go consumes it once. No Steam identity or privacy is fabricated.

The first integrated authored Competitive checkpoint completed Play, Friends,
Create Lobby, Settings, Competitive, Dust II, Accept, No Bots and the original
Go using pointer events, with actual movie OnReady and presented-frame gates.
Both host and guest then passed Ready, Pause, Home and offline reload, exited
0 and reported `failures: 0`. The source was `cae0149` plus its retained fixture
diff (later committed in `0757e0c`); executable SHA256 was
`5118BA5997C7A655FCF2B878AB5352821475520D566FCC208512DF86EA4D571A`.
All ten private captures under `rendered-host-guest-authored-128-1` were
reviewed. The late Competitive guest rendered a spectator view while waiting
for a new round: this establishes joined rendering, not two active players.
It also exposed our address notice overlapping the authored footer. The
follow-up moves only our notice into the clear margin below navigation.

The final Deathmatch fixture additionally cancels the first picker through its
original X, returns the lobby Home, and reopens both cached movies before
selecting Dust II and using the original Go. It requires actual owned phase
changes and completed screenshot callbacks before advancing. Both clients
then pass Ready, Pause, Home and offline reload with `failures: 0` and exit 0.
Ready requires Deathmatch rules, the local player alive and at least two
authoritative alive humans; both logs record two active humans at 128 tick.
All twelve private images were reviewed, including the four authored stages;
Accept and the complete Go footer remain unobscured. Provenance under
`rendered-host-guest-authored-deathmatch-128-final` records source `2cc9d0f`
plus its saved fixture diff, the alive-guard flag and executable SHA256
`452FA2CDA0E6FAFAF380388EA40F7E995BEA79FE53A69DC741616B197D08DF6C`.

The combined locked workspace passes 465 tests with four ignored. Current
release Dust II probe/sim and audio checks, a fresh Competitive rerun and
exact-head CI are the remaining final gates; this still is not OS-input or
1:1 visual fidelity proof.

Dynamic bot slot filling and original competitive warmup remain open.

This is the existing unauthenticated address-based transport. It does not
establish Steam identity, friends-only access or public server discovery.
Original menu geometry and broader fidelity remain measurement work; the
inherited Home News body still has visible left-edge clipping.
