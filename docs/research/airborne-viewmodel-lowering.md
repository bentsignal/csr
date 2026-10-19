# Airborne viewmodel lowering

The October 18 playtest accepted grounded weapon motion but reported a held
pose throughout jumps. A bounded additional vertical component now follows
airborne/grounded state through the apex and landing. It moves the gun and
bone-merged hands together in the current view basis. Shot direction, camera,
collision, velocity and authored animation clips are unchanged.

## Reference and boundary

Read-only CS:GO Legacy 1.38.8.1 observations used ordinary console commands,
stationary M4A4 jumps at 128 ticks and normal time, with repeated trials and
stationary controls. The window was 1600 by 900, viewmodel FOV 60 and offsets
1, 1, -1. Earlier quarter-time captures and pistol/utility observations are
additional context. Original preferences were restored exactly after each
owned session. No native code, SDK or decompiled implementation was used.

The stationary controls retained the same weapon landmarks. Differential
sight/carry-handle measurements during flight support about 0.75 unit of
vertical lowering. Accuracy-shift, running-lowering and vertical-bob console
controls were varied independently. Doubling the right accuracy shift changed
the lateral/angular response; the vertical lowering remained. Changing
running lowering from 21 to 5 or vertical bob from 0.14 to 0.28 did not remove
the stationary-jump lowering.

The implementation uses that approximate vertical plateau and a bounded
10-unit-per-second approach in each direction. The approach fits the small
first airborne displacement and the plateau in the next capture interval;
the screenshots do **not** establish an exact native transition equation.
Template matching is affected by lighting, some front-sight matches are weak,
and a shared carry-handle patch is not two independent visible vertices.
Neither a six-degree-of-freedom fit nor complete jump/landing parity is claimed.

The existing qualified Legacy presentation profile gates the effect. Viewing
life changes, missing/hidden targets, scoped or tracked views and climbable
contacts clear its history. Weapon replacement within the same viewing life
continues sampling before the deferred instance creation return. Clock gaps
and invalid times cannot reuse a previous drop.

Accuracy-related tilt, the native camera's landing dip/kick, periodic bob and
large-fall/duck-jump behavior remain separate unfinished work. This component
adds the observed drop and return; it does not synthesize a landing spring.
The installed-data regression checks that all gun/arm joints move together,
landing returns continuously, death hides the meshes, and render updates leave
the authoritative simulation and aim state unchanged.
