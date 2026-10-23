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
        for (key, value) in &b.entries {
            if key.to_ascii_lowercase().starts_with("muzzle_flash_effect_") {
                if let Some(value) = value.as_str() {
                    out.insert(key.to_ascii_lowercase(), value.into());
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

fn load_style(
    name: &str,
    all: &[Element],
    fs: &vfs::Vfs,
    textures: &mut crate::textures::TextureCache,
    images: &mut Assets<Image>,
    materials: &mut Assets<StandardMaterial>,
    meshes: &mut Assets<Mesh>,
) -> Option<Style> {
    let content_id: content::ContentId = format!("csgo_particle_{name}").parse().ok()?;
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
        let Some(sheets) = particle_data::sheets(&bytes) else {
            continue;
        };
        let sheet = sheets.get(&seq).cloned();
        if !sheets.is_empty() && sheet.is_none() {
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
        let Some(position) = position(shot.player, first) else {
            continue;
        };
        let Some(name) = flashes
            .names
            .get(&shot.weapon)
            .and_then(|f| effect_name(f, first, shot.suppressed))
        else {
            continue;
        };
        let Some(style) = flashes.styles.get(name).and_then(Option::as_ref).cloned() else {
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
            "prefabs" { "base" { "muzzle_flash_effect_1st_person" "loud"
                "muzzle_flash_effect_3rd_person" "world"
                "muzzle_flash_effect_1st_person_alt" "quiet" }
                "derived" { "prefab" "base" "muzzle_flash_effect_3rd_person_alt" "" } }
            "items" { "1" { "name" "weapon_test" "baseitem" "1" "prefab" "derived" } }
        }"#,
        );
        let f = &names["weapon_test"];
        assert_eq!(effect_name(f, true, false), Some("loud"));
        assert_eq!(effect_name(f, true, true), Some("quiet"));
        assert_eq!(effect_name(f, false, false), Some("world"));
        assert_eq!(effect_name(f, false, true), None);
        let f = HashMap::from([("muzzle_flash_effect_1st_person".into(), "loud".into())]);
        assert_eq!(effect_name(&f, true, true), None);
    }
}
