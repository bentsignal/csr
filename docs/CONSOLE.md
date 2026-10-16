# Developer console and controls

Open or close the console with the backquote key (`), above Tab on a US
keyboard. Escape closes it without opening the pause menu. The console works
on Home, during loading and in matches. It captures new movement, shooting, weapon-switch and menu device input while
open. The match continues, so already committed gameplay actions may finish.
Closing clears queued local controls and recaptures the cursor when appropriate.

Enter runs one command. Up/Down recall history; Left/Right/Home/End edit the
line; Tab completes command names. Quoted arguments are accepted. `help` lists
the implemented commands. Unsupported Source commands report an error rather
than pretending to change anything. This first version does not implement
aliases, command chaining, exec scripts, clipboard paste or every Source cvar.
Its dark text console is a functional first version, not measured VGUI parity.

## Settings and FPS

Settings save to the existing player `config/user.cfg`; they survive relaunch.

```text
volume 0.5
sensitivity 2
cl_showfps 1
cl_crosshaircolor 1
cl_crosshairsize 8
cl_crosshairgap 4
cl_crosshairthickness 2
cl_crosshairdot 0
cl_radar_scale 0.5
cl_radar_rotate 1
closeonbuy 1
```

A setting without a value prints its current value. `cl_showfps 1` adds a
compact FPS/frame-time readout to the existing match diagnostics area. F3 still
toggles the full diagnostic HUD. The open console also displays FPS, including
on the pause and Home screens. These are smoothed rendered-frame measurements,
not input-to-photon latency measurements. Crosshair values use the existing
renderer/settings ranges; command support does not establish Source pixel
parity for every cvar.

## Keyboard, mouse buttons and mouse wheel

```text
bind mwheeldown +jump
bind mwheelup +jump
bind space +jump
bind mouse4 +jump
bind mouse5 lastinv
bind x +attack
bind c +attack2
bind x lastinv
bind 1 slot1
bind z invnext
bind y invprev
bindlist
```

Each wheel event requests a jump through the ordinary TickInput path; it does
not hold jump on later ticks. This preserves keyboard jump and allows manual
wheel-jump/bhop testing. It does not enable automatic bunny hopping or claim
movement parity. Restore the ordinary weapon wheel with:

```text
bind mwheelup invprev
bind mwheeldown invnext
```

`unbind mwheelup` or `unbind mwheeldown` disables that direction. `unbind space`
or `bind space ""` removes that keyboard binding without removing mouse or wheel
bindings for the same action. Unbound keys stay unbound after relaunch;
`bind space +jump` restores keyboard jump. Keyboard binds
support the configurable actions shown by `bindlist`: movement,
jump, crouch, walk, use, reload, buy, drop, scoreboard, inspect and primary/secondary attack. For example,
`bind f +use` uses the existing key-swapping behavior: if F already belongs to
inspect, inspect takes Use's former key. Letters and the existing modifier,
space, tab, arrow/navigation keys are supported.

`mouse1` through `mouse5` mean left, right, middle, back and forward buttons.
Each accepts any of those actions, `+attack`, `+attack2`, `invprev`, `invnext`
or `lastinv`. A bind replaces only that button's action; other buttons and
keyboard bindings remain available. For example, `bind mouse5 +attack2` and
`unbind mouse2` move secondary fire to the forward side button. `bind mouse1
+attack` and `bind mouse2 +attack2` restore the default attacks. `unbind mouse4`
or `bind mouse4 ""` clears that button. These bindings save and load with the
other settings. Pointer-capture clicks remain suppressed until release,
including when the click has been rebound to jump.

Keyboard attacks default to unbound, preserving ordinary mouse attacks in old
configs. Both held keys and short taps use the same TickInput attack path as
mouse attacks. A capture-suppressed mouse button does not suppress a keyboard
attack bound to the same command. `unbind x` or `bind x ""` persists the cleared
key without removing mouse attack bindings. Arbitrary command binds and
rebinding the console key remain open. The classic keyboard settings page
currently exposes only its connected sensitivity/inversion controls; the
console makes these action bindings reachable from the classic frontend.

Weapon selection is also configurable: `lastinv`, `slot1` through `slot5`,
`invnext` and `invprev` use the existing one-shot player input path. Old
configs retain Q for the previous weapon and1–5 for the primary, pistol,
knife/Zeus, grenade cycle and bomb slots. These defaults now appear in
`bindlist` instead of bypassing bindings. `unbind q` or `bind q ""` really
removes Q's action; `bind x lastinv` moves it to X and persists that choice.
`bind q lastinv` restores Q. The same applies to number keys, for example
`unbind 1` and `bind 1 slot1`. Selection accepts short taps, obeys normal
gameplay/menu/capture guards, and does not repeat while a key stays held.
Mouse and wheel weapon-selection bindings remain independent.

## Direct grenade selection

```text
use weapon_smokegrenade
bind x "use weapon_smokegrenade"
bind mouse4 "use weapon_flashbang"
```

The same named commands support `weapon_hegrenade`, `weapon_flashbang`,
`weapon_smokegrenade`, `weapon_molotov`, `weapon_incgrenade` and `weapon_decoy`.
Direct bindings default to unbound and append to the existing saved action
indices, preserving earlier movement and attack bindings. Key and mouse taps
select once, obeying the normal gameplay, menu and pointer-capture guards.
The fallback keyboard settings page and `bindlist` list the added actions;
the original movie's settings controls remain unchanged.

`use` resolves the named grenade against the current catalog and the living
player's carried inventory, then queues one ordinary `TickInput.utility`.
An explicit console request can select while the console has focus; it does
not admit hardware input. Home, loading, pause, buy, disconnected, spectator
and noclip states reject the command. The server resolves ownership again at
the actual input tick, so a grenade dropped in the meantime is never granted.
No stock item index, throw timing, physics or network format changes.

## Buying

```text
buy ak47
buy m4a1_silencer
buy vesthelm
buy defuser
```

`buy` resolves weapon names from the active match's catalog, with or without
`weapon_`. Common aliases include `m4a4`, `m4a1s`, `usp`, `p2000`, `zeus`,
`galil`, `sg553` and `scout`. Equipment accepts `vest`/`kevlar`,
`vesthelm`/`assaultsuit` and `defuser`, optionally prefixed with `item_`.
Unknown or ambiguous names report an error.

The command queues one ordinary `TickInput.buy`, including while the console
has focus. It uses the same mode, team, money, buy-time, buy-zone and carrying
limits as the buy menu, and reports the reason when those checks reject it.
A requested purchase is not a grant: authority checks the rules again when
that tick arrives. Home, loading, pause and unready remote sessions reject
purchases. Arbitrary command binds and buy scripts remain unsupported.

## Sessions

```text
maps
map de_dust2
connect 127.0.0.1:27015
status
disconnect
```

`map` validates against the current map picker and requests the ordinary
frontend loader using its selected offline mode, team and bot count. It ends a
remote session before starting an offline map. While loading, use `disconnect`
to cancel before choosing another map. `connect` accepts numeric IPv4/IPv6
addresses (port defaults to 27015), retains the selected team and uses the
normal admission/content-validation path. Hostnames are not resolved yet.
`disconnect` cancels preparation/owned hosting and returns Home. `quit` requests
the ordinary application shutdown. No console command writes gameplay state
directly or bypasses the network authority.

## Runtime verification and limits

The final `db8d1f4` client was checked in two bounded desktop launches using a
private player profile. Real focused keyboard input opened the console,
executed settings/bind commands and left a partially typed `sens` line visible.
Escape resumed the normal match. A second launch read back saved
`cl_showfps 1` and `bind mwheeldown +jump`; the owner's separate config hash
was unchanged, and both clients exited successfully with clean raw logs.

All four 1920×1080 captures were visually reviewed by the implementing agent
and the coordinating agent: `initial-console.png`, `initial-resumed.png`,
`persisted-console.png` and `persisted-resumed.png`. They establish readable
header/prompt rows, typed command output and normal match rendering after
closing. Private captures are retained outside the repository.

This verifies command entry and persisted wheel binding, not physical mouse
wheel timing or native VGUI appearance. Wheel-jump delivery also has a focused
TickInput test; full movement/bhop fidelity and the unsupported commands listed
above remain separate work.

The mouse-button extension was checked at source `fec4511` in two actual
1920x1080 client launches. Typed commands assigned mouse4 to jump, mouse5 to
secondary attack and cleared mouse2; all three values read back after relaunch.
The coordinating agent inspected both console and resumed-match captures;
the original HUD remained intact and the owner's prior configuration was
unchanged. The first capture host exited unexpectedly and did not qualify;
its owned client was closed and the failed run retained separately.

Input regressions cover remapped pointer-capture clicks, keyboard independence,
short mouse taps between fixed ticks and sustained secondary attack. A follow-up
also keeps a fresh side-button attack independent of a capture-held button
bound to the same command. Source `13b18ff` passed 309 client tests (three
installed-data tests ignored), the PLAY build and zero-failure probe/sim.
These checks do not establish physical mouse latency or complete native binds.
