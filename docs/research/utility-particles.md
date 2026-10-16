# Utility particle presentation

Owner feedback on the PR84 build identified smoke and fire rotating as large
cards, a collapsing end-of-life animation, upright grenade projectiles, and no
HE explosion image. This change replaces the earlier hand-built 18-card field
with runtime-loaded particle layers and authored sprite-sheet sequences.

## Content and format sources

- Installed CS:GO Legacy app4465480 (1.38.8.1), read-only VFS:
  `particles/explosions_fx.pcf`, `particles/inferno_fx.pcf`, the layers' VMTs,
  and their VTFs. No game bytes are committed.
- [DMX binary format](https://developer.valvesoftware.com/wiki/DMX/Binary)
  and [independent format research](https://pevhs.ch/tf2/dmx_file_format).
- [VTF resource/particle sequence documentation](https://srctools.readthedocs.io/en/latest/modules/vtf.html).
- Runtime readers are original Rust, limited to binary5/PCF2 and sheet0/1.
  A private srctools installation was used only to inspect installed metadata
  and independently compare parser output. No SDK, leaked or decompiled engine
  implementation was used.

Each named layer gets a typed `csgo_particle_<layer>` ContentId. Asset packs
can declare `mod_particle_<name>` resources at `particles/*.pcf`; normal VMT/VTF
dependencies remain under `materials/`. Pack validation, reopen and VFS mounting
are covered by an original synthetic payload. The VFS can resolve all three
formats through mounted packs as well as the installed game.

## Implemented presentation subset

Smoke uses the installed ring, bottom, late smoke and dissipating dust layers.
Fire uses the installed ground-base, center, main and flame sheets. HE events
create the installed flash core and dust layers. Each layer reads authored
count limits, burst/continuous emission, radius/lifetime ranges, sequence/FPS,
color/alpha, position/offset, gravity and opacity fades where supported. Meshes
batch all particles in a layer, and shared textures are decoded once per load.

Positions belong to a fixed world origin. Individual smoke puffs billboard,
matching the selected renderer's camera-facing intent; the whole volume is
never rotated. The installed `explosion_child_smoke03d_ring` and
`explosion_child_smoke_bottom` renderers explicitly use `orientation_type 1`.
Their cards turn around the vertical Source Z axis (Bevy Y), without pitching
toward an elevated or lowered eye. This follows the documented upright mode in
the [Valve-hosted particle renderer documentation](https://www.dota2.com.cn/wiki/Dota_2_Workshop_Tools/Particles/Particle_System_Renderers.htm).
Their authored roll/spin still rotates the image within that vertical plane.
Other smoke layers retain camera-facing billboards; some individual card
rotation is therefore expected, independently of the anchored volume.
Fire ground cards and upright cards remain fixed in world
orientation. Ending a field fades alpha instead of dragging positions and
scales back into its center. Fire particle bases trace to actual ground. The authored emission scaling
uses a temporary 19-point rendering cover of the current circular field;
these are not native spread cells. Authored distance-based alpha remaps are
applied; the denser fire remains visibly too bright with the current material
subset.

Grenade presentation now tumbles around its mesh center on multiple axes,
retains orientation across bounces, and stops when authoritative velocity rests.
It does not change authoritative flight or collision.

## Limits, not parity claims

This is a bounded subset, not a complete Source particle runtime. SpriteCard
custom blending (including dual-sequence smoke), per-pixel depth blending,
distortion, noise/force operators, complete inferno control-point propagation,
water/surface variants and native random scheduling remain unfinished. Fire
positions approximate the current simulation's circular field with grounded
samples, rather than claiming native spread cells. Its fixed upright cards
avoid camera rotation while a complete authored renderer is pending.

Presentation ends with the authoritative field, including extinguishing.
The simulation's utility durations, obscuration and propagation still need
native reference validation; the installed smoke definition has late child
particles beyond the current simplified authoritative duration. Projectile
angular velocity is a presentation approximation, not measured rigid-body spin.
2015 parity is not established by installed 2023 data.

Validation and private rendered proof are recorded on the PR after build.

## HE flash and dust have separate authored lifetimes

The installed `explosion_child_core04b` flash lasts 0.25–0.30 seconds, while
`explosion_child_dust10a` lasts 0.30 seconds and
`explosion_child_dust06a` lasts 3–5 seconds with a 1–1.2 second fade-in. The
old enclosing 0.5-second emitter timeout erased every layer together, before
the slower dust could develop. The burst owner now survives the latest
authored particle death, including any emission start delay. Each particle
still expires and fades according to its own layer; the white flash is not
extended to the dust's lifetime.

This is presentation only: detonation, damage, fuse timing and gameplay
fields are unchanged. The existing 16-burst bound remains. It does not repair
the ignored HE `$mod2x` material mode or establish its unknown SpriteCard
opacity/gain law, nor does it solve fire's excessive brightness.



## Held Molotov wick presentation

The installed `v_eq_molotov.mdl` contains one mesh bodygroup, authored lighter
flame geometry/material, and a `Wick` attachment on a bottle bone. PullPin has
no animation events; Draw contains its sound and the two release sequences
contain the throw event. There is no missing MDL particle event to replay.

The additional burning-wick effect is the installed
`weapon_molotov_fp_wick` definition in `particles/weapons/cs_weapon_fx.pcf`.
It supplies its own view-model flag, texture/material, radius range, emission
rate, sequence range, animation/lifetime FPS and fades. No hand-tuned flame
position, size or invented light source is added.

The viewmodel publishes the current posed attachment after sampling the
current animation and camera transform. The bounded particle renderer follows
that exact point on viewmodel layer 2, after `viewmodel::update`, with the
bottle's dedicated FOV/depth pass. The presentation key includes player, life
and pull-start identity. Death, switching, hidden viewmodels, noclip, release
and a new life clear the sample/emitter rather than leaving a floating flame.

Without an authored ignition event, the effect begins conservatively when
simulation state reaches Holding after the pull animation. This does not
claim the exact native ignition moment inside the animation. Existing lighter
geometry and clips remain unchanged. SpriteCard crop/frame-blending/depth
operators retain the renderer subset limits above; visual verification is
required before claiming the displayed wick correct.

## Explicit additive-over composition

The installed ground-fire material `fire_explosion_1_oriented.vmt` documents
`$ADDOVERBLEND 1` with the blend factors `ONE:INVSRCALPHA`. This flag takes
precedence over its simultaneous `$ADDSELF 1`. Plain Bevy Add instead computes
`src.rgb * src.a + dst.rgb`, discarding destination attenuation; overlapping
cards can therefore accumulate into an excessively bright mass.

The bounded particle material honors that explicit flag with premultiplied
composition. Its shader retains the prior source contribution, including the
authored overbright factor and all texture/particle/lifetime opacity, while
preserving source alpha for destination attenuation. A zero-opacity particle
contributes no light. Other ADDSELF materials retain the prior approximation.
This addresses one known blend mismatch, not complete SpriteCard parity:
power-function remapping, ramps, depth blending and the remaining ADDSELF
semantics are still unresolved. Rendered brightness must be checked again.

## Authored single-sequence frame interpolation

The installed ground-fire, center/fancy fire and flame materials explicitly
set `$BLENDFRAMES 1`. The renderer previously selected one whole sheet frame,
discarding the fractional time before the next image. It now transports the
next frame's UV and that fraction per card, and mixes the two sampled RGBA
values before fog, particle opacity and the existing blend composition. This
avoids two separate transparent draws, which would attenuate the background
twice. Materials without the supported flag retain one texture sample.

The [sprite-sheet format reference](https://srctools.readthedocs.io/en/latest/modules/vtf.html)
documents frame rectangles and durations. A creator's original
[in-game authoring walkthrough](https://steamcommunity.com/sharedfiles/filedetails/?id=2954928838&l=english)
also demonstrates the `$blendframes` switch as blending adjacent sprite
images. This is author-observed tool behavior, not an engine implementation
reference. The bounded implementation uses linear interpolation within each
authored frame duration, holds the final frame for clamped sequences, and
wraps to the first image for looping sequences. The existing animation clock,
emission, lifetime, gain, tint and additive-over composition are unchanged.

Dual-sequence and max-luminance frame modes are deliberately excluded: their
combination law is not established. In particular, this does not implement
the smoke materials' `$DUALSEQUENCE`, `$SEQUENCE_BLEND_MODE`,
`$MAXLUMFRAMEBLEND2` or `$ZOOMANIMATESEQ2`, nor fire's `$POWERFUNCTION`.
Their unknown laws are not replaced with a guessed tint or gain. This change
targets hard frame cuts and does not claim to solve white fire, dark uniform
smoke or native/2015 material parity. Runtime shader validation and rendered
checks must establish the visible result before delivery.

## Authored particle ordering

Installed `explosions_fx.pcf` explicitly sets `Sort particles` false on
`explosion_child_smoke_bottom`. Installed `inferno_fx.pcf` does the same for
`molotov_groundfire_child_base` and `molotov_groundfire_main_fancy`. The
renderer previously sorted every batch by camera distance each frame,
discarding those explicit settings. These layers now retain their emission
order as particles expire and new particles append. Definitions without this
field keep the previous enabled behavior; this fallback is compatibility,
not a measured native default.

Enabled sorting uses the same current particle center as the drawn card,
including its existing velocity and gravity displacement. Sorting by birth
position could disagree with the displayed positions of rising late smoke.
No trajectories, billboard orientations, emission, tint, opacity, shader or
blend laws change. This addresses ordering only; it does not establish native
sorting parity or fix the remaining dark-smoke/bright-fire material limits.
Focused checks and rendered verification are pending.

## Authored ground-fire surface normals

The installed `inferno_fx.pcf` definition `molotov_groundfire_child_base`
explicitly enables `set normal` on its `Position Modify Place On Ground`
initializer and uses renderer orientation 3. The existing floor trace found
the supporting surface but discarded its normal, leaving every ground card
horizontal. On slopes, half of a card could therefore intersect the ground
while the other half floated above it.

That explicit initializer flag now retains the sampled collision normal.
Ground-card axes are projected into its plane before applying their existing
random yaw and roll. Flat-floor axes remain unchanged; cards without the
explicit flag keep their previous orientation. The trace, emission, particle
centers, material, brightness, fire coverage and authoritative damage are
unchanged. This does not implement native inferno spread cells or the remaining
SpriteCard composition laws.

Source `f64c964` passed all five focused utility-particle tests and a play
launcher build after cleaning all 23 workspace packages for that profile.
Two bounded 1920x1080 Dust II captures then showed the field on the mid ramp
and a flat-floor control. Both were reviewed by the implementing and
coordinating agents. Both clients exited zero, acknowledged the saved image
with one live utility field, had clean raw logs and preserved the owner's
separate configuration. Private receipts retain the immutable executable and
image hashes. These establish rendered placement, without a matched native
or before/after brightness claim. Fire is still visibly too bright; GR07 and
authoritative spread GR08 remain open.

## HE authored initial placement

The loaded HE dust layers contain two explicit positional initializers that
were previously discarded. In installed Legacy `explosions_fx.pcf`,
`explosion_child_dust10a` uses `Position Within Box Random` with Source bounds
[-20,-20,-80] to [20,20,100]. The existing sphere-only loader instead placed
all of that layer's particles at the same origin before their authored motion.
The bounded reader now retains those box endpoints, converts their coordinate
axes and samples each axis independently. Signed endpoint order after the
Source-to-Bevy axis conversion is preserved; it still describes the same box.

`explosion_child_dust06a` uses a 200-unit sphere with explicit
`distance_bias_absolute_value` [0,0,1]. Its position now folds the Source-Z
component above the origin before applying the existing authored offset,
rather than distributing half the dust below the blast. The author of a
[particle-editor example](https://moddota.com/assets/particles/falling-cherry-blossom-petal-for-spring-mood-particle)
describes this field as selecting the sphere's upper half. This is supporting
format interpretation, not a matched native HE appearance claim. Only the
observed exact [0,0,1] flag pattern is admitted; fractional and other axis
patterns remain unsupported. Radial velocity interpretation is unchanged.

The new position subset is explicitly enabled only by the HE layer load.
The loaded smoke and fire also contain absolute-axis declarations, but their
behavior stays unchanged pending their own rendered checks; held wick XY
flags are not generalized. Box/sphere combinations with uncertain initializer
ordering retain the existing sphere interpretation. The same three random
draws supply the corrected position, preserving subsequent tint/fade/rotation
draws. Existing layer ContentIds, runtime VFS/pack reads, emission limits,
textures, material composition, lifetime, gravity and authoritative grenade
state are unchanged. No game assets are included in the repository.

The private read-only metadata audit used the installed PCF SHA256
`9ed53f823d5340cd35883674d5ef5e22c52cf434d59890f7d5e0f13e07d59c5d`
and an independent format reader. Focused source checks cover Source box
corners/center/sample bounds, upper-half placement and the disabled/unhandled
compatibility paths. Compilation and actual before/after captures are pending.
The ignored nonlinear radius-scale bias/timing, tint blending, drag, missing
HE children and `$mod2x` composition still prevent a complete native effect.
