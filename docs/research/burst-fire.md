# Installed burst-fire declarations

The player's CS:GO Legacy `scripts/items/items_game.txt` declares `has burst
mode = 1` for the Glock and FAMAS. Their `cycletime when in burst mode` and
`time between burst shots` values are respectively 0.500/0.050 and
0.550/0.075 seconds. Installed English localization describes
`Attrib_HasBurstMode` as a three-round option. Generic `cycletime alt` is
not the declared burst interval and is not substituted for it.

The bounded implementation uses a validated attribute profile rather than
class-name checks, so content packs can provide the same declarations.
Secondary-input edges toggle an otherwise ready weapon. Each primary press
in burst mode starts up to three ordinary authoritative bullets, limited by
the clip; follow-ups continue after release. Each bullet follows the existing
ammo, spread, recoil, hit, event and sound path. Declared alternate accuracy
and recoil inputs apply in burst mode. FAMAS normal mode remains automatic.

Pending shots live in replicated simulation state, not render timers. Their
deadlines advance from the scheduled interval rather than accumulating tick
rounding; multiple due shots are bounded to the remaining two bullets.
Holstering, dropping, death and objective/freeze interruptions cancel pending
shots. Reload input waits for an active burst to finish. Mode selection stays
with the weapon when it changes owner. Protocol 20 rejects older peers before
admission; pending counts, deadlines, ammo and supported profiles are validated.

Native toggle lockout and the final-shot recovery policy have not been
measured. This version toggles immediately when ready and measures the next
burst cycle from its first bullet. One burst requires one primary-input edge;
holding does not repeatedly initiate bursts. No separate authored toggle clip
was found in the installed Glock/FAMAS viewmodel declarations, so none is
invented. These are explicit playable compatibility choices, not a claim of
complete original-engine timing or recoil parity. Rendered validation is
pending.
