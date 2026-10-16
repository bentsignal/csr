//! Bounded declarative difficulty import from the active content VFS.
//! Named templates configure our AI; original persona selection is not inferred.
use std::collections::BTreeMap;
use sim::bots::Profile;

#[derive(Clone, Debug, Default)]
pub struct Profiles {
    choices: BTreeMap<u8, Option<Profile>>,
}
impl Profiles {
    pub fn load(fs: &vfs::Vfs) -> Self {
        fs.read_string("gamemodes.txt").map(|modes|
            Self::from_text(&modes, &fs.read_string("BotProfile.db").unwrap_or_default())).unwrap_or_default()
    }
    pub fn resolve(&self, choice: u8) -> Result<Option<Profile>, String> {
        self.choices.get(&choice).copied().ok_or_else(||
            "This bot difficulty has no compatible authored profile in the selected content. Choose another difficulty or No Bots.".into())
    }
    pub fn from_text(modes: &str, profiles: &str) -> Self {
        let mut result = Self::default();
        if modes.len() > 4 * 1024 * 1024 { return result; }
        let Ok(doc) = kv::parse(modes) else { return result };
        let root = doc.block("GameModes.txt").unwrap_or(&doc);
        let Some(difficulties) = root.block("botDifficulty") else { return result };
        // No Bots needs no personality file. Missing/malformed profiles must not
        // strand the player without this valid installed zero-quota choice.
        let zero: Vec<_> = difficulties.iter().filter_map(|(_, value)| value.as_block())
            .filter(|entry| entry.str("value") == Some("0")).collect();
        if zero.len() == 1 && zero[0].block("convars").and_then(|c| c.str("bot_quota")) == Some("0") {
            result.choices.insert(0, None);
        }
        if profiles.len() > 1024 * 1024 { return result; }
        // BotProfile uses Default / Template <name> / End, not KeyValues braces.
        // Retain only two numeric fields and the declared difficulty class.
        let mut blocks = BTreeMap::<String, BTreeMap<String, String>>::new();
        let mut active: Option<(String, BTreeMap<String, String>)> = None;
        for raw in profiles.lines() {
            let line = raw.split("//").next().unwrap_or("").trim();
            if line.is_empty() { continue; }
            if let Some((_, fields)) = &mut active {
                if line.eq_ignore_ascii_case("End") {
                    let (name, fields) = active.take().unwrap();
                    if blocks.insert(name, fields).is_some() { return result; }
                } else if let Some((key, value)) = line.split_once('=') {
                    let key = key.trim().to_ascii_lowercase();
                    if matches!(key.as_str(), "skill" | "reactiontime" | "difficulty") {
                        if fields.insert(key, value.trim().to_string()).is_some() { return result; }
                    }
                }
            } else if line.eq_ignore_ascii_case("Default") {
                active = Some(("default".into(), BTreeMap::new()));
            } else if let Some(name) = line.strip_prefix("Template ") {
                active = Some((name.trim().to_ascii_lowercase(), BTreeMap::new()));
            }
        }
        if active.is_some() { return result; }
        let Some(defaults) = blocks.get("default") else { return result };
        let mut seen = std::collections::BTreeSet::new();
        for (_, entry) in difficulties.iter() {
            let Some(entry) = entry.as_block() else { continue };
            let Some(choice) = entry.str("value").and_then(|s| s.parse::<u8>().ok()).filter(|n| *n <= 5) else { continue };
            if !seen.insert(choice) { result.choices.remove(&choice); continue; }
            let Some(cvars) = entry.block("convars") else { continue };
            if choice == 0 {
                if cvars.str("bot_quota") == Some("0") { result.choices.insert(choice, None); }
                continue;
            }
            let (template, class) = match cvars.str("bot_difficulty") {
                Some("0") => ("easy", "EASY"), Some("1") => ("normal", "NORMAL"),
                Some("2") => ("hard", "HARD"), Some("3") => ("expert", "EXPERT"), _ => continue,
            };
            let shooting = match cvars.str("bot_dont_shoot") { Some("0") => true, Some("1") => false, _ => continue };
            let Some(fields) = blocks.get(template) else { continue };
            let field = |name| fields.get(name).or_else(|| defaults.get(name));
            if !field("difficulty").is_some_and(|s| s.eq_ignore_ascii_case(class)) { continue; }
            let number = |name| field(name).and_then(|s| s.parse::<f32>().ok()).filter(|v| v.is_finite());
            let (Some(skill), Some(reaction_time)) = (number("skill"), number("reactiontime")) else { continue };
            let profile = Profile { skill: skill / 100., reaction_time, shooting };
            if sim::bots::Bots::with_profile(profile).is_ok() { result.choices.insert(choice, Some(profile)); }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use glam::Vec3;

    #[test]
    fn content_difficulties_construct_distinct_bots_and_harmless_navigates_without_attacks() {
        let modes = r#"GameModes.txt { botDifficulty {
            none { value 0 convars { bot_quota 0 } }
            harmless { value 1 convars { bot_difficulty 0 bot_dont_shoot 1 } }
            easy { value 2 convars { bot_difficulty 0 bot_dont_shoot 0 } }
            medium { value 3 convars { bot_difficulty 1 bot_dont_shoot 0 } }
            hard { value 4 convars { bot_difficulty 2 bot_dont_shoot 0 } }
            expert { value 5 convars { bot_difficulty 3 bot_dont_shoot 0 } }
        } }"#;
        let personalities = "Default\nSkill = 40\nReactionTime = 0.5\nEnd\n\
            Template Easy\nSkill = 10\nReactionTime = 0.75\nDifficulty = EASY\nEnd\n\
            Template Normal\nDifficulty = NORMAL\nEnd\n\
            Template Hard\nSkill = 75\nReactionTime = 0.25\nDifficulty = HARD\nEnd\n\
            Template Expert\nSkill = 95\nReactionTime = 0.1\nDifficulty = EXPERT\nEnd\n";
        let mut fs = vfs::Vfs::new();
        fs.push_back("owned-pack", vfs::Mount::Memory(BTreeMap::from([
            ("gamemodes.txt".into(), Arc::<[u8]>::from(modes.as_bytes())),
            ("botprofile.db".into(), Arc::<[u8]>::from(personalities.as_bytes())),
        ])));
        let profiles = Profiles::load(&fs);
        assert_eq!(profiles.resolve(0).unwrap(), None);
        let selected: Vec<_> = (2..=5).map(|value|
            sim::bots::Bots::with_profile(profiles.resolve(value).unwrap().unwrap()).unwrap()).collect();
        assert_eq!(selected.iter().map(|bot| bot.skill).collect::<Vec<_>>(), vec![0.1, 0.4, 0.75, 0.95]);
        assert_eq!(selected[1].reaction_time, Some(0.5), "Default fields are inherited");
        let malformed = Profiles::from_text(modes, &personalities.replace("Skill = 10", "Skill = NaN"));
        assert!(malformed.resolve(2).is_err(), "never label a fallback as an admitted tier");
        assert_eq!(malformed.resolve(0).unwrap(), None);
        assert_eq!(Profiles::from_text(modes, "").resolve(0).unwrap(), None);
        let mut floor = collision::CollisionWorld::new();
        let a = Vec3::new(-4096., -4096., 0.);
        let b = Vec3::new(4096., -4096., 0.);
        let c = Vec3::new(4096., 4096., 0.);
        let d = Vec3::new(-4096., 4096., 0.);
        floor.add_triangle([a,b,c], collision::contents::SOLID, 0);
        floor.add_triangle([a,c,d], collision::contents::SOLID, 0);
        floor.finalize();
        let catalog = weapons::Catalog::from_items_game(r#"items_game { items {
            7017 { name weapon_ak47 baseitem 1 attributes { damage 1 "is full auto" 1
                "primary clip size" 30 "primary reserve ammo max" 90 cycletime 0.1 } }
        } }"#).unwrap();
        let mut harmless = sim::World::with_catalog(Arc::new(floor), Arc::new(catalog));
        let bot = harmless.add_bot("Harmless", sim::Team::Terrorist);
        let target = harmless.add_player("Target", sim::Team::CounterTerrorist);
        harmless.player_mut(target).unwrap().movement.origin = Vec3::new(400., 0., 0.);
        harmless.bots = sim::bots::Bots::with_profile(profiles.resolve(1).unwrap().unwrap()).unwrap();
        let start = harmless.player(bot).unwrap().movement.origin;
        let mut shooting = harmless.clone();
        shooting.bots = sim::bots::Bots::with_profile(profiles.resolve(2).unwrap().unwrap()).unwrap();
        let mut control_fired = false;
        for _ in 0..256 {
            harmless.step(&[(target, sim::TickInput::default())]);
            shooting.step(&[(target, sim::TickInput::default())]);
            assert_eq!(harmless.player(bot).unwrap().last_input.buttons
                & (movement::buttons::ATTACK | movement::buttons::ATTACK2), 0);
            assert_eq!(harmless.player(bot).unwrap().shots, 0);
            control_fired |= shooting.events.iter().any(|event|
                matches!(event, sim::GameEvent::Fire { player, .. } if *player == bot));
        }
        assert!(harmless.player(bot).unwrap().movement.origin.truncate().distance(start.truncate()) > 1.,
            "Harmless is not the frozen aggressive=false target-practice mode");
        assert!(control_fired, "the same authored Easy profile with shooting enabled actually fires through authority");
    }
}
