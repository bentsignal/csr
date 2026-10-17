# CS2 controls translation and mouse input handoff

Implementation starts at `af8f046`. This worktree ran no Cargo, rustc, game,
render, native fidelity session or soundcheck. Root must validate the commits;
these changes do not establish native input parity or UI acceptance.

## Read-only evidence

Inspected the installed CS2 text configs only, under Steam's
`userdata/<account>/730/local/cfg/`, plus the installed
`game/csgo/cfg/user_keys_default.vcfg`. No VPK, texture, model, game UI or
game source was used. The new Rust fixtures are independently written examples,
not copies of the player's files or shipped game assets.

The user files have `config { bindings { ... } }` and
`config { convars { ... } }` sections. Bindings are sparse overrides of the
installed default file. Explicit `<unbound>` must remove an inherited binding;
unsupported commands must also remove the inherited action on that physical
key. CS2's `+left`/`+right` are movement controls and `+sprint` is walk. The
observed default file binds numbered slots 6/7/8/9/10 to dedicated grenade
selection. User overrides include wheel jumping and fire-grenade selection.

The inspected convars specify `m_yaw=0.022`, `m_pitch=0.022`,
`sensitivity_y_scale=1`. These are settings evidence, not a hardware calibration.
Bevy 0.19.1 sources were checked before extending keys/input tests:
`bevy_input/src/keyboard.rs`, `bevy_input/src/mouse.rs`, and
`bevy_winit/src/state.rs::device_event`. MouseMotion is raw unfiltered device
motion and the winit adapter forwards DeviceEvent::MouseMotion delta directly.

## Behavior and bounds

- Setup imports after existing data migration, only when `config/user.cfg`
  does not exist. Even an empty or partial existing file wins byte for byte.
  Detection/import failures log a message without preventing play.
- `CSR_STEAM_DIR` confines importer discovery to that Steam root; tests pass
  fixture roots directly. The setup integration fixture uses a subprocess with private
  CSR_DATA_DIR/CSR_STEAM_DIR/CSR_GAME_DIR values; the parent environment and
  working directory remain unchanged. No real Steam path is written.
- Prefer Steam's MostRecent login if it has CS2 config, then config modification
  time, with deterministic ties. Both files come from the same account and
  primary player/slot0. No cloud-cache/other-player files are merged.
- Read inherited bindings from the selected Steam root's listed libraries,
  then apply the user's physical-key overrides. If the default file is absent,
  omitted keys retain rewrite defaults and the receipt says so.
- Translate movement, attacks, use/reload/buy/drop/inspect/scoreboard,
  last weapon, slots 1-5, grenade slots 6-10, supported wheel actions and
  mouse buttons 1-5. Slot10 requests the actually carried Molotov/incendiary
  by catalog identity through TickInput; it never grants equipment.
- Keyboard storage still supports one key per action. Explicit user keys win
  over inherited keys, with deterministic selection and every displaced key
  recorded. Additional mouse/wheel bindings remain independent.
- Import finite sensitivity in the supported 0.1-10 range without rounding or
  clamping. Import pitch inversion only for the supported +/-0.022 magnitude.
  Custom yaw, Y-scale, zoom ratio, toggled duck/walk, aliases, execs, chained
  commands, voice, radio, hand swapping, special keys and analog remaps outside
  the supported model are retained in the receipt, never evaluated.
- The local `config/cs2-import.txt` receipt names sources, includes original
  decoded config text (including unknown sections), and records translation
  decisions. `controls_source` persists a short description in user.cfg. Source
  config files remain untouched, including unsupported values.
- First publication uses a completed, synced temporary file;
  on Windows this now uses same-directory MoveFileExW without replacement or
  copy flags, supporting FAT/exFAT portable storage as well. Other platforms
  use a hardlink and fail closed if unsupported. No writer opens the final
  name: injected partial writes leave no user.cfg and setup can retry. A racing
  saver wins without being replaced. No source is ever opened for writing.
- New sensitivity is 1 in Settings and LocalPlayer. Saved values remain in
  place and input samples the saved preference even before its first Update
  settings pass. Unscoped input keeps the existing 0.022 degrees/count law,
  without dt, DPI, speed-based acceleration, smoothing or extra gain.
- Console toggle is a saved local UI binding outside TickInput. Import honors
  `toggleconsole`, explicit unbinding of backquote, and a literal backslash key.
  Rebinding a console key clears any gameplay ownership of that physical key.
  The console binding is editable via bind/unbind and the controls page.
- Removed the unconditional V noclip shortcut and F3 HUD-debug shortcut so
  unsupported/explicitly unbound keys cannot trigger hidden actions. Noclip
  remains available by console command; detailed HUD diagnostics are opt-in
  through CSR_HUD_DEBUG=1. cl_showfps remains available.

## Controls route and remaining visual verification

The engine controls content is now shared by the fallback page and an explicit
working-classic route. OpenMouseDialog, OpenKeyboardMouseDialog and
OpenKeyboardDialog hand ownership to the binding panel, hide movie surfaces,
block movie input and reject late native options/convar writes. This route
presents only controls and its fixed Apply / Restore Defaults / Back footer;
it does not enable the unrelated fallback rail/pages. Back/Escape returns to
Home or Pause with a new presentation fence, removes the engine root and
consumes that frame's movie input so the same Escape cannot also resume.
The menu-lifecycle track touches these files too: root must reconcile this
explicit ownership/return path with its lifecycle changes, not drop either.

The dedicated vertical body scrolls with the pointer wheel or PageUp/PageDown /
Home/End, keeps offsets across redraws, and does not shrink its rows or footer.
It contains provenance, sensitivity to three decimal places, invert, all five
mouse buttons, both wheel actions, 30 gameplay bind/Clear rows and the local
console toggle bind/Clear row. Capturing the current console key is allowed.
Incrementing sensitivity preserves an imported fractional offset instead of
rounding it to tenths. No native calibration coefficient changed.

This panel is a functional candidate using existing engine controls. Its visual
appearance is NOT claimed to match the historical Scaleform reference; render,
real click/keyboard use and coherent lifecycle acceptance remain root checks.
No Cargo/rustc, game, GUI, shared target or owner config was used by this review
follow-up. The frozen owner candidate/settings remain unchanged.

Root should visually inspect 1920x1080 and a smaller supported resolution,
including all scrolled rows, long provenance, imported sensitivity 1.375,
mouse/wheel cycling, keyboard clear/rebind, Apply, Escape and reopen. Settings
changes should persist, and no held UI click should reach gameplay.

## Validation for the exclusive Cargo slot

Run focused tests first, then the mandatory complete checks from AGENTS.md:

```text
cargo test -p launcher cs2_config::tests
cargo test -p launcher setup::tests::first_run_setup_imports_fake_steam_controls_then_loads_saved_settings
cargo test -p client settings::tests
cargo test -p client console::tests
cargo test -p client menu::tests
cargo test -p client frontend::tests
cargo test -p client player::remote_input_tests
cargo test --workspace
cargo run --release -p launcher -- probe de_dust2
cargo run --release -p launcher -- sim de_dust2
```

Focused tests cover sparse/default merging, explicit unbinding, unknown command
retention, source byte preservation, existing-file preservation, repeated runs,
MostRecent account selection without mixed-account preferences, malformed files,
fully quoted literal-backslash/unbind/console fixtures, interrupted publication
and publication races, full setup followed by Settings::load, source/default
byte preservation, console focus/close/unbind, classic routing and late writes,
controls capture/Clear/Apply/reopen/Back/Escape in a private subprocess,
nonfinite/unsupported values, no interactive rebind swapping during import,
pointer option persistence, carried team fire-grenade requests, V conflicts,
and equal unscoped counts split across different numbers of render samples.
Existing camera/capture tests now expect sensitivity1. No test results are
claimed before root runs them.

Launcher tests create fake Steam and rewrite data directories under unique temp
paths. For a full setup test, use BOTH a private CSR_DATA_DIR and CSR_STEAM_DIR,
with CSR_GAME_DIR pointing at the read-only Legacy install. Never invoke setup
against the frozen owner candidate or use the real Steam directory for a write.

## Bounded native calibration requirements

No native run was performed. Root must schedule an owned helper through
`tools/fidelity/with-native-slot.ps1 -Script <absolute-helper> -NativeSlotGranted`
with existing process/prefs preservation guards. Use Legacy app4465480 as the
mouse reference; the exception here permits CS2 config reading, not asset/UI
parity claims. Preserve frozen `feedback-20261016-9cabef2-staged` unchanged.

1. Record executable/version, display mode/scale, resolution, frame/presentation
   settings, mouse DPI/polling and actual native sensitivity, m_yaw, m_pitch,
   raw-input and acceleration convars. Use owned temporary preferences and
   restore them. Do not change the owner's OS mouse preferences.
2. In an unscoped stationary view away from pitch clamps, measure signed raw
   count totals and before/after yaw/pitch. Trial totals +/-100 and +/-1000
   counts on each axis, sensitivity1 and one noninteger imported sensitivity.
   Normalize yaw wrap. Compare the resulting degrees/count and sign.
3. Repeat each total as one, ten and one hundred event groups and at different
   input speeds/frame caps. The same total must produce the same angle within
   recorded float/readback precision. Log actual totals and per-trial deltas.
4. A paired run requires measured raw HID/WM_INPUT counts or a validated raw
   count generator. Cursor pixels and SendInput relative-motion arguments are
   not proof of device counts. If the helper cannot establish raw totals, return
   that missing measurement capability instead of tuning a coefficient.
5. Keep scoped/zoom calibration separate: the current rendered-FOV ratio and
   unsupported CS2 zoom_sensitivity_ratio are not newly measured or accepted by
   these unscoped tests. Return mismatches with exact settings and count/angle
   receipts before changing any calibration law.
