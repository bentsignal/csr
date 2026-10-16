# 2015 weapon cases and knives

The owner explicitly requested the knives/finishes and weapon cases that
existed through 2015-12-31. This takes precedence over the menu's general
2018 visual reference. This bounded availability policy covers weapon cases
and rare knife model/paint combinations only. It does not complete historical
filtering of ordinary weapon finishes, stickers, souvenir packages, capsules,
music kits or gloves, nor establish exact case odds/wear/seed distributions.

## Installed identity and persistence

`classic_cosmetics` recognizes 39 current Legacy weapon cases by their exact
installed `(definition ID, schema key)` metadata pair, admitting the 17 below.
An arbitrary pack case reusing an integer ID with a different key passes
through. Unknown/future identities also pass through: the catalog currently
has no pack-origin field, so a CS:GO whitelist must not silently ban custom
content. Updating Legacy with new Valve identities requires extending this
recognition table; this is not a universal future-content date classifier.

Knives use recognized Legacy class names and closed model/paint pairs. Other
custom weapon classes pass through. ContentIds and raw catalogs remain
unchanged. All saved inventory records, equipment records and case counts
remain losslessly stored, including later items. Availability is enforced
when listing, looking up/granting, opening/rolling, equipping, and constructing
the existing local loadout. Preserved unavailable knives are not applied to
that loadout. No render system changes gameplay or weapon readiness.

Raw `Catalog.finishes`, `Case.contents` indices and model metadata stay intact.
A filtered contents iterator selects reward indices without renumbering them.
The UI and console use the same availability policy. Existing later records
can therefore become available under a future content policy without a save
migration or losing ownership. No game assets are copied into this repository.

## Dated weapon cases

| Definition | Case | Official release date | Reference |
| --- | --- | --- | --- |
| 4001 | CS:GO Weapon Case | 2013-08-14 | [Valve](https://blog.counter-strike.net/2013/08/7425/) |
| 4002 | eSports 2013 Case | 2013-08-14 | [Valve](https://blog.counter-strike.net/2013/08/7425/) |
| 4004 | CS:GO Weapon Case 2 | 2013-11-08 | [Valve](https://blog.counter-strike.net/2013/11/7955/) |
| 4003 | Operation Bravo Case | 2013-09-19 | [Valve](https://blog.counter-strike.net/2013/09/7647/) |
| 4005 | eSports 2013 Winter Case | 2013-12-18 | [Valve](https://blog.counter-strike.net/2013/12/9589/) |
| 4009 | Winter Offensive Weapon Case | 2013-12-18 | [Valve](https://blog.counter-strike.net/2013/12/9589/) |
| 4010 | CS:GO Weapon Case 3 | 2014-02-12 | [Valve](https://blog.counter-strike.net/2014/02/9583/) |
| 4011 | Operation Phoenix Weapon Case | 2014-02-20 | [Valve](https://blog.counter-strike.net/2014/02/9578/) |
| 4017 | Huntsman Weapon Case | 2014-05-01 | [Valve](https://blog.counter-strike.net/2014/05/9557/) |
| 4018 | Operation Breakout Weapon Case | 2014-07-01 | [Valve](https://blog.counter-strike.net/2014/07/9804/) |
| 4019 | eSports 2014 Summer Case | 2014-07-10 | [Valve](https://blog.counter-strike.net/2014/07/9866/) |
| 4029 | Operation Vanguard Weapon Case | 2014-11-11 | [Valve](https://blog.counter-strike.net/2014/11/10812/) |
| 4061 | Chroma Case | 2015-01-08 | [Valve](https://blog.counter-strike.net/2015/01/11109/) |
| 4089 | Chroma 2 Case | 2015-04-15 | [Valve](https://blog.counter-strike.net/2015/04/11845/) |
| 4091 | Falchion Case | 2015-05-26 | [Valve](https://blog.counter-strike.net/2015/05/11988/) |
| 4138 | Shadow Case | 2015-09-17 | [Valve](https://blog.counter-strike.net/2015/09/12543/) |
| 4186 | Revolver Case | 2015-12-08 | [Valve](https://blog.counter-strike.net/2015/12/13325/) |

The identity table comes from the player's installed Legacy declarative
`items_game.txt`, SHA1 `c1361d351802b7be5f55245c331f5f1d6b36ad35`.
The dates come from the linked official release announcements. Current case
contents still come from the installation, not a copied skin catalog.

## Rare knife pairs and pools

The original five are Bayonet500, Flip505, Gut506, Karambit507 and M9Bayonet508.
[Huntsman509](https://blog.counter-strike.net/2014/05/9557/),
[Butterfly515](https://blog.counter-strike.net/2014/07/9804/),
[Falchion512](https://blog.counter-strike.net/2015/05/11988/) and
[ShadowDaggers516](https://blog.counter-strike.net/2015/09/12543/) complete the
nine admitted rare families. Stock CT/T and ArmsRace gameplay knives are
unaffected.

Original finishes use installed IDs0(vanilla),5,12,38,40,42,43,44,59,72,77,143,175.
The original five additionally admit Chroma98,409,413,414,415-421 and
Damascus410, except M9 uses411. Compatibility is read through installed icon
pairs; no finish is created for a model without installed support. The
installed metadata audit found168 compatible painted pairs plus9vanilla
pairs. This compatibility count is not independent phase-by-phase 2015 asset
identity proof.

Each admitted case retains its right rare group: original five/original
finishes for the early cases and Revolver; original five/Chroma for Chroma
and Chroma2; the single Huntsman, Butterfly, Falchion or Shadow family with
original finishes for its case. Chroma cases do not award vanilla knives.
[Valve's Chroma2 announcement](https://blog.counter-strike.net/2015/04/11845/)
explicitly reuses the six original Chroma finish families.

Later recognized models such as Bowie are unavailable. [Bowie arrived in
February2016](https://blog.counter-strike.net/2016/02/13629/).
A valid old model can also have a later finish: current Legacy has
Doppler617-619 on Butterfly/Shadow and Ultraviolet620/621 on Huntsman/Falchion.
The closed paint pairs prevent matching these by their shared English names.
[Valve introduced second-generation knives in Chroma finishes through Spectrum
in March2017](https://blog.counter-strike.net/2017/03/18198/). Exact translated
names are never used for this availability decision.

## Limits and verification

Historical owned availability differs from current case/drop membership.
[Valve changed Huntsman contents in June2014](https://blog.counter-strike.net/2014/06/9699/):
current case leaves omit five items still in its item set (PoisonDart315,
Curse310, DesertWarfare311, Retribution307, Orion313). Howl309 is another
historical exception, explicitly mentioned in [July2014 notes](https://blog.counter-strike.net/2014/07/9924/).
This PR does not filter ordinary weapon finishes by a union of current cases,
which would incorrectly discard historical owned skins. The23 historical
non-case collection keys and37 souvenir candidates from the private audit
remain follow-up planning metadata; souvenir reward/sticker application parity
is not admitted here.

One mixed-inventory regression checks rejection before mutation, exact owned
record preservation, filtered reward indices, preserved raw catalog indices,
local loadout suppression of a preserved later knife, an admitted old knife,
and custom identity pass-through. One ignored installed-catalog report checks
the real parser's admitted 17 cases/nine families/177 pairs and valid raw indices.
Runtime/required verification is pending the orchestrator's scheduled lease.
