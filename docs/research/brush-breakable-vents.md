# Brush breakables: source candidate, executable validation UNRUN

Historical R3 brush-only report. Its fourteen installed Nuke `func_breakable`
entities are glass windows, **not the actual vent covers**. The separate
[authored studio prop candidate](authored-prop-breakables.md) supplies the
actual cover path; both candidates still require executable verification.

The owner authorized this bounded overnight map-interaction slice on October
18, 2026. It is based on the rotating-prop-door candidate `c9a0270`. No build,
test executable, game, native helper, UI, push or PR was run by this agent.

`func_breakable` was previously rendered in static world batches and baked
into static collision, without health, removal or reset. Supported unparented,
unrotated brush entities now retain their authored convexes separately from
the immutable map/grid. Their identity is the owning map ContentId plus the
stable BSP entity/definition ordinal; no map-name or coordinate cases exist.
Other brush classes, unsupported transforms and entities beyond the 1024
definition bound keep their previous static path.

The mounted BSP supplies health, material, minimum damage and trigger-only
metadata. Mounted `scripts/propdata.txt` can override health and bullet/club
scales, including base inheritance bounded to 16 levels. Standard numeric
Hammer prop-data choices map to their preset names; private packs may name a
preset directly. Missing scales use neutral 1.0 compatibility defaults. These
defaults and native damage rounding/minimum-damage ordering have not been
measured. Health-zero, material-7 and trigger-only declarations remain solid
and cannot be destroyed by the supported attack path.

The ordinary gun/knife TickInput path damages only the first struck brush
after resolving nearer player/dynamic targets. Guns retain existing range
falloff and knife attacks retain their ordinary cooldown/reach and damage.
No new gameplay runs in render systems. Destruction rebuilds the small runtime
overlay containing both posed doors and intact breakables. It shares the map
and grid through Arc; health-only hits do not rebuild collision. Subsequent
pellets/combat traces see the opening immediately. Movement uses the existing
tick-start collision handle and sees that change on the following tick.

The client isolates supported brush faces and projected overlays using the
same compiled topology, material and lightmap builders as the world. Health
state hides those real meshes and their attached bullet decals. PVS cannot
override destruction; correction or round reset can restore intact visibility.
Materials/meshes remain ordinary match-owned entities for render preparation
and teardown. Existing finite material impacts and knife wall audio remain;
there is no invented break sound, fragments, debris or broken-texture substitute.

WorldState includes bounded health and an explicit destroyed bit (zero authored
health is distinct from destroyed). Authority, prediction, snapshot restore,
mode/round restart and client presentation consume the same state. Individual
death/respawn leaves openings intact. Protocol 27 / `0x4353_5257_0000_001b`
rejects older peers. General validation rejects nonfinite, negative, oversized
or contradictory health; prediction admission/correction checks counts and
health/removal against the mounted map before mutating state. The session
catalog digest also incorporates the resolved breakable damage values, so
different mounted prop-data semantics cannot silently join the same session.

## Installed-data evidence and clean-room boundary

Read-only Python inspection of the installed Legacy `de_nuke.bsp` entity lump
found 14 func_breakable declarations, all health 1 / material 0 / spawnflags 0;
two choose propdata 24 and twelve choose 0. The installed propdata file declares
Glass.Window health 1 / bullet scale 0.5 and inherits club scale 1.0. These are
metadata observations, not a compile, decoded-mesh or native behavior receipt.
No third-party assets were added or extracted into the repository.

Public format references are [func_breakable](https://developer.valvesoftware.com/wiki/Func_breakable)
and [prop data](https://developer.valvesoftware.com/wiki/Prop_data). Direct VDC
fetches returned 403 during this work and supplied no additional evidence.
Implementation is original Rust built on the repository's existing brush and
combat seams; no SDK/leaked/decompiled implementation or CS2 assets were used.

## Tests and remaining validation

All test source is UNRUN. Synthetic sim fixtures cover authored bullet/club
scales, actual attack TickInput, partial health, removal and walking through,
64/128 tick prediction/restore equality, clone isolation, round restoration,
death persistence, door-overlay preservation, indestructibility/minimum damage,
occlusion, map-dependent admission and bounded prop-data inheritance. Netcode
fixtures cover wire roundtrip/bounds, wrong-map admission and correction before
mutation, destroyed/intact correction, and rejection of all older revisions.
The existing compiled-mesh fixture now checks isolated topology, UV/lightmap
preservation and entity-origin translation.

Parent must compile and run focused fixtures plus workspace/probe/sim gates
under its exclusive Cargo slot; use the required shared-target profile clean
when changing build cwd. Test the immutable copied executable on Nuke vents,
then representative other Legacy/private-pack brush breakables. Verify real
rendered disappearance, passage, round restoration, impacts and network
correction, and retain apartment/door/ladder regression routes. No fidelity or
performance acceptance item is marked complete by this source candidate.

Limits: func_breakable_surf/prop breakables, parented/rotated/scripted movement,
I/O trigger breaks and OnBreak outputs, touch/pressure/physics/explosive/fire
damage, penetration/shot continuation, native damage defaults/rounding and
gib/explosion/drop spawning remain unsupported. The existing Level static
collision and load-time prop-lighting acceleration do not include runtime
owners; corpse collision/lighting and dynamic shadow parity need parent review.

## Parent integration

Keep the ladder v25 fields and the door v26 fields while retaining this v27
gate; use a later revision if another combined schema already consumed 27.
`rebuild_door_collision` and `with_door_solids` deliberately retain their names
but now include both runtime owners. Do not replace either call with a
door-only list. Reset breakables before resetting/rebuilding doors; restore
both state vectors before rebuilding. Keep the prior ladder adaptation:
authored ladder indices/convexes come from `static_world()`, while obstruction
traces stay on the outer current runtime world.

Likely textual conflicts are client lib scheduling/startup mesh batching,
sim lib/state/combat, netcode prediction/version tests, collision loader/overlay
fields, level digest composition and client feedback/visibility. Keep the
parent's combined ladder/viewmodel/grenade changes; no viewmodel, skinning,
props lighting, movement, PHY decoding or apartment-route implementation was
edited here. No roadmap acceptance box was checked.
