# Runtime bullet impact artwork

Owner item WP09 rejects plain faded circles. This implementation reads the
mounted game's `scripts/decals_subrect.txt` impact groups and translation table,
the impacted surface's inherited `gamematerial`, and referenced VMT/VTF data.
There is no copied list of game decal choices or game artwork in the repo.
Private content packs can supply the same virtual paths without a Legacy
installation. Each material has a stable `ContentId` derived from its VFS name,
retained on the cosmetic entity as `ImpactArtwork`.

The installed Legacy 1.38.8.1 data identifies weighted alternatives, subrect
pixel bounds, parent material/texture and base `$decalscale`. Unsupported
shaders, empty/no-decal entries, invalid bounds and missing dependencies are
skipped; they do not revert to the rejected circles. Inherited surface codes
are used rather than matching material names for words such as "metal".

The current renderer crops each authored subrect, uploads its multiplier data
linearly and uses Bevy 0.19.1's documented-in-source Multiply alpha pipeline
with a two-times neutral-midpoint factor. Unlit multiplication operates on the
already lit wall. Cached Bevy material/mesh/shader sources were read before
using the API; no Source engine/SDK/decompiled implementation was consulted.
Direct online Valve format pages were unavailable during this implementation;
they were not treated as evidence for the blend law.

This is a bounded planar subset, **not native decal parity**. Two-times color
composition, exact gamma/HDR behavior, projection rotation/edge clipping,
distance-dependent dynamic scaling, prop/skinned projection, lifetime/fade and
native random scheduling still need matched observation. Current lifetime and
normal offset retain the pre-existing presentation policy. Existing cosmetic
count limits and round/session teardown remain in force. Art loading is
bounded to 64 groups/256 material variants, 16M source pixels and 1M cropped
pixels per material. Cosmetic variation does not use or alter simulation RNG,
recoil, hit registration or network state.

Verification is pending: synthetic subrect/inheritance checks, workspace and
installed probe/simulation, actual rendered wall impacts, and a native matched
material comparison. A successful data load alone does not close WP09.
