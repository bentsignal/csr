//! Pack tools participate in the ordinary inventory cycle. Their primary use
//! is tick-owned; the retained weapon cannot fire through an activation click.
use super::Module;
use crate::{Player, TickInput};
use movement::buttons;
use std::sync::Arc;
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Equipped {
    pub id: String,
    pub archive_hash:String,
    pub used: bool,
}
pub(crate) fn cycle(player: &mut Player, modules: &[Arc<Module>], forward: bool, time: f32) {
    let tools = if player.is_bot { &[][..] } else { modules };
    let guns = player.weapons.len();
    let count = guns + tools.len();
    if count == 0 {
        return;
    }
    let current = player
        .equipped_mechanic
        .as_ref()
        .and_then(|item| tools.iter().position(|m| m.id == item.id))
        .map_or(player.active, |i| guns + i);
    let next = if forward {
        (current + 1) % count
    } else {
        (current + count - 1) % count
    };
    if next < guns {
        if next != player.active || player.equipped_mechanic.is_some() {
            player.select(next, time);
        }
        return;
    }
    equip(player, &tools[next - guns], time);
}
/// Puts a tool in hand, retaining the CS weapon. Refused mid-throw.
pub(crate) fn equip(player: &mut Player, module: &Module, time: f32) -> bool {
    if player.active_weapon().is_some_and(|w| {
        w.grenade_throw
            .is_some_and(|s| s.phase == crate::utility::ThrowPhase::Throwing)
    }) {
        return false;
    }
    if let Some(w) = player.weapons.get_mut(player.active) {
        w.grenade_throw = None;
        w.reload_end = None;
        w.cancel_burst();
        w.cancel_suppressor(time);
        w.reset_zoom(time);
    }
    player.equipped_mechanic = Some(Equipped {
        id: module.id.clone(),
        archive_hash: module.archive_hash.clone(),
        used: false,
    });
    player.plant_start = None;
    true
}
pub(crate) fn use_equipped(
    player: &mut Player,
    input: &mut TickInput,
    modules: &[Arc<Module>],
    allowed: bool,
    world: &dyn movement::TraceWorld,
) {
    let Some(item) = player.equipped_mechanic.clone() else {
        return;
    };
    let attack = input.buttons & buttons::ATTACK != 0;
    if item.used {
        if !attack {
            player.equipped_mechanic = None;
        }
    } else if allowed && attack && player.last_input.buttons & buttons::ATTACK == 0 {
        if let Some(module) = modules.iter().find(|m| m.id == item.id && m.archive_hash==item.archive_hash) {
            let stop = player.mechanic.as_ref().is_some_and(|s| s.id == item.id);
            super::select(player, (!stop).then_some(module), world);
            player.equipped_mechanic.as_mut().unwrap().used = true;
        } else {
            player.equipped_mechanic = None;
        }
    }
    input.buttons &= !(buttons::ATTACK | buttons::ATTACK2 | buttons::RELOAD);
}
