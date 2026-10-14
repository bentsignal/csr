# Suppressor modes

M4A1-S and USP-S right-click suppressor switching reads the player's installed
Legacy item/model declarations. New weapons remain attached by default; weapons
without a complete removable-suppressor profile keep their existing secondary
behavior (including the permanently suppressed MP5-SD).

Per-player TickInput starts the transition in sim. Suppressor attachment changes
at the model's named completion event, while firing/reloading/toggling remain
blocked through the imported animation duration. Holding right-click cannot
repeat the change. Holstering/drop cancels unfinished playback and preserves the
last completed attachment state. That state travels with the weapon across
pickup, snapshot restore and prediction replay. Protocol 15 includes the state
and each reliable Fire event's shot-time suppression mode.

The client samples the installed nonlooping attach/detach clips and their exact
SHOW/HIDE events, uses the declared attached/detached Draw activity, and schedules
the authored screw/movement sound cues. Holstering/death cancels queued cues;
joining during a transition skips cues already in the past. Held world models
and dropped weapons hide only the model's named silencer bodypart when detached.
World character attach/detach gestures are not implemented by this change; their
bodypart visibility follows committed attachment state rather than borrowing
first-person event timing. Inspect requests during a transition are consumed
and discarded, so holding/requesting inspect cannot override or queue a later
animation.

Shots choose the original schema's sound_special1 or sound_single_shot binding
from the event's mode, so later switching/dropping cannot alter a delayed shot's
sound. A verified classic sound override applies only when its binding matches;
removing a suppressor never reuses the suppressed classic waveform.

The installed 2023 M4A1-S and USP-S transitions each last 4.833 seconds. Named
completion events occur earlier; their times and visibility events are imported,
not copied into constants. Full-clip attack lockout is provisional: declarations
alone do not prove the native input/attack lockout or holster cancellation rules.
No recoil, spread, damage or other mode-dependent ballistic law changes here;
installed alternate attributes need a separately validated mode binding. Native
2015 timing, rendering and audio parity remain open. No engine implementation
or game asset bytes enter the repository.

Validation so far: two focused sim tests pass at 64/128 tick, including authority
versus snapshot replay/prediction identity, trigger/reload lockout, held secondary
input, holster cancellation and drop/pickup preservation. The installed profile check
also passes for M4A1-S/USP-S and confirms MP5-SD stays nonremovable. Twelve focused protocol
tests pass, including rejection of pre-v15 peers before admission. Full workspace,
rendered and audio validation are pending. This draft
is separate from the current staged playtest candidate.
