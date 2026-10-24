# Player contact and movement tests

The October 24 owner request puts core mechanics before menu polish or
multiplayer. F8 Tests / audio offers partner contact, crouched boosting,
a ready-on-head run-boost lane, and the already-installed Legacy surf map.
Partner motion and shooting use ordinary fixed-tick TickInput. Scenario
initialization changes offline spawn configuration and calls normal respawn.

## Reference and implementation

A guarded Legacy 1.38.8.1 Dust II observation at 128 Hz used public bot/place,
movement, getpos_exact and mp_solid_teammates commands. With a visibly live bot,
the human stopped at Y=1824.537 from Y=1800. The clear control reached
Y=2010.809. These are command-observation results, not a native tick-time fit.
Earlier attempts with a missing/dead bot were rejected. Private images and
console receipts remain outside the repository; twelve preferences were
restored exactly and the owned process closed.

Public console help declares teammate solidity 0=pass through, 1=fully solid,
2=supporting tops. Mounted competitive/deathmatch/casual configs select 1/0/2.
The existing standing/crouched hulls are used by the normal movement solver;
no new contact-radius or movement coefficient is introduced. Dead/spectator
players do not block movement; initially overlapping spawns can separate.
The rule travels in snapshots; protocol 33 rejects older layouts.

Moving support translates the rider through collision sweeps and transfers
base momentum once on departure. Flat support, jump departure and snapshot
prediction are covered at 64/128 Hz. This is a practical moving-base baseline,
not measured native run-boost parity. Crouch-to-stand lifting, uneven/dynamic
base behavior and complete player-stack fidelity need further comparison.
The run-boost test starts on a standing partner to isolate departure behavior
from mounting skill. Hold forward and jump; F7 resets the pair.

## Surf test

The private Legacy install contains Source BSP v21 `surf_ski_3_nova_kg_v6`,
including its own team spawn records. F8 selects it without copying any map
content into the repository. F7 respawns immediately. The explicit practice
preset uses ground acceleration 10 and air acceleration 1000; the competitive
preset restores the original world configuration. These are practice choices,
not changes to competitive movement. The historical
[CS:GO surf_beginner workshop description](https://steamcommunity.com/sharedfiles/filedetails/?id=607186931)
provides that practice acceleration pair. Map trigger/teleport completion and
whole-course/native surf parity are not claimed by this initial ramp test.
