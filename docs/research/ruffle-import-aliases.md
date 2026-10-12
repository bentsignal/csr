# Request-scoped Ruffle import aliases

The game pins both Ruffle packages to the immutable owner-fork commit
`5b20c6507c51657dabdad25e172cc81c8587d898`, replacing
`bf1f4c8300d705eb6bffcb78bf08ed486d8c90cd`. This includes the existing
[opt-in missing-call diagnostics](https://github.com/bentsignal/ruffle/pull/1)
and the reviewed [generic alias fix](https://github.com/bentsignal/ruffle/pull/2).
Diagnostics remain disabled unless explicitly enabled; the game adds no new
movie overrides or engine callback behavior in this dependency update.

Public Ruffle source showed that its import-name map retained one local ID.
Multiple local references to a shared exported symbol therefore lost earlier
IDs. The fix retains distinct aliases and carries the exact ImportAssets
request's owned name/ID declarations through asynchronous loading. Each
exported prototype binds only to its request's IDs. Name-based lookup follows
the latest declaration even when requests finish in reverse order.

The request also retains the exact parent MovieClipShared generation. Late
completion cannot bind aliases into a replacement/unloaded parent, copy its
exports, redirect init actions into it, or resume its preload. Replacing a
parent with the same SwfMovie still invalidates that generation. Ordinary
movie loading and character-ID collision policy retain their existing paths.

## Evidence and limits

Only public library source and static installed import/export metadata were
used. No game action/function bodies or assets are part of the patch or tests.
Four synthetic regressions use generated empty movies and export metadata to
exercise distinct alias instantiation, duplicate/latest-ID behavior, single
imports, both two-source completion orders through named instantiation, and
parent replacement/removal with nested completion.

The final Ruffle leaf head `8a70f3a51dd38de8ad94fb9df8261b34e7f8d39b`
passed all 127 core library tests, core Clippy for library/tests with warnings
denied, full formatting, and diff checks. Its exact-head
[fork CI run](https://github.com/bentsignal/ruffle/actions/runs/38255229041)
passed before squash merge. Independent review checked named-export ordering,
generation guards, owned strings, GC tracing, and prototype cloning.

After merging master `d0ba18562ad0456524558215309351c50f358e42`, the combined
application source `7f831e25e88e057b87c5505892569d4414823ecf` passed workspace
tests (442 passed, zero failed, four existing ignored), release Dust II probe
and ten-round simulation (both `failures: 0`), and soundcheck (1,425 decoded,
zero unsupported/crashing). Locked full dependency metadata and the asset
guard passed. Private logs are under
`%USERPROFILE%\csr-fidelity\ruffle-pin-43-7f831e2`.

Fresh original-movie pointer evidence is under
`%USERPROFILE%\csr-fidelity\host-lobby-staged-mouse-go-final`, with
`evidence.json`, logs, and `called-18/19/20/23/24.png`. The images show selected
Dust II with enabled ACCEPT, the No Bots dialog, a clean reopened lobby, and
GO press/release. Native callbacks reached `UpdatedSelections` with the staged
Dust II selection, then `LobbyScreen.LaunchGame()` after pointer events. All
five image hashes and the copied probe executable hash were independently
checked. The helper is `1a69e9e224765cc40e7ccb020812ccd0e84a559b`; executable
SHA256 is `ca7a4c2445715a9e74f5d629b10ace5102aa19ffae2a8860dcf77bc58093dc4f`.

The private combined Ruffle source
`b77177b7413f311f3d8d3bf153f2afd79766d7d7` has the same entire Git tree as
the immutable pin: `8954058b4a5ae349ab5cdcc3d5f2961e1e063438`. This supports
restored authored-control rendering and pointer callbacks for the exact pinned
library. The proof used owned engine fixtures for roster/privacy/getter data
and Ruffle mouse events; no server or game was launched. Production hosting
integration, original engine defaults, menu geometry, and broader fidelity
remain separate work. Captures and executables stay private and outside Git.
