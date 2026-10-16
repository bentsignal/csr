# Authored burst-mode HUD indicator

Status: source prototype stacked on PR202; actual original-clip rendering and
coordinated Cargo verification remain pending. No timing parity is claimed.

The current ammo adapter hides both installed `BurstTypeSingle` and
`BurstTypeBurst` template clips. Its snapshot carries ammunition but no mode,
so a mode-only change cannot request a new HUD raster. This change retains an
optional mode only when the active weapon has a validated declarative burst
profile, and includes it in the existing ammo snapshot/revision. Unsupported
weapons send null. No simulation, network state or weapon switching changes.

A read-only authoring-tag audit of installed `hudweaponmodule.swf` found both
clips under `HudPanel.WeaponPanel`. Each is a one-frame MovieClip labeled
`FullClip`. All AVM instruction/action payloads were skipped. Existing native
Legacy observations from the earlier burst-mode check independently show one
filled and two outlined horizontal bullets before the Glock/FAMAS switch,
then three filled bullets after switching to burst, with unchanged ammunition.
The four existing originals were personally inspected; no new native session
was opened. These 2023 native images do not establish 2015 placement/scaling.

The adapter selects the original one-frame clips through their public
visibility and timeline properties. Only a true supported mode displays the
burst clip; false displays the single-mode clip; null, dead or unsupported
state hides both. Unsupported clip structure rejects the ammo component and
keeps its ordinary fallback. Existing life/owner/viewport/raster ownership and
idle parking remain unchanged, with no extra movie worker. A focused regression
checks mode-only updates, rejection of a stale result, unsupported state and
death/new-life clearing. Final rendered admission must show both original
clip states and transitions back to a non-burst weapon without stale art.
