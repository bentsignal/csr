# Skate on CS maps

Install a private Skate `.csrpack` by opening it, or choose **Content packs →
Install pack**. Load an offline CS map. The **Skateboard** joins your inventory
on the right HUD. Cycle to it with the mouse wheel or Xbox LB/RB, then use
**left click / RT** to skate. Your CS gun returns after releasing the button.
Cycle to the board and use it again to return to CS; while skating, D-pad
Up/Down cycles inventory, and RT uses the board when selected.

**View / Back** on Xbox or **X** on the keyboard remains a direct movement
shortcut. The keyboard action is **Toggle movement mode** in Keyboard / Mouse.
Camera travel animates in both directions, follows movement and clips against
the map. App updates preserve settings and private packs. The older 0.2.0 pack
continues to work; install a private 0.3.0 replacement to add skating sounds.

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
2. Choose **Content packs → Create Skate pack** in CSGO Rewrite. Select
   `skate3rust.exe` in that prepared installation, then a new output `.csrpack`.
   The installed creator needs no Rust, terminal or manual file copying.
3. Open the pack to install it, load a CS map, and press View / Back or X.

Use a **0.3.0** pack for the full runtime with board audio (0.2.0 remains
compatible without sounds). Recreate a rushed 0.1.0 prototype
pack with the current creator; it lacks the animation/skin/graph data.
Installing the replacement updates the same pack ID.

The creator reads your converted install without modifying it. It includes
the original board skin/textures and rig reference GLB (which also contains
the original skater mesh, although only its board is rendered), animation
banks, skeleton/collection data, compiled state graphs, controller patterns,
camera definitions, selected trick-name strings and six converted board-foley
clips for rolling, popping, landing, pushing, grinding and braking. It excludes original
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
an explicit crossover policy. CS materials currently use one native contact
material; board textures render unlit in the CS renderer. Different character
proportions, ledge metadata and camera spaces still need owner playtesting.
The current sound pass uses private original samples, with pose-driven events
and rolling/grind/brake loops that follow speed and stop on mode exit/pause.
It follows the game's master volume and focus mute setting. It does not yet
reproduce the complete native grain/DSP mix or surface-specific audio.
Courses/missions, multiplayer mechanics negotiation and complete original-game
parity are not implemented. CS combat/audio remain available.

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
