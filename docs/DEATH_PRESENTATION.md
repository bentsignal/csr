# Death presentation: measured data and remaining seams

This records a read-only audit on 2026-10-12 of the installed CS:GO Legacy
app 4465480, version 1.38.8.1 (October 2023), and client source
`30a83c8756ff00e45f0a87fbe18dcf777ab6291f`. The owner's current fidelity
target is 2015. These installed files establish available content and
metadata; they do not establish exact 2015 death behavior. No native death
sequence, original kill-feed invocation, or rendered death-notice layout
was observed in this audit.

The readers inspect model headers, animation descriptions, PHY geometry
and key/value metadata, SWF tags, and named function declarations. They
skip original function bodies. Raw assets, extracted images, game scripts,
account identifiers, and private logs are outside the repository. See
[FIDELITY.md](FIDELITY.md) for the evidence workflow.

## Current behavior

`characters::update` explicitly includes `p.alive` in visibility. Dead
players' body and held-weapon meshes become hidden, and their pose update
is skipped. The character cache survives until actor removal or team
change. It has no corpse ownership or life-epoch tracking. The immediate
disappearance is a renderer decision, rather than evidence that death
content is missing from the installation.

The kill feed already exists. `effects::process` consumes authoritative
`GameEvent::Kill`, formats attacker/victim names, weapon and headshot into
text, and retains at most six entries for six seconds. `hud::setup` renders
white 16 px text at top 60/right 16. `hud::menu_visibility` hides all
gameplay HUD during menu/loading. This is a plain-text feed; the classic
death-notice movie is not requested. Existing `ui_assets` weapon icons come
from Panorama paths.

The menu worker becomes inactive during ordinary gameplay after Pause
prewarming. Requesting an additional movie through that worker alone
would not provide a continuously active gameplay HUD.

## Installed character poses and physics

| Definition | Body bones | Included animation bones / sequences | PHY solids / constraints | Matching MDL/PHY checksum |
|---|---:|---:|---:|---:|
| Legacy SAS CT | 98 | 85 / 272 | 16 / 15 | 4090653947 |
| Legacy Phoenix T | 86 | 85 / 272 | 16 / 15 | 3290697062 |

Both body models advertise a `ragdoll` / `ACT_DIERAGDOLL` sequence. That
declaration does not demonstrate how the original game creates a corpse.
Their included animation sets contain standing/crouched body and headshot
death poses. Each uses a 9 by 1 `death_yaw` grid spanning 180 to -180
degrees, with directional entries S/SW/W/NW/N/NE/E/SE/S. Each underlying
clip is one frame at 30 fps. These are static directional poses, with no
authored falling duration. Two of the 85 bone weights are zero. Selection,
blending, and the direction convention remain unmeasured.

The separate `deathpose_lowviolence` clip has 97 frames at 30 fps and no
activity declaration. Its existence does not justify substituting it for
normal death dynamics.

Both PHY files contain 16 separately owned type-0 VPHY hulls. Fourteen
have 26 points/48 triangles; the two ankle hulls have 33 points/62
triangles. Fifteen constraints form a connected rooted tree over solid
IDs 0 through 15. Every solid name binds to its corresponding body
skeleton. The named mass values sum to 300.000016, compared with the
authored `totalmass` value 300.000000. These are file values, not a measured
human-body mass or a calibrated solver.

Collision metadata contains 202 ordered pairs representing 101 distinct
unordered pairs, with no invalid or self pair. Preserve the authored
ordered entries while their interpretation is investigated. Joint-merge
metadata names the left and right ankle/ball pairs. Hull-to-bone basis,
constraint axes, collision-rule meaning, impulses and the original
solver response still need public-format validation and observed behavior.

The current `phy` reader flattens hulls and retains untyped metadata;
static prop loading uses the hulls. An additive bounded typed parser can
retain solid IDs, hull ownership, skeleton names, checksum, physical
values, constraints, collision pairs and joint merges without changing
that existing API. Parser validation is a separate step from implementing
ragdoll dynamics.

## Original classic death-notice metadata

The loose `huddeathnotification.swf` has 1280 by 720 authoring bounds,
30 fps and two root frames. It exports `death-text` and 125 bitmap icon
symbols, including normal/grey weapon variants, headshot, penetration,
suicide, revenge and domination icons. Its imports include `$TextFontBold`
from `gfxfontlib.swf` and `WeaponPNGIcons` from `iconlib.swf`.

The sole dynamic text definition is 16 px, right aligned and HTML enabled,
with authored bounds (-2, 498, -2, 24.05). Its default RGBA value is
(157, 81, 27, 255). This default does not establish runtime team colors,
local-kill highlighting, backgrounds, screen position, or scaling.

Only these named declarations were read:

| Declaration | Declared parameters |
|---|---|
| `AddPanel` | none |
| `SetPanelText` | `notice`, `text`, `isVictim`, `isKiller` |
| `RemovePanel` / `UpdateWidth` | `movieClip` |
| `onLoaded` | none |
| `onUnload` | `mc` |
| `onResize` | `rm` |

They do not prove native call arguments, return handles, invocation order,
loader key, or lifecycle callbacks. All action tags were skipped by the
display-metadata reader. Before wiring the original movie, observe its
actual loading and panel creation/update/removal sequence, image markup,
colors, timing, resizing and match-reset behavior. Do not reconstruct an
original function implementation from these declaration names.

## Authority and lifecycle boundary

Player snapshots contain `life_epoch`, but client interpolation currently
discards that remote epoch when constructing its `Pose`. `remote::drive`
restores the latest snapshot and flattens reliable `EventBatch` events
into `PendingEvents`, losing the batch tick. `Kill` carries attacker,
victim, weapon and headshot; it carries no victim life epoch, death pose,
hit direction or impulse.

A delayed kill event can arrive after the victim has respawned. A corpse
cannot safely bind to that actor's current pose, infer the original force
from a current attacker, or borrow an unrelated impact event. A reviewed
authority-stamped death receipt must supply the required life/timing and
pose context before effects can cross snapshot boundaries. Deterministic
prediction/replay must not produce duplicate death effects.

Corpse entities should have bounded ownership keyed to player, life epoch
and scene/session generation, with explicit removal, respawn, round and
teardown rules. Keep them separate from the surviving player's mesh and
from authoritative dropped weapons. Renderer systems must not mutate
gameplay or damage state.

A future authored death-notice adapter also needs immutable receipt
context for names, teams and local involvement. Current player lookup can
refer to a new life or return `?` after removal. Escape untrusted names
before supplying HTML text, bound pending events/panels, and clear them on
match/session teardown. Give its gameplay HUD surface explicit active
ownership rather than relying on the hidden menu worker.

## Evidence and next verification

Private metadata lives in the `death-presentation-metadata` evidence
folder. These SHA256 values identify the audit inputs and derived records;
they are not downloadable assets.

| Record | SHA256 |
|---|---|
| CT body MDL | `9acf932fc04823ba6984f338c5e30c039b7dd71036bc115e64e797c5ece599bc` |
| T body MDL | `0759190f479d6fcc1a96e4e3de90c759c64d7c4335d961b61e2447ae7d022d0f` |
| CT PHY | `d83e6ec19cbcaf902341b94d3f71c43e48fa3dbf92bc71e16daafe0f72784111` |
| T PHY | `3e80f33ca66ec66b456c6c9a4d4324a942bb105eaa27d0a846fcf58fec4b9360` |
| Classic death-notice SWF | `7612c49c53ac0c86593f31ed85738d3d78ae3b85afbfd3a8a49be861c67cc923` |
| Model/PHY metadata JSON | `58d70d46866816e5852b9b8f63425d2e0c40a9bdacba5ff358381feaaec11fcd` |
| CT/T sequence reports (each) | `4f1ebf01e95d348de0f14e84a318a28497f63c9b6186b42a481ccd9e05e7d480` |
| Named death-notice declarations | `91a5e939e6e975c3c2ab59bc689cf3c2aec10fb30d8bc5fa1963983911c5772d` |
| Death-notice display metadata | `9334cc2c60cfd2e13bbfccf75db3f15aa935fe7155388c3ea9f5ad67229cbcf0` |

The sequence cross-check used the existing headless `seqlist` tool; it is
not a newly built exact-tree test or a rendered animation proof. The SWF
tag meanings follow the [Adobe SWF format specification](https://www.flashrealtime.com/content/dam/Adobe/en/devnet/swf/pdf/swf_file_format_spec_v10.pdf).
No unavailable documentation or original game implementation was used to
assign physics behavior.

The next verifiable steps are bounded parser tests with synthetic malformed
inputs and installed skeleton/checksum invariants; authority receipt tests
covering delayed kills, respawn, actor removal and cancellation; then
coordinated native death/death-notice observations and matched rendered
proofs. Exact 2015 physics, visual timing, colors and call contracts remain
open. This audit closes no ragdoll, HUD or fidelity milestone.
