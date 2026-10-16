# Authored headshot damage multiplier

The authoritative damage calculation multiplied every head hit by four even
though the installed catalog already retained a different `headshot multiplier`.
Resolved CS:GO Legacy 1.38.8.1 schema SHA1
`c1361d351802b7be5f55245c331f5f1d6b36ad35` declares:

| Weapon | Damage | Head multiplier | Armor ratio |
|---|---:|---:|---:|
| M4A1-S | 38 | 3.475 | 1.4 |
| Desert Eagle | 53 | 3.9 | 1.864 |
| M4A4 | 33 | 4 | 1.4 |
| AK-47 | 36 | 4 | 1.55 |

Using the existing armor calculation at zero distance, the ignored M4A1-S
declaration changed helmeted head damage from the declared 92 to 106, making
an otherwise surviving 100-health target die. These numbers are derived from
installed declarative data and the current simulation law, not new native
`player_hurt` observations. They do not establish every damage/armor boundary.

Head hits now use a fully parsed finite positive declaration. Absent, zero,
negative, nonfinite or malformed custom metadata keeps the existing fourfold
fallback. Other hit groups, damage/range/armor constants, aim, recoil and
weapon readiness are unchanged. Existing ContentIds, raw metadata and pack
loading remain intact. Catalog identity gains a semantic salt when a valid
nondefault declaration changes damage, so older builds cannot silently agree
on the same raw schema while applying a different multiplier. Raw schema
identity used by measured recoil/import gates stays unchanged; no gameplay
wire fields are added.

One regression parses a small catalog with the resolved M4A1-S and AK stats,
then fires ordinary `World::step`/`TickInput` shots at a helmeted opponent.
It requires the M4A1-S headshot to leave the target alive, preserves its chest
damage and the absent-declaration AK headshot, and checks a nonfinite custom
declaration retains the fallback. It does not call damage directly or create
a separate weapon simulation. Required compilation/runtime verification is
pending the orchestrator's scheduled checks.

This is current installed Legacy correctness. The owner's 2015 balance target
still needs its own complete dated metadata profile and behavior evidence;
this change does not transplant 2023 body damage into a claimed 2015 profile.
No game implementation code or game asset bytes are included.
