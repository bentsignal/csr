# Deployed decoy body

Owner requirement GR09: a decoy must not vanish while it is still producing
weapon sounds. The renderer previously deleted every grenade body when its
projectile left `World.projectiles`, although an activated decoy continued as
a `World.utility_fields` entry with the same id.

The renderer now keeps that existing posed instance and its last cosmetic
rotation across activation. The authoritative field position owns its resting
translation. Field expiry, round cleanup and time reset remove the instance.
A snapshot containing an already active decoy also creates its installed world
model. Rotation is not carried in the snapshot; a newly observed resting body
uses the model's default orientation. Exact angular/ground-contact fidelity
remains open. No simulation, protocol, sound cadence or lifetime changes.

## Original-game observation

One ordinary underhand decoy throw in installed CS:GO Legacy 1.38.8.1 was
observed at a fixed eye position and angle. The actual selected decoy/HUD,
resting body before its first weapon cue, body during later positional AK
cues, and disappearance at the terminal blast were inspected. The body was
still visible at approximately 19 seconds after input release and absent by
22 seconds; first weapon cue to terminal sound was approximately 17 engine
seconds. These bounded observations establish presence during operation,
not an exact constant lifetime, cadence algorithm or historical 2015 proof.
The current simulation's 15-second active field remains uncalibrated.

Private packet: `decoy-visibility-20261015/native-decoy-20261015-133120`.
The owned game exited, all twelve observed preference files were restored
exactly, and no unowned process was stopped. No raw assets/logs are published.

## Rendered verification

An ordinary overhand throw in source `5e07e54` showed an active decoy body
with its authoritative field at simulation time 7 seconds. At 22 seconds,
the field and body were gone. A subsequent ordinary underhand throw in
`7e8c546` retained its body and rotation after activation. An explicit nearby
active-field screenshot fixture in that source showed the separate snapshot
path creating the installed decoy model at rest. Root inspected the actual
1920x1080 images, with successful process exits and unchanged owner config.

Earlier noclip captures did not deliver player TickInput, so their alleged
throw cases were rejected. An initial far-away snapshot fixture was not
visually readable and was replaced by the bounded nearby fixture. All raw
negative packets remain private and unchanged. Screenshot fixtures prove
presentation plumbing, not native throw physics or network orientation.
