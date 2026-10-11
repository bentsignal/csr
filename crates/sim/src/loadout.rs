//! What each player chose to carry and how it looks, applied by the sim.
//!
//! A client sends its [`Loadout`] once (and again when the player changes
//! it); the sim then equips every weapon the player is given: the chosen
//! knife instead of the team default, the finish (paint, wear, StatTrak,
//! stickers) for each weapon, and in deathmatch the practice primary and
//! pistol. Weapons keep their finish and owner when dropped, so a skin
//! travels with the gun. Choices that the rules do not allow (a rifle as a
//! "knife", the other team's guns) are ignored rather than trusted.

use crate::weapon::{self, Cosmetic, WeaponState};
use crate::{PlayerId, Team, World};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;
use weapons::{Catalog, Side, WeaponClass, WeaponDefinition};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Loadout {
    /// Deathmatch practice weapons by class name: T primary, T pistol,
    /// CT primary, CT pistol.
    pub practice: [Option<String>; 4],
    /// Knife carried instead of the team's default knife (class name).
    pub knife: Option<String>,
    /// Finishes by weapon class name.
    pub skins: BTreeMap<String, Cosmetic>,
}

/// May a team choose this weapon as a practice primary (slot 1) or pistol
/// (slot 2)?
pub fn practice_allowed(def: &WeaponDefinition, team: Team) -> bool {
    let slot = weapon::slot(def);
    (slot == 1 || slot == 2)
        && match def.side {
            Side::Both => true,
            Side::Terrorist => team == Team::Terrorist,
            Side::CounterTerrorist => team == Team::CounterTerrorist,
        }
}

fn default_knife(team: Team) -> &'static str {
    if team == Team::Terrorist { "weapon_knife_t" } else { "weapon_knife" }
}

impl Loadout {
    /// Replaces the deathmatch primary and pistol in `guns` with the
    /// player's practice picks for `team`, where the rules allow them.
    pub(crate) fn practice_weapons(&self, catalog: &Catalog, team: Team, guns: &mut Vec<WeaponState>) {
        let base = if team == Team::CounterTerrorist { 2 } else { 0 };
        for slot in 0..2u8 {
            let Some(def) = self.practice[base + slot as usize]
                .as_deref()
                .and_then(|n| catalog.get(n))
                .filter(|d| practice_allowed(d, team) && weapon::slot(d) == slot + 1)
            else {
                continue;
            };
            guns.retain(|w| weapon::slot(&w.def) != slot + 1);
            guns.push(WeaponState::new(Arc::new(def.clone())));
        }
        guns.sort_by_key(|w| weapon::slot(&w.def));
    }

    /// Equips one weapon for its owner: the chosen knife model and the
    /// finish for its class.
    pub(crate) fn dress(&self, owner: PlayerId, team: Team, catalog: &Catalog, w: &mut WeaponState) {
        if w.def.class == WeaponClass::Knife {
            let name = self
                .knife
                .as_deref()
                .filter(|n| catalog.get(n).is_some_and(|d| d.class == WeaponClass::Knife))
                .unwrap_or(default_knife(team));
            if w.def.class_name != name {
                if let Some(def) = catalog.get(name) {
                    w.set_def(Arc::new(def.clone()));
                }
            }
        }
        w.cosmetic = self.skins.get(&w.def.class_name).map(Cosmetic::sanitized);
        w.cosmetic_owner = Some(owner);
    }
}

impl World {
    /// Sets what a player carries from now on and re-equips the weapons
    /// they own and hold.
    pub fn set_loadout(&mut self, id: PlayerId, loadout: Loadout) {
        let catalog = self.catalog.clone();
        let Some(p) = self.player_mut(id) else { return };
        p.loadout = loadout;
        let team = p.team;
        for w in &mut p.weapons {
            if w.cosmetic_owner == Some(id) {
                p.loadout.dress(id, team, &catalog, w);
            }
        }
    }

    /// Weapons new to their holder (bought, spawned with, picked up for the
    /// first time) take the holder's knife and finishes. Already owned
    /// weapons keep their owner's finish wherever they go.
    pub(crate) fn dress_weapons(&mut self) {
        let catalog = self.catalog.clone();
        for p in &mut self.players {
            let (id, team) = (p.id, p.team);
            for w in p.weapons.iter_mut().filter(|w| w.cosmetic_owner.is_none()) {
                p.loadout.dress(id, team, &catalog, w);
            }
        }
    }
}
