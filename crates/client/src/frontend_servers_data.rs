//! Native browser definitions from the player's install and local UI state.
//! Network requests, address resolution, persistence and joining belong to the caller.
use std::collections::{HashMap, HashSet};

pub const MAX_ROWS: usize = 256;
pub const MAX_TEXT: usize = 256;
const MAX_FILE: usize = 512 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
#[derive(Clone, Debug)]
pub struct Control {
    pub name: String,
    pub field_name: String,
    pub kind: String,
    pub rect: Rect,
    pub text: String,
    pub command: Option<String>,
    pub font: String,
    pub visible: bool,
    pub enabled: bool,
    pub default_button: bool,
}
#[derive(Clone, Debug, Default)]
pub struct Layout {
    pub controls: Vec<Control>,
}
impl Layout {
    pub fn control(&self, name: &str) -> Option<&Control> {
        self.controls
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(name))
    }
    fn parse(bytes: &[u8], tokens: &HashMap<String, String>) -> Result<Self, String> {
        if bytes.len() > MAX_FILE {
            return Err("Browser layout is too large".into());
        }
        let doc = kv::parse(
            kv::decode_text(bytes).trim_end_matches(|c: char| c == '\0' || c.is_whitespace()),
        )
        .map_err(|e| e.to_string())?;
        let root = doc
            .entries
            .first()
            .and_then(|(_, v)| v.as_block())
            .ok_or("Missing browser layout")?;
        if root.entries.len() > 256 {
            return Err("Too many browser controls".into());
        }
        let mut controls = Vec::new();
        let mut names = HashSet::new();
        for (key, value) in root.iter() {
            let Some(block) = value.as_block() else {
                continue;
            };
            let name = key;
            if name.len() > MAX_TEXT || !names.insert(name.to_ascii_lowercase()) {
                return Err("Invalid or duplicate browser field".into());
            }
            let number = |key: &str, extent: bool| -> Result<f32, String> {
                let value = block
                    .str(key)
                    .unwrap_or("0")
                    .parse::<f32>()
                    .map_err(|_| format!("Unsupported {key} in browser field {name}"))?;
                if !value.is_finite() || value.abs() > 32768.0 || (extent && value < 0.0) {
                    return Err(format!("Invalid {key} in browser field {name}"));
                }
                Ok(value)
            };
            let label = block
                .str("labelText")
                .or_else(|| block.str("title"))
                .unwrap_or("");
            let text = tokens
                .get(&label.trim_start_matches('#').to_ascii_lowercase())
                .map_or(label, String::as_str);
            controls.push(Control {
                name: name.into(),
                field_name: bounded(block.str("fieldName").unwrap_or(name)),
                kind: block.str("ControlName").unwrap_or("").into(),
                rect: Rect {
                    x: number("xpos", false)?,
                    y: number("ypos", false)?,
                    width: number("wide", true)?,
                    height: number("tall", true)?,
                },
                text: bounded(text),
                command: block.str("Command").map(bounded),
                font: block.str("font").unwrap_or("Default").into(),
                visible: block.bool("visible").unwrap_or(true),
                enabled: block.bool("enabled").unwrap_or(true),
                default_button: block.bool("Default").unwrap_or(false),
            });
        }
        Ok(Self { controls })
    }
}

#[derive(Clone, Debug)]
pub struct FontInfo {
    pub family: String,
    pub size: f32,
    pub weight: u16,
}
#[derive(Clone, Debug, Default)]
pub struct Scheme {
    colors: HashMap<String, String>,
    settings: HashMap<String, String>,
    pub fonts: HashMap<String, FontInfo>,
    borders: HashMap<String, kv::Value>,
}
impl Scheme {
    pub fn color(&self, key: &str) -> Option<[u8; 4]> {
        let mut value = self
            .settings
            .get(&key.to_ascii_lowercase())
            .or_else(|| self.colors.get(&key.to_ascii_lowercase()))
            .map_or(key, String::as_str);
        for _ in 0..16 {
            let numbers: Option<Vec<u8>> =
                value.split_whitespace().map(|n| n.parse().ok()).collect();
            if let Some(numbers) = numbers {
                if let Ok(color) = numbers.try_into() {
                    return Some(color);
                }
            }
            value = self
                .colors
                .get(&value.to_ascii_lowercase())
                .or_else(|| self.settings.get(&value.to_ascii_lowercase()))?
                .as_str();
        }
        None
    }
    pub fn font(&self, name: &str) -> Option<&FontInfo> {
        self.fonts.get(&name.to_ascii_lowercase())
    }
    /// First declared stroke on each side (top, right, bottom, left).
    pub fn border_colors(&self, name: &str) -> [Option<[u8; 4]>; 4] {
        let mut value = self.borders.get(&name.to_ascii_lowercase());
        for _ in 0..16 {
            match value {
                Some(kv::Value::Str(alias)) => {
                    value = self.borders.get(&alias.to_ascii_lowercase())
                }
                Some(kv::Value::Block(block)) => {
                    return ["Top", "Right", "Bottom", "Left"].map(|side| {
                        block
                            .block(side)?
                            .iter()
                            .find_map(|(_, value)| value.as_block())?
                            .str("color")
                            .and_then(|name| self.color(name))
                    });
                }
                _ => break,
            }
        }
        [None; 4]
    }
    fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_FILE {
            return Err("Browser scheme is too large".into());
        }
        let doc = kv::parse(
            kv::decode_text(bytes).trim_end_matches(|c: char| c == '\0' || c.is_whitespace()),
        )
        .map_err(|e| e.to_string())?;
        let root = doc
            .entries
            .first()
            .and_then(|(_, v)| v.as_block())
            .ok_or("Missing browser scheme")?;
        let strings = |name: &str| -> HashMap<String, String> {
            root.block(name)
                .map(|b| {
                    b.iter()
                        .filter_map(|(k, v)| {
                            v.as_str().map(|v| (k.to_ascii_lowercase(), bounded(v)))
                        })
                        .take(4096)
                        .collect()
                })
                .unwrap_or_default()
        };
        let mut fonts = HashMap::new();
        if let Some(block) = root.block("Fonts") {
            for (name, value) in block.iter().take(256) {
                let Some(font) = value
                    .as_block()
                    .and_then(|b| b.iter().find_map(|(_, v)| v.as_block()))
                else {
                    continue;
                };
                let Some(size) = font
                    .f32("tall")
                    .filter(|s| s.is_finite() && *s > 0.0 && *s <= 256.0)
                else {
                    continue;
                };
                fonts.insert(
                    name.to_ascii_lowercase(),
                    FontInfo {
                        family: bounded(font.str("name").unwrap_or("")),
                        size,
                        weight: font.i64("weight").unwrap_or(0).clamp(0, 1000) as u16,
                    },
                );
            }
        }
        Ok(Self {
            colors: strings("Colors"),
            settings: strings("BaseSettings"),
            fonts,
            borders: root
                .block("Borders")
                .map(|b| {
                    b.iter()
                        .take(256)
                        .map(|(name, value)| (name.to_ascii_lowercase(), value.clone()))
                        .collect()
                })
                .unwrap_or_default(),
        })
    }
}

#[derive(Clone, Debug)]
pub struct Definition {
    pub browser: Layout,
    pub page: Layout,
    pub add: Layout,
    pub info: Layout,
    pub scheme: Scheme,
    pub tokens: HashMap<String, String>,
}
impl Definition {
    pub fn load(game: &vfs::Vfs, platform: &vfs::Vfs) -> Result<Self, String> {
        let bytes = platform
            .read("servers/serverbrowser_english.txt")
            .ok_or("Missing installed server browser localization")?;
        if bytes.len() > MAX_FILE {
            return Err("Browser localization is too large".into());
        }
        let text = kv::decode_text(&bytes);
        let doc =
            kv::parse_escaped(text.trim_end_matches(|c: char| c == '\0' || c.is_whitespace()))
                .map_err(|e| e.to_string())?;
        let mut tokens = HashMap::new();
        if let Some(block) = doc.block("lang").and_then(|b| b.block("Tokens")) {
            for (key, value) in block.iter().take(4096) {
                if let Some(value) = value.as_str() {
                    tokens.insert(key.to_ascii_lowercase(), bounded(value));
                }
            }
        }
        let layout = |file: &str| -> Result<Layout, String> {
            Layout::parse(
                &platform
                    .read(&format!("servers/{file}.res"))
                    .ok_or_else(|| format!("Missing installed {file} layout"))?,
                &tokens,
            )
        };
        Ok(Self {
            browser: layout("dialogserverbrowser")?,
            page: layout("customgamespage")?,
            add: layout("dialogaddserver")?,
            info: layout("dialoggameinfo")?,
            scheme: Scheme::parse(
                &game
                    .read("resource/sourcescheme.res")
                    .ok_or("Missing installed browser scheme")?,
            )?,
            tokens,
        })
    }
    pub fn text<'a>(&'a self, key: &'a str) -> &'a str {
        self.tokens
            .get(&key.to_ascii_lowercase())
            .map_or(key, String::as_str)
    }
}

pub fn bounded(value: &str) -> String {
    let mut out = String::new();
    for c in value.chars().filter(|c| !c.is_control() || *c == '\n') {
        if out.len() + c.len_utf8() > MAX_TEXT {
            break;
        }
        out.push(c);
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ServerKey(pub u64);
#[derive(Clone, Debug)]
pub struct ServerRow {
    pub key: ServerKey,
    pub endpoint: String,
    pub name: Option<String>,
    pub map: Option<String>,
    pub players: Option<(u16, u16)>,
    pub ping_ms: Option<u32>,
    pub favorite: bool,
    pub connectable: bool,
}
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub generation: u64,
    pub rows: Vec<ServerRow>,
    pub busy: bool,
    pub error: Option<String>,
    pub status: Option<String>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Internet,
    Favorites,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Intent {
    Refresh(Tab),
    ProbeAddress(String),
    AddFavorite(String),
    RemoveFavorite { key: ServerKey, generation: u64 },
    Connect { key: ServerKey, generation: u64 },
    CancelAddress,
    Close,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Edit {
    Insert(String),
    Backspace,
    Delete,
    Left,
    Right,
    Home,
    End,
    SelectAll,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AddressEdit {
    text: String,
    cursor: usize,
    anchor: usize,
}
impl AddressEdit {
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn cursor(&self) -> usize {
        self.cursor
    }
    pub fn selection(&self) -> std::ops::Range<usize> {
        self.cursor.min(self.anchor)..self.cursor.max(self.anchor)
    }
    pub fn edit(&mut self, edit: Edit) -> bool {
        let before = self.clone();
        let previous =
            |text: &str, at: usize| text[..at].char_indices().next_back().map_or(0, |(i, _)| i);
        let next =
            |text: &str, at: usize| text[at..].chars().next().map_or(at, |c| at + c.len_utf8());
        match edit {
            Edit::Insert(value) => {
                // Reject the whole paste instead of silently changing a requested endpoint.
                if value.chars().any(char::is_control)
                    || self.text.len() - self.selection().len() + value.len() > MAX_TEXT
                {
                    return false;
                }
                self.text.replace_range(self.selection(), &value);
                self.cursor = self.selection().start + value.len();
                self.anchor = self.cursor;
            }
            Edit::Backspace | Edit::Delete => {
                let mut range = self.selection();
                if range.is_empty() {
                    if edit == Edit::Backspace {
                        range.start = previous(&self.text, self.cursor);
                    } else {
                        range.end = next(&self.text, self.cursor);
                    }
                }
                self.text.replace_range(range.clone(), "");
                self.cursor = range.start;
                self.anchor = self.cursor;
            }
            Edit::Left => {
                self.cursor = previous(&self.text, self.cursor);
                self.anchor = self.cursor;
            }
            Edit::Right => {
                self.cursor = next(&self.text, self.cursor);
                self.anchor = self.cursor;
            }
            Edit::Home => {
                self.cursor = 0;
                self.anchor = 0;
            }
            Edit::End => {
                self.cursor = self.text.len();
                self.anchor = self.cursor;
            }
            Edit::SelectAll => {
                self.cursor = self.text.len();
                self.anchor = 0;
            }
        }
        self.text != before.text || self.cursor != before.cursor || self.anchor != before.anchor
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Model {
    pub tab: Tab,
    pub selected: Option<ServerKey>,
    pub add_open: bool,
    pub address_focused: bool,
    pub address: AddressEdit,
}
impl Model {
    pub fn rows<'a>(&self, snapshot: &'a Snapshot) -> Vec<&'a ServerRow> {
        let rows = &snapshot.rows[..snapshot.rows.len().min(MAX_ROWS)];
        let mut counts = HashMap::new();
        for row in rows {
            *counts.entry(row.key).or_insert(0usize) += 1;
        }
        rows.iter()
            .filter(|r| {
                counts.get(&r.key) == Some(&1) && (self.tab != Tab::Favorites || r.favorite)
            })
            .collect()
    }
    pub fn reconcile(&mut self, snapshot: &Snapshot) {
        if !self
            .rows(snapshot)
            .iter()
            .any(|r| Some(r.key) == self.selected)
        {
            self.selected = None;
        }
    }
    pub fn select(&mut self, key: ServerKey, snapshot: &Snapshot) -> bool {
        if self.rows(snapshot).iter().any(|r| r.key == key) {
            self.selected = Some(key);
            true
        } else {
            false
        }
    }
    pub fn connect(&self, snapshot: &Snapshot) -> Option<Intent> {
        if snapshot.busy || self.add_open {
            return None;
        }
        self.rows(snapshot)
            .into_iter()
            .find(|r| Some(r.key) == self.selected && r.connectable)
            .map(|r| Intent::Connect {
                key: r.key,
                generation: snapshot.generation,
            })
    }
    pub fn command(&mut self, command: &str, snapshot: &Snapshot) -> Option<Intent> {
        match command {
            "connect" | "Connect" => self.connect(snapshot),
            "GetNewList" | "refresh" => {
                (!snapshot.busy && !self.add_open).then_some(Intent::Refresh(self.tab))
            }
            "AddServerByName" if !snapshot.busy => {
                self.add_open = true;
                self.address_focused = true;
                None
            }
            "TestServers" | "OK" if self.add_open && !snapshot.busy => {
                let address = self.address.text().trim();
                if address.is_empty() {
                    None
                } else if command == "OK" {
                    Some(Intent::AddFavorite(address.into()))
                } else {
                    Some(Intent::ProbeAddress(address.into()))
                }
            }
            "Close" if self.add_open => {
                self.add_open = false;
                self.address_focused = false;
                Some(Intent::CancelAddress)
            }
            "Close" => Some(Intent::Close),
            _ => None,
        }
    }
    pub fn blur(&mut self) {
        self.address_focused = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row(key: u64) -> ServerRow {
        ServerRow {
            key: ServerKey(key),
            endpoint: "127.0.0.1:27015".into(),
            name: None,
            map: None,
            players: None,
            ping_ms: None,
            favorite: true,
            connectable: true,
        }
    }
    #[test]
    fn selection_survives_reordering_but_not_removal_or_hidden_tab() {
        let mut state = Model::default();
        let mut snapshot = Snapshot {
            rows: vec![row(1), row(2)],
            ..Default::default()
        };
        assert!(state.select(ServerKey(2), &snapshot));
        snapshot.rows.reverse();
        state.reconcile(&snapshot);
        assert_eq!(
            state.connect(&snapshot),
            Some(Intent::Connect {
                key: ServerKey(2),
                generation: 0
            })
        );
        snapshot.rows[0].favorite = false;
        state.tab = Tab::Favorites;
        state.reconcile(&snapshot);
        assert_eq!(state.selected, None);
        assert!(!state.select(ServerKey(9), &snapshot));
    }
    #[test]
    fn stale_and_busy_rows_cannot_emit_a_connect() {
        let mut state = Model::default();
        let mut snapshot = Snapshot {
            rows: vec![row(1)],
            ..Default::default()
        };
        state.select(ServerKey(1), &snapshot);
        snapshot.busy = true;
        assert_eq!(state.connect(&snapshot), None);
        snapshot.busy = false;
        snapshot.rows[0].connectable = false;
        assert_eq!(state.connect(&snapshot), None);
        snapshot.rows[0].connectable = true;
        snapshot.rows.push(row(1));
        assert_eq!(state.connect(&snapshot), None);
        snapshot.rows.clear();
        assert_eq!(state.connect(&snapshot), None);
        assert_eq!(state.command("arbitrary console command", &snapshot), None);
    }
    #[test]
    fn address_edits_preserve_utf8_and_reject_control_or_oversized_pastes_atomically() {
        let mut input = AddressEdit::default();
        input.edit(Edit::Insert("h\u{e9}st:27015".into()));
        input.edit(Edit::Home);
        input.edit(Edit::Right);
        input.edit(Edit::Delete);
        assert_eq!(input.text(), "hst:27015");
        input.edit(Edit::SelectAll);
        input.edit(Edit::Insert("[::1]:27015".into()));
        assert!(!input.edit(Edit::Insert("\nconnect elsewhere".into())));
        assert!(!input.edit(Edit::Insert("x".repeat(MAX_TEXT + 1))));
        assert_eq!(input.text(), "[::1]:27015");
        input.edit(Edit::Backspace);
        assert_eq!(input.text(), "[::1]:2701");
    }
    #[test]
    fn installed_commands_only_emit_address_requests_from_the_open_dialog() {
        let mut model = Model::default();
        let snapshot = Snapshot::default();
        model
            .address
            .edit(Edit::Insert("host.example:27015".into()));
        assert_eq!(model.command("OK", &snapshot), None);
        model.command("AddServerByName", &snapshot);
        assert_eq!(
            model.command("TestServers", &snapshot),
            Some(Intent::ProbeAddress("host.example:27015".into()))
        );
        assert_eq!(
            model.command("OK", &snapshot),
            Some(Intent::AddFavorite("host.example:27015".into()))
        );
        model.command("Close", &snapshot);
        assert!(!model.address_focused);
        assert_eq!(model.command("Close", &snapshot), Some(Intent::Close));
    }
    #[test]
    fn malformed_layouts_and_cyclic_palette_references_are_rejected() {
        assert!(
            Layout::parse(
                b"layout { a { fieldName same } a { fieldName other } }",
                &HashMap::new()
            )
            .is_err()
        );
        assert!(Layout::parse(b"layout { a { wide NaN } }", &HashMap::new()).is_err());
        let scheme = Scheme::parse(
            b"Scheme { Colors { a b b a good \"1 2 3 4\" } BaseSettings { valid good invalid a } }",
        )
        .unwrap();
        assert_eq!(scheme.color("invalid"), None);
        assert_eq!(scheme.color("valid"), Some([1, 2, 3, 4]));
    }
    #[test]
    fn resource_keys_preserve_aliases_and_only_trailing_terminators_are_ignored() {
        let layout = Layout::parse(
            b"layout { first { fieldName alias } second { fieldName alias } }\0\r\n",
            &HashMap::new(),
        )
        .unwrap();
        assert_eq!(layout.controls.len(), 2);
        assert_eq!(layout.control("first").unwrap().field_name, "alias");
        assert_eq!(layout.control("second").unwrap().field_name, "alias");
        assert!(Layout::parse(b"layout { broken\0 }", &HashMap::new()).is_err());
    }
}
