# Desktop fidelity feedback, 2026-10-12

The owner could navigate the performance-fixed classic menu and play a bot
match, but said the result does not yet feel like CS:GO. The target is the
2015 gameplay and sound experience, with the supplied classic menu reference;
new convenience features follow fidelity. The attached gameplay capture is
retained privately, outside git.

- [x] Reset first-person animation and masked bone pose on every respawn,
      including the same owner/team/weapon. The owner's hand/weapon was broken
      after a respawn and corrected itself after reload.
- [ ] Measure/remove unsupported camera-turn viewmodel lag. The owner noticed
      left/right hand sway not matching the target. Keep authored animation.
- [x] Fix recoil accumulation during held fire without recovering between
      automatic-fire cooldown ticks. Release, reload, dry fire and semi-auto
      recovery remain covered at 64 and 128 tick ([PR #55](https://github.com/bentsignal/global-offensive/pull/55)).
- [ ] Measure per-shot bullet
      directions, camera punch, recovery and weapon-specific spray against the
      real game. Deterministic invented patterns are not a fidelity result.
- [x] Route the mounted M4A1-S suppressor through its proper special sound,
      and verify the shipped 2015 AK near/far and suppressed M4 near waves
      ([PR #56](https://github.com/bentsignal/global-offensive/pull/56)).
- [ ] Verify other classic weapon audio and the original mixing/DSP;
      do not label modern installed waves as historical without evidence.
- [ ] Measure jumping and landing speed. The owner felt a landing slowdown;
      preserve or change penalties based on observed target behavior.
- [ ] Reproduce the broken-looking Dust II arches in the owner's screenshot;
      distinguish geometry, material, lightmap and normal problems using
      matched viewpoints. Improve the remaining lighting/material gaps.
- [x] Render menus at the actual fullscreen/window drawable size, applying
      saved windowed sizes in physical pixels and preserving OS resizes
      ([PR #57](https://github.com/bentsignal/global-offensive/pull/57)).
- [ ] Finish full menu content/layout fidelity, including the clipped home
      news placeholder; resolution checks do not establish original content.
- [ ] Implement original kill feed and death presentation. Dead actors
      currently disappear; death animations/ragdolls remain unfinished.
- [ ] Complete classic settings, gameplay mechanics and HUD coverage against
      measured references. A usable menu is not complete menu fidelity.

Every completed item needs specific code/data provenance and appropriate
regression or observed evidence. Local captures and historical game assets
remain private. PRs should distinguish reproducible correctness fixes from
historical parity that still needs an older runtime/reference.

## Verified desktop checkpoint

The combined source `cff4b7a760b8803cd66b6807d05891bd7aa68e9a`
passed 561 workspace tests (six opt-in tests ignored), the separately enabled
installed-M4 production-system respawn regression, release Dust II probe and
simulation (zero failures each), and all 1,425 audio decodes. Respawn resets
masked bones and playback even when owner/team/weapon stay the same
([PR #54](https://github.com/bentsignal/global-offensive/pull/54)); the regression
is an actual-model ECS check, not a rendered or physical-input respawn proof.

At actual 2560x1440 fullscreen with optional traces disabled, the normal
menu stress check produced 489 fresh image assignments in its measurement
window. Forty FIFO input receipts had p95 41.450 ms and maximum 47.896 ms,
with no pending receipt or failures. Presentation assignment intervals had
mean/p95/max 40.851/64.667/91.606 ms. The finite FIFO boundary scheduler and
byte-preserving opaque capture shortcut landed in
[PR #63](https://github.com/bentsignal/global-offensive/pull/63).

The bot fixture passed movement, fire, reload and pause/resume with four
fresh acknowledged, inspected captures at fullscreen. Its 679 engine-update
samples had mean/p95/max 7.587/11.693/36.519 ms. Six separate display stages
also produced acknowledged and inspected images at 1600x900, 1920x1080 and
2560x1440, including fullscreen, unrelated volume changes and return to
windowed mode ([PR #59](https://github.com/bentsignal/global-offensive/pull/59)).
Actual monitor scale was one; higher-DPI transitions have causal source tests.
These fixtures measure engine input/update and assigned raster frames, not
physical OS input or compositor presentation latency. Private images stay
outside the repository.

Geometry now respects supported compiled face primitives
([PR #58](https://github.com/bentsignal/global-offensive/pull/58)), but matched
Dust II captures were unchanged: this does not close the arch/lighting report.
Native Legacy observations show brief viewmodel turn lag and a landing speed
penalty; they do not calibrate this rewrite's behavior or establish 2015 parity.
Exact spray, other historical sounds, original HUD/death presentation and
lighting remain open. The default installed Dust II is the later remake;
an official compatibility archive's date alone does not prove 2015 fidelity.

## Owner follow-up on the desktop checkpoint

The owner tested the updated CT/M4, T/AK and menu shortcuts. The broken hand
after respawn did not recur, and fullscreen menu fidelity was visibly better.
The M4 firing sound seemed correct. The AK has the right underlying sound,
but its tone and overlapping shots still differ from the original. Neither
waveform identity nor a subjective positive M4 impression closes mixing/DSP
fidelity. The owner could not find suitable comparison clips; research must
continue without requiring one.

The sustained spray still climbs incorrectly and does not match the original
pattern. Held-fire recovery is a correctness fix, not a finished pattern.
Jumping/movement also still feel wrong. The target is measured 1:1 behavior
in Rust/Bevy, including CS:GO surfing, rather than a plausible approximation.

- [ ] Verify 2015 weapon mix/operator inputs, voice lifetime/overlap, pitch,
      speaker routing and DSP with controlled single-shot and burst recordings.
      Keep native 2023 observations distinct from historical configuration.
- [ ] Check grounding before jump/duck and preserve actual impact velocity
      through collision clipping; validate defects before changing behavior.
- [ ] Measure surf ramps, plane intersections, seams and speed retention at
      64/128 tick with explicitly recorded native server cvars and map identity.
      Keep physics in the shared simulation and preserve network/prediction
      compatibility for any changed state or mechanics profile.

The leaked/decompiled CS:GO implementation remains outside the project's
clean-room boundary. Public format documentation, dated declarative game data
and observed native execution remain available for exact reconstruction.
