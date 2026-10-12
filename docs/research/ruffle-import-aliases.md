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

Application workspace, release Dust II probe/simulation, sound decoding, and
private rendered-menu checks remain pending for this pin update. The library
regressions establish alias correctness; they do not establish that a specific
original menu control is restored. That requires fresh screenshots and actual
mouse-driven engine evidence from the integrated application. Original menu
geometry and broader fidelity remain separate measurement work.
