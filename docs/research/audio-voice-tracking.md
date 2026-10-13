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

Focused compilation, workspace/release/decode checks, exact-head CI and granted
actual1/3/5 request/sink/render measurements are pending this source checkpoint.
PCM/native historical mix admission remains separate. The owner's reported
AK tone/overlap mismatch is not declared fully fixed by a tracking correction.
