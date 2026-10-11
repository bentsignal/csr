CSGO REWRITE
============

A from-scratch rewrite of Counter-Strike: Global Offensive in Rust. It plays
using the CS:GO Legacy files you already have from Steam.

This is a fan project. It is not affiliated with or endorsed by Valve. It
contains no Valve game files: it reads your own CS:GO Legacy install.


WHAT YOU NEED
-------------
* Windows 10 or 11 (64-bit) and a graphics card with DirectX 12 or Vulkan.
* CS:GO Legacy, installed through Steam. It is called "Counter-Strike:
  Global Offensive" in your Steam library. If you do not have it installed,
  the game offers to open Steam's installer for you.


FIRST LAUNCH
------------
1. Unzip this folder anywhere (for example your Desktop).
2. Double-click "Play CSGO.bat".
   Windows may say "Windows protected your PC". Click "More info", then
   "Run anyway". (The game is not code-signed.)
3. The game finds CS:GO Legacy on your PC and asks "Use this folder?".
   Click Yes. If it is not installed, click Yes to open Steam's installer,
   let it download, then start the game again.
4. That's it. The game remembers the folder from then on.


PLAYING
-------
Play CSGO.bat           the main menu; pick a map and mode
Change CSGO Folder.bat  pick the CS:GO Legacy folder again

WASD move, Space jump, Ctrl crouch, Shift walk, LMB fire, R reload,
E use/defuse, 1-5 / Q / wheel weapons, G drop, B buy menu, Tab scoreboard,
Esc menu. Plant: select the bomb (5) in a bomb site and hold LMB.


YOUR DATA
---------
Everything the game saves (settings, skins, cases, the CS:GO folder choice)
is in the "userdata" folder next to the game. Delete that folder to reset
to a fresh install. Copy it to keep your progress when updating.


TROUBLESHOOTING
---------------
* "That folder holds Counter-Strike 2": you picked Counter-Strike 2. Pick
  the csgo.exe in Steam's "csgo legacy" folder instead.
* The console window shows what the game is doing; if something fails, the
  message there (or in the pop-up) says why.


CREDITS AND LICENCES
--------------------
Built on the Bevy engine. Licensed under Apache-2.0 (LICENSE.txt). CS:GO
assets are read from your own install and belong to Valve.
