//! Bounded muzzle-flash fallback: one authored additive PCF sprite per Fire.
//! This is not a particle simulator: smoke, trails, lights, warped geometry and
//! spatial emitter operators are omitted. Assets stay in the mounted VFS.
use crate::{
    effects::PendingEvents,
    particle_data::{self, Element, Sheet, Value},
    player::{LocalPlayer, WorldPlayerCamera},
    tracers::Muzzles,
    LoadedLevel,
};
use bevy::{asset::RenderAssetUsages, mesh::PrimitiveTopology, prelude::*};
use std::collections::{HashMap, HashSet};

const LIMIT: usize = 128;

#[derive(Clone, Debug)]
struct Shot {
    serial: u64,
    player: sim::PlayerId,
    weapon: String,
    suppressed: bool,
}
#[derive(Resource, Default)]
pub struct Flashes {
    serial: u64,
    pending: Vec<Shot>,
    loaded: bool,
    names: HashMap<String, HashMap<String, String>>,
    styles: HashMap<String, Option<Style>>,
    textures: crate::textures::TextureCache,
    reported_selections: HashSet<(String, bool, bool, bool)>,
}
#[derive(Clone)]
struct Style {
    material: Handle<StandardMaterial>,
    frames: Vec<Handle<Mesh>>,
    sheet: Option<Sheet>,
    radius: f32,
    life: f32,
    rate: f32,
}
#[derive(Component)]
pub struct Flash {
    born: f32,
    owner: sim::PlayerId,
    first: bool,
    style: Style,
}

/// Read the authoritative batch once, before effects::process drains it. Never
/// derive shots from held attack, animation time, tracers or last_fire timestamps.
pub fn capture(events: Res<PendingEvents>, mut flashes: ResMut<Flashes>) {
    flashes.pending.clear();
    for event in &events.0 {
        if let sim::GameEvent::Fire {
            player,
            weapon,
            suppressed,
            ..
        } = event
        {
            flashes.serial = flashes.serial.wrapping_add(1);
            let serial = flashes.serial;
            if flashes.pending.len() < LIMIT {
                flashes.pending.push(Shot {
                    serial,
                    player: *player,
                    weapon: weapon.clone(),
                    suppressed: *suppressed,
                });
            }
        }
    }
}

fn declarations(text: &str) -> HashMap<String, HashMap<String, String>> {
    let Ok(doc) = kv::parse(text) else {
        return HashMap::new();
    };
    let Some(root) = doc.block("items_game") else {
        return HashMap::new();
    };
    let prefabs: HashMap<_, _> = root
        .get_all("prefabs")
        .filter_map(kv::Value::as_block)
        .flat_map(|b| &b.entries)
        .filter_map(|(k, v)| v.as_block().map(|b| (k.to_ascii_lowercase(), b)))
        .collect();
    fn collect(
        b: &kv::Block,
        prefabs: &HashMap<String, &kv::Block>,
        out: &mut HashMap<String, String>,
        depth: usize,
    ) {
        if depth >= 16 {
            return;
        }
        for name in b.str("prefab").unwrap_or("").split_whitespace() {
            if let Some(p) = prefabs.get(&name.to_ascii_lowercase()) {
                collect(p, prefabs, out, depth + 1);
            }
        }
        // Installed items_game declares these in each prefab's visuals block.
        // Apply each ancestor's visuals before the descendant/item overrides;
        // top-level fields remain accepted for existing content packs.
        for fields in std::iter::once(b).chain(b.get_all("visuals").filter_map(kv::Value::as_block))
        {
            for (key, value) in &fields.entries {
                if key.to_ascii_lowercase().starts_with("muzzle_flash_effect_") {
                    if let Some(value) = value.as_str() {
                        out.insert(key.to_ascii_lowercase(), value.into());
                    }
                }
            }
        }
    }
    let mut result = HashMap::new();
    for item in root
        .get_all("items")
        .filter_map(kv::Value::as_block)
        .flat_map(|b| &b.entries)
        .filter_map(|(_, v)| v.as_block())
    {
        if item.str("baseitem") != Some("1") {
            continue;
        }
        let Some(name) = item.str("name") else {
            continue;
        };
        let mut fields = HashMap::new();
        collect(item, &prefabs, &mut fields, 0);
        if !fields.is_empty() {
            result.insert(name.into(), fields);
        }
    }
    result
}

fn effect_name(fields: &HashMap<String, String>, first: bool, suppressed: bool) -> Option<&str> {
    let key = if first {
        "muzzle_flash_effect_1st_person"
    } else {
        "muzzle_flash_effect_3rd_person"
    };
    // Both historical explicit silenced declarations and Legacy alternate-mode
    // declarations are accepted. Missing suppressed data must NOT become the
    // loud weapon's flash (notably schemas with no USP suppressed declaration).
    let value = if suppressed {
        fields
            .get(&format!("{key}_silenced_alt"))
            .or_else(|| fields.get(&format!("{key}_alt")))
            .or_else(|| fields.get(key).filter(|name| name.contains("silenced")))
    } else {
        fields.get(key)
    }?;
    (!value.is_empty()).then_some(value.as_str())
}

fn op<'a>(ops: &[&'a Element], name: &str) -> Option<&'a Element> {
    ops.iter()
        .copied()
        .find(|e| e.text("functionName").eq_ignore_ascii_case(name))
}
fn lower(e: &Element, all: &[Element]) -> Option<(f32, f32, f32, f32, [u8; 4], usize)> {
    let init = e.refs("initializers", all);
    let render = e.refs("renderers", all);
    let renderer = op(&render, "render_animated_sprites")?;
    if renderer.number("orientation_type", 0.0) != 0.0 {
        return None;
    }
    let emit = e.refs("emitters", all);
    if op(&emit, "emit_instantaneously").is_none()
        && !op(&emit, "emit_continuously")
            .is_some_and(|e| e.number("emission_duration", 1.0) <= 0.03)
    {
        return None;
    }
    let remap = |field: f32| {
        init.iter().copied().find(|e| {
            e.text("functionName")
                .eq_ignore_ascii_case("remap initial scalar")
                && e.number("output field", 3.0) == field
        })
    };
    let life = op(&init, "Lifetime Random")
        .map(|e| e.number("lifetime_min", 0.0))
        .or_else(|| remap(1.0).map(|e| e.number("output minimum", 0.0)))?;
    // Only brief flash layers; never reinterpret smoke as a flash.
    if !(0.005..=0.05).contains(&life) {
        return None;
    }
    let radius = op(&init, "Radius Random")
        .map(|e| e.number("radius_min", 0.0))
        .or_else(|| {
            op(&init, "Remap Noise to Scalar")
                .filter(|e| e.number("output field", 3.0) == 3.0)
                .map(|e| e.number("output minimum", 0.0))
        })
        .or_else(|| remap(3.0).map(|e| e.number("output minimum", 0.0)))
        .unwrap_or_else(|| e.number("radius", f32::NAN));
    if !(0.0..=32.0).contains(&radius) || radius == 0.0 {
        return None;
    }
    let mut alpha = op(&init, "Alpha Random")
        .map_or(e.number("alpha", 255.0), |e| e.number("alpha_min", 255.0))
        / 255.0;
    if let Some(remap) = remap(7.0) {
        let initial = remap.number("output minimum", alpha);
        alpha = if remap.number("output is scalar of initial random range", 0.0) != 0.0 {
            alpha * initial
        } else {
            initial
        };
    }
    let color = op(&init, "Color Random")
        .map_or(e.color("color", [255; 4]), |e| e.color("color1", [255; 4]));
    let seq = op(&init, "Sequence Random").map_or(e.number("sequence_number", 0.0), |e| {
        e.number("sequence_min", 0.0)
    });
    Some((
        radius,
        life,
        alpha.clamp(0.0, 1.0),
        renderer.number("animation rate", 1.0),
        color,
        seq.max(0.0) as usize,
    ))
}

fn quad(uv: [f32; 4]) -> Mesh {
    let [u0, v0, u1, v1] = uv;
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![
            [-1., -1., 0.],
            [1., -1., 0.],
            [1., 1., 0.],
            [-1., -1., 0.],
            [1., 1., 0.],
            [-1., 1., 0.],
        ],
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0., 0., 1.]; 6]);
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_UV_0,
        vec![[u0, v1], [u1, v1], [u1, v0], [u0, v1], [u1, v0], [u0, v0]],
    );
    mesh
}

/// VTF 7.0-7.2 stores image bytes immediately after its legacy header and
/// thumbnail, without the resource directory read by particle_data::sheets.
/// In particular the installed rifle vent uses a single-frame VTF 7.1.
/// Only a valid single 2D image can use full UVs; corrupt modern sheets must
/// still fail closed instead of displaying an atlas as one enormous flash.
fn sprite_sheets(bytes: &[u8]) -> Option<HashMap<usize, Sheet>> {
    if bytes.get(..4)? != b"VTF\0" {
        return None;
    }
    let major = u32::from_le_bytes(bytes.get(4..8)?.try_into().ok()?);
    let minor = u32::from_le_bytes(bytes.get(8..12)?.try_into().ok()?);
    if major == 7 && minor < 3 {
        if bytes.len() < if minor == 2 { 65 } else { 64 } {
            return None;
        }
        let texture = vtf::Vtf::parse(bytes).ok()?;
        return (texture.frames == 1
            && texture.faces == 1
            && texture.depth == 1
            && texture.width > 0
            && texture.height > 0)
            .then(HashMap::new);
    }
    if major != 7 || !(3..=5).contains(&minor) {
        return None;
    }
    let sheets = particle_data::sheets(bytes)?;
    if sheets.is_empty() {
        // Legacy muzzleflash4 is also shipped as VTF 7.4 with no sheet resource.
        // Its PCF requests sequence 2, but the texture is one static image, not
        // an atlas. Permit that bounded static-card fallback after validating
        // the image layout; do not do this for a missing sequence in an atlas.
        let texture = vtf::Vtf::parse(bytes).ok()?;
        if texture.frames != 1
            || texture.faces != 1
            || texture.depth != 1
            || texture.width == 0
            || texture.height == 0
        {
            return None;
        }
    }
    Some(sheets)
}

fn load_style(
    name: &str,
    all: &[Element],
    fs: &vfs::Vfs,
    textures: &mut crate::textures::TextureCache,
    images: &mut Assets<Image>,
    materials: &mut Assets<StandardMaterial>,
    meshes: &mut Assets<Mesh>,
) -> Option<Style> {
    // Authored names such as ..._FP are case-sensitive PCF lookup keys;
    // normalize only the content identity, never reject or rename that key.
    let content_id = content::ContentId::csgo(content::ContentKind::Particle, name);
    let root = all
        .iter()
        .position(|e| e.kind == "DmeParticleSystemDefinition" && e.name == name)?;
    let mut queue = std::collections::VecDeque::from([root]);
    let mut seen = HashSet::new();
    while let Some(index) = queue.pop_front() {
        if seen.len() >= 64 {
            break;
        }
        if !seen.insert(index) {
            continue;
        }
        let Some(e) = all.get(index) else { continue };
        for child in e.refs("children", all) {
            if child.number("delay", 0.0) != 0.0 {
                continue;
            }
            if let Some(Value::Ref(index)) = child.attrs.get("child") {
                if queue.len() < 64 {
                    queue.push_back(*index);
                }
            }
        }
        let Some((radius, life, alpha, rate, color, seq)) = lower(e, all) else {
            continue;
        };
        let material_path = e.text("material").replace('\\', "/");
        let material_path = format!(
            "materials/{}{}",
            material_path,
            if material_path.ends_with(".vmt") {
                ""
            } else {
                ".vmt"
            }
        );
        let Some(text) = fs.read_string(&material_path) else {
            continue;
        };
        let Ok(doc) = kv::parse(&text) else { continue };
        let Some(body) = doc.entries.first().and_then(|(_, v)| v.as_block()) else {
            continue;
        };
        // Unsupported SpriteCard blending stays omitted; never show an entire
        // atlas or substitute a generic bright disc for missing game content.
        if (body.str("$additive") != Some("1") && body.f32("$addself").unwrap_or(0.0) != 1.0)
            || body.f32("$dualsequence").unwrap_or(0.0) != 0.0
            || body.f32("$addoverblend").unwrap_or(0.0) != 0.0
        {
            continue;
        }
        let Some(texture) = body.str("$basetexture") else {
            continue;
        };
        let Some(bytes) = fs.read(&vmt::vtf_path(texture)) else {
            continue;
        };
        let Some(sheets) = sprite_sheets(&bytes) else {
            continue;
        };
        let sheet = sheets.get(&seq).cloned();
        if sheet.is_none() && !sheets.is_empty() {
            continue;
        }
        if sheet.as_ref().is_some_and(|s| s.frames.len() > 64) {
            continue;
        }
        let Some(texture) = textures.load(fs, images, texture, true) else {
            continue;
        };
        let material = materials.add(StandardMaterial {
            base_color_texture: Some(texture.handle),
            base_color: Color::srgba(
                color[0] as f32 / 255.,
                color[1] as f32 / 255.,
                color[2] as f32 / 255.,
                alpha,
            ),
            unlit: true,
            alpha_mode: AlphaMode::Add,
            cull_mode: None,
            ..default()
        });
        let frames = if let Some(s) = &sheet {
            s.frames.iter().map(|uv| meshes.add(quad(*uv))).collect()
        } else {
            vec![meshes.add(quad([0., 0., 1., 1.]))]
        };
        info!(
            "Muzzle effect {content_id}: bounded sprite {} ({radius} inches, {life}s)",
            e.name
        );
        return Some(Style {
            material,
            frames,
            sheet,
            radius,
            life,
            rate,
        });
    }
    debug!("Muzzle effect {name}: no supported brief additive sprite; omitted");
    None
}

#[allow(clippy::too_many_arguments)]
pub fn draw(
    mut commands: Commands,
    mut flashes: ResMut<Flashes>,
    level: Res<LoadedLevel>,
    muzzles: Res<Muzzles>,
    local: Res<LocalPlayer>,
    time: Res<Time>,
    cameras: Query<&Transform, (With<WorldPlayerCamera>, Without<Flash>)>,
    mut live: Query<(Entity, &Flash, &mut Transform, &mut Mesh3d)>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    if !flashes.loaded {
        flashes.loaded = true;
        let fs = &level.0.vfs;
        flashes.names = fs
            .read_string("scripts/items/items_game.txt")
            .map(|t| declarations(&t))
            .unwrap_or_default();
        if let Some(all) = fs
            .read("particles/weapons/cs_weapon_fx.pcf")
            .and_then(|b| particle_data::pcf(&b))
        {
            let names: HashSet<String> = flashes
                .names
                .values()
                .flat_map(|m| m.values().cloned())
                .collect();
            for name in names.into_iter().take(LIMIT) {
                let style = load_style(
                    &name,
                    &all,
                    fs,
                    &mut flashes.textures,
                    &mut images,
                    &mut materials,
                    &mut meshes,
                );
                flashes.styles.insert(name, style);
            }
        }
        let supported = flashes
            .styles
            .values()
            .filter(|style| style.is_some())
            .count();
        info!(
            "Muzzle library: {} weapon declarations, {supported}/{} supported effects",
            flashes.names.len(),
            flashes.styles.len()
        );
        if supported == 0 {
            warn!("Muzzle library has no supported effects; inspect mounted items_game visuals and PCF/VMT/VTF data");
        }
    }
    let pending = std::mem::take(&mut flashes.pending);
    let now = time.elapsed_secs();
    let camera = cameras.single().ok();
    let position = |owner, first| {
        if first {
            muzzles.first.filter(|(id, _)| *id == owner).map(|(_, p)| p)
        } else {
            muzzles.world.get(&owner).copied()
        }
    };
    let mut count = 0;
    for (entity, flash, mut transform, mut mesh) in &mut live {
        let age = now - flash.born;
        let still_first = flash.owner == local.view_target() && !local.noclip;
        if age >= flash.style.life
            || still_first != flash.first
            || camera.is_none()
            || position(flash.owner, flash.first).is_none()
        {
            commands.entity(entity).despawn();
            continue;
        }
        count += 1;
        transform.translation = position(flash.owner, flash.first).unwrap();
        transform.rotation = camera.unwrap().rotation;
        let frame = flash.style.sheet.as_ref().map_or(0, |s| {
            s.frame(age, flash.style.life, flash.style.rate, false)
        });
        mesh.0 = flash.style.frames[frame].clone();
    }
    let Some(camera) = camera else { return };
    for shot in pending {
        if count >= LIMIT {
            break;
        }
        let first = shot.player == local.view_target() && !local.noclip;
        let muzzle = position(shot.player, first);
        let name = flashes
            .names
            .get(&shot.weapon)
            .and_then(|f| effect_name(f, first, shot.suppressed))
            .map(str::to_owned);
        let style = name
            .as_ref()
            .and_then(|name| flashes.styles.get(name))
            .and_then(Option::as_ref)
            .cloned();
        // One receipt per weapon/view/mode/attachment state, bounded independently of Fire rate.
        // The effect's spelling alone does not identify its camera: installed
        // Legacy assigns the unsuffixed rifle effect to both views. Record the
        // actual first-person selection and attachment before deciding to omit.
        if flashes.reported_selections.len() < LIMIT
            && flashes
                .reported_selections
                .insert((shot.weapon.clone(), first, shot.suppressed, muzzle.is_some()))
        {
            info!("Muzzle selection: weapon={} first_person={first} suppressed={} effect={} style={} attachment={}",
                shot.weapon, shot.suppressed, name.as_deref().unwrap_or("<undeclared>"), style.is_some(), muzzle.is_some());
        }
        let (Some(position), Some(name), Some(style)) = (muzzle, name, style) else {
            continue;
        };
        // One card, at the rendered attachment, with the authored lower radius
        // and alpha. No dynamic light, multiplicative burst or invented plume.
        commands.spawn((
            Name::new(format!("muzzle:{name}:{}", shot.serial)),
            Mesh3d(style.frames[0].clone()),
            MeshMaterial3d(style.material.clone()),
            Transform {
                translation: position,
                rotation: camera.rotation,
                scale: Vec3::splat(style.radius),
            },
            bevy::light::NotShadowCaster,
            bevy::light::NotShadowReceiver,
            Flash {
                born: now,
                owner: shot.player,
                first,
                style,
            },
        ));
        count += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Synthetic format fixture, not copied game content: one opaque RGBA pixel.
    fn legacy_sprite() -> Vec<u8> {
        let mut bytes = vec![0; 68];
        bytes[..4].copy_from_slice(b"VTF\0");
        bytes[4..8].copy_from_slice(&7u32.to_le_bytes());
        bytes[8..12].copy_from_slice(&1u32.to_le_bytes());
        bytes[12..16].copy_from_slice(&64u32.to_le_bytes());
        bytes[16..18].copy_from_slice(&1u16.to_le_bytes());
        bytes[18..20].copy_from_slice(&1u16.to_le_bytes());
        bytes[24..26].copy_from_slice(&1u16.to_le_bytes());
        bytes[56] = 1;
        bytes[57..61].copy_from_slice(&(-1i32).to_le_bytes());
        bytes[64..].fill(255);
        bytes
    }

    fn modern_static_sprite() -> Vec<u8> {
        let mut bytes = vec![0; 92];
        bytes[..64].copy_from_slice(&legacy_sprite()[..64]);
        bytes[8..12].copy_from_slice(&4u32.to_le_bytes());
        bytes[12..16].copy_from_slice(&88u32.to_le_bytes());
        bytes[63..65].copy_from_slice(&1u16.to_le_bytes());
        bytes[68..72].copy_from_slice(&1u32.to_le_bytes());
        bytes[80..84].copy_from_slice(&0x30u32.to_le_bytes());
        bytes[84..88].copy_from_slice(&88u32.to_le_bytes());
        bytes[88..].fill(255);
        bytes
    }

    #[test]
    fn legacy_single_sprite_is_not_mistaken_for_a_resource_directory() {
        let mut bytes = legacy_sprite();
        assert!(sprite_sheets(&bytes).unwrap().is_empty());
        assert!(sprite_sheets(&bytes[..64]).is_none()); // truncated pixel data
        bytes[8..12].copy_from_slice(&3u32.to_le_bytes());
        assert!(sprite_sheets(&bytes).is_none()); // no directory; not a legacy fallback
        bytes[8..12].copy_from_slice(&1u32.to_le_bytes());
        bytes[24..26].copy_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&[255; 4]);
        assert!(sprite_sheets(&bytes).is_none()); // unsupported multi-frame image
    }

    #[test]
    fn first_person_child_chain_and_static_sequence_produce_a_renderable_style() {
        let names = declarations(
            r#""items_game" {
            "prefabs" { "gun" { "visuals" {
                "muzzle_flash_effect_1st_person" "test_flash_FP"
            } } }
            "items" { "1" { "name" "weapon_test" "baseitem" "1" "prefab" "gun" } }
        }"#,
        );
        let name = effect_name(&names["weapon_test"], true, false).unwrap();
        let definition = Element {
            kind: "DmeParticleSystemDefinition".into(),
            name: "test_vent_FP".into(),
            attrs: HashMap::from([
                ("material".into(), Value::Text("test/sprite.vmt".into())),
                (
                    "initializers".into(),
                    Value::Array(vec![Value::Ref(1), Value::Ref(4), Value::Ref(5)]),
                ),
                ("renderers".into(), Value::Array(vec![Value::Ref(2)])),
                ("emitters".into(), Value::Array(vec![Value::Ref(3)])),
            ]),
        };
        let operator = |name: &str| Element {
            attrs: HashMap::from([("functionname".into(), Value::Text(name.into()))]),
            ..Default::default()
        };
        let mut life = operator("Lifetime Random");
        life.attrs
            .insert("lifetime_min".into(), Value::Number(0.028));
        let mut radius = operator("Remap Noise to Scalar");
        radius
            .attrs
            .insert("output minimum".into(), Value::Number(3.0));
        let mut sequence = operator("Sequence Random");
        sequence
            .attrs
            .insert("sequence_min".into(), Value::Number(2.0));
        let root = Element {
            kind: "DmeParticleSystemDefinition".into(),
            name: name.into(),
            attrs: HashMap::from([("children".into(), Value::Array(vec![Value::Ref(7)]))]),
        };
        let child = Element {
            kind: "DmeParticleChild".into(),
            name: "test_vent_FP".into(),
            attrs: HashMap::from([("child".into(), Value::Ref(0))]),
        };
        let elements = [
            definition,
            life,
            operator("render_animated_sprites"),
            operator("emit_instantaneously"),
            radius,
            sequence,
            root,
            child,
        ];
        let mut fs = vfs::Vfs::new();
        fs.push_back(
            "synthetic muzzle fixture",
            vfs::Mount::Memory(std::collections::BTreeMap::from([
                (
                    "materials/test/sprite.vmt".into(),
                    std::sync::Arc::<[u8]>::from(
                        &br#""SpriteCard" { "$basetexture" "test/sprite" "$additive" "1" }"#[..],
                    ),
                ),
                (
                    "materials/test/sprite.vtf".into(),
                    std::sync::Arc::<[u8]>::from(modern_static_sprite()),
                ),
            ])),
        );
        let mut textures = crate::textures::TextureCache::default();
        let mut images = Assets::<Image>::default();
        let mut materials = Assets::<StandardMaterial>::default();
        let mut meshes = Assets::<Mesh>::default();
        let style = load_style(
            name,
            &elements,
            &fs,
            &mut textures,
            &mut images,
            &mut materials,
            &mut meshes,
        )
        .expect(
            "first-person child graph and a nonzero sequence on a static image must reach the sprite renderer",
        );
        assert_eq!(style.frames.len(), 1);
        assert!(style.sheet.is_none());
        assert_eq!(style.radius, 3.0);
        assert_eq!(style.life, 0.028);
        assert!(meshes.get(&style.frames[0]).is_some());
        let texture = materials
            .get(&style.material)
            .unwrap()
            .base_color_texture
            .as_ref()
            .unwrap();
        assert!(images.get(texture).is_some());
    }

    /// Opt-in, read-only asset check for the parent verification slot. No game
    /// process or renderer is created; validates the real Rust parser/loader.
    #[test]
    #[ignore = "requires the player's installed CS:GO Legacy assets"]
    fn installed_first_person_rifles_and_explicit_fp_child_have_styles() {
        let dir = std::env::var_os("CSR_GAME_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::PathBuf::from(
                    r"C:\Program Files (x86)\Steam\steamapps\common\csgo legacy\csgo",
                )
            });
        let mut fs = vfs::Vfs::new();
        fs.mount_game_dir(&dir).unwrap();
        let names = declarations(&fs.read_string("scripts/items/items_game.txt").unwrap());
        let all =
            particle_data::pcf(&fs.read("particles/weapons/cs_weapon_fx.pcf").unwrap()).unwrap();
        let mut textures = crate::textures::TextureCache::default();
        let mut images = Assets::<Image>::default();
        let mut materials = Assets::<StandardMaterial>::default();
        let mut meshes = Assets::<Mesh>::default();
        let mut effects = vec!["weapon_muzzle_flash_assaultrifle_FP"];
        for weapon in ["weapon_ak47", "weapon_m4a1", "weapon_aug"] {
            effects
                .push(effect_name(&names[weapon], true, false).expect("first-person declaration"));
        }
        for effect in effects {
            let style = load_style(
                effect,
                &all,
                &fs,
                &mut textures,
                &mut images,
                &mut materials,
                &mut meshes,
            )
            .unwrap_or_else(|| {
                panic!("first-person rifle effect {effect} must have a supported sprite")
            });
            assert!(!style.frames.is_empty());
            assert!(style.radius > 0.0 && style.radius <= 7.0);
            assert!(style.life > 0.0 && style.life <= 0.03);
        }
    }

    #[test]
    fn actual_fire_receipts_keep_serials_and_bound_the_render_queue() {
        use bevy::ecs::system::RunSystemOnce;
        let mut app = App::new();
        app.init_resource::<Flashes>()
            .insert_resource(PendingEvents(
                (0..LIMIT + 2)
                    .map(|_| sim::GameEvent::Fire {
                        player: 1,
                        weapon: "weapon_test".into(),
                        origin: glam::Vec3::ZERO,
                        suppressed: true,
                    })
                    .collect(),
            ));
        app.world_mut().run_system_once(capture).unwrap();
        let flashes = app.world().resource::<Flashes>();
        assert_eq!(flashes.serial, (LIMIT + 2) as u64);
        assert_eq!(flashes.pending.len(), LIMIT);
        assert!(flashes
            .pending
            .windows(2)
            .all(|p| p[1].serial == p[0].serial + 1));
        assert!(flashes.pending.iter().all(|p| p.suppressed));
        // Effects drains this same authority batch before the next capture.
        app.world_mut().resource_mut::<PendingEvents>().0.clear();
        app.world_mut().run_system_once(capture).unwrap();
        assert!(app.world().resource::<Flashes>().pending.is_empty());
    }

    #[test]
    fn prefab_modes_preserve_explicit_empty_and_missing_suppressed_effects() {
        let names = declarations(
            r#""items_game" {
            "prefabs" { "base" { "visuals" { "muzzle_flash_effect_1st_person" "loud"
                "muzzle_flash_effect_3rd_person" "world"
                "muzzle_flash_effect_1st_person_alt" "quiet" } }
                "derived" { "prefab" "base" "visuals" { "muzzle_flash_effect_3rd_person_alt" "" } } }
            "items" { "1" { "name" "weapon_test" "baseitem" "1" "prefab" "derived"
                "visuals" { "muzzle_flash_effect_1st_person" "item_loud" } } }
        }"#,
        );
        let f = &names["weapon_test"];
        assert_eq!(effect_name(f, true, false), Some("item_loud"));
        assert_eq!(effect_name(f, true, true), Some("quiet"));
        assert_eq!(effect_name(f, false, false), Some("world"));
        assert_eq!(effect_name(f, false, true), None);
        let f = HashMap::from([("muzzle_flash_effect_1st_person".into(), "loud".into())]);
        assert_eq!(effect_name(&f, true, true), None);
    }
}
