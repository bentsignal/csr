# Legacy v44 prop hitboxes

Classic Inferno's probe rejected `models/props_urban/light_fixture01.mdl`
as an unsupported hitbox layout, which also prevented importing its model.
This change reads the observed v44 AABB layout rather than omitting its
hitbox or suppressing the model error.

## Clean-room basis

The independent [studio-header format documentation](https://pysourcesdk.github.io/ValveMDL/datastructures/studiohdr.html)
describes the v44–48 header. Its [hitbox format documentation](https://pysourcesdk.github.io/ValveMDL/datastructures/misc.html)
lists sets and bone-local AABBs with reserved storage. No SDK implementation,
leaked source, or decompiled code was used.

The actual private classic Inferno BSP has SHA1
`7563DD93D99FAC8C9CCEAD3E15BC1DCB650BD30B` and contains no embedded override
for this model. Its selected installed Legacy VPK model has SHA256
`68CAD385696CB64997AA615B7BF7CCA9D10A6A257BAB83605339CD84650BFD51`.
It is a 1,552-byte v44 model with one named set and one nonempty AABB.
The existing zero-set early return was already correct.

A bounded audit of all 121 installed `props_urban` model headers found six
v44 models: `light_fixture01`, `patio_table2`, `railing04`, `railing04long`,
`railing04med`, and `railing04small`. All six have a 12-byte set and a
68-byte AABB record, with zero in the last 32 bytes. Their hitbox-name words
point to empty strings when interpreted from the file start, while adding
the hitbox record's address would leave the file. The set-name offsets
remain set-relative.

This observed v44 name convention differs from the documentation's general
nested-offset description. The implementation makes the version distinction
explicit; it does not retry malformed newer-model offsets as absolute.

## Supported boundary

Only v44 joins the existing v48/v49 readers. v44 uses file-relative hitbox
names, ordinary bone-local AABBs, and requires zero reserved bytes. Existing
count budgets, table bounds, bone bounds, finite geometry, ordered AABB
bounds, and bounded terminated-name validation remain active. Other
previously unsupported versions remain unsupported. v48/v49 retain their
existing relative names and geometry interpretation.

Synthetic tests cover a nonempty absolute name, retained bounds/group,
malformed names, bones, bounds, reserved data, and unsupported versions.
An installed-data test imports complete MDL/VVD/VTX geometry for all six
observed v44 props and checks that each authored AABB remains present.

Verification is pending the scheduled focused and combined checks. This is
a parser correction; no native pixel or collision-parity claim is made.
