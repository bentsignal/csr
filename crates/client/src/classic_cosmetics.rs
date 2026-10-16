//! Classic case/knife availability only, through 2015-12-31.
//! Raw catalogs and saved inventories remain complete. Unrecognized identities
//! pass through for future content packs; these are Legacy identities, not a
//! whitelist of arbitrary ContentIds. See docs/research/classic-case-knives.md.
use crate::cosmetics::{Case, Finish, is_rare_knife};

pub(crate) fn case_available(case: &Case) -> bool {
    if !known_case(case) {
        return true;
    }
    matches!(
        case.id,
        4001 | 4002
            | 4003
            | 4004
            | 4005
            | 4009
            | 4010
            | 4011
            | 4017
            | 4018
            | 4019
            | 4029
            | 4061
            | 4089
            | 4091
            | 4138
            | 4186
    )
}

pub(crate) fn item_available(weapon: &str, paint: u32) -> bool {
    if !is_rare_knife(weapon) {
        return true;
    }
    let original = matches!(
        weapon,
        "weapon_bayonet"
            | "weapon_knife_flip"
            | "weapon_knife_gut"
            | "weapon_knife_karambit"
            | "weapon_knife_m9_bayonet"
    );
    let second = matches!(
        weapon,
        "weapon_knife_tactical"
            | "weapon_knife_butterfly"
            | "weapon_knife_falchion"
            | "weapon_knife_push"
    );
    (original || second) && (original_paint(paint) || (original && chroma_paint(weapon, paint)))
}

pub(crate) fn finish_available(finish: &Finish) -> bool {
    item_available(&finish.weapon, finish.id)
}

pub(crate) fn reward_available(case: &Case, finish: &Finish) -> bool {
    if !case_available(case) || !finish_available(finish) {
        return false;
    }
    if !known_case(case) || !is_rare_knife(&finish.weapon) {
        return true;
    }
    let weapon = finish.weapon.as_str();
    match case.id {
        4017 => weapon == "weapon_knife_tactical" && original_paint(finish.id),
        4018 => weapon == "weapon_knife_butterfly" && original_paint(finish.id),
        4091 => weapon == "weapon_knife_falchion" && original_paint(finish.id),
        4138 => weapon == "weapon_knife_push" && original_paint(finish.id),
        4061 | 4089 => {
            matches!(
                weapon,
                "weapon_bayonet"
                    | "weapon_knife_flip"
                    | "weapon_knife_gut"
                    | "weapon_knife_karambit"
                    | "weapon_knife_m9_bayonet"
            ) && chroma_paint(weapon, finish.id)
        }
        4001 | 4002 | 4003 | 4004 | 4005 | 4009 | 4010 | 4011 | 4019 | 4029 | 4186 => {
            matches!(
                weapon,
                "weapon_bayonet"
                    | "weapon_knife_flip"
                    | "weapon_knife_gut"
                    | "weapon_knife_karambit"
                    | "weapon_knife_m9_bayonet"
            ) && original_paint(finish.id)
        }
        _ => false,
    }
}

fn original_paint(paint: u32) -> bool {
    matches!(
        paint,
        0 | 5 | 12 | 38 | 40 | 42 | 43 | 44 | 59 | 72 | 77 | 143 | 175
    )
}
fn chroma_paint(weapon: &str, paint: u32) -> bool {
    matches!(paint, 98 | 409 | 413 | 414 | 415..=421)
        || (paint == 411 && weapon == "weapon_knife_m9_bayonet")
        || (paint == 410 && weapon != "weapon_knife_m9_bayonet")
}
fn known_case(case: &Case) -> bool {
    LEGACY_CASE_IDENTITIES.contains(&(case.id, case.key.as_str()))
}

// Metadata identities from the installed Legacy declarative schema, SHA1
// c1361d351802b7be5f55245c331f5f1d6b36ad35. An ID collision in a custom pack
// does not make that pack a Valve case: the exact schema key must match too.
const LEGACY_CASE_IDENTITIES: &[(u32, &str)] = &[
    (4001, "crate_valve_1"),
    (4002, "crate_esports_2013"),
    (4003, "crate_operation_ii"),
    (4004, "crate_valve_2"),
    (4005, "crate_esports_2013_winter"),
    (4009, "crate_community_1"),
    (4010, "crate_valve_3"),
    (4011, "crate_community_2"),
    (4017, "crate_community_3"),
    (4018, "crate_community_4"),
    (4019, "crate_esports_2014_summer"),
    (4029, "crate_operation_vanguard"),
    (4061, "crate_community_6"),
    (4089, "crate_community_7"),
    (4091, "crate_community_8"),
    (4138, "crate_community_9"),
    (4186, "crate_community_10"),
    (4187, "crate_community_11"),
    (4233, "crate_community_12"),
    (4236, "crate_community_13"),
    (4281, "crate_gamma_2"),
    (4288, "crate_community_15"),
    (4351, "crate_community_16"),
    (4352, "crate_community_17"),
    (4403, "crate_community_18"),
    (4471, "crate_community_19"),
    (4482, "crate_community_20"),
    (4548, "crate_community_21"),
    (4598, "crate_community_22"),
    (4620, "crate_community_23"),
    (4669, "crate_community_24"),
    (4695, "crate_community_25"),
    (4698, "crate_community_26"),
    (4717, "crate_community_27"),
    (4747, "crate_community_28"),
    (4790, "crate_community_29"),
    (4818, "crate_community_30"),
    (4846, "crate_community_31"),
    (4880, "crate_community_32"),
];

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires read-only installed CS:GO Legacy catalog"]
    fn installed_2015_weapon_cases_and_knives_report() {
        let dir = vfs::find_csgo_dir().expect("CS:GO Legacy install");
        let mut fs = vfs::Vfs::new();
        fs.mount_game_dir(&dir).unwrap();
        let catalog = crate::cosmetics::Catalog::load(&fs).unwrap();
        let cases: Vec<_> = catalog
            .cases
            .iter()
            .filter(|c| !c.souvenir && known_case(c) && case_available(c))
            .map(|c| c.id)
            .collect();
        let knives: Vec<_> = catalog
            .finishes
            .iter()
            .filter(|f| is_rare_knife(&f.weapon) && finish_available(f))
            .collect();
        let models: std::collections::BTreeSet<_> =
            knives.iter().map(|f| f.weapon.as_str()).collect();
        assert_eq!(cases.len(), 17);
        assert_eq!(models.len(), 9);
        assert_eq!(knives.len(), 177);
        assert!(
            catalog
                .cases
                .iter()
                .any(|c| c.id == 4880 && !case_available(c))
        );
        assert!(
            catalog
                .finishes
                .iter()
                .any(|f| f.weapon == "weapon_knife_survival_bowie" && !finish_available(f))
        );
        assert!(
            catalog
                .cases
                .iter()
                .all(|c| c.contents.iter().all(|&i| i < catalog.finishes.len()))
        );
        for case in catalog.cases.iter().filter(|c| cases.contains(&c.id)) {
            assert!(
                catalog
                    .available_contents(case)
                    .any(|i| is_rare_knife(&catalog.finishes[i].weapon))
            );
        }
        eprintln!(
            "CLASSIC_CASE_KNIFE_REPORT raw_cases={} raw_finishes={} admitted_weapon_cases={cases:?} admitted_knife_models={models:?} admitted_knife_pairs={}",
            catalog.cases.len(),
            catalog.finishes.len(),
            knives.len()
        );
    }
}
