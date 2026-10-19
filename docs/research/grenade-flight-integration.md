# Grenade free-flight displacement

October 19, 2026. Original CS:GO Legacy 1.38.8.1 public projectile receive-state observations establish a free-flight integration correction. This does not establish complete historical throw or smoke-lineup parity. No SDK, leaked/decompiled source or CS2 behavior is used.

The simulation applied the existing 320-unit gravity update, then used the final velocity for the entire position step. This adds excess downward displacement each tick. Advancing position with the mean of the previous and updated velocity matches the observed pre-contact intervals while retaining the final velocity for collision response. The grounded HE bot target/clearance arc now uses the same displacement law.

Two original 128 Hz intervals, anchored at observed moving states rather than an inferred release instant, have vertical endpoint errors 0.2197/0.1026 with the previous step and 0.0146/0.0049 with mean velocity. The conservative two-endpoint position-rounding allowance is 0.0625 units. The regression replays both intervals through authority and restored prediction; a 64 Hz resampling is explicitly a consistency check, not independently observed native 64 Hz behavior.

Gravity, release strength/timing, inherited movement, collision masks, restitution, fuse and effects retain their existing constants. Contacts can occur earlier or later along the corrected path. The displacement code is shared by grenade kinds; the direct reference here is HE before contact. Cross-kind contact/rest/support and bot prediction checks remain required. The existing grenade content/simulation identity advances to reject peers with the previous flight rule.

Remaining acceptance includes original release behavior across strengths, pitch, movement and jump throws; native bounce parity; and historical smoke lineups on matching map revisions and tick rates. A passing pre-contact regression alone does not close those outcomes.
