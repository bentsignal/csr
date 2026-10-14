# Suppressor modes

M4A1-S and USP-S right-click suppressor switching is being implemented from the
player's installed Legacy item/model declarations. The default remains attached.
The implementation keeps authority in TickInput-driven simulation and carries
mode/transition state through ordinary snapshots and dropped weapon state.

Installed model metadata provides attach/detach clips, completion and visibility
events, and sound cues. It does not alone prove native attack lockout semantics
or historical 2015 parity. Those limits remain explicit. No engine source or
asset bytes are copied into the repository.

Validation is pending; this draft is not part of the current playtest candidate.
