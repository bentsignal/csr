# Desktop fidelity feedback, 2026-10-12

The owner could navigate the performance-fixed classic menu and play a bot
match, but said the result does not yet feel like CS:GO. The target is the
2015 gameplay and sound experience, with the supplied classic menu reference;
new convenience features follow fidelity. The attached gameplay capture is
retained privately, outside git.

- [ ] Reset first-person animation and masked bone pose on every respawn,
      including the same owner/team/weapon. The owner's hand/weapon was broken
      after a respawn and corrected itself after reload.
- [ ] Measure/remove unsupported camera-turn viewmodel lag. The owner noticed
      left/right hand sway not matching the target. Keep authored animation.
- [ ] Fix recoil accumulation during held fire, then measure per-shot bullet
      directions, camera punch, recovery and weapon-specific spray against the
      real game. Deterministic invented patterns are not a fidelity result.
- [ ] Trace silenced M4A1-S firing to the proper soundscript entry/waveform.
      Find and verify the original 2015 AK-47 and other classic weapon audio;
      do not label modern installed waves as historical without evidence.
- [ ] Measure jumping and landing speed. The owner felt a landing slowdown;
      preserve or change penalties based on observed target behavior.
- [ ] Reproduce the broken-looking Dust II arches in the owner's screenshot;
      distinguish geometry, material, lightmap and normal problems using
      matched viewpoints. Improve the remaining lighting/material gaps.
- [ ] Render menus at the actual fullscreen/window drawable size and DPI,
      preserving crispness and layout at supported resolutions.
- [ ] Implement original kill feed and death presentation. Dead actors
      currently disappear; death animations/ragdolls remain unfinished.
- [ ] Complete classic settings, gameplay mechanics and HUD coverage against
      measured references. A usable menu is not complete menu fidelity.

Every completed item needs specific code/data provenance and appropriate
regression or observed evidence. Local captures and historical game assets
remain private. PRs should distinguish reproducible correctness fixes from
historical parity that still needs an older runtime/reference.
