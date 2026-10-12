# Current scene asset ownership

Call `remote_scene_assets::gather(&mut World)` after scene spawn commands and
resources have been applied. It returns deterministic, deduplicated Mesh/Image
AssetIds for MatchEntity mesh/material/image-node components and the retained
CharacterModels, ViewModels and GroundModels assets. Persistent entities are
excluded even if generic ownership previously added MatchEntity. Models that have not yet
spawned instances remain owners. Every bound image field of their WorldMaterial
is included, covering reused fallback textures, lightmaps, prop lighting and
cosmetic surfaces. ViewModels exposes a read-only `assets()` iterator including
weapon, decorations, gloves and both arms collections.

Pass the result to RenderGate::begin, keeping the scene owners alive during
preparation. Gathering neither creates strong handles nor retains assets.
Missing optional model resources are allowed; missing WorldMaterial storage or
an owned material is a typed error. Mesh/Image IDs and the temporary material
dependency set are each bounded by the gate's MAX_SCENE_ASSETS policy.

Do not use global asset-store snapshots or after-minus-before IDs as ownership.
TextureCache's temporary fallbacks/loaded textures, unused prop-light images
and replaced model entries can create fresh assets without final scene owners.
Old persistent menu assets and despawned scenes can also remain in the stores
until asset-removal events run. This helper omits them, while including reused
handles actually owned by the current scene.

This covers initial map/model CPU/render preparation. It does not claim to
prewarm future shader variants or enumerate renderer-internal resources,
unspawned StandardMaterial feedback/utility pools, or every future UI image.
Their loading and eventual visual readiness remain separate integration work.
No gameplay state, GPU resources or asset stores are mutated by gathering.
