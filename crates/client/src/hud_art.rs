//! HUD images and the installed map overview. Never exposes unspotted enemies.
use bevy::prelude::*;
use bevy::text::LineBreak;
use crate::{ui_assets::UiAssets, player::{LocalPlayer, SimWorld}};
use crate::hud_layout::{self as layout, unit as hu, font as hf};

#[derive(Component)]
pub struct ArmorIcon;
#[derive(Component)]
pub struct BuyIcon;
#[derive(Component)]
pub struct RadarDot(sim::PlayerId);
#[derive(Component)]
pub(crate) enum RadarLayout { Viewport, Money, Buy }
#[derive(Resource, Default)]
pub struct Radar { pub(crate) origin: Vec2, pub(crate) scale: f32, material: Option<Handle<crate::radar_view::RadarMaterial>> }
#[derive(Resource, Default)]
pub struct Inventory { signature: String, root: Option<Entity> }
#[derive(Component)]
struct InventorySlot(u8);
#[derive(Component)]
struct SelectionHint(u8);
#[derive(Component)]
struct ActiveWeaponName;

fn icon(commands: &mut Commands, handle: Handle<Image>, node: Node) -> Entity {
    commands.spawn((ImageNode::new(handle), node)).id()
}

pub fn setup(mut commands: Commands, art: Res<UiAssets>, level: Res<crate::LoadedLevel>, world: Res<SimWorld>, mut images: ResMut<Assets<Image>>, mut materials: ResMut<Assets<crate::radar_view::RadarMaterial>>) {
    if std::env::var_os("CSR_NO_HUD").is_some() {
        commands.init_resource::<Radar>();
        commands.init_resource::<Inventory>();
        return;
    }
    for (key, x) in [("health", 24.0), ("armor", 178.0)] {
        if let Some(image) = art.image(key) {
            let entity = icon(&mut commands, image, Node { position_type: PositionType::Absolute, left: layout::legacy(x), bottom: layout::legacy(24.0), width: layout::legacy(27.0), height: layout::legacy(27.0), ..default() });
            commands.entity(entity).insert(crate::combat_hud::Fallback::Health);
            if key == "armor" { commands.entity(entity).insert(ArmorIcon); }
        }
        commands.spawn((crate::combat_hud::Fallback::Health, Node { position_type: PositionType::Absolute, left: layout::legacy(x), bottom: layout::legacy(12.0), width: layout::legacy(118.0), height: layout::legacy(2.0), ..default() }, BackgroundColor(Color::srgba(0.9, 0.9, 0.88, 0.7))));
    }
    if let Some(image) = art.image("cart") {
        let entity = icon(&mut commands, image, Node { position_type: PositionType::Absolute, width: hu(22.0), height: hu(22.0), ..default() });
        commands.entity(entity).insert((BuyIcon, RadarLayout::Buy));
    }
    // The unmasked overview is sampled into a responsive circular viewport. Image
    // and ally markers use the same player-centered interpolated transform.
    let mut radar = Radar::default();
    if let Some(doc) = level.0.overview().and_then(|text| kv::parse(&text).ok()) {
        if let Some(root) = doc.iter().find_map(|(_, v)| v.as_block()) {
            let number = |k| root.str(k).and_then(kv::parse_f32).unwrap_or(0.0);
            radar.origin = Vec2::new(number("pos_x"), number("pos_y"));
            radar.scale = number("scale");
            let material = root.str("material").unwrap_or("");
            if let Some(img) = level.0.vfs.read(&format!("resource/overviews/{}_radar.dds", level.0.name)).or_else(|| level.0.vfs.read(&format!("resource/{material}_radar.dds"))).and_then(|bytes| radar_image(&bytes)) {
                let material = materials.add(crate::radar_view::RadarMaterial {
                    overview: images.add(img),
                    view: crate::radar_view::view(Vec2::splat(0.5), 0.5, false, 0.0),
                });
                commands.spawn((RadarLayout::Viewport, MaterialNode(material.clone()), Node { position_type: PositionType::Absolute, ..default() }));
                radar.material = Some(material);
            } else { warn!("Radar image not available: {material}"); }
        }
    }
    for p in &world.0.players {
        if let Some(image) = art.image(if p.team == sim::Team::CounterTerrorist { "radar-ct" } else { "radar-t" }) {
            let entity = icon(&mut commands, image, Node { position_type: PositionType::Absolute, width: hu(8.0), height: hu(8.0), ..default() });
            commands.entity(entity).insert((RadarDot(p.id), Visibility::Hidden, GlobalZIndex(1)));
        }
    }
    commands.insert_resource(radar);
    commands.init_resource::<Inventory>();
}

pub(crate) fn layout(settings: Res<crate::settings::Settings>, mut nodes: Query<(&RadarLayout, &mut Node)>) {
    let diameter = layout::RADAR_DIAMETER * settings.radar_hud_size;
    for (part, mut node) in &mut nodes {
        let (left, top) = match part {
            RadarLayout::Viewport => (layout::RADAR_LEFT, layout::RADAR_TOP),
            RadarLayout::Money => (60.0, layout::RADAR_TOP + diameter + 32.0),
            RadarLayout::Buy => (24.0, layout::RADAR_TOP + diameter + 32.0),
        };
        if node.left != hu(left) { node.left = hu(left); }
        if node.top != hu(top) { node.top = hu(top); }
        if matches!(part, RadarLayout::Viewport) {
            if node.width != hu(diameter) { node.width = hu(diameter); }
            if node.height != hu(diameter) { node.height = hu(diameter); }
        }
    }
}

/// Group by Source selection slot, preserving inventory order within a slot.
/// Utility shares one row and label; a rebound label cannot move icon geometry.
fn inventory_slots(slots: impl IntoIterator<Item = u8>) -> [Vec<usize>; 5] {
    let mut groups: [Vec<usize>; 5] = std::array::from_fn(|_| vec![]);
    for (index, slot) in slots.into_iter().enumerate() {
        if let Some(group) = groups.get_mut(slot.wrapping_sub(1) as usize) { group.push(index); }
    }
    groups
}

fn radar_image(bytes: &[u8]) -> Option<Image> {
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    if bytes.get(..4)? != b"DDS " { return None; }
    let u32_at = |at| Some(u32::from_le_bytes(bytes.get(at..at+4)?.try_into().ok()?));
    let (w, h) = (u32_at(16)? as usize, u32_at(12)? as usize);
    if w == 0 || h == 0 || w > 4096 || h > 4096 { return None; }
    let format = match bytes.get(84..88)? { b"DXT1" => vtf::Format::Dxt1, b"DXT3" => vtf::Format::Dxt3, b"DXT5" => vtf::Format::Dxt5, _ => return None };
    let rgba = vtf::decode_rgba8(format, bytes.get(128..)?, w, h)?;
    Some(Image::new(Extent3d { width: w as u32, height: h as u32, depth_or_array_layers: 1 }, TextureDimension::D2, rgba, TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::RENDER_WORLD))
}

#[allow(clippy::too_many_arguments)]
pub fn update(mut commands: Commands, art: Res<UiAssets>, world: Res<SimWorld>, local: Res<LocalPlayer>, radar: Res<Radar>, poses: Res<crate::interpolation::RenderPoses>, time: Res<Time<Fixed>>, settings: Res<crate::settings::Settings>, loc: Res<crate::localize::Localization>, mut materials: ResMut<Assets<crate::radar_view::RadarMaterial>>, mut state: ResMut<Inventory>, mut armor: Query<&mut ImageNode, With<ArmorIcon>>, mut carts: Query<&mut Visibility, (With<BuyIcon>, Without<RadarDot>)>, mut dots: Query<(Entity, &RadarDot, &mut Node, &mut Visibility), Without<BuyIcon>>) {
    let _frame_scope = crate::input_latency::frame_scope("hud_art");
    let w = &world.0;
    let Some(p) = w.player(local.view_target()) else { return };
    let to_uv = |pos: glam::Vec3| Vec2::new(pos.x-radar.origin.x, radar.origin.y-pos.y) / (1024.0 * radar.scale);
    let camera = poses.sample(p, time.overstep_fraction());
    // Local mouse look is current even between simulation ticks, matching the
    // camera's update_camera branch. Spectated players retain sampled angles.
    let yaw = if local.spectating.is_some() && !local.noclip { camera.angles.y } else { local.yaw };
    let map_view = crate::radar_view::view(to_uv(camera.origin), settings.radar_scale, settings.radar_rotate, yaw);
    if radar.scale > 0.0 {
        if let Some(mut material) = radar.material.as_ref().and_then(|handle| materials.get_mut(handle)) {
            material.view = map_view;
        }
    }
    for mut image in &mut armor { if let Some(handle) = art.image(if p.helmet { "helmet" } else { "armor" }) { image.image = handle; } }
    for mut visibility in &mut carts { *visibility = if p.alive && p.in_buy_zone && w.competitive() && w.round.can_buy(&w.rules, w.time()) { Visibility::Inherited } else { Visibility::Hidden }; }
    let present: std::collections::HashSet<_> = dots.iter().map(|(_, dot, _, _)| dot.0).collect();
    for player in w.players.iter().filter(|player| !present.contains(&player.id)) {
        if let Some(image) = art.image(if player.team == sim::Team::CounterTerrorist { "radar-ct" } else { "radar-t" }) {
            let entity = icon(&mut commands, image, Node { position_type: PositionType::Absolute, width: hu(8.0), height: hu(8.0), ..default() });
            commands.entity(entity).insert((RadarDot(player.id), Visibility::Hidden, GlobalZIndex(1)));
        }
    }
    for (entity, dot, mut node, mut visibility) in &mut dots {
        *visibility = Visibility::Hidden;
        if w.player(dot.0).is_none() { commands.entity(entity).despawn(); continue; }
        let Some(teammate) = w.player(dot.0).filter(|t| t.team == p.team && t.alive) else { continue };
        if radar.scale <= 0.0 { continue; }
        let pos = poses.sample(teammate, time.overstep_fraction()).origin;
        let offset = crate::radar_view::marker_offset(map_view, to_uv(pos));
        let diameter = layout::RADAR_DIAMETER * settings.radar_hud_size;
        let offset = offset * (diameter / crate::radar_view::DIAMETER);
        if offset.length() > diameter / 2.0 - 4.0 { continue; }
        node.left = hu(layout::RADAR_LEFT + diameter / 2.0 + offset.x - 4.0);
        node.top = hu(layout::RADAR_TOP + diameter / 2.0 + offset.y - 4.0);
        *visibility = Visibility::Inherited;
    }
    let slot_labels: [String; 5] = std::array::from_fn(|i| settings.selection_binding_label(i as u8 + 1));
    let signature = format!("{:?}|{}|{}|{:?}|{:?}|{:?}", p.id, p.alive, p.active, p.weapons.iter().map(|wp| &wp.def.class_name).collect::<Vec<_>>(), w.players.iter().map(|p| (p.id, p.team, p.alive)).collect::<Vec<_>>(), slot_labels);
    if signature == state.signature { return; }
    state.signature = signature;
    if let Some(root) = state.root.take() { commands.entity(root).despawn(); }
    state.root = Some(commands.spawn((crate::hud::GameHud,Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), ..default() })).with_children(|root| {
        if p.alive {
            let groups = inventory_slots(p.weapons.iter().map(|weapon| sim::weapon::slot(&weapon.def)));
            root.spawn(Node { position_type: PositionType::Absolute, bottom: hu(layout::INVENTORY_BOTTOM), right: hu(24.0), flex_direction: FlexDirection::Column, align_items: AlignItems::End, ..default() }).with_children(|inventory| {
                for (slot, indices) in groups.iter().enumerate().filter(|(_, indices)| !indices.is_empty()) {
                    inventory.spawn((InventorySlot(slot as u8 + 1), Node { position_type: PositionType::Relative, width: hu(layout::INVENTORY_ICON_WIDTH), height: hu(layout::INVENTORY_PITCH), flex_shrink: 0.0, ..default() })).with_children(|row| {
                        // Reserve a stable icon plane. Key labels extend LEFT
                        // from the right edge above it, independently of width.
                        row.spawn(Node { position_type: PositionType::Absolute, right: hu(0.0), top: hu(0.0), height: hu(layout::INVENTORY_ICON_HEIGHT), align_items: AlignItems::Center, column_gap: hu(4.0), ..default() }).with_children(|icons| {
                            for &index in indices {
                                let weapon = &p.weapons[index];
                                let key = weapon.def.class_name.trim_start_matches("weapon_");
                                let color = if index == p.active { Color::WHITE } else { Color::srgba(0.8, 0.82, 0.82, 0.5) };
                                let width = if slot == 3 { layout::INVENTORY_ICON_HEIGHT } else { layout::INVENTORY_ICON_WIDTH };
                                if let Some(image) = art.image(key).or_else(|| art.image("knife_default_ct").filter(|_| key.contains("knife"))) {
                                    icons.spawn((ImageNode { color, ..ImageNode::new(image) }, Node { width: hu(width), height: hu(layout::INVENTORY_ICON_HEIGHT), flex_shrink: 0.0, ..default() }));
                                } else {
                                    icons.spawn((Text::new(loc.weapon_name(&weapon.def)), TextColor(color), TextFont { font: bevy::text::FontSource::Handle(art.regular.clone()), font_size: hf(12.0), ..default() }));
                                }
                            }
                        });
                        row.spawn((SelectionHint(slot as u8 + 1), Text::new(slot_labels[slot].clone()), TextLayout::new(Justify::Right, LineBreak::NoWrap), TextColor(Color::WHITE), TextFont { font_size: hf(12.0), ..default() }, Node { position_type: PositionType::Absolute, right: hu(-18.0), top: hu(layout::INVENTORY_HINT_TOP), ..default() }));
                        if indices.contains(&p.active) {
                            let weapon = &p.weapons[p.active];
                            // Only descendants inherit overflow clipping. Keep
                            // the full name on one line, extending left, inside
                            // the vertical band before the next selection hint.
                            row.spawn((ActiveWeaponName, Node { position_type: PositionType::Absolute, right: hu(0.0), top: hu(layout::INVENTORY_ICON_HEIGHT), width: hu(layout::INVENTORY_ICON_WIDTH), height: hu(layout::INVENTORY_PITCH + layout::INVENTORY_HINT_TOP - layout::INVENTORY_ICON_HEIGHT), overflow: Overflow::clip_y(), ..default() })).with_children(|name| {
                                name.spawn((Text::new(loc.weapon_name(&weapon.def)), TextLayout::new(Justify::Right, LineBreak::NoWrap), TextColor(Color::WHITE), TextFont { font_size: hf(12.0), ..default() }, Node { position_type: PositionType::Absolute, right: hu(0.0), top: hu(0.0), ..default() }));
                            });
                        }
                    });
                }
            });
        }
        for (side, left) in [(sim::Team::CounterTerrorist, true), (sim::Team::Terrorist, false)] {
            let inset = hu(layout::CENTER_WIDTH / 2.0 + layout::CENTER_GAP);
            root.spawn(Node { position_type: PositionType::Absolute, top: hu(0.0), left: if left { Val::Auto } else { percent(50) }, right: if left { percent(50) } else { Val::Auto }, margin: if left { UiRect::right(inset) } else { UiRect::left(inset) }, column_gap: hu(layout::PLAYER_GAP), ..default() }).with_children(|team| {
                for player in w.players.iter().filter(|p| p.team == side) {
                    if let Some(image) = art.image("avatar") {
                        team.spawn((ImageNode { color: if player.alive { Color::WHITE } else { Color::srgba(0.3,0.3,0.3,0.5) }, ..ImageNode::new(image) }, Node { width: hu(layout::PLAYER_WIDTH), height: hu(layout::PLAYER_HEIGHT), flex_shrink: 0.0, border: UiRect::bottom(hu(2.0)), ..default() }, BackgroundColor(if left { Color::srgb(0.23,0.36,0.48) } else { Color::srgb(0.46,0.39,0.24) })));
                    }
                }
            });
        }
    }).id());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utility_shares_one_selection_row_without_reordering_weapons() {
        let rows = inventory_slots([2, 1, 3, 4, 4, 4, 4, 4, 5]);
        assert_eq!(rows, [vec![1], vec![0], vec![2], vec![3, 4, 5, 6, 7], vec![8]]);
        assert_eq!(rows.iter().filter(|row| !row.is_empty()).count(), 5);
        assert_eq!(inventory_slots([2, 3, 4]).iter().filter(|row| !row.is_empty()).count(), 3);
        assert!(inventory_slots([0, 6]).iter().all(Vec::is_empty));
    }

    #[test]
    fn real_inventory_system_preserves_hint_policy_and_player_props() {
        use crate::settings::{MouseAction, Settings, KEY_SELECTION_FIRST};
        use std::sync::Arc;
        let catalog = weapons::Catalog::from_items_game(r#"items_game { items {
            9101 { name weapon_glock baseitem 1 }
            9102 { name weapon_knife baseitem 1 }
            9103 { name weapon_hegrenade baseitem 1 }
            9104 { name weapon_flashbang baseitem 1 }
        } }"#).unwrap();
        let mut world = sim::World::new(Arc::new(collision::CollisionWorld::new()));
        let id = world.add_player("local fixture", sim::Team::CounterTerrorist);
        world.add_player("other fixture", sim::Team::Terrorist);
        world.players[0].alive = true;
        world.players[0].weapons = ["weapon_glock", "weapon_knife", "weapon_hegrenade", "weapon_flashbang"]
            .map(|name| sim::weapon::WeaponState::new(Arc::new(catalog.get(name).unwrap().clone()))).to_vec();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(SimWorld(world))
            .insert_resource(LocalPlayer::new(id, glam::Vec3::ZERO, 0.0))
            .insert_resource(UiAssets { regular: default(), bold: default(), images: [
                ("glock".into(), default()), ("knife".into(), default()),
                ("hegrenade".into(), default()), ("flashbang".into(), default()),
                ("avatar".into(), default()),
            ].into() })
            .init_resource::<Radar>()
            .init_resource::<Inventory>()
            .init_resource::<crate::interpolation::RenderPoses>()
            .init_resource::<crate::localize::Localization>()
            .init_resource::<Settings>()
            .init_resource::<Assets<crate::radar_view::RadarMaterial>>()
            .add_systems(Update, update);
        let inspect = |app: &mut App| {
            let world = app.world_mut();
            let mut rows = world.query::<(&InventorySlot, &Node)>();
            let mut geometry: Vec<_> = rows.iter(world).map(|(slot, node)| (slot.0, node.width, node.height)).collect();
            geometry.sort_by_key(|row| row.0);
            // Policy regression only: MinimalPlugins does not shape glyphs,
            // compute UI bounds or prove font fit. Root's render gate does.
            let mut hints = world.query::<(&SelectionHint, &Text, &TextLayout, &Node)>();
            let mut labels: Vec<_> = hints.iter(world).map(|(slot, text, text_layout, node)| {
                assert_eq!(text_layout.linebreak, LineBreak::NoWrap);
                assert_eq!(text_layout.justify, Justify::Right);
                assert_eq!((node.left, node.width), (Val::Auto, Val::Auto));
                assert_eq!((node.right, node.top), (hu(-18.0), hu(layout::INVENTORY_HINT_TOP)));
                (slot.0, text.0.clone())
            }).collect();
            labels.sort_by_key(|row| row.0);
            let mut names = world.query_filtered::<(&Node, &Children), With<ActiveWeaponName>>();
            assert_eq!(names.iter(world).count(), 1);
            for (band, children) in names.iter(world) {
                assert_eq!(band.overflow, Overflow::clip_y());
                assert_eq!(band.top, hu(layout::INVENTORY_ICON_HEIGHT));
                assert_eq!(band.height, hu(layout::INVENTORY_PITCH + layout::INVENTORY_HINT_TOP - layout::INVENTORY_ICON_HEIGHT));
                assert_eq!(children.len(), 1);
                for &child in children.iter() {
                    let text_layout = world.get::<TextLayout>(child).unwrap();
                    assert_eq!(text_layout.linebreak, LineBreak::NoWrap);
                    assert_eq!(text_layout.justify, Justify::Right);
                    assert_eq!(world.get::<Text>(child).unwrap().0, "GLOCK");
                }
            }
            (geometry, labels)
        };
        app.update();
        let initial_root = app.world().resource::<Inventory>().root.unwrap();
        let (geometry, labels) = inspect(&mut app);
        assert_eq!(labels, [(2, String::from("2")), (3, String::from("3")), (4, String::from("4"))]);
        app.update();
        assert_eq!(app.world().resource::<Inventory>().root, Some(initial_root), "unchanged HUD must retain its tree");
        {
            let mut settings = app.world_mut().resource_mut::<Settings>();
            settings.rebind(KEY_SELECTION_FIRST + 2, KeyCode::KeyK);
            settings.rebind_mouse(KEY_SELECTION_FIRST + 2, MouseButton::Back);
        }
        app.update();
        let (rebound_geometry, labels) = inspect(&mut app);
        assert_eq!(labels[0], (2, String::from("K / Mouse 4")));
        assert_eq!(rebound_geometry, geometry, "rebinding must retain authored slot allocations");
        {
            let mut settings = app.world_mut().resource_mut::<Settings>();
            settings.rebind(KEY_SELECTION_FIRST + 2, KeyCode::ControlRight);
            settings.mouse_binds.fill(MouseAction::Action(KEY_SELECTION_FIRST + 2));
        }
        // Exercise the persisted model too, retaining every accepted alternative.
        let saved = Settings::parse(&app.world().resource::<Settings>().encode());
        app.insert_resource(saved);
        app.update();
        let (long_geometry, labels) = inspect(&mut app);
        assert_eq!(labels[0], (2, String::from("Control R / Mouse 1 / Mouse 2 / Mouse 3 / Mouse 4 / Mouse 5")));
        assert_eq!(long_geometry, geometry);
        app.world_mut().resource_mut::<Settings>().clear_action_binding(KEY_SELECTION_FIRST + 2);
        app.update();
        assert_eq!(inspect(&mut app).1[0], (2, String::new()));

        // Preserve production team/death properties in both modes while their
        // avatar ink/allocation geometry remains an explicit visual gate.
        for mode in [sim::GameMode::Deathmatch, sim::GameMode::Competitive] {
            {
                let mut world = app.world_mut().resource_mut::<SimWorld>();
                world.0.mode = mode;
                world.0.players[1].team = sim::Team::Terrorist;
                world.0.players[1].alive = false;
            }
            app.update();
            let mut tiles = app.world_mut().query::<(&ImageNode, &BackgroundColor)>();
            let tiles: Vec<_> = tiles.iter(app.world()).map(|(image, background)| (image.color, background.0)).collect();
            assert_eq!(tiles.len(), 2);
            assert!(tiles.contains(&(Color::WHITE, Color::srgb(0.23,0.36,0.48))));
            assert!(tiles.contains(&(Color::srgba(0.3,0.3,0.3,0.5), Color::srgb(0.46,0.39,0.24))));
            let previous_root = app.world().resource::<Inventory>().root;
            {
                let mut world = app.world_mut().resource_mut::<SimWorld>();
                world.0.players[1].team = sim::Team::CounterTerrorist;
                world.0.players[1].alive = true;
            }
            app.update();
            assert_ne!(app.world().resource::<Inventory>().root, previous_root);
            let mut tiles = app.world_mut().query::<(&ImageNode, &BackgroundColor)>();
            assert_eq!(tiles.iter(app.world()).count(), 2);
            assert!(tiles.iter(app.world()).all(|(image, background)| image.color == Color::WHITE && background.0 == Color::srgb(0.23,0.36,0.48)));
        }
    }
}
