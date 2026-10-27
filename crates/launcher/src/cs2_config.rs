//! Read-only first-run control translation. Only config text is opened; no CS2
//! assets are mounted and no aliases, execs, binds or console commands execute.
use client::settings::{self, ACTION_COMMANDS, MouseAction, Settings, WheelAction};
use std::{
    collections::BTreeMap,
    io::Write,
    path::{Path, PathBuf},
};

const KEYS_FILE: &str = "cs2_user_keys_0_slot0.vcfg";
const CONVARS_FILE: &str = "cs2_user_convars_0_slot0.vcfg";
const REPORT_FILE: &str = "config/cs2-import.txt";

struct Source {
    account: String,
    cfg: PathBuf,
    defaults: Option<PathBuf>,
}

/// Setup calls this after migration, so a previous rewrite's controls always
/// win. CSR_STEAM_DIR confines discovery to a fake Steam root in harnesses.
pub fn first_run(data: &Path) -> std::io::Result<()> {
    if data.join("config/user.cfg").try_exists()? {
        return Ok(());
    }
    let roots = if let Some(root) = std::env::var_os("CSR_STEAM_DIR") {
        vec![PathBuf::from(root)]
    } else {
        let mut roots = vfs::steam_libraries();
        if let Some(root) = crate::steam::steam_root() {
            roots.insert(0, root);
        }
        roots
    };
    import_from(data, &roots)
}

fn section(path: &Path, name: &str, report: &mut String) -> Option<kv::Block> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => {
            report.push_str(&format!("Unreadable source {}: {error}\n", path.display()));
            return None;
        }
    };
    report.push_str(&format!("Source: {}\n", path.display()));
    // Include unknown sections too (for example analogbindings), so the local
    // receipt retains source values this version cannot yet translate.
    report.push_str(&format!(
        "Original config text (not executed): {:?}\n",
        kv::decode_text(&bytes)
    ));
    let doc = match kv::parse_bytes(&bytes) {
        Ok(doc) => doc,
        Err(error) => {
            report.push_str(&format!("Malformed source: {error}; ignored\n"));
            return None;
        }
    };
    let block = doc
        .block("config")
        .and_then(|config| config.block(name))
        .cloned();
    if block.is_none() {
        report.push_str(&format!("Missing config/{name}; ignored\n"));
    }
    block
}

fn discover(roots: &[PathBuf]) -> Option<Source> {
    let mut candidates = Vec::new();
    for root in roots {
        // loginusers uses SteamID64 while userdata uses its account-id part.
        let recent = std::fs::read(root.join("config/loginusers.vdf"))
            .ok()
            .and_then(|bytes| kv::parse_bytes(&bytes).ok())
            .and_then(|doc| {
                doc.block("users").and_then(|users| {
                    users.iter().find_map(|(id, value)| {
                        if value.as_block()?.str("MostRecent")? != "1" {
                            return None;
                        }
                        id.parse::<u64>()
                            .ok()?
                            .checked_sub(76561197960265728)
                            .map(|id| id.to_string())
                    })
                })
            });
        let Ok(accounts) = std::fs::read_dir(root.join("userdata")) else {
            continue;
        };
        for account in accounts.flatten() {
            let name = account.file_name().to_string_lossy().into_owned();
            if !name.parse::<u32>().is_ok_and(|id| id != 0) {
                continue;
            }
            let cfg = account.path().join("730/local/cfg");
            let modified = [KEYS_FILE, CONVARS_FILE]
                .iter()
                .filter_map(|file| std::fs::metadata(cfg.join(file)).ok()?.modified().ok())
                .max();
            let Some(modified) = modified else { continue };
            let preferred = recent.as_ref() == Some(&name);
            candidates.push((preferred, modified, name, cfg, root));
        }
    }
    // Deterministic ties, and never combine controls belonging to two accounts.
    candidates.sort_by(|a, b| (&a.0, &a.1, &a.2, &a.3).cmp(&(&b.0, &b.1, &b.2, &b.3)));
    let (_, _, account, cfg, root) = candidates.pop()?;
    let defaults = vfs::libraries_from(vec![root.clone()]).into_iter().find_map(|library| {
        // Only the explicitly authorized text file, never a VPK or game UI.
        let file = library.join("steamapps/common/Counter-Strike Global Offensive/game/csgo/cfg/user_keys_default.vcfg");
        file.is_file().then_some(file)
    });
    Some(Source {
        account,
        cfg,
        defaults,
    })
}

/// Kept explicit rather than executing a command parser. CS2 left/right are
/// strafe and sprint is walk; Source1's +left/+right are turn commands.
fn translated_command(command: &str) -> Option<&str> {
    Some(match command {
        "<unbound>" | "" => "",
        "+left" => "+moveleft",
        "+right" => "+moveright",
        "+sprint" => "+speed",
        "slot6" => "use weapon_hegrenade",
        "slot7" => "use weapon_flashbang",
        "slot8" => "use weapon_smokegrenade",
        "slot9" => "use weapon_decoy",
        "toggleconsole" => "toggleconsole",
        other if ACTION_COMMANDS.contains(&other) => other,
        _ => return None,
    })
}

fn bindings(
    settings: &mut Settings,
    defaults: Option<&kv::Block>,
    overrides: &kv::Block,
    report: &mut String,
) -> usize {
    // Sparse user files override physical keys, including unsupported commands
    // and <unbound>. Do not let a default jump survive SPACE +voicerecord.
    let mut effective = BTreeMap::new();
    for (explicit, block) in [(false, defaults), (true, Some(overrides))] {
        if let Some(block) = block {
            for (key, value) in block.iter() {
                if let Some(command) = value.as_str() {
                    report.push_str(&format!(
                        "{} binding {key:?} = {command:?}\n",
                        if explicit { "User" } else { "Default" }
                    ));
                    effective.insert(key.to_ascii_lowercase(), (explicit, command.to_string()));
                }
            }
        }
    }
    let movement_binding = settings.binds[settings::KEY_MOVEMENT_MODE];
    if defaults.is_some() {
        // The native CS configuration has no rewrite movement-mode command.
        // Preserve this independent action while replacing native CS bindings.
        settings.binds[..settings::KEY_MOVEMENT_MODE].fill(None);
        settings.console_key = None;
        settings.mouse_binds.fill(MouseAction::None);
        settings.wheel_up = WheelAction::None;
        settings.wheel_down = WheelAction::None;
    }
    for key in effective.keys() {
        if let Some(key) = settings::source_key(key) {
            settings.unbind_key(key);
        }
        if let Some(index) = mouse_index(key) {
            settings.mouse_binds[index] = MouseAction::None;
        }
        match key.as_str() {
            "mwheelup" => settings.wheel_up = WheelAction::None,
            "mwheeldown" => settings.wheel_down = WheelAction::None,
            _ => {}
        }
    }
    let mut entries: Vec<_> = effective.into_iter().collect();
    // An explicit key wins over an inherited key when the current settings
    // model can retain only one keyboard key per action. Record every loss.
    entries.sort_by_key(|(_, (explicit, _))| *explicit);
    let mut count = 0;
    for (key, (_, source_command)) in entries {
        let normalized = source_command.trim().to_ascii_lowercase();
        let Some(command) = translated_command(&normalized) else {
            report.push_str(&format!(
                "Unsupported binding {key:?} = {source_command:?}; retained here, never executed\n"
            ));
            continue;
        };
        let applied = if let Some(index) = mouse_index(&key) {
            if let Some(action) = MouseAction::parse(command) {
                settings.mouse_binds[index] = action;
                true
            } else {
                false
            }
        } else if matches!(key.as_str(), "mwheelup" | "mwheeldown") {
            if let Some(action) = WheelAction::parse(command) {
                if key == "mwheelup" {
                    settings.wheel_up = action;
                } else {
                    settings.wheel_down = action;
                }
                true
            } else {
                false
            }
        } else if let Some(keycode) = settings::source_key(&key) {
            if command.is_empty() {
                true
            } else if command == "toggleconsole" {
                settings.set_console_key(keycode);
                true
            } else if let Some(index) = ACTION_COMMANDS.iter().position(|value| *value == command) {
                if let Some(old) = settings.binds[index] {
                    report.push_str(&format!("Additional keyboard key {old:?} for {command:?} cannot be retained; selected {key:?}\n"));
                }
                // Import must not use rebind's interactive swap semantics.
                settings.binds[index] = Some(keycode);
                true
            } else {
                false
            }
        } else {
            false
        };
        if applied {
            count += 1;
            report.push_str(&format!("Translated {key:?} -> {command:?}\n"));
        } else {
            report.push_str(&format!(
                "Unsupported device binding {key:?} = {source_command:?}; retained here\n"
            ));
        }
    }
    if let Some(key) = movement_binding {
        let claimed = settings.binds[..settings::KEY_MOVEMENT_MODE].contains(&Some(key))
            || settings.console_key == Some(key);
        if !claimed {
            settings.binds[settings::KEY_MOVEMENT_MODE] = Some(key);
        }
    }
    count
}

fn mouse_index(key: &str) -> Option<usize> {
    ["mouse1", "mouse2", "mouse3", "mouse4", "mouse5"]
        .iter()
        .position(|value| *value == key)
}

fn convars(settings: &mut Settings, block: &kv::Block, report: &mut String) -> usize {
    let mut count = 0;
    for (key, value) in block.iter() {
        let Some(value) = value.as_str() else {
            continue;
        };
        let number = value.parse::<f32>().ok().filter(|value| value.is_finite());
        let applied = match (key.to_ascii_lowercase().as_str(), number) {
            // Do not silently clamp an imported preference into another value.
            ("sensitivity", Some(value)) if (0.1..=10.0).contains(&value) => {
                settings.sensitivity = value;
                true
            }
            ("m_pitch", Some(value)) if value.abs() == 0.022 => {
                settings.invert_y = value < 0.;
                true
            }
            ("m_yaw", Some(0.022)) | ("sensitivity_y_scale", Some(1.0)) => true,
            _ => false,
        };
        report.push_str(&format!(
            "{} convar {key:?} = {value:?}\n",
            if applied {
                "Translated"
            } else {
                "Unsupported; retained"
            }
        ));
        count += usize::from(applied);
    }
    count
}

fn import_from(data: &Path, roots: &[PathBuf]) -> std::io::Result<()> {
    let target = data.join("config/user.cfg");
    if target.try_exists()? {
        return Ok(());
    }
    let Some(source) = discover(roots) else {
        return Ok(());
    };
    let mut report = format!(
        "CS2 controls import (read-only), Steam account {}\nExisting rewrite user.cfg always wins. No commands were executed.\nUnsupported source values are retained below and source files are unchanged.\nOnly primary player / slot0 is imported; autoexec/aliases are not evaluated.\n",
        source.account
    );
    let mut settings = Settings::default();
    let mut count = 0;
    if let Some(block) = section(&source.cfg.join(CONVARS_FILE), "convars", &mut report) {
        count += convars(&mut settings, &block, &mut report);
    }
    if let Some(block) = section(&source.cfg.join(KEYS_FILE), "bindings", &mut report) {
        let defaults = source
            .defaults
            .as_ref()
            .and_then(|path| section(path, "bindings", &mut report));
        if defaults.is_none() {
            report.push_str(
                "CS2 default bindings unavailable; omitted keys keep rewrite defaults.\n",
            );
        }
        count += bindings(&mut settings, defaults.as_ref(), &block, &mut report);
    }
    if count == 0 {
        return Ok(());
    }
    settings.controls_source = format!(
        "CS2 controls imported (Steam account {}); details: {REPORT_FILE}",
        source.account
    );
    std::fs::create_dir_all(data.join("config"))?;
    // Never truncate existing preferences, even if another process creates
    // them while discovery is running. Publish the report before user.cfg.
    let report_path = data.join(REPORT_FILE);
    let mut report_file = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(report_path)?;
    report_file.write_all(report.as_bytes())?;
    publish_settings(&target, settings.encode().as_bytes(), |file, bytes| file.write_all(bytes))?;
    eprintln!("{}", settings.controls_source);
    Ok(())
}

/// Publish only a closed, synced, complete file. A failed write leaves no final
/// name, so first-run setup can retry. The publication never replaces a winner.
fn publish_settings(
    target: &Path,
    encoded: &[u8],
    write: impl FnOnce(&mut std::fs::File, &[u8]) -> std::io::Result<()>,
) -> std::io::Result<()> {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
    let temp = target.with_file_name(format!(".cs2-import-{}-{nonce}.tmp", std::process::id()));
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&temp)?;
    let prepared = write(&mut file, encoded).and_then(|_| file.sync_all());
    drop(file); // Windows rename must not retain the writer's open handle.
    let result = prepared.and_then(|_| publish_completed(&temp, target));
    let _ = std::fs::remove_file(&temp);
    match result {
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        result => result,
    }
}

#[cfg(windows)]
fn publish_completed(temp: &Path, target: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
    }
    let temp: Vec<_> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
    let target: Vec<_> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    // Same-directory rename supports portable FAT/exFAT too. WRITE_THROUGH,
    // deliberately WITHOUT REPLACE_EXISTING or COPY_ALLOWED: no partial copy
    // can appear at the destination, and a concurrently saved user.cfg wins.
    let moved = unsafe { MoveFileExW(temp.as_ptr(), target.as_ptr(), 0x8) };
    if moved != 0 { Ok(()) } else { Err(std::io::Error::last_os_error()) }
}

#[cfg(not(windows))]
fn publish_completed(temp: &Path, target: &Path) -> std::io::Result<()> {
    // Fail closed on filesystems without hardlinks; never copy to the final
    // name or use std::fs::rename (which would replace an existing preference).
    std::fs::hard_link(temp, target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;
    #[test]
    fn native_radio_binding_cannot_erase_the_rewrite_movement_action() {
        let defaults=kv::parse(DEFAULTS).unwrap();
        let native=kv::parse("bindings { x radio2 }").unwrap();
        let mut settings=Settings::default();
        let key=settings.binds[settings::KEY_MOVEMENT_MODE];
        bindings(&mut settings,defaults.block("config").unwrap().block("bindings"),native.block("bindings").unwrap(),&mut String::new());
        assert_eq!(settings.binds[settings::KEY_MOVEMENT_MODE],key);
        let native=kv::parse("bindings { x +jump }").unwrap();
        bindings(&mut settings,None,native.block("bindings").unwrap(),&mut String::new());
        assert_eq!(settings.binds[settings::KEY_MOVEMENT_MODE],None);
        assert_eq!(settings.binds[4],key);
    }
    #[test]
    fn completed_publish_retries_after_partial_write_and_never_clobbers_a_winner() {
        let (data, _, _) = fixture("publish");
        std::fs::create_dir_all(data.join("config")).unwrap();
        let target = data.join("config/user.cfg");
        let encoded = Settings::default().encode();
        let failed = publish_settings(&target, encoded.as_bytes(), |file, bytes| {
            file.write_all(&bytes[..7])?;
            Err(std::io::Error::other("injected interrupted write"))
        });
        assert!(failed.is_err());
        assert!(!target.exists(), "partial output must not block first-run retry");
        assert_eq!(std::fs::read_dir(target.parent().unwrap()).unwrap().count(), 0);
        publish_settings(&target, encoded.as_bytes(), |file, bytes| file.write_all(bytes)).unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), encoded);
        std::fs::remove_file(&target).unwrap();
        // A competing saver wins after the importer has prepared its temp file.
        publish_settings(&target, encoded.as_bytes(), |file, bytes| {
            file.write_all(bytes)?;
            std::fs::write(&target, "winner preferences")
        }).unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "winner preferences");
    }

    #[test]
    fn quoted_literal_backslash_console_and_explicit_unbinds_round_trip() {
        let defaults = kv::parse(include_str!("../tests/fixtures/controls-defaults.vcfg")).unwrap();
        let overrides = kv::parse(include_str!("../tests/fixtures/controls-user.vcfg")).unwrap();
        let mut settings = Settings::default();
        bindings(&mut settings, defaults.block("config").unwrap().block("bindings"),
            overrides.block("config").unwrap().block("bindings").unwrap(), &mut String::new());
        let restored = Settings::parse(&settings.encode());
        assert_eq!(restored.console_key, settings::source_key("\\"));
        assert!(!restored.binds.contains(&settings::source_key("`")));
        assert!(!restored.binds.contains(&settings::source_key("v")));
        assert!(!restored.binds.contains(&settings::source_key("f3")));
        assert_eq!(restored.binds[0], settings::source_key("w"));
        assert_eq!(restored.binds[4], None);
        assert_eq!(restored.wheel_up, WheelAction::Jump);
    }
    const DEFAULTS: &str = r#"config { bindings {
        w +forward s +back a +left d +right SPACE +jump CTRL +duck SHIFT +sprint
        q lastinv g drop 1 slot1 2 slot2 3 slot3 4 slot4 5 slot5
        MOUSE1 +attack MOUSE2 +attack2 MWHEELUP invprev MWHEELDOWN invnext
    } }"#;
    const OVERRIDES: &str = r#"config { bindings {
        SPACE +voicerecord SHIFT +duck CTRL +sprint q drop g slot8 v slot6
        z slot7 n slot10 1 <unbound> 2 <unbound> 3 <unbound>
        MOUSE4 slot3 MOUSE5 slot1 c slot2 MWHEELUP +jump MWHEELDOWN +jump
        p "exec personal.cfg; quit" h switchhands
    } }"#;

    fn fixture(name: &str) -> (PathBuf, PathBuf, PathBuf) {
        let root =
            std::env::temp_dir().join(format!("csr-cs2-import-{}-{name}", std::process::id()));
        // Each test owns a unique fresh directory; no env changes or real Steam.
        let root = root.join(
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
                .to_string(),
        );
        let steam = root.join("fakeSteam");
        let cfg = steam.join("userdata/123/730/local/cfg");
        std::fs::create_dir_all(&cfg).unwrap();
        let defaults = steam.join("steamapps/common/Counter-Strike Global Offensive/game/csgo/cfg");
        std::fs::create_dir_all(&defaults).unwrap();
        std::fs::write(defaults.join("user_keys_default.vcfg"), DEFAULTS).unwrap();
        std::fs::write(cfg.join(KEYS_FILE), OVERRIDES).unwrap();
        std::fs::write(cfg.join(CONVARS_FILE), r#"config { convars { sensitivity 1.375 m_pitch -0.022 m_yaw 0.022 zoom_sensitivity_ratio 0.818 } }"#).unwrap();
        (root.join("rewrite"), steam, cfg)
    }

    #[test]
    fn sparse_import_translates_physical_controls_and_keeps_sources_byte_identical() {
        let (data, steam, cfg) = fixture("controls");
        let before = [KEYS_FILE, CONVARS_FILE].map(|file| std::fs::read(cfg.join(file)).unwrap());
        import_from(&data, &[steam]).unwrap();
        let settings =
            Settings::parse(&std::fs::read_to_string(data.join("config/user.cfg")).unwrap());
        assert_eq!(settings.sensitivity, 1.375);
        assert!(settings.invert_y);
        for (action, key) in [
            (0, "w"),
            (1, "s"),
            (2, "a"),
            (3, "d"),
            (5, "shift"),
            (6, "ctrl"),
            (10, "q"),
            (16, "z"),
            (17, "g"),
            (15, "v"),
            (23, "c"),
            (29, "n"),
        ] {
            assert_eq!(settings.binds[action], settings::source_key(key), "{key}");
        }
        for action in [4, 21, 22, 24] {
            assert_eq!(settings.binds[action], None);
        }
        assert_eq!(
            settings.mouse_binds,
            [
                MouseAction::Attack,
                MouseAction::Attack2,
                MouseAction::None,
                MouseAction::Action(24),
                MouseAction::Action(22)
            ]
        );
        assert_eq!(settings.wheel_up, WheelAction::Jump);
        assert_eq!(settings.wheel_down, WheelAction::Jump);
        assert!(settings.controls_source.contains("123"));
        let report = std::fs::read_to_string(data.join(REPORT_FILE)).unwrap();
        assert!(report.contains("exec personal.cfg; quit"));
        assert!(report.contains("zoom_sensitivity_ratio"));
        assert!(report.contains("Unsupported binding"));
        for (index, file) in [KEYS_FILE, CONVARS_FILE].iter().enumerate() {
            assert_eq!(std::fs::read(cfg.join(file)).unwrap(), before[index]);
        }
    }

    #[test]
    fn existing_rewrite_file_is_preserved_even_when_empty_or_partial() {
        for (name, existing) in [
            ("empty", ""),
            (
                "saved",
                "sensitivity=2.71828\nunknown=retain me\nbind0=ArrowUp\n",
            ),
        ] {
            let (data, steam, _) = fixture(name);
            std::fs::create_dir_all(data.join("config")).unwrap();
            std::fs::write(data.join("config/user.cfg"), existing).unwrap();
            import_from(&data, &[steam]).unwrap();
            assert_eq!(
                std::fs::read_to_string(data.join("config/user.cfg")).unwrap(),
                existing
            );
            assert!(!data.join(REPORT_FILE).exists());
        }
    }

    #[test]
    fn most_recent_login_wins_without_mixing_accounts_and_import_runs_only_once() {
        let (data, steam, cfg) = fixture("accounts");
        let other = steam.join("userdata/456/730/local/cfg");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(
            other.join(CONVARS_FILE),
            "config { convars { sensitivity 8 } }",
        )
        .unwrap();
        std::fs::create_dir_all(steam.join("config")).unwrap();
        std::fs::write(
            steam.join("config/loginusers.vdf"),
            "users { 76561197960265851 { MostRecent 1 } }",
        )
        .unwrap();
        import_from(&data, std::slice::from_ref(&steam)).unwrap();
        let initial = std::fs::read(data.join("config/user.cfg")).unwrap();
        assert_eq!(
            Settings::parse(std::str::from_utf8(&initial).unwrap()).sensitivity,
            1.375
        );
        std::fs::write(
            cfg.join(CONVARS_FILE),
            "config { convars { sensitivity 4 } }",
        )
        .unwrap();
        import_from(&data, &[steam]).unwrap();
        assert_eq!(
            std::fs::read(data.join("config/user.cfg")).unwrap(),
            initial
        );
    }

    #[test]
    fn invalid_values_and_alias_chains_are_not_silently_clamped_or_executed() {
        let mut settings = Settings::default();
        let mut report = String::new();
        let block =
            kv::parse("sensitivity NaN m_pitch 0.01 m_yaw 0.04 sensitivity_y_scale 2").unwrap();
        assert_eq!(convars(&mut settings, &block, &mut report), 0);
        assert_eq!(settings.sensitivity, 1.);
        assert!(!settings.invert_y);
        for command in [
            "alias",
            "+jump; +attack",
            "exec autoexec.cfg",
            "toggle sensitivity 1 2",
            "slot12",
        ] {
            assert_eq!(translated_command(command), None);
        }
        let (data, steam, cfg) = fixture("malformed");
        std::fs::write(cfg.join(KEYS_FILE), "config { bindings {").unwrap();
        std::fs::write(
            cfg.join(CONVARS_FILE),
            "config { convars { sensitivity inf } }",
        )
        .unwrap();
        import_from(&data, &[steam]).unwrap();
        assert!(!data.join("config/user.cfg").exists());
    }

    #[test]
    fn missing_defaults_keep_omitted_rewrite_keys_but_honor_explicit_unbinds() {
        let mut settings = Settings::default();
        let overrides =
            kv::parse(r#"SPACE "<unbound>" q "+attack" MOUSE1 "<unbound>" MWHEELUP "my_alias""#)
                .unwrap();
        let mut report = String::new();
        bindings(&mut settings, None, &overrides, &mut report);
        assert_eq!(settings.binds[0], settings::source_key("w"));
        assert_eq!(settings.binds[4], None);
        assert_eq!(settings.binds[21], None);
        assert_eq!(
            settings.binds[settings::KEY_ATTACK],
            settings::source_key("q")
        );
        assert_eq!(settings.mouse_binds[0], MouseAction::None);
        assert_eq!(settings.wheel_up, WheelAction::None);
        assert!(report.contains("my_alias"));
    }

    #[test]
    fn explicit_keyboard_override_wins_without_swapping_default_keys() {
        let defaults = kv::parse("w +forward u +forward s +back").unwrap();
        let overrides = kv::parse("uparrow +forward w +back").unwrap();
        let mut settings = Settings::default();
        let mut report = String::new();
        bindings(&mut settings, Some(&defaults), &overrides, &mut report);
        assert_eq!(settings.binds[0], settings::source_key("uparrow"));
        assert_eq!(settings.binds[1], settings::source_key("w"));
        assert_eq!(
            settings
                .binds
                .iter()
                .filter(|key| **key == settings::source_key("s"))
                .count(),
            0
        );
        assert!(report.contains("Additional keyboard key"));
    }
}
