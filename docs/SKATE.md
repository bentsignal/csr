# Skate on CS maps

Install a private Skate `.csrpack` by opening it, or choose **Content packs →
Install pack**. Load an offline CS map. **View / Back** on an Xbox controller,
or **X** on the keyboard, switches CS/skate without changing your character,
weapons or ammunition. The keyboard action is **Toggle movement mode** in
Keyboard / Mouse. App updates preserve your settings and private packs.

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

Use a **0.2.0** pack for the full runtime. Recreate a rushed 0.1.0 prototype
pack with the current creator; it lacks the animation/skin/graph data.
Installing the replacement updates the same pack ID.

The creator reads your converted install without modifying it. It includes
the original board skin/textures and rig reference GLB (which also contains
the original skater mesh, although only its board is rendered), animation
banks, skeleton/collection data, compiled state graphs, controller patterns,
camera definitions and selected trick-name strings. It excludes original
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
Skate sound, courses/missions, multiplayer mechanics negotiation and complete
original-game parity are not implemented. CS combat/audio remain available.

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
