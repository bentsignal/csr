# Authored dropped weapon models

Ground weapons previously reused their held world model. An owned Legacy
1.38.8.1 stationary M4A4 drop identifies the separate authored
`models/weapons/w_rif_m4a1_dropped.mdl`. The installed schema already declares
`model_dropped`; this is runtime data, not an engine naming convention.

The catalog now retains inherited `model_dropped`, and ground rendering loads
that asset through the selected level VFS, keyed by ContentId. Packs may omit
the field; an empty declaration or unavailable dropped asset keeps the existing
held-model fallback. Held weapon resources remain separate. Ammo/ownership,
finish and decoration tags, and per-instance suppressor hiding keep their
existing paths. Retained ground meshes/materials also enter the multiplayer
scene preparation gate and leave with normal match-resource cleanup.

The parser-semantics salt distinguishes this interpretation of the same schema
without changing raw schema identity or gameplay wire state. This addition
keeps the existing static placement heuristic. It does not implement toss,
flight, contact or settling physics; owner feedback WP08 remains open.

The frozen integration executable containing source e282328 rendered two
ordinary pending-drop/TickInput cases. Actual traces select the authored
M4A4 and M4A1-S dropped models, switch the active weapon to USP-S and retain
zero shots. Both 1600x900 original images visibly show intact ground weapons
on their sides; the attached M4A1-S suppressor is present. This admits selected
model/visible integrity, not exact native rest orientation, finish coverage,
detached-mode presentation or native motion. Native screenshots and raw asset
metadata remain private; no game asset bytes are included in the repository.

A separate public physics diagnostic is not a velocity reference: its freshly
observed client prediction index was 346, while the later dropped server label
was 348. Its blank reply does not establish the correct physics object's
capability. Both identity observations and the negative receipt are retained;
no native toss coefficients are inferred from that run.
