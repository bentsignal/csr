# Authored Audio Settings and real playback

The current original Audio Settings page displays only Master Volume and an
empty microphone panel. Its installed declarative `scripts/audio_options.txt`
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
- The empty microphone panel is an unfinished original subcomponent. Discover
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
