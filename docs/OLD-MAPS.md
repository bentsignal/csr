# Old maps

The classic versions of the competitive maps (old Dust II, old Inferno, old
Nuke, and the maps as they were in 2018) come from Valve's own Steam
Workshop uploads: whenever Valve changed a map it published the old version
as "`<Map>` [Compatibility Version 1.xx.x.x]" so old demos keep playing
([research §1](research/2018-era.md#1-old-map-versions-on-the-steam-workshop)).
Nothing of these maps is in this repository or in our releases; the game
fetches them on the player's PC (owner decision 6).

Code: `crates/workshop` (table, Steam lookup, download), `crates/level`
(`maps.rs` lists maps for menus; `Level::load` fetches an old map on first
use).

## Names

An old map is `<original>_<year>`:

- **Pre-rework classics:** the year is the last year that version was
  played, which is also the year Valve archived it: `de_dust2_2017` (before
  the October 2017 rework), `de_inferno_2016`, `de_nuke_2016`,
  `de_train_2015`, `de_vertigo_2017`.
- **"2018" maps:** the map as it was around August 2018, the era of the
  reference screenshots: `de_dust2_2018`, `de_inferno_2018`, `de_nuke_2018`,
  `de_cache_2018`, `de_train_2018`, `de_mirage_2018`, `de_overpass_2018`,
  `de_cbble_2018`, `de_canals_2018`. Valve uploads a snapshot just before it
  changes a map, so the 2018 state is the first snapshot uploaded after
  August 2018. That assumes a snapshot exists for every change, which is
  unverified per map.

Names never collide with the install's maps, so nothing shadows them. Menus
show `OldMap::title` ("Dust II (2017)") and `OldMap::note`.

`csr maps` prints the full table with Workshop ids and versions.

## How a map gets onto the PC

`workshop::Store::fetch` (and so `csr fetch-map`, and the first
`play`/`probe`/`sim` of an old map):

1. Already in `<data>\workshop\maps\<name>.bsp`: done.
2. A Steam library already holds the item
   (`steamapps\workshop\content\730\<id>\*_legacy.bin`, any library Steam
   knows about): its SHA-1 is checked and the `.bsp` is unpacked from it.
   Nothing is downloaded.
3. Otherwise it asks Steam's public, keyless
   `ISteamRemoteStorage/GetPublishedFileDetails` for the item, checks that
   the answer matches the built-in table (Valve's account
   76561198082857351, app 730, size, and the SHA-1 that ends the CDN URL),
   and downloads `file_url` from `cdn.steamusercontent.com` to
   `<data>\workshop\download\<id>.bin.part`. Interrupted downloads resume
   (HTTP Range). The finished file must match the table's size and SHA-1.
4. The `.bin` is a zip holding one `.bsp`; it is unpacked (the zip CRC is
   checked) to `<name>.bsp.part` and renamed into place, so a half-written
   map never appears. The `.bin` is then deleted.

`<data>` is the player data folder (`%LocalAppData%\CSGO-Rewrite\userdata`
for installed copies, the repository root for source builds, where
`/workshop/` is git-ignored). `csr remove-map <name|all>` deletes maps.

The `workshop` folder is mounted at the lowest priority of the file system,
after the game's VPK and loose files. The map's own pakfile is mounted first
as for any map, so its embedded cubemaps and patched materials win.

## What old maps miss

Old BSPs were built against the game files of their day. Assets the 2023
CS:GO Legacy install no longer has are reported by `csr probe <map>` and
drawn with placeholders in game. The install also has no `.nav` (bot
navigation) and no radar overview for these names; see the probe and sim
results in the PR that added them.

## Adding a map

Find Valve's upload (research §1 lists the known ones), check it:

```sh
curl -s -X POST https://api.steampowered.com/ISteamRemoteStorage/GetPublishedFileDetails/v1/ \
  -d 'itemcount=1&publishedfileids[0]=<id>'
```

The `creator` must be `76561198082857351` and `consumer_app_id` 730. Add an
`OldMap` to `workshop::OLD_MAPS` with `file_size` and the 40-hex SHA-1 from
the end of `file_url`, then run `csr probe` and `csr sim` on it.
