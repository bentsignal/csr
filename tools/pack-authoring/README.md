# Pack authoring (developers only)

This workspace package contains original map builders and other authoring
helpers. It is not a dependency of the player client or launcher. Its output
must be installed as ordinary content packs and selected by ContentId.

`surf_course::files()` produces the original surf/skate course's BSP and
materials. `mods/skate` includes these files and a map definition in the
private Skate pack it creates. Removing that pack removes the map from Play.
`surf_course::pack()` builds an independent original asset pack for developer
checks; no built-in map loader fallback is permitted.

The GPL provider and private-data creator are independent workspaces under
`mods/skate-runtime` and `mods/skate`. `tools/build-skate-tools.ps1` builds
them on WINSERVE. Releases publish an optional runtime archive and a separate
authoring archive; neither enters the base installer or updater payload.
The client fetches the exact publisher-owned runtime for its source revision
only when an installed pack requests it. Arbitrary executables and download
URLs inside packs remain prohibited.

For local source playtests set `CSR_PROVIDER_DEV_DIR` to the local tool output
folder. Installed builds locate the matching public release automatically.
Original game assets must stay in private `.csrpack` files, never Git or a
public release asset.
