# Exact-count screenshot audio probes

`CSR_SOUND_PROBE=ak47_single|ak47_three|ak47_five` is enabled only when the
existing screenshot-run gate is active. Run with `--bots 0`, without a noclip
`--camera` override. The existing sound-probe spawn position is retained and
the ordinary TickInput aims upward. This is a test fixture, not a gameplay or
live audio scheduling change.

The fixture holds ATTACK beginning at simulation time1.5 until the requested
number of actual local `GameEvent::Fire` events has been observed immediately
after authoritative `World::step`. It then releases ATTACK on the next tick,
including subsequent render-frame catch-up ticks. The total6.5sec fixture
allows the known waveform tails to finish; it does not use that duration to
presume a shot count.

`World::step` increments tick after producing events. The caller captures the
pre-step producer tick, and the observer checks the returned tick is its
successor. Logs record that exact producer tick, tick rate, owner and post-shot
clip, separate from render audio request/sink clocks and PCM onset. A repeated
observation of the same step is ignored. Wrong weapons, extra shots or an
incomplete count fail the fixture. No Fire wire schema or normal production
event timing is changed.

Synthetic-floor/catalog tests drive actual World/TickInput at64 and128 tick
and check1/3/5 actual fires, ammo decrement, increasing producer ticks, immediate
release and no later shots. Local compilation/runtime capture remain pending
the serialized target/native slots. These tests establish fixture counting,
not original-game firing cadence or mixing fidelity.
