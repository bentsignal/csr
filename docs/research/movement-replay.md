# Movement reference replay

The developer-only `movement_replay` launcher example runs an explicit command
list through `sim::World::step` on an already mounted map. It does not change
player setup, add a movement preset or implement map push/teleport triggers.
Native command-phase alignment and 2015 behavior remain measurement questions.

Run `cargo run -p launcher --example movement_replay -- <private-fixture.json>`.
The fixture and its output belong outside the repository. The example prints
JSON receipts; it neither writes native settings nor launches the original game.

Schema 1 requires exactly these fields:

| Field | Meaning |
| --- | --- |
| `schema` | Integer 1 |
| `map` | Already mounted BSP name |
| `map_sha1`, `catalog_sha1` | Digests of the exact BSP and parsed item schema |
| `tick_rate` | 64 or 128 |
| `config` | Complete serialized `movement::MoveConfig`; no omitted defaults |
| `weapon` | Definition in that catalog; supplies held-weapon movement speed |
| `initial_movement` | Complete serialized `movement::PlayerMove` |
| `commands` | 1–4096 complete serialized `sim::TickInput` values |

Commands accept movement, view angles and jump/duck/walk only. Purchases,
combat, weapon switches, utility and tracked inputs reject this limited fixture.
All physics fields must be finite and nonnegative; the harness bounds numeric
configuration and position/velocity magnitudes at one million. These are
receipt admission limits, not asserted native cvar ranges. An actor dying
invalidates this movement-only segment.

Each receipt records the pre-step producing tick, successor published tick,
actual movement state and movement events. A command list runs through the
same simulation boundary as gameplay. It does not establish that one observed
native server tick contained exactly one command, that a native post callback
matches our published phase, or that its initial crouch/stamina state is known.
Those facts must be demonstrated by the separate native observer contract
before building a comparison fixture. Preserve native observations that cannot
be mapped yet rather than silently replacing them with defaults.

BSP identity uses the loader's retained digest, before pakfile mounting; catalog
identity uses the exact input successfully parsed afterward. Keep the native
capture's stronger private provenance alongside the fixture. Equal map names
alone do not admit a comparison, and a missing catalog identity rejects replay.
The example refuses to fetch an absent map as a side effect.

Current source preparation includes rejected-command checks and actual
first-command jumping through `World::step` at both rates. Compilation,
installed-data execution and native paired trajectory comparison are pending.
No exact-movement or full surfing claim follows from synthetic replay alone.
