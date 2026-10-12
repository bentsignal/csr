# Home News surface observations

Status: read-only diagnosis, 2026-10-12. No rendering or callback implementation
is changed by this note. This gap does not block the authored hosting flow.

The Home captures in the coordinated Host/Guest controller proofs at both
64 and 128 tick show News text cut off on the left. The same artifact occurs
in the isolated installed-movie helper. Moving the authored clip or changing
padding is not justified by the measurements below.

## Observed runtime contract

The installed Legacy movie emits
`CScaleformComponent_Blog.OnLoadFinished("mc:_level31.Panel.Blog", 0)` and
`OnReady()` in the private helper. The helper does not answer Blog callbacks.
Read-only inspection exposes these public member names on that clip:
`InitChromeHTMLRenderTarget`, `UpdateHTMLScrollbar`, `ShowBlogPanel`,
`HideBlogPanel`, `EnableInput`, and the HTML mouse/key handlers. Their argument
contracts and engine semantics remain unknown. No authored function bodies,
bytecode or leaked code were inspected. None of those functions was invoked
to infer an argument shape.

At 1920x1080, our existing helper queried standard Flash `localToGlobal`,
`getBounds`, width/height, visibility and public runtime properties:

| Clip | Global origin | Bounds relative to `_level0` | Local width/height |
| --- | --- | --- | --- |
| `Blog` | 698.65, 358.25 | 698.65, 358.25 to 1633.25, 1089.95 | 622.95, 487.65 |
| `Blog.NewsScroll` | 702.15, 373.75 | 699.65, 360.4 to 1609.85, 1089.95 | 606.7, 486.2 |
| `Blog.chrome_html_mc` | 703.65, 419 | Empty geometry | 0, 0 |

The empty clip's bounds return the Flash empty-bounds sentinel, not a usable
rectangle. Its public `LastScreenWidth`, `LastScreenHeight`, `__width` and
`__height` are all zero; `BrowserStarted` is true. Its `chromeNav` contains a
`ChromeHTML` movie clip, but this does not establish that an HTML browser or
render target exists. The visible body starts around x=734 in the image; this
is a visual observation, not a measured mask boundary or new layout constant.

`NewsScroll` is separately visible and contains `HeaderText` and `BodyText`.
Primitive text inspection identifies the latter as the installed "Test Blog"
placeholder. Both fields report `hscroll=0`, `maxhscroll=0`, `scroll=1`,
`maxscroll=1`, left-aligned HTML, `wordWrap=true` and `autoSize=none`. Therefore
the observed left clipping is not explained by a nonzero horizontal text
scroll. These observations support an uninitialized hosted surface with an
authored placeholder still visible; they do not prove the exact clipping cause
or a replacement surface's position, dimensions or ownership.

## Primary public documentation

[Valve's Steam HTML Surface overview](https://partner.steamgames.com/doc/features/html_surface?l=english)
describes a Chromium Embedded Framework browser with explicit creation, sizing,
navigation and destruction. Its
[public interface reference](https://partner.steamgames.com/doc/api/isteamhtmlsurface?language=english)
describes `HTML_NeedsPaint_t` pixels in BGRA format, dimensions, damage rectangle,
scroll positions and a page serial for rejecting obsolete page images. These
are useful generic surface ownership concepts. They do not document this
Legacy movie's `InitChromeHTMLRenderTarget` arguments, prove Legacy uses that
Steam API, or justify requiring Steam for our implementation.

[Autodesk's public Texture reference](https://help.autodesk.com/cloudhelp/ENU/Scaleform-Help/cpp_ref/06360.html)
describes transferring image data into a renderer texture and restricting
texture update operations to the rendering thread. This establishes a generic
Scaleform image/texture seam, not the game's custom HTML binding. Searches of
the Autodesk and Steamworks documentation did not identify the named Legacy
initialization method's contract. No SDK implementation was copied.

The older [Scaleform spike](scaleform-spike.md) identifies the HTML target as a
future integration seam. Its proposed news image is not an implemented or
verified native surface contract.

## Private provenance and limits

The evidence stays outside Git under
`%USERPROFILE%\csr-fidelity\home-news-geometry-{1,2,3}`. Each directory has
`invocation.json`, stdout/stderr logs and private PNGs. The successful owned
processes were PID41896, PID61852 and PID44332, respectively, all exit0. The
first directory's successful invocation supersedes an earlier invalid-CLI run.
No process remains active. No game configuration was changed or server launched.

The copied `host-lobby-staged-mouse-go-final\menu_probe.exe` has SHA256
`CA7A4C2445715A9E74F5D629B10ACE5102AA19FFAE2A8860DCF77BC58093DC4F`.
Its helper source is `1a69e9e224765cc40e7ccb020812ccd0e84a559b`; combined Ruffle
source `b77177b7413f311f3d8d3bf153f2afd79766d7d7` has the same Git tree as
the reviewed immutable pin `5b20c6507c51657dabdad25e172cc81c8587d898`.
The exact combined-source proof is described in
[the import alias evidence](ruffle-import-aliases.md).

Successful stdout SHA256 values, ordered by directory:

- 1: `52BDB55235F3084F44BE73E436F33E61380826B7AD4715DB5C782324D560578C`
- 2: `4D3F04854C28F5F5D259803BB075E20E8F7C05851C9562B57330A91161C6127D`
- 3: `6B1592C662191A180A6FA0D6AE9FD5116D9A47FFB70E1D7D521FA8C689F8716B`

The helper is an owned offscreen Ruffle fixture. Its background `(0,0)` pointer
event is an engine-input fixture required by the helper CLI, not OS interaction.
The observations do not establish original-engine HTML behavior, live news,
mask geometry, callback argument values or 1:1 browser fidelity.

## Next bounded scope

Keep the installed geometry and placeholder untouched until an observed native
callback contract or primary public documentation establishes how a surface is
attached. Then test an asset-free, explicitly owned surface first: dimensions,
resize, immutable generation, late paint rejection and teardown on Home
replacement. Only a validated attachment should replace or suppress the
placeholder. Avoid guessing arguments, manufacturing original news content or
adding an unrelated padding correction.
