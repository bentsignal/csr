# Skate on CS maps

Install a private Skate `.csrpack` by opening it, or choose **Content packs →
Install pack**. Load an offline CS map. The **Skateboard** joins your inventory
on the right HUD. Cycle to it with the mouse wheel or Xbox LB/RB, then use
**left click / RT** to skate. Your CS gun returns after releasing the button.
While skating, D-pad Up/Down cycles CS weapons, Right fires, and Left reloads.
View / Back or X returns to CS. The D-pad skips the board while riding; mouse
wheel cycling still reaches it, and using it again returns to CS.

**View / Back** on Xbox or **X** on the keyboard remains a direct movement
shortcut. The keyboard action is **Toggle movement mode** in Keyboard / Mouse.
Camera travel animates in both directions, follows movement and clips against
the map. App updates preserve settings and private packs. The older 0.2.0 pack
continues to work; install a private **0.4.0** replacement for material-specific
granular rolling and marker sounds. A 0.3.0 pack retains its earlier sounds.

The held board uses your CS hands, a shaped equipment icon, and the same map
ambient/sun lighting as CS models, both held and riding. Use **Inspect**
(default **F**) or click the **left stick** while holding it to turn the board
over and examine the deck, underside and trucks. Gameplay has no control-text
overlay. Returning to CS checks the complete player hull against live geometry,
including uphill corners and low ceilings; an airborne switch retains momentum.

Place a session marker with **LB + D-pad Down** while grounded, then hold
**LB + D-pad Up** to return. The original native eligibility, geometry checks,
hold timing and actor reset run in the provider. Markers survive CS/Skate mode
switches in the same life/map and are included in deterministic snapshot replay.
While holding LB, the original session-marker panel appears at the upper
left. Placement/return play private original UI cues. The original trick
display (trick names, sequence score, multiplier and line meter) runs its own
movie in the provider and is drawn at the lower left from the pack's fonts and
bitmaps. Packs made before HUD support show neither panel; recreate the pack.
Blocked destinations are refused. Vertical walking-camera look is corrected
without changing the right stick's on-board trick mapping.

Use an Xbox 360, One or Series controller through Windows XInput. Skate uses
the original rewrite's controller mapping: A/X push, left stick steers, right
stick performs flick tricks, triggers grab, B brakes, and Y gets off/on the
board. Original gesture, switch-stance, bail/recovery and camera logic run in
the Skate coordinator. Start pauses. View / Back returns to CS at any time.
While skating, D-pad Right fires the CS gun, Left reloads, Up/Down changes
weapons. Holding LB leaves D-pad commands with Skate. CS also supports basic
controller movement, aiming, firing and weapon selection.

## Create a pack from your own game

1. Own Skate 3 and prepare your copy with the
   [Skate 3 Rust Engine](https://github.com/SK8-ENGINE/skate-3-rust-engine).
   Follow its local Xbox 360 ISO / extracted `default.xex` conversion workflow.
2. Developers download the separate **Skate-Authoring-windows-x64.zip** from
   the matching release and run `Skate-Pack-Creator.exe`. Select `skate3rust.exe`
   in that prepared installation, then a new output `.csrpack`. The creator
   is developer tooling and is never bundled into the player client or menus.
3. Open the pack to install it, load a CS map, and press View / Back or X.

Use a **0.6.0** pack to include the course in Play; older full-runtime packs
still enable skating on other maps. Recreate a rushed 0.1.0 prototype
pack with the current creator; it lacks the animation/skin/graph data.
Installing the replacement updates the same pack ID.

The creator reads your converted install without modifying it. It includes
the original board skin/textures and rig reference GLB (which also contains
the original skater mesh, although only its board is rendered), animation
banks, skeleton/collection data, compiled state graphs, controller patterns,
camera definitions, selected trick-name strings, converted board foley and
marker cues, five complete rolling recordings with original grain tuning, the
converted trick-display movie data with its font atlases and bitmaps, and the
converted session-marker display list and textures.
It excludes original
executables, native libraries, Lua programs, original game code and Skate
maps. The resulting file contains original game assets: keep it private,
outside this repository and public releases. This project does not provide
or download disc images or privately converted assets.

## Implemented scope and limits

The full upstream coordinator runs the seven-body board, physical skeleton,
animation/motion/action graphs, original controller gestures, tricks, grinds,
bails, walking/remounting, camera and scoring. CS characters follow that pose
with their own body proportions and weapon grips; the board uses the owned
GLB skin, inverse binds and embedded textures, including truck/wheel bones.
Leg retargeting solves CS limb lengths to the native ankle targets so its
longer legs do not push the boots through the board.
This replaces the initial keyboard/hull/rigid-board prototype.

CS geometry is converted to native metres/Y-up and compiled with the original
collision welding, adjacency and query metadata. Doors and intact breakables
use the upstream solid-contact and moving-query paths. CS ledges do not have
Skate-authored grind splines: exposed top/side edges provide derived splines,
an explicit crossover policy. CS materials currently use one native physical
contact material; board textures and CS arms use map lighting. Different character
proportions, ledge metadata and camera spaces still need owner playtesting.
Rolling uses the native two-truck granular player, complete slow-to-fast
recordings and original speed-position curves. The contacted Source material
selects concrete, rough concrete/asphalt, wood or metal recordings. Cosmetic
PCM crosses the trusted provider boundary into a bounded audio stream, outside
snapshots and state hashes. Recordings are prepared with the map. Foley,
grind/brake and marker cues use private samples. Master volume and focus mute
apply. The complete native contact/EQ/environment mix is still future work.
Missions, multiplayer mechanics negotiation and complete original-game
parity are not implemented. CS combat/audio remain available.

## Surf / Skate Clip Course

Install the private **0.6** pack, then open **Play → Offline with bots →
Content pack maps → Surf / Skate Clip Course**. The map is included in that
pack as `skate3_map_surf_skate_clip`; removing the pack removes its Play entry.
There is no built-in course fallback. The developer-only original map builder
lives in `tools/pack-authoring`.
No third-party map or texture, or compiled BSP, is checked into Git.
It has a high starting platform, a descending blue surf ramp, a 13,600-unit
concrete plaza with curved wood ramps, rails, ledges, a bank and a quarter
pipe, a 900-unit ramp-to-ramp gap, a 400-unit exit transfer into the second
surf ramp, a 5,800-unit finish deck and a broad catch floor. Stay fast for the
big gap; the south lane bypasses it and missed jumps have a flat catch area.
The course itself supplies bounded surf movement settings. F8 is optional.

Run forward off the start, hold toward the first ramp (A / left strafe when
looking along the course), then switch to Skate in the gap. Land, flick a trick,
then use View / Back or X after leaving the end kicker to catch the next ramp.
F7 restarts the course; native markers remain available. Both handoffs retain
airborne velocity. The private `skatecheck --route` proof drives this complete
route through the actual simulators, including a recognized kickflip, the
large skate gap and the final runout. `skatecheck --ramps` rides every ramp
approach at multiple speeds, including the reverse landing transitions.

Windows XInput reports are forwarded unchanged to the original Pad/action
mapping. WINSERVE has no connected physical controller, so automated checks
exercise actual raw reports, not a hardware playtest. Test your own controller
for hot-plugging, flick tricks, grabs, camera and mode switching.

## Public source and developer checks

The CS host and process contract remain Apache-2.0. The optional coordinator
and creator are **GPL-3.0-only**, attributed to dumbad and upstream contributors.
Public upstream is pinned to
[`2e166977`](https://github.com/SK8-ENGINE/skate-3-rust-engine/tree/2e16697797c8358d16f747a8259354ff2921041a).
Corresponding adapted source, lockfiles, notices and build instructions live in
`mods/skate-runtime` and `mods/skate` at each release's source revision.
Public installers include both notices, GPL text and exact source links.
CI builds the programs without original assets or private packs.

```powershell
./tools/build-skate-tools.ps1 -OutputDir C:/private/skate-tools
C:/private/skate-tools/Skate-Pack-Creator.exe
```

Unattended private conversion:
`Skate-Pack-Creator.exe --create <path/to/skate3rust.exe> <new/output.csrpack>`.
Existing outputs are never overwritten.

Private integration check (no fixture is committed): set `CSR_SKATE_RUNTIME`
to that build's `Skate-Runtime.exe`, then run
`cargo run -p level --example skatecheck -- <private.csrpack> [map]`.
It exercises native grinds, walking/remounting, doors/breakables, both host
tick rates, ollies, mode switching, inventory preservation, snapshot replay,
real-map movement and finite CS animation retargeting. Actual rendered
push/ollie/flick-trick captures complement the headless checks.
See [MECHANICS.md](MECHANICS.md) for the process and pack contracts.
