# Grounded HE bot planning uses the actual release

The bot planner and clearance check used a fixed 750-unit raw-aim arc even
when its installed-content HE definition selected the independently measured
grounded 675/10-degree release profile. Consequently the planner could approve
and aim a trajectory that the ordinary throw would not follow.

Authoritative spawning and planning now share the existing nonmutating release
calculation: the raw-aim 18-unit hull trace, mode-specific velocity profile and
actual player movement inheritance. Those calculations are factored without
changing the human/projectile law. Only an owned, profiled HE on a grounded bot
uses the new planner. Other grenades, absent profiles, airborne and tracked/aim
actors retain the previous planner. The installed metadata/content gates remain
at import; no names alone receive new launch constants.

A bounded two-angle numerical solve targets the existing simulated trajectory,
including its discrete gravity update. Its convergence/iteration limits are AI
numerical parameters, not native movement or projectile coefficients. Existing
24-segment hull clearance and nearby-target contact acceptance remain; tactical
target selection, movement wait and ordinary TickInput pull/release are retained.
No state field or wire protocol changes.

One witness uses an ordinary World/TickInput HE throw and checks its target
crossing, then inserts a real wall and verifies both clearance rejection and
actual projectile contact. It compares the planner against gameplay rather than
asserting copied speed constants. The airborne fallback stays exact. Compilation
and actual bot rendering remain pending; this is internal planner consistency,
not native AI, full grenade trajectory, surfing or historical parity.
