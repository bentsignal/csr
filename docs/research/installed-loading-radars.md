# Current installed loading radars

Status: source candidate stacked on PR198. No new rendered admission yet.

The installed Nuke and Mirage each contain a 1024x1024 DXT1 overview and an
80x80 collection emblem, but no `maps/<map>_loading.jpg`. The current bridge
already admits those installed radars, redirects only the current original
movie's exact overview request, preserves aspect inside its authored box and
reports the actual fit's Init with the selected map and generation. Production
loading waits for a matching post-Init raster and another application iteration
before synchronous scene imports. No new image source or layout is required.

The custom loading status was hidden only for the two pinned archives. This
change applies that existing behavior to every current fitted radar, retaining
the exact selected serial, successful art-raster fence and initialized context.
The archive-only diagnostic predicate remains strict, so an ordinary map cannot
satisfy the existing archive proof. Missing, failed, unsupported or stale art
keeps the functional status surface. Error messages, mouse Cancel and Escape
are unchanged. The optional JPEG path is not newly admitted by this change.

One focused predicate regression distinguishes installed and archive art,
current and stale generations, absent Init and the unsupported non-radar path.
Syntax and diff checks precede integration; Cargo and actual Nuke/Mirage loading
captures remain pending the orchestrator's shared-resource lease. The earlier
owner report of generic Nuke/loading proportions is not closed by asset
presence or source inspection. Native layout parity remains open.

The existing 16-stage frontend controller has one mutually exclusive opt-in,
`CSR_TEST_FLOW_INSTALLED_LOADING=1`, selecting installed Nuke then Mirage
through the original map cards. Its phases, pointer cadence, ordinary match
entry, Video roundtrip and cleanup are unchanged. Loading captures require
the real fit listener's current map/serial/target Init acknowledgment, two
subsequent raster assignments and the same production painted predicate. A
natural Loading interval too short to capture fails; no completion callback
or import delay is injected. The archive option retains its opaque image and
archive-only predicates. The ordinary observer reports its Init only after
the production fit callback passes its ownership checks, rather than treating
an early diagnostic image callback as a current-generation acknowledgment.
