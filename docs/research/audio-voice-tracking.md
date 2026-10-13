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
Matched process-loopback PCM and native historical mix admission are separate
checks. The owner's reported AK tone/overlap mismatch is not declared fully
fixed by a tracking correction.

## Process output control and bot render smoke

Private process-loopback single-shot controls recorded the baseline and fixed
executables at 44.1 kHz, stereo PCM16. Both recorders were ready before the
actual Fire; packet receipts reported zero discontinuities and timestamp
errors. Both game/recorder pairs exited zero, with exact preference preservation.

After whole-window alignment, A/B coherence was 0.9999999834 and normalized
RMS difference was about 0.00002157 (0.707 PCM16 least-significant bits).
Single-shot output is effectively unchanged by this tracking correction.
These are whole process recordings: suit/landing and possible ambient
contributions remain in the signals. No cleaned waveform, calibrated gain or
native historical mixer parity follows. Burst output was also measured below;
command-time voice stealing is not a measured audible cutoff.

The corrected executable also completed a separate stationary Dust II bot
render smoke at 1600x900 with 2400 rendered frames. The CLI bot option 3
schedules three opponents and two teammates. Its 2280 samples after the
declared 120-frame warmup averaged 6.28 ms (159.1 internal frames/s), with
p95 7.87 ms, p99 11.09 ms and maximum 15.39 ms. The saved screenshot was
inspected for scene, AK/hands, HUD and a rendered teammate. This is a render
check with diagnostic audio logging; interactive input latency, an authoritative
actor census and native performance equivalence were not measured.

## Matched burst output

Separate baseline/corrected recordings used the same two executables, fresh
userdata, ordinary counted input and process-loopback format. Three-shot runs
both produced Fire ticks 192,205,218 at 128 Hz, final clip 27 and three observed
sinks. Five-shot runs added ticks 231,244 and ended at clip 25. Baseline lost
the pending voice records and issued no steals; corrected issued one oldest
voice removal for three shots and three removals for five shots. Each qualified
game/recorder pair exited zero, reported no packet discontinuities or timestamp
errors, and preserved twelve Steam preference targets exactly.

The verified installed AK waveform was compared with untouched whole-process
recordings. Fitted full-overlap and two-voice instantaneous-cutoff hypotheses
used each recording's own correlation markers; no cues were subtracted.

| Recording | Full overlap coherence / residual RMS | Two-voice coherence / residual RMS |
| --- | --- | --- |
| Baseline, three shots | 0.94752 / 0.05626 | 0.90585 / 0.07454 |
| Corrected, three shots | 0.90958 / 0.07474 | 0.95024 / 0.05603 |
| Baseline, five shots | 0.94508 / 0.06564 | 0.86807 / 0.09971 |
| Corrected, five shots | 0.87123 / 0.09976 | 0.94322 / 0.06750 |

The fit preference reverses in both bursts. In the three-shot pair, markers
are 100 ms apart in both recordings. After aligning the first markers, the raw
pair exceeds an analytical 10-PCM16-LSB difference at frame 8757, 63 samples
before the third marker at 8820. Earlier difference RMS is 0.0789 LSB. This
supports a real output overlap change near the third shot, beyond bookkeeping.
Neither the threshold nor the fitted cutoff is an audibility or exact stop
boundary. Whole-window RMS is slightly higher in the corrected three-shot run;
fewer retained voices does not establish lower loudness.

The five-shot markers are 100/100/100/100 ms in baseline and 110/100/100/100 ms
in corrected output. Their raw difference therefore begins around the second
shot, before the third-shot steal. This pair cannot isolate that steal's cutoff
by subtraction. The recordings were not time-warped or gain-normalized to hide
the difference. Producer tick spacing, request clocks, sink observations and
sample-correlation markers remain separate timing domains. Draw, bolt, landing,
suit, ambient, resampling and output processing remain in these signals.

An initial corrected five-shot capture failed a growing-log reader bound and
is retained as an unqualified harness failure. The qualified retry reads only
the initial byte-length snapshot, rejecting truncation, oversize and malformed
complete records; final whole-file validation remains strict. This harness
change did not alter either executable or audio policy.

Private three-shot proof IDs are `044412-e3b759b1` and `045849-e2e10444`;
five-shot IDs are `051035-27bc8dcc` and `054005-ccd517c7`, all on 2026-10-13.
The paired report SHA256 values are respectively
`2d966d4c52e6e0a415ef6109528093330732c5f7fa98d52f8a5d01715030bf69` and
`54edc4dacbd81cdd23594f7fd8277f06109b1a3b99bde2f4a7456c87b816a46b`.
Independent offline analysis regenerated the five-shot report in memory and
verified both original PCM hashes. Eight synthetic analysis regressions pass.
Raw PCM, screenshots and game assets remain private. No original-game mix,
historical DSP/EQ, authored stop curve or complete tone match is claimed.
