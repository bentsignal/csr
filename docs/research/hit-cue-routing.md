# Attacker head-hit audio

October 19, 2026. Clean-room observations use owned CS:GO Legacy 1.38.8.1, public console output, visible impacts and actual live targets. No game assets or captures are distributed here. This is a scoped correction, not an independent 2015 audio or complete mixing-parity claim.

## Observation and change

M4A1-S nonlethal head hits produce a head Feedback entry, with a distinct helmet variant selected at contact. Fatal flesh headshots and AWP fatal helmet headshots additionally produce the ordinary headshot death entry. Body hits instead retain ordinary armor/flesh impacts. Native source ownership remains the struck player.

The renderer previously selected only ordinary damage entries and never requested either Feedback entry. It now routes head Damage to the appropriate Feedback entry for the viewed attacker and adds the ordinary headshot cue on a fatal head Kill. Contact protection and death-hit positions are immutable event data, so later armor loss, death or respawn cannot reclassify the cue. Body routing and the existing routing for victim/bystander listeners remain unchanged. Their exact native dispatch contracts are still open.

The sound parser now retains finite, ordered, explicit Foley distance bounds. Feedback declares 120..4000 units and ordinary helmet declares 120..1400; entries without overrides keep 400..1100. An explicit malformed field rejects the override. This reuses the existing fitted falloff, spatial panning, gains and victim ownership. It does not turn SNDLVL_NONE into a global hit-confirmation ping.

## Validation and limits

Production event dispatch, deferred voice creation and actual spatial emitters are covered for fatal/nonfatal helmet/flesh head pairs, body impacts and unrelated/noclip perspectives. Fixtures check the authored range survives the previous cutoff and that fatal Feedback/death entries coexist. Existing public soundcheck explicitly requires and decodes both Feedback entries.

At runtime commit a89c062, all 1,108 workspace tests pass (53 ignored), normal Release succeeds, installed sound decoding reports zero failures, and the immutable executable completes Dust II probe and simulation with zero failures. Native observations establish cue selection for the attacker's perspective; full distance-curve/mixing parity, audible onset under catch-up, victim/bystander parity and original-era waveform parity remain separate acceptance work.
