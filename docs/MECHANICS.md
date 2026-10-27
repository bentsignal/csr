# Mechanics packs: API 1

Version 1 asset/recipe packs retain their existing data-only behavior. Version
2 adds `[[mechanics]]` definitions, each providing a custom-origin `mode`
ContentId, a display name, an `api_version = 1`, a `mechanics/*.wasm` module,
a `mechanics/*.csrconfig` file and an optional `models/*.csrmesh` rigid visual.
The visual also has a model ContentId in `[[resources]]` and `provides`.
All definitions participate in the same strict provides/conflict/hash checks.
Native libraries, original executables, scripts and WASI remain unsupported.

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

## Distribution boundary

The host contains only the general API/runtime. The Skate adapter is a separate
GPL Rust workspace under `mods/skate`, compiled to WebAssembly and embedded in
the separate GPL pack creator. Its pinned upstream implementation and license
are preserved. No original game assets are required to build either program.
Pack creators read original content only from a player's prepared installation.
The ordinary installer/file association/drop path installs their output;
updating the app preserves it. See [SKATE.md](SKATE.md).
