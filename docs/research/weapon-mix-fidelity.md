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

The trace also reports a tracked voice missing AudioPlayer at prune time and
whether a prior Update observed that player component. An unobserved missing
component alone does not prove a pending spawn; the hardware-free regression
deliberately withholds a command flush to isolate that case, then separately
removes an observed voice. It does not establish the production schedule order
between the independent effects/audio chains or change pruning policy.

The causal tests distinguish a queued player from an actual sink without an
audio device, and coalesce each request's first sink observation. Combined
verification source `c241d64bb52a1ce18872a43f2392d246e30e157c` contains the exact
trace and counted-shot leaves plus movement master `aa183c1`: 609 workspace
tests passed, six ignored; release Dust II probe and ten-round simulation had
zero failures; soundcheck decoded 1425 sounds with zero unsupported/crashing.
Both published leaf heads passed CI before merging as PR69 and PR71. These
checks do not establish actual game PCM onset or historical mixing parity.

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

The initial sparse private `-game` fixtures failed to reach netconsole within
the bound. A fresh fixture with explicit private-first MOD and installed
Legacy MOD fallback loaded successfully while keeping DEFAULT_WRITE_PATH
private. Its loose historical files did not produce historical mixer values.
A separate private VPK containing exactly the same three historical files,
byte-for-byte, did: FirstWeapons volume/level/DSP 0.31/1/0 and DistWeapons
2.8/1/0, with no matching Weapons1 query output. This proves those effective
mixer declarations; it does not prove the weapon cue or operator graph.

The VPK gate's complete startup condebug log contains 76 operator KeyValues
parse errors and an unable-to-load warning before netconsole connected, plus
`Weapon_AK47.Single` precache failure (no such sound-script entry). An explicit
operator flush later repeats 76 parse errors. The netconsole-only pre-flush
log had zero errors because it omitted startup; it cannot support a successful
startup interpretation. `snd_sos_print_operators` lists available operator
types, not proof that an imported weapon stack was accepted. The historical
cue/graph admission gate therefore failed despite mixer acceptance.

A subsequent same-writer inline VPK control contained byte-identical installed
versions of those three scripts. Its full startup log also had operator parse
and cue-precache failures (96 parse errors). This does not isolate older
declaration syntax as the cause. The earlier normal 2023 control had no
condebug startup capture; its later netconsole output cannot establish stock
graph acceptance either. A fresh no-VPK/no-script-override private MOD
counterpart with full startup capture subsequently exited zero. Its complete
35,495-byte startup log has zero operator KeyValues errors, operator-load
warnings or `Weapon_AK47.Single` mentions. Unrelated precache and command
warnings remain. Read-only mixer queries produced no group rows and operator
printing was refused with cheats disabled; absence of warnings does not prove
cue execution, effective mixer values or graph acceptance. Three actual Legacy
module paths/hashes were verified. Twelve preference targets and the 83
top-level installed configuration files snapshotted by this run were unchanged;
it did not take its own recursive configuration snapshot. Private proof ID is
`native-stock-mod-control-gate-20261013-055513`, full-startup SHA256
`514f77e67e352e7abdd85977499f466a3591db6d56a9e728ad5848013bd38c20`.
Actual loader bytes and cue execution remain separate qualification steps
before historical PCM or numeric mixer changes.

A separately executed numbered-VPK stock counterpart also completed the same
read-only loading/query sequence, with the same three installed script inputs.
Its 35,951-byte complete startup log likewise has zero operator KeyValues
errors, operator-load warnings or AK single-cue mentions; the same unrelated
warnings, empty group queries and cheat refusal remain. It exited zero,
preserved twelve preference targets and all 89 recursively snapshotted installed
configuration files, and verified three actual Legacy module hashes. Private
proof is `native-numbered-stock-readonly-gate-20261013-063450`, startup SHA256
`0c3ab11eefee236ff3a0203d8490f92f132f8075e070e9a6dd99358a6481e2de`.
Search-path output alone does not establish which VPK entry the native loader
read. The contrast with the failing inline containers motivates direct file
fingerprints; it does not yet identify a container defect or admit a cue graph.

Private startup-log SHA256 is
`40a9c00f54f7da1d285e4d83930d3a0761023d3871f78dd1b4b3ea3cc2df4c82`;
netconsole-log SHA256 is
`dd571691195dfa110666b7c371b581ed54ae37df548447bbf50060803289b1f7`.
The VPK SHA256 is
`a217525c43be63198f55e7dba15261e881d086ff1e32010523dd91adb358f213`;
its three extracted entry hashes match the historical inputs above. Raw
scripts, container and logs remain private. No dated numeric/schema fields
were changed to force loading. Text grammar validation by our own parser is
not equivalence with the native engine's declaration loader.

The native VPK session exited 0, restored all twelve preference targets,
kept installed loose configuration unchanged and removed 185 new known
byproducts. Team 3, health 100 and zero velocity were observed, but the exposed
VScript player binding lacked GetActiveWeapon; active weapon/ammo identity
remains unproven. No firing or game PCM capture occurred. Native2023 reading
dated2015 data is not a2015 executable; actual mix comparison remains pending
effective cue/graph acceptance, shooter identity and controlled captures.

The intended comparison uses quiet baseline, one/three/five acknowledged shots,
exact wave fingerprints and full tails, then compares PCM onset, stereo identity,
spectrum and overlapping envelopes. Counts come from actual shots/ammo/cue
evidence, not presumed command sleep durations. No historical DSP/EQ, hidden
layers or stop curves are inferred from names alone.

Matched CSR single-, three- and five-shot process-output controls are recorded
in [audio-voice-tracking.md](audio-voice-tracking.md#matched-burst-output).
They establish an observed burst-overlap change after the deferred voice fix;
they do not establish original-game mixing parity.
