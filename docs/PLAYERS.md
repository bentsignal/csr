CSGO REWRITE
============

A from-scratch rewrite of Counter-Strike: Global Offensive in Rust. It plays
using the CS:GO Legacy files you already have from Steam.

This is a fan project. It is not affiliated with or endorsed by Valve. It
contains no Valve game files: it reads your own CS:GO Legacy install.


WHAT YOU NEED
-------------
* Windows 10 or 11 (64-bit) and a graphics card with DirectX 12 or Vulkan.
* Steam, with CS:GO Legacy installed. It is called "Counter-Strike: Global
  Offensive" in your Steam library and is free. If it is not installed, the
  game offers to install it for you.


INSTALLING
----------
1. Download CSGORewrite-win-Setup.exe (from the link you were sent, or the
   Releases page: github.com/bentsignal/global-offensive/releases).
   If your browser asks whether to keep the file, choose Keep.
2. Run it. Windows may say "Windows protected your PC". Click "More info",
   then "Run anyway". (The game is not code-signed, so Windows does not
   recognise it yet. You only see this once.)
3. It installs in a few seconds, without asking for an administrator,
   puts "CSGO Rewrite" on your desktop and in the Start menu, and starts.
4. The game finds CS:GO Legacy by itself. If it is not installed, click
   "Install it with Steam" and then Install in Steam. Leave the CSGO Rewrite
   window open: the game starts by itself when the download is done.
5. It asks once whether to add CSGO Rewrite to your Steam library. If you
   say yes, it shows up next to your other games with its own artwork. If
   Steam is open it restarts Steam to do this; pick "Not now" if you are in
   a Steam call, and it asks again next time.


UPDATES
-------
Automatic. Each time you start the game it checks for a new version in the
background. When there is one, it downloads while you play and you get a
notification. It installs the next time you start the game, which takes a
few seconds. You never need to download the installer again.


PLAYING
-------
Start CSGO Rewrite from the desktop, the Start menu or Steam.

WASD move, Space jump, Ctrl crouch, Shift walk, LMB fire, R reload,
E use/defuse, 1-5 / Q / wheel weapons, G drop, B buy menu, Tab scoreboard,
Esc menu. Plant: select the bomb (5) in a bomb site and hold LMB.


YOUR DATA
---------
Everything the game saves (settings, skins, cases, the CS:GO folder choice)
is in this folder (paste it into the File Explorer address bar):

    %LocalAppData%\CSGO-Rewrite\userdata

It is kept when the game updates and when you uninstall it. Delete it to
start over. If you played an unzipped copy before installing, the first
start copies your progress from it (from your Desktop, Downloads or
Documents).


CONTENT PACKS
-------------
After installing CSGO Rewrite, double-click a .csrpack file to install it.
The game checks the file and shows progress with a Cancel button. If the
file cannot be installed, a message explains why. A successful install
opens the game, or shows a notification if the game is already running.
Cancelling before installation finishes keeps the previous pack version.
Packs are kept with your other data and survive game updates.
Choosing installed pack maps from the menu is still being added.


UNINSTALLING
------------
Windows Settings > Apps > Installed apps > CSGO Rewrite > Uninstall. This
also takes it out of your Steam library (close Steam first for that).


PORTABLE ZIP (NO INSTALL)
-------------------------
Each release also has a CSGO-Rewrite-<version>-windows-x64.zip. Unzip it
anywhere and double-click "Play CSGO.bat" ("Change CSGO Folder.bat" picks
the CS:GO Legacy folder again). It keeps its data in the "userdata" folder
next to the game and does not update itself.


TROUBLESHOOTING
---------------
* "That folder holds Counter-Strike 2": you picked Counter-Strike 2. Pick
  the csgo.exe in Steam's "csgo legacy" folder instead.
* To choose a different CS:GO Legacy folder, or to add the game to Steam
  after saying no, press Win+R and run one of:
      "%LocalAppData%\CSGORewrite\current\CSGO-Rewrite.exe" setup
      "%LocalAppData%\CSGORewrite\current\CSGO-Rewrite.exe" add-to-steam
* If something fails, a message says why.


CREDITS AND LICENCES
--------------------
Built on the Bevy engine. Licensed under Apache-2.0 (LICENSE.txt). CS:GO
assets are read from your own install and belong to Valve.
