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
| `weapon_zoom`, `weapon_speed` | Explicit unscoped level 0 and observed held-weapon speed matching that definition |
| `initial_movement` | Complete serialized `movement::PlayerMove` |
| `commands` | 1–4096 complete serialized `sim::TickInput` values |

Commands accept movement, view angles and jump/duck/walk only. Purchases,
combat, weapon switches, utility and tracked inputs reject this limited fixture.
All physics fields must be finite and nonnegative; the harness bounds numeric
configuration and position/velocity magnitudes at one million. These are
receipt admission limits, not asserted native cvar ranges. An actor dying
invalidates this movement-only segment.

The tool supports explicitly proven unscoped idle segments. A native callback's
weapon integer is a requested selection, not an actual held definition: capture
the held class/definition, scope and speed separately. Every Rust receipt records
its actual held identity and rejects changes during replay, including a knife
replacement by the normal loadout system. Serialized input/state fields must be
complete and exact, including explicit null optional fields; unknown keys reject.

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
The example retains its mounted VFS and calls the direct reader, which has no
fetch path. Validated installed surf map names are supported even though the
competitive menu's current map catalog filters for de_/cs_ names.

Verification passed 609 workspace tests (six ignored) and the two focused
example tests, including first-command jumping through `World::step` at both
rates. CI runs the focused example tests explicitly because workspace testing
compiles examples without running their unit tests.
Release Dust II probe and ten-round simulation each reported zero failures;
the sound audit decoded 1425 entries with no unsupported or crashing sounds.

Private installed Dust II fixtures also executed at 64 and 128 ticks: 24
commands retained the actual knife identity and emitted a jump at ordinal 8.
A repeat of the 64-tick run produced byte-identical receipts. The first fixture
started above the floor and correctly did not jump; a separate 96-command
neutral replay established each grounded Rust initial state before the jump
fixture. These inputs and states are synthetic current-Rust observations,
not native observations. Map and parsed catalog digests were checked by the
tool. Native paired trajectory comparison remains pending; no exact-movement
or full surfing claim follows from synthetic replay alone.
