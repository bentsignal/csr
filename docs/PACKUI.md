# Private content in the front end

The Home screen has a Content packs manager beside the authored classic menu.
Players can choose a literal `.csrpack` file in the Windows picker or drop one
onto the game. The existing launcher double-click handler remains available.
No game assets, test packs or screenshots are included in the repository.

The manager owns one install or metadata worker, with at most eight queued
sources and one native picker. Progress and cancellation stay on the main UI;
reading, validation and publication run in the existing pack jobs. A cancelled
job is polled until its terminal result, including truthful success if atomic
publication already committed. Closing the game waits for worker cleanup and
terminates only its owned picker helper.

Installed map rows retain their immutable `CatalogEntry` through `MapRequest`
and the asynchronous loader. `Level::load_catalog_map` verifies the full archive
identity before mounting. Updating a pack does not change a running level; a
stale selection fails and requires a fresh catalog. Rows use manifest captions,
not ContentIds or storage paths. Maps launch in deathmatch; hostage rules and
arbitrary pack mechanics remain outside this implementation.

Catalog refresh happens on Home entry, explicit Refresh, installation completion
and return of primary-window focus. A drop during Loading or Match displays
progress and a temporary result on the current UI camera. The manager uses eight
map rows per page and a bounded scroll area. While it owns input, matching movie
key/button releases occur outside the stage, followed by focus loss; reopening
movie input restores current window focus without replaying held actions.

## Verification

Synthetic-pack tests cover bounded queueing, literal Unicode source paths,
terminal cancellation, metadata retention after a failed install and exit after
owned worker cleanup. Input tests cover balanced key/button ownership across the
overlay. The existing picker tests exercise its bounded protocol and process
ownership without pretending to interact with a real native dialog.

`CSR_TEST_PACK_FLOW=<absolute private source>` with `--screenshot` opts into a
single-process engine-input proof: drop, install, metadata refresh, Bevy pointer
hit testing on the map row, hash-bound rendered match, Home teardown and malformed
file rollback. Captures are saved beside the requested image. This harness does
not claim an operating-system file-drop or native-picker interaction test.

Rendered proof and full workspace/Legacy checks are pending on the integration
head. Actual native-picker interaction is also pending: the Windows automation
helper reports that its native pipe is unavailable after retry and reset.
