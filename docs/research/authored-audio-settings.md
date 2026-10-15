# Authored Audio Settings and real playback

The current original Audio Settings page displays only Master Volume and a
large empty black panel. Its installed declarative `scripts/audio_options.txt`
contains more rows, but our widget adapter admits only settings with working
backends. This track connects additional real playback controls and preserves
the original widgets, labels, ranges, callbacks and navigation. It does not add
dummy controls, replacement UI, assets or authored movie instructions.

## Initial contract and backend inventory

- Master `volume` already persists and updates existing ordinary/spatial sinks.
- `snd_mute_losefocus` is an installed original choice with inverted Yes/No
  values. Current playback does not apply focus muting; this can be connected to
  actual window focus without changing the saved master volume.
- Installed music rows name menu, round-start, round-end, objective, ten-second,
  death-camera and MVP gains. Music must actually play through the corresponding
  gains before these become enabled options. Original music declarations and
  available game events are the next evidence gate, not guessed filenames.
- VOIP/caster controls require real voice transport, not bot voice-line gain.
  Device/output/HRTF controls likewise require actual backend support. Bevy
  0.19's current output resource is private and opens its default device; an
  original dropdown alone would not make device switching work.
- The empty black panel is an unfinished original subcomponent. Discover
  its public layout/visibility contract rather than inventing microphone input
  or an empty replacement graphic.

Settings additions should be delegated to a separate audio-options module,
with minimal integration in shared parsing, serialization and numeric-convar
methods. Existing master settings, keyboard bindings and slider drag ownership
must remain intact. Console/UI changes must affect the same persistent values.

The installed retained Classic definitions do not establish an exact2015 option
catalog or sound mix. Historical option availability, music-event timing/gains,
full rendered page content, real playback, persistence, focus transitions and
Home/Pause navigation remain verification gates. No fidelity pass is claimed at
this initial checkpoint.

The first implementation checkpoint adds the original `snd_mute_losefocus`
choice and persistent audio-options delegation. It applies zero actual playback
gain while unfocused when selected, retaining the saved master value and
restoring current sounds on focus return. Focus-only changes do not reapply
display/material settings. Frozen `d2dbc42` passed focused audio-options and
settings tests, the PLAY launcher build, and soundcheck (1425 decodable sounds,
zero skipped or crashing). Actual original-widget rendering and playback/focus
verification remain pending; this is not an overall Audio Settings pass.

Original-only music metadata is available under
`sound/music/valve_csgo_01/game_sounds_music.txt` in the Legacy VPK. The retained
item definitions currently label id1 as `valve_cs2_01` and original-CSGO as70;
neither a first-entry/default-id assumption nor any CS2-named music is admitted.
Only explicit original-CSGO declarations can supply future cue paths/gains.

The black-center diagnosis also remains open. Public placement metadata names
`ResizeTop.ResizePanel`, horizontal/vertical resize controls, `DeadZone` and
safe-zone brackets, with no microphone-named placed clip. The actual Audio trace
queries safe-zone bounds. The "microphone panel" description was an inference;
public runtime geometry/visibility must identify the stray art before changing
its ownership. No timeline action bodies were inspected.
