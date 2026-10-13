# Classic weapon mix: declaration audit and onset diagnostics

The verified classic waveform policy currently replaces sample bytes while
retaining installed Legacy mixing metadata. Matching the waveform alone does
not establish the original firing mix, overlapping tails or DSP. This audit
leaves those claims open; it does not introduce gain or EQ changes.

## Declarative evidence

The dated shipped-data capture is SteamTracking/GameTracking commit
`fcb66c41571b716820b1e1c148e0cba9a318f772` (2015-12-31). It is a public archive
of shipped configuration, not an official Valve-hosted repository or an engine
implementation. Raw scripts remain private. Installed comparison data is
CS:GO Legacy 1.38.8.1, not CS2.

| Declaration | Dated 2015 | Installed Legacy |
| --- | --- | --- |
| AK near cue volume / level | 0.7 / 100 dB | 0.9 / 79 dB |
| AK near mixer group | FirstWeapons | Weapons1 |
| AK volume graph third input | constant 0.55 override | mixer volume output |
| Default mixer FirstWeapons / Weapons1 volume, DSP | 0.31, 0 / absent | absent / 0.7, 1 |
| Default mixer DistWeapons volume, DSP | 2.8, 0 | 0.7, 0.3 |
| Suppressed M4 near cue volume | 1.0 | 0.9 |
| Suppressed M4 distant partner | declared | absent |
| Suppressed M4 falloff maximum | 1377 | 800 |

AK's constant replaces the imported mixer-volume input. Multiplying it by
FirstWeapons volume again would misread the graph. These rows are graph inputs,
not a verified scalar reconstruction of the native output. The modern renderer
also applies its measured category gain and distance curve and currently has
no implementation of the declared operator DSP or speaker graph.

Both inspected historical weapon cues use normal pitch with no random range.
The imported near start stack declares an entity-specific limit of two matching
entries; the AK distant stack declares one. Whether native stop semantics count
the newly starting entry, truncate immediately or ramp out remains a measurement
question. Historical M4 distant routing stays audit-only pending observation;
the existing near-only classic policy is preserved.

Private source hashes:

- Historical weapons script: `5f222a78d10dac7acd1109317ae7e2519f7a400e72013591029fdbbf4b5dbf41`.
- Historical operator stacks: `dafa200b72aeedbf791b422a77afd1dd0edd44cfe82c21d88e1bc0050cf0e74b`.
- Historical mixers: `5c6d8f3503ff54249811d35262ad520bee7f441aa1ecee149a9da02e7b237f61`.

## Producer and timing audit

`effects::process` dispatches the firing cue from `GameEvent::Fire`. Foley
schedules draw/reload sequence sounds, without a PrimaryAttack sound path.
Installed AK, M4A1-S and M4A4 viewmodel metadata each contains three attack
sequences with zero client-sound events; the listed events are muzzle/brass.
This does not support duplicate attack animation audio for these models.

`PendingEvents` retains events without individual Fire tick stamps. Several
fixed ticks can drain in one rendered frame. AudioPlayer spawn and old-voice
despawn are deferred commands; Bevy creates the playback sink later. A
render-time world tick cannot recover an individual shot's original tick.

`CSR_AUDIO_TRACE=1` adds opt-in logs for the queued request's entity, audio
clock, entry, wave, owner, channel, pitch and initial gain; voice-steal commands;
and the first Update that actually observes a sink. Request elapsed time is a
monotonic diagnostic interval. Neither request nor sink observation is a shot
tick or audible PCM onset. No numeric mixing policy changes accompany this
instrumentation.

The causal tests distinguish a queued player from an actual sink without an
audio device, and coalesce each request's first sink observation. Compilation
and integrated verification are pending the serialized build slot.

## Reference capture preparation and limits

A private recorder uses Microsoft's documented process include-tree loopback
API with exact PID/executable/creation-time ownership, bounded capture and
cleanup. Its quiet synthetic stereo pulses were recovered with zero reported
discontinuities/timestamp errors; a separate owned sibling's frequencies were
excluded. Wrong PID creation time and executable were rejected before recording.
No system volume, audio endpoint or microphone setting was changed. This is
process-loopback PCM, not a hardware output or raw engine-source tap.
[Microsoft process-loopback sample](https://learn.microsoft.com/en-us/samples/microsoft/windows-classic-samples/applicationloopbackaudio-sample/)
and [packet timestamp contract](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iaudiocaptureclient-getbuffer)
describe the public API used.

A private `-game` fixture mounts only its copied dated declarative scripts and
the player's actual Legacy content. Its first load gate produced a black
window and no netconsole within the bound; it was shut down and exact native
preferences were restored. Installed loose configuration stayed unchanged.
No shots or native mix acceptance were established. Even a successful future
fixture measures the 2023 engine interpreting dated declarations, not a 2015
executable. Native sound/PCM comparison remains pending.

The intended comparison uses quiet baseline, one/three/five acknowledged shots,
exact wave fingerprints and full tails, then compares PCM onset, stereo identity,
spectrum and overlapping envelopes. Counts come from actual shots/ammo/cue
evidence, not presumed command sleep durations. No historical DSP/EQ, hidden
layers or stop curves are inferred from names alone.
