//! The engine side of the Scaleform API: every `GameInterface.*`,
//! `CScaleformComponent_*.*` and `gameAPI.*` call the movies make lands here.
//! The spike answers with plausible local data shaped like the reference
//! screenshot; unknown calls return `undefined`, as calling a missing native
//! would.

use std::collections::BTreeMap;
use std::rc::Rc;

use ruffle_core::external::Value;

use crate::rewrite::Localization;

pub struct Bridge {
    pub loc: Rc<Localization>,
    pub root_ready: bool,
    pub data: Data,
    /// Call counts by name, for the report.
    pub calls: BTreeMap<String, usize>,
    pub handled: BTreeMap<String, usize>,
    pub trace_calls: bool,
}

/// What the engine would know: the local profile, friends, store and so on.
pub struct Data {
    pub xuid: String,
    pub name: String,
    pub level: u32,
    pub xp: u32,
    pub xp_per_level: u32,
    pub xp_bonuses: String,
    pub skill_group: u32,
    pub wins: u32,
    pub commends: [u32; 3],
    pub flair: Vec<u32>,
    pub friends: Vec<Friend>,
}

pub struct Friend {
    pub xuid: String,
    pub name: String,
    /// 0 offline, 1 online, 2 busy, 3 away, 4 snooze (Steam persona states).
    pub state: u32,
    pub presence: String,
    pub in_game: bool,
}

impl Data {
    /// Values that reproduce docs/reference/menu-2018-home.png.
    pub fn reference() -> Self {
        let friend = |n: usize, name: &str, state: u32| Friend {
            xuid: format!("7656119800000{:04}", 100 + n),
            name: name.to_string(),
            state,
            presence: String::new(),
            in_game: false,
        };
        let mut friends = vec![Friend {
            xuid: "76561198000000100".into(),
            name: "Savage212".into(),
            state: 1,
            presence: "Competitive - CS:GO Community".into(),
            in_game: true,
        }];
        for (i, (name, state)) in [
            ("Press F to Pay Respects", 1),
            ("[2BAD] Eiswürfel", 1),
            ("AdaM", 3),
            ("an accident / ohio", 3),
            ("Anjocaido792", 3),
            ("Anonim", 3),
            ("AresQQ ][ imba trade", 3),
            ("artwork", 4),
            ("Azzidahaka", 1),
            ("Banksii", 3),
            ("Chewy", 3),
            ("Clover", 4),
            ("Dank Tovmassian Edition", 1),
            ("Deception", 4),
            ("Deceptionz", 1),
            ("DM3 ArrowHead47 [tr]", 4),
            ("drake", 3),
            ("FX | Sean", 4),
            ("Goodwin ッ", 3),
        ]
        .into_iter()
        .enumerate()
        {
            friends.push(friend(i + 1, name, state));
        }
        // The panel shows 20 of the 50 friends; the rest only count.
        while friends.len() < 50 {
            let i = friends.len();
            friends.push(friend(i, &format!("zz friend {i}"), 0));
        }
        Self {
            xuid: "76561198000000001".into(),
            name: "c0ke RIP PSU".into(),
            level: 3,
            xp: 3500,
            xp_per_level: 5000,
            xp_bonuses: "2".into(),
            skill_group: 9,
            wins: 439,
            commends: [13, 12, 12],
            // Service medals and operation coins (item definition indices).
            flair: vec![1357, 874, 1338, 4355, 1030, 1367, 1331],
            friends,
        }
    }
}

fn s(v: &str) -> Value {
    Value::String(v.to_string())
}

fn n(v: impl Into<f64>) -> Value {
    Value::Number(v.into())
}

fn arg_str(args: &[Value], i: usize) -> String {
    match args.get(i) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}

fn arg_num(args: &[Value], i: usize) -> f64 {
    match args.get(i) {
        Some(Value::Number(n)) => *n,
        Some(Value::String(s)) => s.parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

impl Bridge {
    pub fn new(loc: Rc<Localization>) -> Self {
        Self {
            loc,
            root_ready: false,
            data: Data::reference(),
            calls: BTreeMap::new(),
            handled: BTreeMap::new(),
            trace_calls: std::env::var_os("SPIKE_TRACE_CALLS").is_some(),
        }
    }

    pub fn call(&mut self, name: &str, args: &[Value]) -> Value {
        *self.calls.entry(name.to_string()).or_default() += 1;
        let result = self.dispatch(name, args);
        if let Some(r) = &result {
            *self.handled.entry(name.to_string()).or_default() += 1;
            if self.trace_calls {
                tracing::info!("{name}{args:?} -> {r:?}");
            }
        } else if self.trace_calls {
            tracing::info!("{name}{args:?} -> (unhandled)");
        }
        result.unwrap_or(Value::Undefined)
    }

    fn translate(&self, token: &str) -> String {
        self.loc.translate_whole(token).unwrap_or(token).to_string()
    }

    fn friend(&self, xuid: &str) -> Option<&Friend> {
        self.data.friends.iter().find(|f| f.xuid == xuid)
    }

    fn dispatch(&mut self, name: &str, args: &[Value]) -> Option<Value> {
        let (object, method) = name.split_once('.').unwrap_or(("", name));
        let object = object.strip_prefix("CScaleformComponent_").unwrap_or(object);
        let d = &self.data;
        let me = |args: &[Value]| arg_str(args, 0) == d.xuid;
        Some(match (object, method) {
            ("csr", "RootReady") => {
                self.root_ready = true;
                Value::Undefined
            }
            (_, "Translate") => s(&self.translate(&arg_str(args, 0))),

            // Console variables.
            ("GameInterface", "GetConvarNumber") => match arg_str(args, 0).as_str() {
                "safezonex" | "safezoney" => n(1.0),
                "hud_scaling" => n(0.85),
                "sf_ui_tint" => n(0.0),
                _ => n(0.0),
            },
            ("GameInterface", "GetConvarBoolean") => Value::Bool(arg_str(args, 0) == "play_with_friends_enabled"),
            ("GameInterface", "GetConvarString") => s(""),

            // Local profile.
            ("MyPersona", "GetXuid") => s(&d.xuid),
            ("MyPersona", "IsInventoryValid") => Value::Bool(true),
            ("MyPersona", "IsVacBanned") => n(0),
            ("MyPersona", "GetLauncherType") => s(""),
            ("MyPersona", "HasPrestige") => Value::Bool(false),
            ("MyPersona", "GetXpPerLevel") => n(d.xp_per_level),
            ("MyPersona", "GetActiveXpBonuses") => s(&d.xp_bonuses),
            ("MyPersona", "GetMyOfficialTournamentName") => s(""),
            ("MyPersona", "GetMyOfficialTeamName") => s(""),
            ("MyPersona", "GetMyMedalRankByType") => n(0),
            ("MyPersona", "GetMyMedalAdditionalInfo") => s(""),
            ("Inventory", "GetMaxLevel") => n(40),
            ("Inventory", "GetFlairItemId") => s("0"),
            ("Inventory", "GetFlairItemName") => s(""),
            ("PartyList", "GetFriendPrimeEligible") => Value::Bool(true),
            ("GameTypes", "GetActiveSeasionIndexValue") => n(-1),
            ("Medals", "GetAchievementMedalTypesCount") => n(0),
            ("SteamOverlay", "GetAppID") => n(730),
            ("SteamOverlay", "GetSteamCommunityURL") => s("https://steamcommunity.com"),

            // Profile card fields are asked through the friends list, by xuid.
            ("FriendsList", "GetFriendName") if me(args) => s(&d.name),
            ("FriendsList", "GetFriendName") => s(self.friend(&arg_str(args, 0)).map_or("", |f| &f.name)),
            ("FriendsList", "GetFriendLevel") if me(args) => n(d.level),
            ("FriendsList", "GetFriendXp") if me(args) => n(d.xp),
            ("FriendsList", "GetFriendCompetitiveRank") if me(args) => n(d.skill_group),
            ("FriendsList", "GetFriendCompetitiveWins") if me(args) => n(d.wins),
            ("FriendsList", "GetFriendCompetitiveRankType") => s("Competitive"),
            ("FriendsList", "GetFriendCommendations") if me(args) => {
                let i = match arg_str(args, 1).as_str() {
                    "friendly" => 0,
                    "teaching" => 1,
                    _ => 2,
                };
                n(d.commends[i])
            }
            ("FriendsList", "GetFriendDisplayItemDefCount") if me(args) => n(d.flair.len() as u32),
            ("FriendsList", "GetFriendDisplayItemDefByIndex") if me(args) => {
                s(&d.flair.get(arg_num(args, 1) as usize).map(|v| v.to_string()).unwrap_or_default())
            }
            ("FriendsList", "GetFriendDisplayItemDefFeatured") => s("0"),

            // Friends panel.
            ("SteamOverlay", "BSignedInToFriends") => Value::Bool(true),
            ("MyPersona", "GetLicenseType") => s("purchased"),
            ("FriendsList", "GetCount") => n(d.friends.len() as u32),
            ("FriendsList", "GetXuidByIndex") => {
                s(d.friends.get(arg_num(args, 0) as usize).map_or("", |f| &f.xuid))
            }
            ("FriendsList", "GetFriendStatus") => {
                let f = self.friend(&arg_str(args, 0));
                s(&f.map(|f| match (f.in_game, f.state) {
                    (true, _) => f.presence.clone(),
                    (_, 3) => "Away".into(),
                    (_, 4) => "Snooze".into(),
                    (_, 2) => "Busy".into(),
                    (_, 0) => "Offline".into(),
                    _ => "Online".into(),
                }).unwrap_or_default())
            }
            ("FriendsList", "GetFriendStatusBucket") => {
                let f = self.friend(&arg_str(args, 0));
                s(match f {
                    Some(f) if f.in_game => "PlayingCSGO",
                    Some(f) if f.state > 0 => "Online",
                    _ => "Offline",
                })
            }
            ("FriendsList", "IsFriendJoinable") | ("FriendsList", "IsFriendInvited") => Value::Bool(false),
            ("FriendsList", "IsFriendWatchable") | ("FriendsList", "IsFriendPlayingCSGO") => {
                Value::Bool(self.friend(&arg_str(args, 0)).is_some_and(|f| f.in_game))
            }
            ("FriendsList", "GetFriendRelationship") => s("friend"),
            ("FriendsList", "GetFriendRequestsNotificationNumber") | ("FriendsList", "GetFriendRequestsCount") => n(0),
            ("PartyBrowser", "GetInvitesCount") => n(0),
            ("PartyBrowser", "GetProgress") => n(100),
            ("PartyBrowser", "GetResultsCount") => n(10),
            ("Teammates", "GetCount") => n(23),
            ("Teammates", "GetSecondsAgoFinished") => n(0),
            ("FriendsList", "GetFriendMedalRankByType") => n(0),
            ("ItemData", "GetItemInventoryImage") => s(&flair_image(arg_num(args, 0) as u32)),
            ("ItemData", "GetItemName") => s(""),

            // Top bar, warnings, client update.
            ("News", "GetCurrentActiveSurveyID") | ("News", "GetMySurveyVote") => n(0),
            ("News", "IsNewClientAvailable") => Value::Bool(false),
            ("News", "GetActiveTournamentEventID") => n(0),
            ("Inventory", "GetUnacknowledgeItemsCount") => n(0),
            ("Predictions", "GetGroupRemainingPredictionSeconds") => n(0),
            ("CompetitiveMatch", "HasOngoingMatch") => Value::Bool(false),
            ("CompetitiveMatch", "GetCooldownSecondsRemaining") => n(0),
            ("MyPersona", "GetMyNotifications") => s(""),
            ("MyPersona", "IsConnectedToGC") => Value::Bool(true),
            ("GameTypes", "GetActiveSeasonIndexValue") => n(-1),
            ("EmbeddedStream", "IsVideoPlaying") => Value::Bool(false),

            // Overwatch.
            ("Overwatch", "GetAssignedCaseDescription") => s("OWC#XXXXXXX023"),
            ("Overwatch", "GetEvidencePreparationError") => s(""),
            ("Overwatch", "GetEvidencePreparationPercentage") => n(0),

            // Store carousel.
            ("Store", "BStoreDisabledInBetaMode") => Value::Bool(false),
            ("Store", "GetBannerEntryCount") => n(BANNER.len() as u32),
            ("Store", "GetBannerEntryDefIdx") => n(BANNER.get(arg_num(args, 0) as usize).map_or(0, |b| b.0)),
            ("Store", "GetBannerEntryCustomFormatString") => {
                s(BANNER.get(arg_num(args, 0) as usize).map_or("", |b| b.1))
            }
            ("Store", "IsBannerEntryMarketLink") => {
                n(BANNER.get(arg_num(args, 0) as usize).map_or(0, |b| b.2 as u32))
            }
            ("Inventory", "GetFauxItemIDFromDefAndPaintIndex") => s(&format!("faux{}", arg_num(args, 0) as u32)),
            ("Inventory", "IsCouponCrate") => Value::Bool(false),
            ("Inventory", "GetItemName") => s(&self.item_name(item_def(&arg_str(args, 0)))),
            ("Store", "GetStoreItemOriginalPrice") | ("Store", "GetStoreItemSalePrice") => {
                s(item_info(item_def(&arg_str(args, 0))).map_or("", |i| i.price))
            }
            ("Store", "GetStoreItemPercentReduction") => {
                s(item_info(item_def(&arg_str(args, 0))).map_or("", |i| i.discount))
            }
            ("Store", "IsItemNew") => Value::Bool(false),
            ("Store", "GetStoreItemName_Underscores") => {
                s(item_info(item_def(&arg_str(args, 0))).map_or("", |i| i.key))
            }
            ("Inventory", "GetRawDefinitionKey") => {
                let stattrak = arg_str(args, 1) == "will_produce_stattrak"
                    && item_info(item_def(&arg_str(args, 0))).is_some_and(|i| i.stattrak);
                n(stattrak as u32)
            }
            ("Inventory", "IsTool") => Value::Bool(item_def(&arg_str(args, 0)) == 1200),
            ("Inventory", "GetItemCapabilityByIndex") => {
                s(if item_def(&arg_str(args, 0)) == 1200 { "nameable" } else { "" })
            }
            ("Inventory", "GetItemInventoryImage") => {
                s(item_info(item_def(&arg_str(args, 0))).map_or("", |i| i.image))
            }
            ("Inventory", "GetItemSet") => s(""),
            ("Inventory", "GetCacheTypeElementsCount") if arg_str(args, 0) == "Coupons" => n(COUPONS.len() as u32),
            ("Inventory", "GetCacheTypeElementFieldByIndex") if arg_str(args, 0) == "Coupons" => {
                match arg_str(args, 2).as_str() {
                    "defidx" => n(COUPONS.get(arg_num(args, 1) as usize).map_or(0, |c| c.0)),
                    "expiration_date" => n(1_533_200_000),
                    _ => Value::Undefined,
                }
            }
            // "Offers Available Till: 1 Day 1 Hour 31 Mins 19 Secs".
            ("Store", "GetSecondsUntilTimestamp") => n(91_879),
            ("Inventory", "GetLootListItemIdByIndex") => {
                let kit = COUPONS.iter().find(|c| c.0 == item_def(&arg_str(args, 0))).map_or(0, |c| c.1);
                s(&format!("faux{kit}"))
            }
            _ => return None,
        })
    }
}

/// Store banner entries: (definition index, tile format, market link).
/// The first page is the reference (Horizon Case, Name Tag); four more pages
/// give its five page dots.
const BANNER: &[(u32, &str, bool)] = &[
    (4482, "double", true),
    (1200, "single", false),
    (4482, "double", true),
    (1200, "single", false),
    (4482, "double", true),
    (1200, "single", false),
    (4482, "double", true),
    (1200, "single", false),
    (4482, "double", true),
    (1200, "single", false),
];

/// StatTrak music kit coupons: (coupon definition, music kit definition).
const COUPONS: &[(u32, u32)] = &[(20094, 9001), (20095, 9002), (20096, 9003), (20097, 9004)];

struct ItemInfo {
    name_token: &'static str,
    image: &'static str,
    price: &'static str,
    discount: &'static str,
    key: &'static str,
    stattrak: bool,
}

fn item_def(id: &str) -> u32 {
    id.trim_start_matches("faux").parse().unwrap_or(0)
}

fn item_info(def: u32) -> Option<ItemInfo> {
    let info = |name_token, image, price, discount, key, stattrak| ItemInfo { name_token, image, price, discount, key, stattrak };
    Some(match def {
        4482 => info("#CSGO_crate_community_20", "econ/weapon_cases/crate_community_20", "", "", "crate_community_20", false),
        1200 => info("#CSGO_Tool_Name_Tag", "econ/tools/tag", "£1.45", "", "", false),
        20094..=20097 => info("", "", "", "-35%", "", true),
        9001 => info("", "econ/music_kits/mordfustang_01", "", "", "", true),
        9002 => info("", "econ/music_kits/neckdeep_01", "", "", "", true),
        9003 => info("", "econ/music_kits/skog_01", "", "", "", true),
        9004 => info("", "econ/music_kits/roam_01", "", "", "", true),
        _ => return None,
    })
}

impl Bridge {
    fn item_name(&self, def: u32) -> String {
        item_info(def).map(|i| self.translate(i.name_token)).unwrap_or_default()
    }
}

/// `image_inventory` of the coins used above (from items_game.txt).
fn flair_image(def: u32) -> String {
    // Approximates the reference row; the exact coins are not identifiable.
    let name = match def {
        1357 => "operation_breakout_gold",
        874 => "collectible_pin_hydra",
        1338 => "operation_vanguard_gold",
        4355 => "operation_bravo_silver",
        1030 => "operation_phoenix_gold",
        1367 => "5yearcoin",
        1331 => "service_medal_2018_lvl1",
        _ => return String::new(),
    };
    format!("econ/status_icons/{name}")
}
