//! Built-in answers for engine calls the game's [`Engine`](crate::Engine)
//! leaves alone. They describe a player who owns the game, is not banned,
//! has the current client and no pending alerts, so the movies show their
//! normal screens instead of warnings.
//!
//! The movies test exact types: `undefined` is neither `0` nor `false` in
//! ActionScript, and `_visible = undefined` is ignored, so a missing answer
//! shows ban warnings, the licence upsell and the update bar.

use crate::Value;

/// Engine components the movies use as `_global.CScaleformComponent_<Name>`.
pub(crate) const COMPONENTS: &[&str] = &[
    "Blog", "CompetitiveMatch", "EmbeddedStream", "Fantasy", "FriendsList", "GameTypes",
    "IME", "ImageCache", "Inventory", "ItemData", "Leaderboards", "Loadout", "MatchDraft",
    "MatchInfo", "MatchList", "MatchStats", "Medals", "Missions", "MyPersona", "News",
    "Overwatch", "PartyBrowser", "PartyList", "Predictions", "SteamOverlay", "Store",
    "Streams", "Teammates", "UGC",
];

pub(crate) fn call(object: &str, method: &str, args: &[Value]) -> Option<Value> {
    let object = object.strip_prefix("CScaleformComponent_").unwrap_or(object);
    let arg0 = args.first().map(Value::str).unwrap_or_default();
    let n = |v: f64| Some(Value::Number(v));
    let b = |v: bool| Some(Value::Bool(v));
    let s = |v: &str| Some(Value::String(v.to_string()));
    match (object, method) {
        ("GameInterface", "GetConvarNumber") => match arg0.as_str() {
            "safezonex" | "safezoney" => n(1.0),
            "hud_scaling" => n(crate::HUD_SCALING),
            _ => n(0.0),
        },
        ("GameInterface", "GetConvarBoolean") => b(arg0 == "play_with_friends_enabled"),
        ("GameInterface", "GetConvarString") => s(""),

        // Licence, bans, client version, alerts.
        ("MyPersona", "GetLicenseType") => s("purchased"),
        ("MyPersona", "IsVacBanned") => n(0.0),
        ("MyPersona", "GetLauncherType") => s(""),
        ("MyPersona", "GetMyNotifications") => s(""),
        ("MyPersona", "IsConnectedToGC") => b(true),
        ("MyPersona", "IsInventoryValid") => b(true),
        ("MyPersona", "HasPrestige") => b(false),
        ("MyPersona", "GetMyOfficialTournamentName") | ("MyPersona", "GetMyOfficialTeamName") => s(""),
        ("MyPersona", "GetMyMedalRankByType") => n(0.0),
        ("MyPersona", "GetMyMedalAdditionalInfo") => s(""),
        ("CompetitiveMatch", "HasOngoingMatch") => b(false),
        ("CompetitiveMatch", "GetCooldownSecondsRemaining") => n(0.0),
        ("News", "IsNewClientAvailable") => b(false),
        ("News", "GetCurrentActiveSurveyID") | ("News", "GetMySurveyVote") => n(0.0),
        ("News", "GetActiveTournamentEventID") => n(0.0),
        ("Inventory", "GetUnacknowledgeItemsCount") => n(0.0),
        ("Predictions", "GetGroupRemainingPredictionSeconds") => n(0.0),
        // No operation running (the misspelling is the engine's).
        ("GameTypes", "GetActiveSeasionIndexValue") | ("GameTypes", "GetActiveSeasonIndexValue") => n(-1.0),
        ("EmbeddedStream", "IsVideoPlaying") => b(false),
        ("SteamOverlay", "GetAppID") => n(730.0),
        ("SteamOverlay", "GetSteamCommunityURL") => s("https://steamcommunity.com"),

        // No Overwatch case, nothing in the store until the game says so.
        ("Overwatch", "GetAssignedCaseDescription") | ("Overwatch", "GetEvidencePreparationError") => s(""),
        ("Overwatch", "GetEvidencePreparationPercentage") => n(0.0),
        ("Store", "BStoreDisabledInBetaMode") => b(false),
        ("Store", "GetBannerEntryCount") => n(0.0),
        ("Inventory", "GetCacheTypeElementsCount") => n(0.0),
        ("Inventory", "GetSortMethodsCount") => n(0.0),
        ("Inventory", "GetInventoryCount") => n(0.0),
        // Enumeration APIs require typed count/string answers. An undefined
        // count made inventorypanel spin until the VM timeout.
        ("Inventory", "GetSortMethodByIndex") => s(""),
        ("Inventory", "GetInventoryItemIDByIndex") => s("0"),

        // Friends: signed in, nobody on the list.
        ("SteamOverlay", "BSignedInToFriends") => b(true),
        ("FriendsList", "GetCount") => n(0.0),
        ("FriendsList", "GetFriendRequestsNotificationNumber") | ("FriendsList", "GetFriendRequestsCount") => n(0.0),
        ("FriendsList", "GetFriendCommendations") | ("FriendsList", "GetFriendCompetitiveWins") => n(0.0),
        ("FriendsList", "GetFriendCompetitiveRank") | ("FriendsList", "GetFriendMedalRankByType") => n(0.0),
        ("FriendsList", "GetFriendDisplayItemDefCount") => n(0.0),
        ("FriendsList", "GetFriendDisplayItemDefFeatured") => s("0"),
        ("FriendsList", "GetFriendCompetitiveRankType") => s("Competitive"),
        ("PartyBrowser", "GetInvitesCount") | ("PartyBrowser", "GetResultsCount") => n(0.0),
        ("PartyBrowser", "GetProgress") => n(100.0),
        ("PartyList", "GetFriendPrimeEligible") => b(false),
        ("PartyList", "IsPartySessionActive") => b(false),
        ("Teammates", "GetCount") | ("Teammates", "GetSecondsAgoFinished") => n(0.0),
        ("Medals", "GetAchievementMedalTypesCount") => n(0.0),
        ("Inventory", "GetMaxLevel") => n(40.0),
        ("Inventory", "GetFlairItemId") => s("0"),
        ("Inventory", "GetFlairItemName") => s(""),
        _ => None,
    }
}
