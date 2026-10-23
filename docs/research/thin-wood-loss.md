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

A `0.91` prefactor before the existing linear thickness factor fits the two rifle
samples and independent AWP sample within 0.5 raw damage. It applies only if the
entry and **every** crossed face have that exact declaration. Mixed materials,
hidden nonmatching faces, other declarations and unknowns retain prior behavior.
The existing thickness budget, four-exit bound, BSP-solid guard, range, armor,
hit groups and exhausted-pellet rule are preserved. Catalog policy salt is v6.

This calibrates a parameter in the existing continuous practical curve. It does
not reconstruct the native formula. Other thicknesses extrapolate that same
curve and remain unmeasured; there is no special cutoff or taper at four units.
Labels display rounded entry damage, and material loss is separate from final
player health after range, hit-group and armor processing. All native sessions
restored twelve exact preference snapshots. Private captures remain outside git.

Execution checks are pending.
