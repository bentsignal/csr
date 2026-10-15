# Recoil: dated data and the measurement gap

The owner requested a **2015 gameplay baseline**. This audit identifies dated
weapon configuration and separates it from the still unverified recoil
procedure. It does not change weapon behavior or establish a pixel-perfect
spray pattern. The installed reference executable is CS:GO Legacy 1.38.8.1
(2023), not a runnable 2015 build. Existing classic menu work remains separate.

## Historical source identity

The [2015-12-31 shipped-data capture](https://github.com/SteamTracking/GameTracking/commit/fcb66c41571b716820b1e1c148e0cba9a318f772)
is a third-party SteamTracking archive of game data, not official Valve
hosting. Its `steam.inf` declares patch **1.35.1.6**, client/server version
**254**. Those declarations identify the captured data; they do not supply
the corresponding native executable or prove an engine algorithm.

Files were obtained by exact Git blob identity and independently checked
against Git's blob header and byte hash. Raw files stay outside this repo.
The following are derived configuration facts, not a copied impulse table.

| Definition | Seed | Angle | Angle variance | Magnitude | Magnitude variance | Crouch recovery | Stand recovery |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| AK-47 | 223 | 0 | 70 | 30 | 0 | 0.381571 | 0.460000 |
| M4A4 (`weapon_m4a1`) | 38965 | 0 | 70 | 23 | 0 | 0.302625 | 0.423676 |
| M4A1-S normal fields | 38965 | 0 | 65 | 25 | 3 | 0.302625 | 0.423676 |
| M4A1-S alt fields | 38965 | 0 | 65 | 21 | 0 | same recovery fields | same recovery fields |

AK/M4A4 values come from the standalone shipped `WeaponData` files; M4A1-S
values resolve from the item schema. The normal/alt labels are data field
names: the schema alone does not prove their mapping to suppressor state.
Sources: [AK-47 data](https://github.com/SteamTracking/GameTracking/blob/fcb66c41571b716820b1e1c148e0cba9a318f772/csgo/csgo/scripts/weapon_ak47.txt),
[M4A4 data](https://github.com/SteamTracking/GameTracking/blob/fcb66c41571b716820b1e1c148e0cba9a318f772/csgo/csgo/scripts/weapon_m4a1.txt),
[item schema](https://github.com/SteamTracking/GameTracking/blob/fcb66c41571b716820b1e1c148e0cba9a318f772/csgo/csgo/scripts/items/items_game.txt).

This is one late-2015 snapshot, not proof that every 2015 update behaved
identically. Valve's [December 15 recovery article](https://blog.counter-strike.net/2015/12/13394/)
records a rollback of rifle/pistol changes to their pre-Winter Update state.
Later, the [August 2016 Second Shot article](https://blog.counter-strike.net/2016/08/15304/)
introduced a different recovery method. Valve's [August 5, 2016 notes](https://blog.counter-strike.net/2016/08/15381/)
also explicitly introduced recoil reset on reload. Modern behavior must
not be labeled 2015 merely because it is observed in the installed game.

## Installed data and current code

The installed 2023 schema retains the five listed seed/angle/variance/
magnitude inputs for AK/M4A4. Its recovery values differ:

| Definition | Crouch initial | Crouch final | Stand initial | Stand final |
| --- | ---: | ---: | ---: | ---: |
| AK-47 | 0.305257 | 0.419728 | 0.368000 | 0.506000 |
| M4A4 / M4A1-S | 0.242100 | 0.332888 | 0.338941 | 0.466044 |

The installed transition fields specify bullets 2 through 5. The old
`recovery time stand` / `recovery time crouch` keys are present and nonzero;
they hold the initial values, rather than being replaced by the final keys.
A source-only audit mirrored the repository's repeated-table merge and
prefab traversal and found these values in all three base definitions.
Dust2's embedded pak does not shadow the catalog; the inspected installed
VPK index has no `items_game.txt` entry, so this input is the loose schema.
This audit is not a new executed Rust catalog test.

`weapons::WeaponDefinition.attributes` already preserves the final and
transition fields. Its typed `Accuracy` fields lift the old keys, and
`sim::weapon::think` uses those initial values with a minimum of 0.05.
Thus these rifles do **not** demonstrate a missing-key fallback to 0.05;
the unimplemented distinction is the native recovery procedure.

The current `sim::weapon::recoil_pattern` uses the project's own RNG seeded
by seed plus definition ID, independent angle/magnitude sampling, a
first-three-shot angle multiplier of 0.25, conversion of 0.02 degrees per
unit, and accumulated offsets. Recovery moves backwards through that
pattern at 12 steps per second. These choices have not been validated by
this historical data. The pattern cache now selects the declared alternate
recoil fields in burst mode; that selects data, not a measured procedure.
Matching seed values and deterministic tests do not
establish the original sampler, filtering, impulse decay or trajectory.

The held-automatic-trigger cooldown correction prevents recovery between
burst shots. It fixes an internal semantic discontinuity without changing
these numeric choices. Current code has no explicit reload recoil reset;
recovery during a reload can naturally bring its index to zero. That must
be measured separately from an explicit reset policy.

## Next measurement boundary

The installed declarative `modevents.res` exposes `bullet_impact` with float
coordinates and `weapon_fire` with weapon and silenced fields. This supplies
a possible observation surface; the current harness does not yet prove it
receives those events. Event coordinates also do not establish complete
ray origin, camera punch, or a historical event schema.

1. Use an owned native session, with the
   [userdata preservation boundary](FIDELITY.md), fixed position/view,
   verified weapon/ammo/mode, acknowledged settings and executable identity.
2. Prove a receiver using public demo/event format documentation or a
   documented runtime observer. Record actual shot ticks and ammo changes;
   console-command receipt is not an exact shot timestamp. Keep the work
   clean-room, with no Source SDK or leaked/decompiled implementation.
3. Measure no-spread bullet rays separately from ordinary spread, and
   camera/viewmodel motion separately from bullet direction. Verify the
   origin/punch observation surface rather than assume one exists.
4. Record tapping, bursts, full magazines, release, empty clip, reload,
   switching, stance and mode transitions. Fit the procedure against
   held-out traces at 64/128 ticks and prediction reconciliation.
5. Replace arbitrary choices only with measurement-constrained behavior
   or an explicitly provenance-bound profile tied to content identity and
   schema/mode. Publish derived observations, not game data or captures.

A 2023 trace can establish an installed-game profile. It cannot establish
2015 pixel-perfect recoil without calibrated historical observations or
matching native execution. Landing remains a separate measurement problem;
the completed 2023 reference session observed real landing slowdown.

## Bounded installed-game spray observation

The next observation first admits one real AK shot. Acknowledged settings
alone are insufficient: before/after captures must show the actual AK ammo
count changing and a new impact marker. `weapon_accuracy_nospread` and
`sv_showimpacts` must report their requested values before this shot.
`cl_pdump` is an attempted public prediction overlay, not an assumed punch
receiver. Earlier `net_showevents` attempts produced no shots or events and
are not calibration evidence.

Only after that admission can full-magazine capture proceed. Any change to
view-only recoil settings is recorded separately from the unchanged bullet
recoil scale. A fixed camera must be established from static scene features,
not inferred from a cvar name. Slow-time capture records image begin/end
intervals and actual ammo transitions; console receipt times are not exact
server shot ticks. Repeated magazines must have fresh, expired prior impact
overlays and a recovered, reloaded weapon.

A usable result needs derived angular ray observations and independent
repeat agreement before replacing the current generator. Native images,
console logs, settings, executable identity and account-bearing session
receipts stay outside the repository. Any resulting profile identifies the
installed 2023 reference and its limitations; unchanged historical recoil
configuration alone does not establish a 2015 trajectory.
## Private provenance

Research output: `csr-fidelity/recoil-reference-2015`, outside the repo.
No assets, raw scripts, native logs or account identifiers are published.

| Input | SHA256 |
| --- | --- |
| Historical item schema | `8F9F82542D61246E8C0447A7E6CBC8D9F5FF6E80E65D9DB7708CEECC09E2C0DB` |
| Historical AK-47 data | `C46608D591A64BD78D40E7E31EB00CBC9D8C77582D27248ACEB4427FBF150C73` |
| Historical M4A4 data | `E1100970423BD3D3C53CBB72BE4D127BACDC3826B59DF89E318C1EE0238FA38C` |
| Historical steam.inf | `793D6F1A98D5ADF9BCE2EEC5D6FBC572D218AFFB91D5B342DFA7371FA4EE7608` |
| Installed loose schema | `510E09B68A01D88EDBA2342972960025FD6484AAAA58365FCF47D8513DE79623` |
| Installed event metadata | `FF9D593FC7FBEA9A5AF2CE5BC0816B7B99FE2B592AA01FC73A8347AAF26B1206` |
| Source-only resolution audit | `A1D6D35EC3D271A779562ED4694B844319E1CD472CFDC7633CF71F4E4695797E` |
