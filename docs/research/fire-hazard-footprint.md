# Bot fire hazards share the existing burn footprint

All current `fire_at` callers pass prospective actor feet/origin coordinates:
NAV area centers, portals using the destination area's ground height, the bot's
actual movement origin, and tactical hiding positions rebuilt at NAV ground+2
and checked with a standing hull. None passes an eye, chest or field source.
The public query documents that coordinate contract explicitly.

The query formerly used a 130-unit horizontal/65-unit vertical cylinder while
actual burn damage used a 140-unit source-to-chest sphere. Ground origins at
135 units could burn without being reported hazardous; an origin 125 units
away and 50 units higher could be avoided despite receiving no burn damage.

Hazard and damage now share the existing distance and solid-visibility
predicate, using the same field-source+12 and feet-to-chest+32 offsets and
strict radius boundary. The old burn radius is shared, not retuned. Damage,
armor/friendly rules, terrain coordinates, cadence, lifetime, smoke extinction
and projectile behavior are unchanged. The preceding solid-wall correction
remains intact. The existing actual-burn wall witness includes both boundary
controls; no new test framework or gameplay constant is introduced.

This is internal bot hazard/damage consistency, not native fire propagation.
The simulation still has one fire field; the client-only 19-point visual stamp
is not authoritative spread. Native cell spacing, ordering, growth timing,
collision/height acceptance and per-cell lifetime remain unmeasured. GR08 stays
open. Compilation and bot rendering are pending for this separate candidate.
