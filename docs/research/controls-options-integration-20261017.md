# Controls and Options integration handoff (2026-10-17)

Source integration only; no compiled or runtime acceptance is claimed. Work is
isolated on `fix/controls-options-integration` in
`C:/Users/shawn/.t3/worktrees/global-offensive/controls-options-integration`.
The root release build at `cc66e5e` and its checkout were not touched.

## Root apply order and replacement mapping

Start with the five original controls commits, in exactly this order:

```text
cf29e1ca9c426c022cf55b204bc6eef5938ddcf3
8f3ddae25cb7e7202c922564e478d1b0f5bd88e2
1791f83c8205a2ddf1eab28ce3f352aca758a63d
80e635341eecae69f6ad497d7fff3532103fa30d
c51987b4a7be42581771fd290e79fc6c46a804cc
```

Then apply the two rebased lifecycle commits below, followed by this branch's
final scoped-defaults fix (which contains this handoff). The final agent reply
supplies the complete ordered series including that fix's immutable hash.

| Original input | Integrated replacement |
| --- | --- |
| `92cbd5d0b82ae5a9a2b1120b09a246e3d61d0f25` | `ec9c31bf2ae45479161472f6f59e599fe09476db` |
| `42b9b8fe5cf417579f2ce69ce04a934b0babb2b6` | `4dea1b4346b4d61590eb848136a3e1435c978233` |

`a1ae85f2246ee805b4ab1d2615f48a1cfb6e6fae` has the same tree and parent as
92cbd5d; it is an alternative input, not another layer. Do not apply either
original lifecycle input or original 42b9b8f after these replacements. The old
7298/9d89/ecfa controls series is superseded by the five controls commits above.

## Integration resolutions

- Kept `menu: Res<crate::menu::Menu>` in `frontend::input`: controls ownership
  and just-closed input suppression require it. Lifecycle removed the resource
  without a textual conflict, which would leave undefined `menu` references.
- The frontend root display requires both `!menu.classic_controls` and the
  lifecycle `front.menu_presented()` readiness/fence check. Controls hide the
  movie surface; callback-era readbacks cannot bypass presentation admission.
- Initialized `controls_open: false` in every lifecycle-added Data fixture,
  including the fresh/adopted worker fixture added by 42b9b8f.
- Kept both controls and lifecycle regressions, the receiver captured at the
  Escape decision, real shim fixture, and fresh/adopted readback fixtures.

## Restore Defaults scope

Both the working classic controls footer and fallback Controls footer dispatch
`Action::ControlsDefaults`; other settings pages retain their existing action.
`Settings::restore_controls_defaults` resets all 30 keyboard actions, the local
console key, all five mouse actions, both wheel actions, sensitivity (1), and
pitch inversion. It clears the short controls-source label because the active
controls are now rewrite defaults. It does not erase the existing import receipt.

Audio volume/options, video/display/exposure, crosshair, radar, FPS display,
menu blur, blood, hitmarker and close-on-buy preferences remain unchanged.
Raw input is always the existing raw Bevy device-motion path; this stack has no
saved raw-input toggle or zoom-ratio setting to reset. Scoped scaling retains
the existing rendered-FOV behavior. Unsupported imported zoom/custom-axis
values remain in the import receipt; no new support or native calibration is
claimed. In particular, the inherited 0.022 coefficient is not accepted native
measurement evidence.

The new `controls_restore_apply_reload_preserves_unrelated_preferences` fixture
uses a child test process with a unique fake temporary working/data directory.
It gives every unrelated persisted field a nondefault sentinel, modifies every
controls category, dispatches the actual Restore and Apply menu actions, reloads
Settings from disk, and reopens controls for both Home and Pause owners. It also
checks Restore does not save before Apply. The existing controls layout fixture
now requires a controls-only defaults button and rejects the all-settings one.
These fixtures were authored and parsed, not executed here.

## Retained controls/import behavior

The integration retains literal-backslash console binding, removal of hidden V
noclip and F3 debug shortcuts, existing-file preservation, same-account/slot0
read-only CS2 import, completed synced temporary output and atomic publication
without replacement. Import code was inspected and left unchanged. No Steam or
owner configuration was opened for writing or used by a runtime fixture here.

## Checks performed and pending

Performed only source inspection, `git diff --check`, and `rustfmt --edition
2024 --emit stdout --config skip_children=true` parsing on all 12 Rust files
changed from af8f046. Formatting output was discarded; sources were not globally
reformatted. No Cargo, build, rustc, test execution, native/game/UI run,
shared-target access, push or PR was performed. The original worktree remains
clean and unchanged.

Root owns compilation and execution in its exclusive build slot, including all
focused checks in [the controls handoff](cs2-controls-import-2026-10-16.md) and
[the lifecycle handoff](menu-transition-lifecycle-20261016.md), plus:

```text
cargo test -p client controls_restore_apply_reload_preserves_unrelated_preferences
cargo test -p client controls_body_scrolls_independently_of_footer_and_keeps_every_binding_row
```

Then complete the required workspace/probe/sim checks. When switching build
worktrees, follow AGENTS.md's explicit-profile, absolute-target cleaning and
immutable executable verification rules. This child did not acquire that slot.

Runtime acceptance remains pending: cold Options; immediate close/reopen;
Home/Audio/Home and Audio/Play/Offline Bots; Home/Pause controls Back and Escape;
keyboard press/release and OS repeat while Options is cold; held UI clicks;
retained/delayed cancel/unload callbacks; delayed GPU readbacks in fresh/adopted
workers; and production controls at 1920x1080 and a smaller supported resolution
with all rows reachable, capture/Clear, mouse/wheel cycling, Restore/Apply/reload
and reopen. Inspect intermediate and settled captures and existing Home-return
timings. UI16-01/02 and complete historical visual/input parity remain open.
