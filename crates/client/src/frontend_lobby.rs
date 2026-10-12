//! Owned host selection through installed LobbyScreen/StartSinglePlayer movies.
//! Epoch proxy labels are our bookkeeping, not original game API contracts.
use crate::menu::MapRequest;
use scaleform::{Calls, Value};
use std::collections::HashMap;

pub(super) const ADDRESS_NOTICE: &str =
    "Start the server, then open Pause and choose Invite friends.";

/// Map requests originate in a trusted catalog. Native text only indexes it;
/// it never constructs a filesystem/archive path or a pack identity.
pub(super) struct Catalog {
    requests: HashMap<String, MapRequest>,
    preferred: String,
    deathmatch: bool,
}
impl Catalog {
    pub fn installed(options: &crate::ClientOptions, maps: &[String]) -> Self {
        let requests: HashMap<String, MapRequest> = maps
            .iter()
            .map(|name| {
                let mut selected = options.clone();
                selected.map = name.clone();
                (name.clone(), selected.into())
            })
            .collect();
        Self::trusted_requests(options, requests)
    }
    /// The caller supplies catalog-owned keys and immutable requests, including
    /// pack identity when available. Native callback strings never call this.
    pub fn trusted_requests(
        options: &crate::ClientOptions,
        requests: impl IntoIterator<Item = (String, MapRequest)>,
    ) -> Self {
        Self {
            requests: requests.into_iter().collect(),
            preferred: options.map.clone(),
            deathmatch: options.deathmatch,
        }
    }
    // Future pack grouping must supply a trusted typed request and authored
    // group metadata. It must never turn a native label into an archive path.
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Phase {
    Closed,
    Browser,
    Lobby,
    Picker,
    Starting,
    Queued,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Api {
    Lobby,
    Picker,
}
#[derive(Clone)]
struct Display {
    kind: String,
    mode: String,
    group: String,
}
struct Checked {
    text: String,
    map: MapRequest,
    display: Display,
    kind_id: String,
    mode_id: String,
}
pub(super) struct Start {
    pub epoch: u64,
    pub map: MapRequest,
}
pub(super) struct Scene {
    epoch: u64,
    phase: Phase,
    catalog: Catalog,
    display: Option<Display>,
    difficulty: Option<u8>,
    checked: Option<Checked>,
    pending: Option<Checked>,
    staged: Option<MapRequest>,
}
impl Scene {
    pub fn new(catalog: Catalog) -> Self {
        Self {
            epoch: 0,
            phase: Phase::Closed,
            catalog,
            display: None,
            difficulty: None,
            checked: None,
            pending: None,
            staged: None,
        }
    }
    pub fn active(&self) -> bool {
        self.phase != Phase::Closed
    }
    pub fn picker(&self) -> bool {
        self.phase == Phase::Picker
    }
    pub fn staged(&self) -> bool {
        self.phase == Phase::Lobby && self.staged.is_some()
    }
    pub fn staged_map(&self) -> Option<String> {
        (self.phase == Phase::Lobby)
            .then(|| self.staged.as_ref().map(|map| map.options.map.clone()))
            .flatten()
    }
    pub fn phase(&self) -> Phase {
        self.phase
    }
    fn advance(&mut self) -> bool {
        self.checked = None;
        self.pending = None;
        self.staged = None;
        self.difficulty = None;
        if let Some(next) = self.epoch.checked_add(1) {
            self.epoch = next;
            true
        } else {
            self.phase = Phase::Closed;
            false
        }
    }
    pub fn cancel(&mut self) {
        self.advance();
        self.phase = Phase::Closed;
        self.display = None;
    }
    pub fn begin(&mut self, root: &kv::Block) -> bool {
        if !self.advance() {
            return false;
        }
        self.phase = Phase::Browser;
        let (kind, mode) = if self.catalog.deathmatch {
            ("gungame", "deathmatch")
        } else {
            ("classic", "competitive")
        };
        self.display =
            root.path(&["gameTypes", kind, "gameModes", mode, "mapgroupsSP"])
                .and_then(kv::Value::as_block)
                .and_then(|groups| {
                    groups.iter().find_map(|(group, _)| {
                        let maps = root.path(&["mapgroups", group, "maps"])?.as_block()?;
                        (maps.entries.len() == 1 && maps.entries[0].0 == self.catalog.preferred)
                            .then(|| Display {
                                kind: kind.into(),
                                mode: mode.into(),
                                group: group.into(),
                            })
                    })
                });
        true
    }
    pub fn open_lobby(&mut self) -> bool {
        if self.phase != Phase::Browser {
            return false;
        }
        self.phase = Phase::Lobby;
        true
    }
    pub fn choose(&mut self, epoch: u64) -> bool {
        if self.phase != Phase::Lobby || epoch != self.epoch {
            return false;
        }
        if !self.advance() {
            return false;
        }
        self.phase = Phase::Picker;
        true
    }
    pub fn proxy(&self, api: Api) -> String {
        format!(
            "{}{}",
            match api {
                Api::Lobby => "OwnedLobby",
                Api::Picker => "OwnedPicker",
            },
            self.epoch
        )
    }
    pub fn receiver(&self, object: &str) -> Option<(Api, u64)> {
        for api in [Api::Lobby, Api::Picker] {
            if self.active() && object == self.proxy(api) {
                return Some((api, self.epoch));
            }
        }
        None
    }
    pub fn owned_receiver(object: &str) -> bool {
        object.starts_with("OwnedLobby") || object.starts_with("OwnedPicker")
    }
    pub fn difficulty(&mut self, epoch: u64, args: &[Value]) {
        if self.phase != Phase::Picker || epoch != self.epoch {
            return;
        }
        self.checked = None;
        self.pending = None;
        self.difficulty = match args {
            [Value::Number(0.0)] => Some(0),
            [Value::Number(3.0)] => Some(3),
            _ => None,
        };
    }
    pub fn check(
        &mut self,
        epoch: u64,
        root: &kv::Block,
        text: Option<&str>,
    ) -> Result<(), String> {
        if self.phase != Phase::Picker || epoch != self.epoch {
            return Err("Game settings are no longer active.".into());
        }
        self.checked = None;
        self.pending = None;
        let no_bots = match self.difficulty {
            Some(0) => true,
            Some(3) => false,
            _ => {
                return Err(
                    "Choose No Bots or Medium Bots; other difficulties are not available yet."
                        .into(),
                );
            }
        };
        let text = text.ok_or("Choose game settings again.")?;
        let selected = crate::frontend_game::offline_selection(root, text)?;
        let doc = kv::parse(text).map_err(|_| "Game settings could not be read.")?;
        let game = doc.block("Game").ok_or("Choose game settings again.")?;
        if doc
            .block("Options")
            .and_then(|options| options.str("anytypemode"))
            != Some("0")
        {
            return Err("Choose one supported game mode.".into());
        }
        let kind = game.str("type").unwrap();
        let mode = game.str("mode").unwrap();
        let group = game.str("mapgroupname").unwrap();
        let kind_data = root
            .path(&["gameTypes", kind])
            .and_then(kv::Value::as_block)
            .ok_or("Choose a supported mode.")?;
        let mode_data = kind_data
            .path(&["gameModes", mode])
            .and_then(kv::Value::as_block)
            .ok_or("Choose a supported mode.")?;
        let mut map = self
            .catalog
            .requests
            .get(&selected.map)
            .cloned()
            .ok_or("Choose a map from the local catalog.")?;
        map.options.deathmatch = selected.deathmatch;
        if no_bots {
            map.options.bots = 0;
        } else if map.options.bots == 0 {
            return Err(
                "Medium Bots are unavailable with the current bot configuration. Choose No Bots."
                    .into(),
            );
        }
        self.checked = Some(Checked {
            text: text.into(),
            map,
            display: Display {
                kind: kind.into(),
                mode: mode.into(),
                group: group.into(),
            },
            kind_id: kind_data
                .str("value")
                .ok_or("Game type metadata is missing.")?
                .into(),
            mode_id: mode_data
                .str("value")
                .ok_or("Game mode metadata is missing.")?
                .into(),
        });
        Ok(())
    }
    pub fn query(&mut self, epoch: u64, text: Option<&str>) {
        if self.phase != Phase::Picker || epoch != self.epoch {
            return;
        }
        self.pending = self
            .checked
            .take()
            .filter(|checked| text == Some(checked.text.as_str()));
    }
    pub fn updated(&mut self, epoch: u64, args: &[Value]) -> bool {
        if self.phase != Phase::Picker || epoch != self.epoch {
            return false;
        }
        let Some(pending) = self.pending.take() else {
            return false;
        };
        // Third argument was NaN in the installed movie, not an established
        // skirmish default. The bounded checked query already rejects skirmish.
        let matches = matches!(args, [Value::String(kind), Value::String(mode), _, Value::String(group), Value::Number(0.0)]
            if kind == &pending.kind_id && mode == &pending.mode_id && group == &pending.display.group);
        if !matches {
            return false;
        }
        self.display = Some(pending.display);
        self.staged = Some(pending.map);
        self.phase = Phase::Lobby;
        true
    }
    pub fn chooser_unloaded(&mut self) {
        // Expected unload follows UpdatedSelections and retains its stage.
        if self.phase == Phase::Picker {
            self.advance();
            self.phase = Phase::Lobby;
        }
    }
    pub fn start(&mut self, epoch: u64, args: &[Value]) -> Option<Start> {
        if epoch != self.epoch || self.phase != Phase::Lobby || !args.is_empty() {
            return None;
        }
        let map = self.staged.take()?;
        self.phase = Phase::Starting;
        Some(Start { epoch, map })
    }
    pub fn reject_start(&mut self, epoch: u64) {
        if epoch == self.epoch && self.phase == Phase::Starting {
            self.phase = Phase::Lobby;
        }
    }
    pub fn consume(&mut self, epoch: u64) -> bool {
        if epoch != self.epoch || self.phase != Phase::Starting {
            return false;
        }
        self.phase = Phase::Queued;
        true
    }
    pub fn party_setting(&self, key: &str) -> Option<Value> {
        if !self.active() {
            return None;
        }
        let display = self.display.as_ref();
        match key {
            "system/access" => Some("public".into()), // current open-address capability, not a Valve default
            "game/type" => display.map(|value| value.kind.clone().into()),
            "game/mode" => display.map(|value| value.mode.clone().into()),
            "game/mapgroupname" => display.map(|value| value.group.clone().into()),
            _ => None,
        }
    }
    pub fn show_browser(&self, path: &str, name: &str, calls: &mut Calls) {
        calls.call(
            "_global.csrBindings",
            "bindOwnedLifecycle",
            vec![path.into(), self.proxy(Api::Lobby).into()],
        );
        calls.set(path, "_visible", true.into());
        calls.call(&format!("{path}.FriendsPanel"), "Init", vec![]);
        calls.call(
            path,
            "InitLobby",
            vec![name.into(), "0".into(), false.into()],
        );
        calls.call(path, "UpdateFriendCount", vec![0.0.into()]);
        // There is no authenticated friends directory: clear template rows,
        // and withhold the online-count claim rather than present fake metadata.
        calls.set(
            &format!("{path}.FriendsPanel.LobbyCount"),
            "_visible",
            false.into(),
        );
        calls.call(path, "showPanel", vec![]);
        for button in ["JoinButton", "GamercardButton"] {
            calls.call(
                &format!("{path}.FriendsPanel.{button}"),
                "setDisabled",
                vec![true.into()],
            );
        }
    }
    pub fn show(&self, path: &str, name: &str, calls: &mut Calls) {
        calls.call(
            "_global.csrBindings",
            "bindOwnedLifecycle",
            vec![path.into(), self.proxy(Api::Lobby).into()],
        );
        calls.call(
            "_global.csrBindings",
            "bindOwnedApi",
            vec!["LobbyAPI".into(), self.proxy(Api::Lobby).into()],
        );
        calls.set(path, "_visible", true.into());
        let panel = format!("{path}.LobbyPanel.Panels.LobbyMainPanel");
        calls.call(&panel, "Init", vec![]);
        calls.call(
            path,
            "InitLobby",
            vec![name.into(), "0".into(), true.into()],
        );
        calls.call(path, "UpdateLobbyHost", vec![name.into(), "0".into()]);
        calls.call(
            path,
            "UpdateLobbyPlayer",
            vec![
                0.0.into(),
                "0".into(),
                name.into(),
                false.into(),
                0.0.into(),
                0.0.into(),
                0.0.into(),
                "".into(),
                0.0.into(),
            ],
        );
        if let Some(display) = &self.display {
            calls.set(
                &format!("{panel}.GameSettingsText"),
                "_visible",
                true.into(),
            );
            calls.call(
                path,
                "UpdateSessionSettingsInScaleform",
                vec![
                    display.kind.clone().into(),
                    display.mode.clone().into(),
                    display.group.clone().into(),
                    "".into(),
                    "public".into(),
                    0.0.into(),
                ],
            );
        } else {
            calls.set(
                &format!("{panel}.GameSettingsText"),
                "_visible",
                false.into(),
            );
        }
        calls.call(path, "showPanel", vec![]);
        calls.call(path, "ShowLobby", vec![]);
        calls.call(path, "ForcePanel", vec![1.0.into()]);
        calls.call(path, "SetNavOptions", vec![self.staged.is_some().into()]);
        for button in ["InviteFriends", "ButtonPermissions"] {
            calls.call(
                &format!("{panel}.{button}"),
                "setDisabled",
                vec![true.into()],
            );
        }
        // These authored service/privacy captions do not describe an open-address server.
        for caption in [
            "PermissionsText",
            "PrimeText",
            "PrimeStatus",
            "PrimeTooltip",
        ] {
            calls.set(&format!("{panel}.{caption}"), "_visible", false.into());
        }
    }
    pub fn show_picker(&self, path: &str, calls: &mut Calls) {
        calls.call(
            "_global.csrBindings",
            "bindOwnedLifecycle",
            vec![path.into(), self.proxy(Api::Picker).into()],
        );
        calls.call(
            "_global.csrBindings",
            "bindOwnedApi",
            vec!["SinglePlayerAPI".into(), self.proxy(Api::Picker).into()],
        );
        calls.set(path, "_visible", true.into());
        calls.call(
            path,
            "InitDialogData",
            vec![false.into(), true.into(), false.into()],
        );
        calls.call(path, "showPanel", vec![]);
        let modes = format!("{path}.Panel.Panel.ModeChooser.TabDefault");
        let selected = if self
            .display
            .as_ref()
            .is_some_and(|display| display.mode == "deathmatch")
        {
            "ModeDeathmatch"
        } else {
            "ModeCompetitive"
        };
        calls.call(&format!("{modes}.{selected}"), "Action", vec![]);
        for mode in ["ModeCasual", "ModeWingman", "ModeWargames"] {
            calls.call(&format!("{modes}.{mode}"), "setDisabled", vec![true.into()]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const QUERY: &str = "System { network offline } Game { type classic mode competitive mapgroupname mg_de_dust2 skirmishmode undefined } Options { action create anytypemode 0 } Contexts {} Properties {}";
    fn modes() -> kv::Block {
        kv::parse("gameTypes { classic { value 0 gameModes { competitive { value 1 mapgroupsSP { mg_de_dust2 0 } } } } } mapgroups { mg_de_dust2 { maps { de_dust2 0 } } } maps { de_dust2 {} }").unwrap()
    }
    fn scene() -> Scene {
        let mut scene = Scene::new(Catalog::installed(
            &crate::ClientOptions::default(),
            &["de_dust2".into()],
        ));
        assert!(scene.begin(&modes()));
        assert!(scene.open_lobby());
        scene
    }
    fn picker(scene: &mut Scene) -> u64 {
        assert!(scene.choose(scene.epoch));
        scene.epoch
    }
    fn values(group: &str) -> Vec<Value> {
        vec![
            "0".into(),
            "1".into(),
            Value::Number(f64::NAN),
            group.into(),
            0.0.into(),
        ]
    }
    fn checked(scene: &mut Scene, epoch: u64) {
        scene.difficulty(epoch, &[0.0.into()]);
        scene.check(epoch, &modes(), Some(QUERY)).unwrap();
        scene.query(epoch, Some(QUERY));
    }
    #[test]
    fn chooser_completion_stages_but_only_empty_start_consumes_exactly_once() {
        let mut scene = scene();
        let epoch = picker(&mut scene);
        checked(&mut scene, epoch);
        assert!(scene.start(epoch, &[]).is_none());
        assert!(scene.updated(epoch, &values("mg_de_dust2")));
        scene.chooser_unloaded();
        assert!(scene.start(epoch, &[true.into()]).is_none());
        let start = scene.start(epoch, &[]).unwrap();
        assert_eq!(start.map.options.map, "de_dust2");
        assert_eq!(start.map.options.bots, 0);
        assert!(start.map.pack.is_none());
        assert!(scene.start(epoch, &[]).is_none());
        assert!(scene.consume(start.epoch));
        assert!(!scene.consume(start.epoch));
    }
    #[test]
    fn cancel_reopen_rejects_stale_zeroarg_start_and_pending_completion() {
        let mut scene = scene();
        let old = picker(&mut scene);
        checked(&mut scene, old);
        let old_receiver = scene.proxy(Api::Picker);
        scene.cancel();
        assert!(scene.begin(&modes()));
        assert!(scene.open_lobby());
        let current = picker(&mut scene);
        checked(&mut scene, current);
        assert!(scene.receiver(&old_receiver).is_none());
        scene.query(old, Some(QUERY));
        assert!(!scene.updated(old, &values("mg_de_dust2")));
        assert!(scene.start(old, &[]).is_none());
        assert!(scene.updated(current, &values("mg_de_dust2")));
        assert!(scene.start(old, &[]).is_none());
        assert!(scene.start(current, &[]).is_some());
    }
    #[test]
    fn changed_query_wrong_group_and_unmatched_unload_cannot_stage() {
        let mut scene = scene();
        let epoch = picker(&mut scene);
        scene.difficulty(epoch, &[0.0.into()]);
        scene.check(epoch, &modes(), Some(QUERY)).unwrap();
        scene.query(epoch, Some(&format!("{QUERY} ")));
        assert!(!scene.updated(epoch, &values("mg_de_dust2")));
        checked(&mut scene, epoch);
        assert!(!scene.updated(epoch, &values("different")));
        checked(&mut scene, epoch);
        scene.chooser_unloaded();
        assert!(!scene.updated(epoch, &values("mg_de_dust2")));
        assert!(scene.start(scene.epoch, &[]).is_none());
    }
    #[test]
    fn opaque_paths_unsupported_difficulty_and_uncataloged_maps_are_rejected() {
        let mut scene = scene();
        let epoch = picker(&mut scene);
        assert!(scene.check(epoch, &modes(), Some(QUERY)).is_err());
        scene.difficulty(epoch, &[5.0.into()]);
        assert!(scene.check(epoch, &modes(), Some(QUERY)).is_err());
        scene.difficulty(epoch, &[0.0.into()]);
        assert!(
            scene
                .check(
                    epoch,
                    &modes(),
                    Some(&QUERY.replace("mg_de_dust2", "../private"))
                )
                .is_err()
        );
        scene.catalog.requests.clear();
        assert!(scene.check(epoch, &modes(), Some(QUERY)).is_err());
        assert!(scene.pending.is_none());
        assert!(scene.staged.is_none());
    }
    #[test]
    fn home_invalidates_already_queued_start_at_main_world_consumption() {
        let mut scene = scene();
        let epoch = picker(&mut scene);
        checked(&mut scene, epoch);
        assert!(scene.updated(epoch, &values("mg_de_dust2")));
        let start = scene.start(epoch, &[]).unwrap();
        scene.cancel();
        assert!(!scene.consume(start.epoch));
    }
    #[test]
    fn third_completion_argument_is_not_claimed_as_a_skirmish_default() {
        let mut scene = scene();
        let epoch = picker(&mut scene);
        checked(&mut scene, epoch);
        let mut args = values("mg_de_dust2");
        args[2] = Value::Undefined;
        assert!(scene.updated(epoch, &args));
        // A real custom skirmish in the checked query remains unsupported.
        let epoch = picker(&mut scene);
        scene.difficulty(epoch, &[0.0.into()]);
        assert!(
            scene
                .check(
                    epoch,
                    &modes(),
                    Some(&QUERY.replace("skirmishmode undefined", "skirmishmode 2"))
                )
                .is_err()
        );
    }
    #[test]
    fn native_map_name_does_not_reconstruct_the_trusted_catalog_request() {
        let mut scene = scene();
        let mut owned = crate::ClientOptions::default();
        owned.map = "catalog-owned-canonical-id".into();
        scene.catalog = Catalog::trusted_requests(
            &crate::ClientOptions::default(),
            [("de_dust2".into(), owned.into())],
        );
        let epoch = picker(&mut scene);
        checked(&mut scene, epoch);
        assert!(scene.updated(epoch, &values("mg_de_dust2")));
        let start = scene.start(epoch, &[]).unwrap();
        assert_eq!(start.map.options.map, "catalog-owned-canonical-id");
    }
    #[test]
    fn medium_requires_a_trusted_bot_count_and_staged_map_is_read_only() {
        let mut scene = scene();
        let epoch = picker(&mut scene);
        scene
            .catalog
            .requests
            .get_mut("de_dust2")
            .unwrap()
            .options
            .bots = 0;
        scene.difficulty(epoch, &[3.0.into()]);
        assert!(
            scene
                .check(epoch, &modes(), Some(QUERY))
                .unwrap_err()
                .contains("Choose No Bots")
        );
        assert!(scene.staged_map().is_none());
        scene.query(epoch, Some(QUERY));
        assert!(!scene.updated(epoch, &values("mg_de_dust2")));
        scene
            .catalog
            .requests
            .get_mut("de_dust2")
            .unwrap()
            .options
            .bots = 2;
        scene.difficulty(epoch, &[3.0.into()]);
        scene.check(epoch, &modes(), Some(QUERY)).unwrap();
        scene.query(epoch, Some(QUERY));
        assert!(scene.updated(epoch, &values("mg_de_dust2")));
        assert_eq!(scene.staged_map().as_deref(), Some("de_dust2"));
        assert_eq!(scene.staged.as_ref().unwrap().options.bots, 2);
        assert_eq!(scene.staged_map().as_deref(), Some("de_dust2"));
        let start = scene.start(epoch, &[]).unwrap();
        assert_eq!(start.map.options.bots, 2);
        assert!(scene.staged_map().is_none());
    }
    #[test]
    fn exhausted_epoch_cannot_reuse_an_old_receiver() {
        let mut scene = scene();
        scene.epoch = u64::MAX;
        assert!(!scene.begin(&modes()));
        assert!(!scene.active());
        assert!(scene.receiver("OwnedLobby18446744073709551615").is_none());
    }
}
