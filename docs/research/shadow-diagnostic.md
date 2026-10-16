# Developer shadow-factor diagnostic

`CSR_SHADOW_DIAGNOSTIC=1` selects an immutable startup diagnostic. Ordinary
startup installs no diagnostic systems, lights, material variants or shadow
shader. Ordinary materials retain their current lightmap, exposure and gain
policy. This is plumbing for a controlled comparison, not native lighting
composition or a completed fidelity claim.

The diagnostic uses the loaded BSP's existing `light_environment` direction.
It does not require an authored `env_cascade_light`: the installed Dust2 BSP
has no such authored entity, while the native renderer reports a runtime
cascade light. A zero-illuminance Bevy directional light prepares only shadow
maps; owned static world/prop material variants display their received shadow
factor as grayscale. Magenta identifies absent light/cascade data. Bevy's
public shadow sampler returns an unshadowed factor outside cascade coverage;
white alone therefore does not prove coverage or native agreement.

Layer31 is reserved for this diagnostic. Active, owned main-world cameras
receive it in addition to their original masks. Scene-owned `MapClusters`
meshes in a finite receiver region swept towards the sun also receive it.
This caster admission is independent of the current camera PVS, so a nearby
off-view caster can contribute. Each frame recomputes the original PVS result
before restoring meshes that leave the region, including movement within a
single cluster. Bounds conservatively reuse the existing batch bounds; no
world or mesh is cloned.

Sky, viewmodel, translucent materials, unowned entities and entities carrying
`Persistent` are excluded, including entities carrying both ownership markers.
Only opaque and masked static materials qualify. Each original material gets
one retained variant, shared by qualifying owners. The shadow-depth fragment
uses the existing base alpha, terrain blend/modulation, tint alpha and alpha
threshold; it does not read Bevy's unrelated StandardMaterial uniform layout.

The scope is bounded to eight active world cameras, 8,192 owned static meshes
and 1,024 material variants. Exceeding a bound, another directional light, or
an existing layer31 owner refuses the scope and logs a diagnostic reason.
CPU scope transitions also log the actual light entity and admitted camera,
caster and variant counts; the public `shadow_diagnostic::status(&World)`
receipt refreshes these counts. Preparing, inactive and unavailable states
cannot be presented as a successful all-white diagnostic. An active CPU scope
still requires a real rendered proof before claiming shadow submission.
Replacement, cancellation and return to the menu restore only recorded
material handles, render masks and current PVS visibility, despawn the owned
light, and release variant handles. No gameplay or authoritative state changes.

The implementation uses the installed Bevy0.19.1 `Material` shadow/prepass
callbacks, per-view directional light bindings, `fetch_directional_shadow`,
`CascadeShadowConfigBuilder`, `RenderLayers`, and visibility propagation APIs.
Its depth shader is a custom material fragment, with a tested 80-byte prefix
of the WorldParams serialization, including the authored layer UV scale/offset
fields. The normal forward shader receives
the diagnostic shader definition only in the opt-in specialization.

Coverage400 and caster reach400 are diagnostic Source-inch bounds. The native
status's raw400 is not a calibration of cascade projection, splits or reach.
Bevy's default cascade splits, bias, filtering and two-sided prepass behavior
are not claimed to match CS:GO. Dynamic character casting, extended authored
prop-shadow flags, VHV lighting, multi-light selection and changes to an
original material after its variant is cached are outside this first slice.
Native shadow-toggle isolation and matched rendered comparisons remain needed
before choosing any baked/direct composition law. No exposure adjustment or
speculative lightmap subtraction is included.

## Asset-free pipeline fixture

The `client` example `shadow_fixture --output <fresh absolute PNG>` creates
only procedural quads and three code-generated textures. Run separate off
and on processes in the coordinated GPU slot: leave the startup environment
absent for off, and set `CSR_SHADOW_DIAGNOSTIC=1` for on. It uses the same
WorldMaterial callbacks and forward/shadow-depth shaders; it does not load
CS:GO, install CsgoPlugin, or exercise BSP/PVS scene admission.

An orthographic camera observes a non-casting receiver and four casters:
opaque, front-facing checker mask, reversed two-sided checker mask, and an
opaque caster outside the camera frustum. The synthetic layout unit is ten
Source inches, leaving twenty-inch holes larger than the unchanged Bevy
shadow bias/filter footprint. Scene generation reports one camera, one
receiver, four casters, three materials, two meshes and three images. Off
creates no light and disables the material's shadow callback; on creates one
zero-illuminance layer31 directional light. No shadow bias or normal-render
lighting parameter is changed.

The fixture waits for the existing RenderGate's complete asset IDs and two
settled render preparation/submission cycles, then five more main frames.
It checks the off-view caster's real camera-frustum exclusion and, on mode,
its real cascade membership. Camera-projected receiver rectangles identify
the opaque, masked, reversed-mask and off-view shadows. The actual screenshot
callback measures dark/white/magenta pixel counts: on requires predominantly
dark opaque regions, both dark and white in each checker-mask region, and no
magenta missing-data pixels. Off requires no dark receiver pixels. These
synthetic acceptance ratios are not a native shadow-law calibration.

PNG saving and acknowledgment must succeed before the owned app exits zero.
Failures retain the screenshot when available and exit nonzero. A3,600-frame
and30-second schedule bound covers preparation and capture; these bounds are
evaluated when the main-world schedule advances, not an external process
watchdog. The paired images were independently inspected in the validation below. No
native, historical lighting, normal-game performance or default-map output
claim follows from this fixture alone.

## Verified diagnostic scope

Production source `86e6885d8ba7b0a16e5dc711edc9cd70298a7f50` passed 626
workspace tests with six ignored, release Dust2 probe and simulation with zero
failures, and soundcheck with 1,425 decodable sounds, zero skipped and zero
that would crash. The play launcher and fixture builds also exited zero.

Owned synthetic on/off runs saved 960x640 PNGs and exited naturally with code
zero. Independent image and pixel review confirmed opaque, front-facing mask,
reversed two-sided mask and off-view shadows. RenderGate was Ready before the
screenshot acknowledgment. Each receiver region contained 7,104 pixels;
all four had zero magenta pixels:

| Receiver region | Dark | White |
| --- | ---: | ---: |
| Opaque | 6,992 | 112 |
| Front-facing checker mask | 3,076 | 3,255 |
| Reversed two-sided checker mask | 2,997 | 3,392 |
| Off-view opaque | 7,104 | 0 |

The disabled run had 7,104 white pixels in every region. The separate
missing-diagnostic case produced the same disabled PNG and correctly failed
the gate's enabled-diagnostic admission; its app exit was zero. These are
synthetic pipeline proofs, not CS:GO screenshots.

Six installed Dust2 captures paired off/diagnostic at the documented
`upper_tunnel`, `ct_garage` and `ct_spawn_mid` cameras, all at 1600x900.
All exited zero, had no shader/device errors and were independently inspected.
The enabled scopes reported respectively 111/64, 200/105 and 188/99
casters/material variants. The disabled upper-tunnel image differed from the
verified pre-diagnostic `333ce1a` image at the same camera/resolution in one
pixel: mean absolute RGB difference 0.0000449074, maximum component difference
74. This one regression comparison establishes neither byte equality nor
normal interactive performance.

Private receipts `root-86e6885-independent-review.json` and
`root-86e6885-dust2-review.json` retain source, artifact and PNG identities,
recomputed pixels and inspection results; no game assets are published.
These captures prove the opt-in diagnostic path operates on an installed
scene. They do not establish native shadow filtering, a baked/direct lighting
composition law, or 2015/2018 lighting fidelity. Normal rendering remains on
its existing lighting path.
