# Close buy menu after purchase

The installed Legacy `scripts/game_options.txt` declares `closeonbuy` as a
Yes=1 / No=0 choice at priority850. Its localized label is read from the same
install. The original Game Settings scene now admits that row through the
existing options bridge; no movie or game assets are included here.

The preference accepts only finite numeric0 or1 and persists in the player
data folder's existing `config/user.cfg`. Missing/invalid values preserve the
current stay-open default (No). This preserves rewrite behavior; the native
default and exact close timing have not been observed.

The buy menu still queues purchases through ordinary `TickInput`, and the
simulation still validates and applies them. Only a successful local
`GameEvent::Purchase` can arm the optional close while the menu is open.
Failed purchases, another player's purchases and receipts arriving while
closed cannot arm a future opening. The next buy-menu input turn consumes the
request once, after resetting `just_closed`, so the existing cursor-grab path
recaptures input. No look sway, simulation behavior or protocol field changes.

Focused tests cover schema choice/reopen and saved numeric admission, plus
real purchase success/rejection/other-player events, No, one-shot closing and
cursor recapture. Compilation, those tests and a rendered original-menu
capture are integration checks; no native timing or complete settings parity
is claimed.
