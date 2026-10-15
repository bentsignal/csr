# Offline picker power-button availability

Owner feedback UI08: the authored top-right power button is disabled while
Offline with Bots is open. It should remain available and require confirmation
before exiting, avoiding accidental shutdown.

## Current evidence and scope

The cached Home/Match worker lifecycle and owned picker epochs are already on
the base branch. Preserve those ownership fences and the player's original
installed Scaleform widgets; do not add a replacement power icon or dialog.

Static review finds no dedicated quit-confirmation route in the current
frontend bridge. Generic BasePanelRunCommand dispatch closes other pages and
passes unknown commands to the main thread; quit/quitgame then exits directly.
That does not prove which authored power-button callback is missing or whether
the picker blocks the original hit target. Actual public callback/property and
rendered evidence is needed before choosing a fix.

The next observation will inspect the original Home power target, open the
original picker and compare its public enable/input state. Establish the
authored confirmation widget/callback through ordinary public API observations
and legitimate references. Do not inspect authored function bodies or bytecode.

No runtime pass or original-confirmation fidelity is claimed yet. Private
captures remain outside the repository. Builds and graphical observations use
the parent agent's shared resource leases.

## Historical confirmation reference

Cuperschmid and Hildebrand's own CS:GO interface evaluation in the
[SBGames 2013 proceedings, Figure 3, printed page 373](https://www.sbgames.org/sbgames2013/proceedings/artedesign/44-dt-paper.pdf)
shows the original exit confirmation: a black dialog titled `EXIT GAME`, a
confirmation question, and `OK` / `Cancel` controls. This supports restoring an
original confirmation instead of immediately exiting from a menu click.

The paper identifies a Mac OS X test environment. Its dated screenshot proves
the historical dialog and labels, not exact 2015/2018 PC geometry or picker
input behavior. A private copy/render was inspected; copyrighted screenshots
are not committed. The installed widget still needs a public runtime contract.
