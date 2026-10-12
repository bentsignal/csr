# Classic inventory capability enumeration

The classic inventory bridge answers the observed read-only native methods
`GetItemCapabilitiesCount(itemId)` and `GetItemCapabilityByIndex(itemId, index)`.
The plural `Capabilities` in the count method is intentional: the installed
tile's public `GetItemCapabilityCount()` forwards to that native name.

Private offscreen probes used two synthetic owned inventory rows backed by the
player's installed Chroma Case (definition 4061) and Chroma Case Key (1323).
Settled pointer movement followed by separate right-button press/release showed
the authored context menu. Public tile getter calls and native callback logs
established the count/list signatures; no authored function bodies were read.
Game assets and raw captures remain outside the repository.

The bridge resolves an existing opaque local inventory ID to its definition,
then reads enabled root `capabilities` flags from installed `items_game.txt`.
It combines repeated item/prefab tables, preserves unrelated inherited flags,
and honors later-parent/local overrides. The returned list uses schema declaration
order; exact original native ordering has not been measured. Missing, cyclic or
overlarge inheritance fails closed. Unknown owned IDs return count zero, and
invalid list indices return `undefined`; these are bridge fallback policies,
not measured original engine error values.

The installed Chroma Case inherits `decodable` and `can_delete`. Its
`item_class` is `supply_crate`, and its tool restriction matches the key's
`community_case_6`. The key's tool type is `decoder_ring`. These data fields do
**not** establish native `IsTool`, decoding-error, selected-tool, consumption or
reward semantics. In particular, returning the compatibility restriction from
`GetDecodeableRestriction` produced an authored error token in a private negative
probe, so that mapping is withheld.

This change enumerates metadata only. Authored case accept/reward callbacks,
key ownership, loot preview population, original reel timing and loot odds are
unverified and are not implemented by this bridge. A capability token does not
authorize a local mutation. The source fallback's local case opening remains a
separate implementation, with no claim of Chroma reference fidelity.

Tests cover prefab overrides, repeated tables, disabled/malformed flags,
inheritance failure, hidden owned items, opaque-ID lookup and invalid indices.
An explicit ignored test reads the actual Legacy schema without changing it:

```text
cargo test -p client --lib installed_chroma_metadata -- --ignored
```

It checks the Chroma capability flags and the separate class/tool/restriction
fields, without treating those fields as native API behavior.
