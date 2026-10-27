# Skate on CS maps

Install a Skate `.csrpack` by opening it, or use **Content packs -> Install
pack** in the game. Load any CS map. **X** switches between CS movement and
skating without changing your character, weapons, ammunition or map. Rebind
**Toggle movement mode** in Keyboard / Mouse if desired.

While skating, use your movement keys to push, brake and steer. Space ollies;
crouch gives the stronger pop; Shift brakes. Mouse aim, firing, reloading,
weapon selection, health and damage still use CS rules. The camera follows
behind your CS character and clips against the map. Switching back restores
the ordinary first-person CS view.

## Create a pack from your own game

1. Own Skate 3 and prepare your copy using the
   [Skate 3 Rust Engine](https://github.com/SK8-ENGINE/skate-3-rust-engine).
   Its setup accepts your Xbox 360 ISO or extracted `default.xex` and converts
   the content locally. Follow that project's setup instructions.
2. In CSGO Rewrite, choose **Content packs -> Create Skate pack**. Select
   `skate3rust.exe` in your prepared installation and choose where to save
   the `.csrpack`. The bundled creator needs no Rust, Python or manual copying.
3. Open the saved pack to install it. Load a CS map and press X.

The creator reads the converted installation without modifying it. It includes
the board geometry, selected normal-difficulty riding/jump settings, the GPL
movement module and its notices. It does not include original game executables,
skater bodies, maps or the original game's source code. The resulting file
contains your game's assets: keep it out of this repository and public releases.
The game updater preserves installed packs in your existing player data.

## Current playable scope

This first crossover brings pushing, coasting, braking, steering, ramp/curb
contact and ollies onto CS maps, with CS models holding and using CS weapons.
The pack calls the public rewrite's `skate-core` push, braking, steering and
ground-jump functions. Private curves and coefficients are read from the
owner's converted game data.

It uses CS swept-hull collision rather than the upstream seven-body board and
physical skeleton. Keyboard push cadence and truck-tilt-to-curvature mapping
are crossover policies. The character uses a planted CS stance and CS combat
gestures; board geometry currently uses flat material colors. Full Skate
animation retargeting, textured decks, tricks/grinds, wipeouts and controller
gestures remain future integration work. This does not claim exact Skate 3
physics or full gameplay parity. Movement packs currently run in offline
sessions; network sessions reject active movement-module state.

## Source, licenses and developer builds

The independent CSGO Rewrite host/API remain Apache-2.0. The optional adapter,
module and pack creator are GPL-3.0-only, using
[`SK8-ENGINE/skate-3-rust-engine` at 2e166977](https://github.com/SK8-ENGINE/skate-3-rust-engine/tree/2e16697797c8358d16f747a8259354ff2921041a).
Preserve dumbad and the upstream contributors' attribution. Corresponding
adapter source, build scripts and dependency lock are in `mods/skate` in the
same source revision as the CSGO Rewrite release. See its `LICENSE` and
`NOTICE`; the pinned upstream source remains available at the link above.
The creator and module are built without any game data by CI. Original assets
are converted only on the owner's machine and are never CI inputs/artifacts.

For developers with Rust and the repository's Windows build prerequisites:

```powershell
./tools/build-skate-tools.ps1 -OutputDir C:/private/skate-tools
C:/private/skate-tools/Skate-Pack-Creator.exe
```

For unattended private conversion, use
`Skate-Pack-Creator.exe --create <path/to/skate3rust.exe> <new/output.csrpack>`.
It refuses to replace an existing output. The older three-argument developer
mode accepts `<converted assets folder> <module.wasm> <new private folder>`;
`packs`' existing `packtool` can build that folder into a `.csrpack`.

The focused headless check is
`cargo run -p level --example skatecheck -- <private.csrpack> [map]`.
It exercises both tick rates, installed module execution, an ollie, mode
switching, preserved inventory, snapshot replay and real-map movement.
No private fixture is committed. API details are in [MECHANICS.md](MECHANICS.md).
