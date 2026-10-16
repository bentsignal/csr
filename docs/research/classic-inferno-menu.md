# Classic Inferno beside the remake

The Offline with Bots chooser previously offered the pre-rework Dust II archive
but omitted the pre-rework Inferno that the owner requested. The same original
map-card schema now offers Inferno (2016) beside installed Inferno, keeping each
selection mapped to its distinct catalog name and ContentId.

The existing pinned Workshop definition is `de_inferno_2016`, Valve compatibility
item 778803540, archived build 1.35.5.2 before the October 2016 rework. This dated
archive is not proof of exact 2015 content. Retrieval, cancellation, private
runtime storage and checksums use the existing archive loader; no game assets
or new downloads enter the public repository. The card currently borrows the
installed Inferno artwork and controls, with the archive year in its title.
Exact old loading art and menu imagery remain separate fidelity work.

Only the two explicitly offered pre-rework archives are added. Arbitrary native
callback text cannot become a map path, and later compatibility snapshots stay
out of this menu. Original map-group ordering is retained with each archive
inserted immediately after its base map, then contiguous UI indices rebuilt.

The existing Dust II schema test is retained. One focused Inferno test checks
that the remake and archive resolve separately, the original art symbol is
preserved, the group remains adjacent and an unoffered later snapshot is
rejected. The combined `d71d5f6` workspace and PLAY checks passed. Its installed
classic Inferno probe and simulation report zero failures after the separate
v44 prop importer correction in #170. Before that correction, the real probe
rejected `light_fixture01.mdl`; the same failure was reproduced on the earlier
RELEASE baseline and was not suppressed.

The actual 1920x1080 chooser shows both Inferno cards adjacent, followed by
Mirage, Nuke and Overpass. Its additional column moved Dust II on row two.
The first rendered lifecycle consequently clicked Vertigo and correctly failed
the expected-map assertion; that negative is retained. The opt-in fixture now
uses the observed Dust II card position and can select the archived Inferno
card with `CSR_TEST_FLOW_CLASSIC_INFERNO=1`. The fixture sends ordinary authored
pointer input; it does not bypass the map callback.

The corrected combined PLAY `ec95b6a` completed the ordinary launch through
Dust II, Pause, original Video vsync Disabled/Enabled/Disabled, Back, Resume,
Home and the archived Inferno card. Actual loaded-level logs identify
`de_inferno_2016`; the private map matches the pinned BSP SHA1. The two chooser
images, old Inferno match and three changed Video images were personally
reviewed at 1920x1080. Both Home visits start one music sink and stop it before
Loading; owner configuration is unchanged. This verifies the selection and
session integration, not exact old card art, overall lighting, physical input
latency or download/cancellation behavior. The initial wrong-card fixture
negative is retained. The corrected full workspace also passes; combined
RELEASE verification also passed on immutable730F23C3, including both map
probes/simulations, all1,425 sound decodes and the same ordinary archived-card
lifecycle. Six changed1920x1080 images were independently reviewed. Merged
master601ebc3 has the exact tested ec95b6a crates tree; no separate branch
RELEASE build or complete native map parity is claimed.
