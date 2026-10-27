# Mechanics packs

The shipped full-runtime process protocol is version 2. Explicit Start/Pose
checkpoints keep session markers in the host's snapshot state and carry them
across mode switches, with pack identity/hash checks. Original marker geometry
validation runs again before native actor reset; markers are never hidden
process-global gameplay state. CS return placement checks the complete CS hull
against the current world before dropping the mode. Network protocol 37 rejects
mechanics/tool/checkpoint state until multiplayer pack admission is implemented.
Six exact, bounded RIFF/WAVE paths are admitted as optional private board audio;
runtime bundles still reject executable/script paths and arbitrary audio names.

Version 1 asset/recipe packs retain their existing data-only behavior. Version
2 adds `[[mechanics]]` definitions, each providing a custom-origin `mode`
ContentId, a display name, an `api_version = 1`, a `mechanics/*.wasm` module,
a `mechanics/*.csrconfig` file and an optional `models/*.csrmesh` rigid visual.
The visual also has a model ContentId in `[[resources]]` and `provides`.
All definitions participate in the same strict provides/conflict/hash checks.
Native libraries, original executables, scripts and WASI remain unsupported.

`api_version = 2` selects an explicitly supported application provider with
`provider = "skate3-v2"` and a `mechanics/*.csrdata` module. It is a private
data bundle, not an executable. The installed app chooses its own sibling
`Skate-Runtime.exe`; pack manifests cannot choose a program or arguments.
The legacy API 1 WebAssembly contract below remains available.

Installed packs are snapshotted for an offline match. Input selects only the
next admitted module; it cannot supply a module path or bytes. Simulation owns
selection, movement, opaque state, lifecycle, collision and CS combat. Changing
the installed file affects the next match, not one already running. A trap,
budget violation, missing module or invalid result returns the player to CS
movement without changing inventory. Multiplayer module negotiation is not
implemented; remote snapshots with an active mechanic are rejected.

## WebAssembly interface

Modules export `memory`, `csr_packet() -> i32` and `csr_step() -> i32`. The
pointer addresses a 384-word packet, with zero returned from a successful step.
Every word is an IEEE f32 in little-endian order. The Rust definitions and
encoding helpers are in `crates/mechanics-api`; this crate has no engine or
standard-library dependency. Source space is inches, +Z up; angles are degrees.

Packet inputs are API version, timestep, feet origin, velocity, ground contact,
view yaw/pitch, normalized forward/right controls, jump/crouch/brake, activation
reset, simulation time, 32 retained state words and 256 immutable config words.
Outputs are feet origin, velocity, contact, independent board pitch/yaw/roll,
visual rider height and 32 next-state words. See the named offsets in the API
crate. Modules never mutate CS inventory, health, weapon definitions or rules.

An optional import, `csr_v1.trace(i32) -> i32`, consumes a 21-word query in the
module's memory: start xyz, end xyz, hull mins xyz, hull maxs xyz, then the
host writes fraction, end xyz, normal xyz, start-solid and all-solid. It uses
the current player/map collision backend and `MASK_PLAYERSOLID`. Invalid
coordinates/hulls/pointers trap; a query spans at most 1024 inches on each
axis, bounding host-side broadphase work. It does not expose arbitrary host memory.

Each tick instantiates the validated module afresh. Globals, heap and hidden
memory cannot carry state across ticks; replay needs only the explicit packet
and the same immutable module/config/archive hash. The only clock is the
simulation's time. The host provides no filesystem, networking, randomness,
OS handles or WASI. Limits: 2 MiB module, 4 MiB memory, one memory/instance,
4096 table elements, 500,000 fuel units and 64 traces per invocation. Start
sections are refused. Outputs must be finite, velocity is bounded to 3500
units/second, position is bounded, per-tick displacement is bounded, and rider
height is 0..16 inches. Installation checks the container, imports and ABI
under the same limits without calling the gameplay step.

The config is a bounded JSON array of exactly 256 finite floats. A visual is
bounded JSON with `parts`: each has positions, normals, triangle indices and
an RGBA color, in Source space relative to the board. It is limited to 8 MiB,
32 parts and 100,000 vertices with validated indices/normals/colors. No visual
entry contains external paths, embedded programs or renderer-specific handles.
V2 may carry bounded `licenses/*.txt`. All special payloads must be declared.

## Trusted process providers (API 2)

`crates/mechanics-link` defines bounded, length-prefixed postcard messages.
Initialization supplies the admitted data-cache path and immutable CS scene:
static triangles, compound door/breakable convex parts and derived grind
splines. Tick frames contain the raw Xbox report, host timestep and live CS
door/breakable state. Results include movement, physical state, complete world
bone matrices, native camera and trick/score. Positions use Source inches/Z-up;
bone matrices use provider metres/Y-up with explicit rig conversion matrices.
The current message limit is 64 MiB. Provider failures return to CS movement.

Only `World::step` advances this process. It accumulates host time into the
original native integration clock. A cloneable persistent journal owns every
frame, the original spawn/momentum, archive hash and provider executable hash.
SHA-256 prefix receipts distinguish equal-length divergent histories. The
process is a cache: mismatched/restored history rebuilds the coordinator and
replays those inputs. Prepared immutable collision/animation metadata are
cached; doors/breakables update native solid contacts without recompiling the
map. Restoring long histories can be expensive. Network admission continues
to reject active mechanics until negotiation/prediction are designed for it.

Offline packs also expose a selectable inventory tool. Its selected content ID,
archive hash and primary-use release latch belong to player snapshots, while
the previous CS weapon remains retained. Primary use is consumed by the tool;
it cannot also fire that gun. Available tools come only from the match's frozen
admitted modules. Network admission rejects selected tools as well as active
mechanics. Immutable provider/map data and board materials are prepared with
the map; this preparation cannot select a mode or advance player simulation.

Rendering interpolates native local bone transforms and camera frames between
completed native steps, preserving skeleton lengths. It cannot advance physics.
The same retargeted CS pose supplies authoritative hitboxes; render overstep
only affects presentation. Mode selection, inventory and damage stay in sim.

`CSRDATA1` bundles are limited to 128 MiB, 2048 files, 240-byte paths and
64 MiB per entry. The allowlist admits the known converted GLB, animation
banks, skeleton/collection data, compiled graphs, controller/camera data and
filtered trick language. Programs, Lua, traversal, case aliases, Windows
alternate streams and external paths are rejected. Cache paths are checked
component by component; cached bytes are verified and repaired against the
bundle. Materialized data lives under the user's data folder, never the
original install or replaced application folder.

## Distribution boundary

The host contains the general API/runtime and trusted-provider connection.
The full Skate coordinator is a separate GPL Rust workspace under
`mods/skate-runtime`, shipped as an independent executable. `mods/skate`
contains the GPL creator and the historical API 1 adapter. Pinned upstream
implementations, notices, build scripts and locks are preserved. Public
packages include GPL text and exact corresponding-source links for their
revision. No original game assets are required to build these programs.
Pack creators read original content only from a player's prepared installation.
The ordinary installer/file association/drop path installs their output;
updating the app preserves it. See [SKATE.md](SKATE.md).
