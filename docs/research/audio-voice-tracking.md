# Audio voice tracking after deferred playback requests

Two private counted single-shot runs of the unchanged `52df4bc` build
(production crates equal master `3928316`, executable SHA256
`9b941bce07d446739b61dc00bd92732e506a402732a44628054c6f923d482815`)
recorded an actual Fire, a classic AK request, an unobserved missing-player
prune at that request clock, then the same entity's first observed AudioSink.
The corrected harness run exited0 with exact twelve Steam preferences unchanged,
rendered the AK/HUD29/90 at1600x900 and verified one producer Fire at tick192,
128Hz. Request clock6.979179 and sink-observation elapsed2.456ms are diagnostic
values, not a PCM onset or native firing/mixing measurement. Raw captures/logs
remain private; no game assets are included here.

## Cause and correction

`play_with` reserves an entity through Commands and immediately publishes its
voice to `SoundLibrary.playing`. The separate Update audio observer queried for
AudioPlayer before that producer's command buffer had necessarily applied,
and removed the reserved voice. The entity could subsequently play normally
while channel/entry limits no longer knew about it.

Lifecycle pruning now runs in Last. Bevy0.19.1 Main orders FixedUpdate work,
Update and PostUpdate before Last; each schedule applies its final deferred
commands by default. AudioPlugin creates playback sinks and cleans completed
DESPAWN voices in PostUpdate. Thus the Last query sees actual spawned players
or already removed/cancelled entities, rather than a producer's reserved ID.

Update still follows the listener, advances the audio clock, reapplies gains
and observes sinks. No gain, pitch, waveform identity, routing, declared voice
limit or firing timing changes. Missing players are removed even if never
observed; there is no retain-forever pending flag or guessed age timeout.
A queued AudioPlayer without a sink remains queued according to Bevy's existing
behavior; this change does not invent an audio-device/source-loading timeout.

Current owned emitters are effects::process in Update (including jump/land
helpers), foley::update in offline FixedUpdate and remote Update, and recursive
near/distant play_with. Ownerless menu/buy/frontend UI and ambient requests also
run before Last; they retain their existing untracked/channel semantics.
Any future Last audio producer must apply its commands before lifecycle prune
or choose an earlier producer schedule. A schedule configured to withhold its
final command flush would likewise need an explicit flush before this boundary.

## Verification boundary

Hardware-free regressions drive actual play_with and force the observed
producer-to-observer gap without an intervening deferred flush. They check
retention through Update, cleanup after real component removal, local five-shot
entry limit2, spatial near2/far1 independently per owner, cancellation before
first observation, and FixedUpdate/late PostUpdate requests before Last.
They do not open an audio device or prove audible stop curves.

The FixedUpdate boundary case manually runs the real FixedUpdate schedule;
it is not a full TimePlugin-driven fixed-loop test. Its second producer runs
in the App's actual PostUpdate schedule. The burst tests use actual play_with
and synthetic source bytes without AudioPlugin or an audio device.

On compiled source `8f3e0747db98561ddd8cec02a2b2ae03eaab2cd0`, focused audio
tests passed14, workspace passed613 with6ignored, release Dust II probe loaded
all1258prop models with failures0, ten-round simulation had failures0, and
soundcheck decoded1425 sounds with0unsupported/0crashing. Both exact-head
CI checks passed in run38340729464.

A private RED control changed only the test harness's prune placement back to
Update, before its pending witness, with the producer's command flush still
withheld. Local bursts failed with tracked0 versus1 and spatial near voices
failed with0 versus2; late PostUpdate voices lacked observation. Pending and
cancellation cases also failed their witness assertions because Update had
already erased Playing. Their RED result alone does not prove a cancellation
leak. Exact original source bytes were restored before full verification.

## Corrected runtime receipt

One-, three- and five-shot fixtures used the same copied play executable,
SHA256 `4932b5130bf575a6b3f6d76b014116461addd1b91793b7036259b97b229c5d9b`,
from that clean source. Each used fresh private userdata, volume0.5, bot-free
Deathmatch and ordinary counted TickInput, with an observed grounded clear-sky
ray and no impact/damage contamination. All owned processes exited0 and were
absent afterward; all twelve Steam preferences remained byte-identical.

| Actual fires | Producer ticks at128Hz | Clip after final shot | Oldest-voice steal commands |
| --- | --- | --- | --- |
| 1 | 192 | 29 | 0 |
| 3 | 192,205,218 | 27 | 1 |
| 5 | 192,205,218,231,244 | 25 | 3 |

Every Fire produced its verified classic AK near alias request and an actual
sink observation, with pitch100, cue volume0.9, master0.5 and initial gain0.45.
No gun voice was pruned before observation. Shots three through five removed
the oldest tracked voice at their replacement request clocks; surviving voices
were subsequently reaped in Last with prior player observation confirmed.
These are request/sink/command observations, not PCM onsets or measured audible
stop curves. The local nonspatial gun requests did not invent a distant layer.

Private run identifiers are `032647-80237b1d` (single), `032815-16a484d1`
(three) and `032919-0651e331` (five), all on2026-10-13. Their1600x900 PNGs were
independently inspected: sky-facing Dust II, rendered AK/hands, health/armor100
and ammunition29/90,27/90,25/90. PNG SHA256 values respectively:

- `dfbc1d8c49033d72975e0303dd68687206a59ae8ac7b1191dacbea33ddbd7815`
- `233527d0ca9f4a7d6d42636c5d181809d116a70066c0f1a756a4e1f5e750583d`
- `6834264b96fcba551bd6a25abea3205fb959116b1677252f9717a36c10dd53d7`

Raw logs, provenance, preference receipts and screenshots remain private.
This documentation checkpoint changes no compiled source or Cargo inputs.
Matched process-loopback PCM and native historical mix admission remain
separate. The owner's reported AK tone/overlap mismatch is not declared fully
fixed by a tracking correction.
