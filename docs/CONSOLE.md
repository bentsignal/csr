# Developer console and controls

Open or close the console with the backquote key (`), above Tab on a US
keyboard. Escape closes it without opening the pause menu. The console works
on Home, during loading and in matches. The game cannot receive movement,
shooting, weapon switches, menu clicks or typed commands while it owns input.
Closing clears queued actions and recaptures the game cursor when appropriate.

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

## Keyboard and mouse wheel

```text
bind mwheeldown +jump
bind mwheelup +jump
bind space +jump
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

`unbind mwheelup` or `unbind mwheeldown` disables that direction. Keyboard binds
support the existing 13 configurable actions shown by `bindlist`: movement,
jump, crouch, walk, use, reload, buy, drop, scoreboard and inspect. For example,
`bind f +use` uses the existing key-swapping behavior: if F already belongs to
inspect, inspect takes Use's former key. Letters and the existing modifier,
space, tab, arrow/navigation keys are supported. Keyboard unbind, arbitrary
command binds, attack/mouse-button rebinding and slot/Q/console rebinding are
not implemented in this first version. The classic keyboard settings page
currently exposes only its connected sensitivity/inversion controls; the
console makes these action bindings reachable from the classic frontend.

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
