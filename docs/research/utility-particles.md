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
matching the selected material's camera-facing intent; the whole volume is
never rotated. Fire ground cards and upright cards remain fixed in world
orientation. Ending a field fades alpha instead of dragging positions and
scales back into its center. Fire particle bases trace to actual ground. The authored emission scaling
uses a temporary 19-point rendering cover of the current circular field;
these are not native spread cells. Authored distance-based alpha remaps keep
the denser layers from becoming an additive white mass.

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
