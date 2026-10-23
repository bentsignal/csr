# Practical thin-wood loss calibration

Three guarded original CS:GO Legacy 1.38.8.1 sessions fired one ordinary shot
through the same Inferno post. The shooting eye was `(2108, 2370, 223.218811)`,
yaw 180, velocity zero. A post-shot observer camera separated the original
penetration labels. No camera movement occurred before the shot.

- AK-47: entry damage 36, lost damage 7.8, thickness 10.3 cm, entry distance 3.2 m.
- M4A4: entry 33, lost 7.3, same thickness/distance. The actual M4A4 and 30-to-29
  magazine change were inspected; the earlier slot-only attempt retained an AK
  and was excluded.
- AWP: entry 115, lost 20.1, same thickness/distance. Actual AWP and 5-to-4 magazine
  change were inspected. Public `weapon_accuracy_nospread 1` removed the unscoped
  miss risk; this controls the ray, not the penetration coefficients.

The native endpoints both identify `wood`. The installed post VMT resolves
`$surfaceprop wood`; its declaration is `W`, penetration modifier `0.9`, damage
modifier `0.6`. The previous practical curve retained roughly 52% of AK/M4 entry
power at this post, while the original samples retain roughly 78%.

The first implementation incorrectly used synthetic AWP penetration power 3.
Installed `weapon_awp_prefab` declares 2.5; AK and M4A4 both declare 2. The
previous claim that all three samples fit within 0.5 raw damage was incorrect:
with its actual power, the 0.91 curve predicts AWP loss 21.98 rather than 20.1.

Fitting the same single prefactor to the three observations with their mounted
powers gives 0.925315 (least squares), rounded to `0.925`. Predicted losses are
AK 7.325, M4A4 6.715 and AWP 20.444: all within 0.6 raw damage. It applies
only if the entry and **every** crossed face have that exact declaration. Mixed materials,
hidden nonmatching faces, other declarations and unknowns retain prior behavior.
The existing thickness budget, four-exit bound, BSP-solid guard, range, armor,
hit groups and exhausted-pellet rule are preserved. Catalog policy salt is v7.

A separate guarded AK shot at pitch 30 through the same post gives entry 36,
lost 7.9 and thickness 11.9cm at distance 3.7m. The zero-velocity shooting eye
was `(2108, 2370, 297.119568)`, noclip enabled, public nospread enabled. Both
side views identify wood at both endpoints. The existing geometry computes
`4 / cos(30 degrees)` units; the corrected curve predicts loss 8.04. This is a
nearby oblique holdout, not evidence for thick cover or other material families.

This calibrates a parameter in the existing continuous practical curve. It does
not reconstruct the native formula. Other thicknesses extrapolate that same
curve and remain unmeasured; there is no special cutoff or taper at four units.
Labels display rounded entry damage, and material loss is separate from final
player health after range, hit-group and armor processing. All native sessions
restored twelve exact preference snapshots. Private captures remain outside git.

The regression now uses actual AWP power 2.5, includes the geometric oblique
holdout, and keeps continuity and hidden-backing controls. The installed-data
fixture also checks all three mounted weapon powers to prevent another
synthetic-parameter mismatch. All 1,334 workspace tests, the installed Inferno
firing/opaque controls at 64/128 Hz, the normal PLAY build and immutable Dust II
probe/sim pass for this correction. CI is pending. Prior #315 checks passed
with the incorrect synthetic AWP value and do not establish its advertised
AWP error bound.
