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
rejected. Workspace checks, real-data probe/simulation, actual rendered card
selection and optimized combined verification are pending; this is not yet a
delivered menu feature.
