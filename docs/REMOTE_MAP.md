# Local preparation for remote joining

`client/src/remote_map.rs` is a renderer-independent loader boundary. Call
`LocalMapRequest::resolve(&info.content, game_dir, &installed_packs,
&archive_store)` in the existing bounded loader worker, then `prepare()` on
that owned request. Both calls perform disk I/O. The request retains explicit
local stores, the validated advertised manifest and an immutable selected
private catalog entry. The advertised `map_name` never selects a path.

Builtin maps resolve through the discovered CS:GO install definitions.
Archives must already be installed in the supplied archive store; neither
Steam copying nor network acquisition is invoked. Private maps resolve only
through the supplied validated catalog. A catalog containing skipped invalid
or conflicting archives is rejected conservatively, including errors for an
unrelated pack, so admission cannot silently use the scanner's first retained
conflicting definition. Private archive replacement invalidates the retained
selection instead of silently refreshing it.

Preparation reuses Level's canonical scoped loaders, checks its retained BSP
and successfully parsed catalog digests, and compares the complete manifest
before creating a map-only `sim::World`. Missing or malformed catalogs have no
network identity even though offline Level loading supplies a fallback catalog.
`PreparedMap` owns the Level, map-only World and verified ContentManifest.
No players, bots, loadouts, simulation stepping, GPU work or admission happen
in this module. The frontend sets the negotiated rate, finishes match/GPU
creation, then calls `RemoteClient::provide_map` with the world and manifest.

Typed `PrepareError` values distinguish malformed advertisements, unsupported
readers, unavailable installs, absent or ambiguous local definitions, invalid
catalogs, changed selections, loading failures and content mismatches. Refresh
requires a new explicit resolution; errors never fall back to generic data or
an advertised URL. Cancellation and bounded worker ownership stay with the
existing frontend loader. Drop an obsolete prepared result without admitting
it. This leaf does not yet schedule rendered joining.
