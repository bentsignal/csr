//! Bounded impact/blood effects and damage feedback from simulation events.
use crate::{
    coords::{rotation_to_bevy, to_bevy},
    effects::Effects,
    player::{LocalPlayer, SimWorld},
};
use bevy::prelude::*;
use std::collections::HashSet;

#[path = "impact_puffs.rs"]
mod impact_puffs;

const MAX_FEEDBACK_FX: usize = 512;

#[derive(Resource)]
pub struct FeedbackAssets {
    disc: Handle<Mesh>,
    quad: Handle<Mesh>,
    blood: Handle<StandardMaterial>,
    hole: Handle<StandardMaterial>,
    dust: Handle<StandardMaterial>,
    decals: crate::impact_decals::Catalog,
    puffs: impact_puffs::Catalog,
}
#[derive(Component)]
pub struct ImpactFx {
    born: f32,
    life: f32,
    velocity: Vec3,
    billboard: bool,
}
#[derive(Component)]
pub struct ImpactArtwork(pub content::ContentId);

/// A map-owned cosmetic contact, independent of the firing player's lifetime.
/// No parent/skeleton lookup or render-frame collision query selects its owner.
#[derive(Component)]
pub(crate) struct DoorImpactMark {
    map: content::ContentId,
    impact: sim::doors::Impact,
    surface: u32,
}

impl DoorImpactMark {
    fn pose(&self, map: &content::ContentId, collision: &collision::CollisionWorld,
        states: &[sim::doors::State]) -> Option<Transform> {
        if self.map != *map || !self.impact.valid() { return None; }
        let def = collision.doors.get(self.impact.door as usize)?;
        let state = states.get(self.impact.door as usize)?;
        if self.surface != def.surface || self.impact.pos.length() > def.radius + 0.25 { return None; }
        let rotation = def.rotation(state.angle);
        let normal = self.impact.normal.normalize_or_zero();
        Some(Transform::from_translation(to_bevy(def.origin + rotation * (self.impact.pos + normal * 0.12)))
            .with_rotation(rotation_to_bevy(rotation) * Quat::from_rotation_arc(Vec3::Z, to_bevy(normal))))
    }
}

/// Several fixed ticks may accumulate before one render. Contacts before the
/// latest round reset belong to the previous world, even if processed together.
fn reset_feedback(events: &mut Vec<sim::GameEvent>) -> bool {
    let Some(reset) = events.iter().rposition(|event| matches!(event, sim::GameEvent::RoundStart { .. }))
        else { return false; };
    events.drain(..reset);
    true
}

#[derive(Component)]
pub struct HurtEdge(pub u8);
#[derive(Resource, Default)]
pub struct Hurt {
    time: Option<f32>,
    direction: glam::Vec3,
    amount: f32,
}

/// A new impact mark has priority over an older cosmetic effect. Keep the
/// existing global entity bound; deferred despawns must not choose one victim
/// twice when several impacts arrive in the same render frame.
fn reserve_impact_slot(
    commands: &mut Commands,
    budget: &mut usize,
    retired: &mut HashSet<Entity>,
    candidates: impl Iterator<Item = (Entity, f32)>,
) -> bool {
    if *budget > 0 {
        *budget -= 1;
        return true;
    }
    let oldest = candidates.filter(|(entity, _)| !retired.contains(entity))
        .min_by(|a, b| a.1.total_cmp(&b.1));
    if let Some((entity, _)) = oldest {
        commands.entity(entity).despawn();
        retired.insert(entity);
        true
    } else {
        false
    }
}

pub fn setup(
    mut commands: Commands,
    local: Res<LocalPlayer>,
    level: Res<crate::LoadedLevel>,
    time: Res<Time>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let blood = level
        .0
        .vfs
        .read("materials/decals/blood1.vtf")
        .and_then(|bytes| vtf::Vtf::parse(&bytes).ok())
        .and_then(|v| {
            let mut rgba = v.rgba8(0, 0, 0)?;
            for pixel in rgba.chunks_exact_mut(4) {
                multiply_decal_to_alpha(pixel);
            }
            use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
            let mut image = Image::new(
                Extent3d {
                    width: v.width as u32,
                    height: v.height as u32,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                rgba,
                TextureFormat::Rgba8UnormSrgb,
                bevy::asset::RenderAssetUsages::RENDER_WORLD,
            );
            image.sampler = bevy::image::ImageSampler::linear();
            Some(images.add(image))
        });
    let assets = FeedbackAssets {
        disc: meshes.add(Circle::new(1.0)),
        quad: meshes.add(Rectangle::new(1.0, 1.0)),
        blood: materials.add(StandardMaterial {
            base_color: Color::srgba(0.28, 0.008, 0.005, 0.88),
            base_color_texture: blood,
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            cull_mode: None,
            ..default()
        }),
        hole: materials.add(StandardMaterial {
            base_color: Color::srgba(0.035, 0.026, 0.018, 0.92),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            cull_mode: None,
            ..default()
        }),
        dust: materials.add(StandardMaterial {
            base_color: Color::srgba(0.66, 0.59, 0.44, 0.65),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            cull_mode: None,
            ..default()
        }),
        decals: crate::impact_decals::Catalog::load(&level.0.vfs, &mut meshes, &mut images, &mut materials),
        puffs: impact_puffs::Catalog::load(&level.0.vfs, &mut materials, &mut images),
    };
    // Queue these pipeline/material variants during startup so the first
    // short-lived hit puff is not lost to asynchronous shader compilation.
    let (forward, _, _) = movement::angle_vectors(glam::Vec3::new(local.pitch, local.yaw, 0.0));
    let pos = to_bevy(local.curr_eye + forward * 16.0);
    let now = time.elapsed_secs();
    for (blood, moving) in [(true, true), (false, true), (false, false)] {
        spawn(
            &mut commands,
            &assets,
            pos,
            -to_bevy(forward),
            0.001,
            blood,
            moving,
            Vec3::ZERO,
            now,
            2.0,
        );
    }
    if let Some(decal) = assets.decals.prewarm() {
        commands.spawn((
            Mesh3d(decal.mesh.clone()), MeshMaterial3d(decal.material.clone()),
            Transform::from_translation(pos)
                .with_rotation(Quat::from_rotation_arc(Vec3::Z, -to_bevy(forward)))
                .with_scale(Vec3::splat(0.00001)),
            ImpactFx { born: now, life: 2., velocity: Vec3::ZERO, billboard: false },
        ));
    }
    if let Some(material) = assets.puffs.prewarm_material() {
        // Vertex color adds a pipeline variant to the existing textured Blend
        // path. Warm it with zero coverage before the first short impact.
        let mut mesh = Mesh::from(Rectangle::new(0.001, 0.001));
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0_f32; 4]; 4]);
        commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(material),
            Transform::from_translation(pos),
            ImpactFx { born: now, life: 2., velocity: Vec3::ZERO, billboard: false },
            bevy::light::NotShadowCaster, bevy::light::NotShadowReceiver));
    }
    commands.insert_resource(assets);
    commands.init_resource::<Hurt>();
    for side in 0..4 {
        commands.spawn((
            Node {
                position_type: PositionType::Absolute,
                ..default()
            },
            GlobalZIndex(8),
            BackgroundColor(Color::NONE),
            HurtEdge(side),
        ));
    }
}

fn spawn(
    commands: &mut Commands,
    assets: &FeedbackAssets,
    pos: Vec3,
    normal: Vec3,
    size: f32,
    blood: bool,
    moving: bool,
    velocity: Vec3,
    now: f32,
    life: f32,
) {
    let normal = normal.normalize_or_zero();
    if normal.length_squared() < 0.5 || !pos.is_finite() {
        return;
    }
    commands.spawn((
        Mesh3d(if blood {
            assets.quad.clone()
        } else {
            assets.disc.clone()
        }),
        MeshMaterial3d(if blood {
            assets.blood.clone()
        } else if moving {
            assets.dust.clone()
        } else {
            assets.hole.clone()
        }),
        Transform::from_translation(pos + normal * 0.12)
            .with_rotation(Quat::from_rotation_arc(Vec3::Z, normal))
            .with_scale(Vec3::splat(size)),
        ImpactFx {
            born: now,
            life,
            velocity,
            billboard: moving,
        },
    ));
}

// DecalModulate uses neutral 128 grey; DXT compression shifts it by a few
// levels. Discard that neutral fringe so overlapping puffs have no square.
// Convert the coverage in memory to an alpha mask for the particle material.
fn multiply_decal_to_alpha(pixel: &mut [u8]) {
    let darkest = pixel[0].min(pixel[1]).min(pixel[2]);
    let coverage = (120u16.saturating_sub(darkest as u16) * 255 / 120) as u8;
    let alpha = (coverage as u16 * pixel[3] as u16 / 255) as u8;
    pixel.copy_from_slice(&[255, 255, 255, alpha]);
}

pub fn impact_sound(surface: &str) -> &'static str {
    let name = surface.to_ascii_lowercase();
    if name.contains("metal") {
        "SolidMetal.BulletImpact"
    } else if name.contains("wood") || name.contains("crate") {
        "Wood.BulletImpact"
    } else if name.contains("glass") {
        "Glass.BulletImpact"
    } else {
        "Concrete.BulletImpact"
    }
}

/// Shared by player damage and hits on dynamic-world targets.
#[allow(clippy::too_many_arguments)]
fn blood_hit(
    commands: &mut Commands,
    assets: &FeedbackAssets,
    collision: &collision::CollisionWorld,
    pos: glam::Vec3,
    direction: glam::Vec3,
    eye: Vec3,
    now: f32,
    enabled: bool,
    budget: &mut usize,
) {
    if !enabled || *budget < 8 {
        return;
    }
    *budget -= 8;
    for i in 0..6 {
        let phase = i as f32 * 2.4;
        let drift = to_bevy(direction) * 40.0
            + Vec3::new(
                phase.cos() * 24.0,
                10.0 + phase.sin() * 18.0,
                phase.sin() * 24.0,
            );
        spawn(
            commands,
            assets,
            to_bevy(pos) + drift * 0.03,
            (eye - to_bevy(pos)).normalize_or_zero(),
            3.0 + i as f32,
            true,
            true,
            drift,
            now,
            0.22 + i as f32 * 0.025,
        );
    }
    let tr = collision.trace_ray(
        pos + direction * 3.0,
        pos + direction * 180.0,
        collision::contents::MASK_SOLID,
    );
    if tr.hit() && !tr.start_solid && !collision.is_sky(tr.surface) {
        spawn(
            commands,
            assets,
            to_bevy(tr.end),
            to_bevy(tr.normal),
            16.0,
            true,
            false,
            Vec3::ZERO,
            now,
            25.0,
        );
    }
    let floor = collision.trace_ray(
        pos,
        pos - glam::Vec3::Z * 100.0,
        collision::contents::MASK_SOLID,
    );
    if floor.hit() && !floor.start_solid {
        spawn(
            commands,
            assets,
            to_bevy(floor.end),
            to_bevy(floor.normal),
            12.0,
            true,
            false,
            Vec3::ZERO,
            now,
            25.0,
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub fn update(
    mut commands: Commands,
    mut effects: ResMut<Effects>,
    mut assets: ResMut<FeedbackAssets>,
    props: Res<crate::surfaces::SurfaceProps>,
    mut hurt: ResMut<Hurt>,
    settings: Res<crate::settings::Settings>,
    world: Res<SimWorld>,
    local: Res<LocalPlayer>,
    level: Res<crate::LoadedLevel>,
    time: Res<Time>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut particles: Query<(Entity, &mut Transform, &mut ImpactFx, Option<&DoorImpactMark>, Option<&mut impact_puffs::Puff>)>,
    mut edges: Query<(&HurtEdge, &mut Node, &mut BackgroundColor)>,
) {
    let _frame_scope = crate::input_latency::frame_scope("feedback");
    let now = time.elapsed_secs();
    let dt = time.delta_secs();
    let eye = to_bevy(local.render_eye);
    let mut budget = MAX_FEEDBACK_FX.saturating_sub(particles.iter().count());
    let mut retired = HashSet::new();
    let reset = reset_feedback(&mut effects.feedback);
    if reset {
        for (e, _, _, _, puff) in &mut particles {
            if let Some(puff) = puff { puff.release_mesh(&mut meshes); }
            commands.entity(e).despawn();
        }
        hurt.time = None;
        budget = MAX_FEEDBACK_FX;
    }
    for event in effects.feedback.drain(..) {
        match event {
            sim::GameEvent::RoundStart { .. } => {}
            sim::GameEvent::Impact {
                pos,
                normal,
                surface,
                door,
                ..
            } => {
                if level.0.collision.is_sky(surface) {
                    continue;
                }
                let prop = props.prop_of(&level.0.vfs, level.0.collision.surface_name(surface));
                if let Some(decal) = props.get(&prop).and_then(|p| p.game_material.as_deref())
                    .and_then(|code| assets.decals.choose_for_surface(
                        &level.0.vfs, level.0.collision.surface_name(surface), code)) {
                    let normal = to_bevy(normal).normalize_or_zero();
                    let attachment = door.map(|impact| DoorImpactMark { map: level.0.id.clone(), impact, surface });
                    let pose = match &attachment {
                        Some(mark) => mark.pose(&level.0.id, &level.0.collision, &world.0.doors),
                        None => (normal.length_squared() > 0.5 && pos.is_finite()).then(||
                            Transform::from_translation(to_bevy(pos) + normal * 0.12)
                                .with_rotation(Quat::from_rotation_arc(Vec3::Z, normal))),
                    };
                    if normal.length_squared() > 0.5 && pos.is_finite()
                        && pose.is_some()
                        && reserve_impact_slot(&mut commands, &mut budget, &mut retired,
                            particles.iter().filter(|_| !reset).map(|(e, _, fx, _, _)| (e, fx.born))) {
                        if std::env::var_os("CSR_WEAPON_TRACE").is_some() {
                            info!("impact artwork: {:?} at {:?}; surface={prop}, game_material={:?}", decal.id, pos, props.get(&prop).and_then(|p| p.game_material.as_deref()));
                        }
                        let mut mark = commands.spawn((Mesh3d(decal.mesh), MeshMaterial3d(decal.material), ImpactArtwork(decal.id),
                            pose.unwrap(),
                            ImpactFx { born: now, life: 30., velocity: Vec3::ZERO, billboard: false }));
                        if let Some(attachment) = attachment {
                            mark.insert(attachment);
                        } else if let Some(index) = level.0.collision.breakables.iter()
                            .position(|def| def.contains_surface(pos, crate::coords::to_source(normal))) {
                            mark.insert(crate::breakables::BreakableMesh(vec![index]));
                        }
                    }
                }
                let authored_puff = if budget > 0 {
                    props.get(&prop).and_then(|p| p.game_material.as_deref())
                        .and_then(|code| assets.puffs.spawn(&mut commands, &mut meshes,
                            code, pos, normal, eye))
                } else { None };
                if let Some((entity, life)) = authored_puff {
                    budget -= 1;
                    commands.entity(entity).insert(ImpactFx {
                        born: now, life, velocity: Vec3::ZERO, billboard: false,
                    });
                    continue;
                }
                for i in 0..3 {
                    if budget == 0 { break; }
                    budget -= 1;
                    let drift = to_bevy(normal) * 25.0
                        + Vec3::new(i as f32 * 9.0 - 9.0, 8.0, 5.0 - i as f32 * 5.0);
                    spawn(
                        &mut commands,
                        &assets,
                        to_bevy(pos),
                        to_bevy(normal),
                        1.0 + i as f32 * 0.4,
                        false,
                        true,
                        drift,
                        now,
                        0.2,
                    );
                }
            }
            sim::GameEvent::Damage {
                attacker,
                victim,
                health,
                pos,
                ..
            } => {
                let origin = world
                    .0
                    .player(attacker)
                    .map_or(pos - glam::Vec3::X, |p| p.eye());
                let direction = (pos - origin).normalize_or_zero();
                if victim == local.view_target() {
                    hurt.time = Some(now);
                    hurt.direction = -direction;
                    hurt.amount = (health as f32 / 45.0).clamp(0.35, 1.0);
                }
                blood_hit(
                    &mut commands,
                    &assets,
                    &world.0.collision,
                    pos,
                    direction,
                    eye,
                    now,
                    settings.blood,
                    &mut budget,
                );
            }
            _ => {}
        }
    }
    if !reset {
        for (e, mut transform, mut fx, attachment, puff) in &mut particles {
            if retired.contains(&e) {
                if let Some(puff) = puff { puff.release_mesh(&mut meshes); }
                continue;
            }
            if now - fx.born > fx.life {
                if let Some(puff) = puff { puff.release_mesh(&mut meshes); }
                commands.entity(e).despawn();
                continue;
            }
            if let Some(puff) = puff {
                *transform = puff.pose(now - fx.born, fx.life, eye);
                continue;
            }
            if let Some(mark) = attachment {
                let Some(pose) = mark.pose(&level.0.id, &level.0.collision, &world.0.doors) else {
                    commands.entity(e).despawn();
                    continue;
                };
                *transform = pose;
            }
            if fx.billboard {
                transform.translation += fx.velocity * dt;
                fx.velocity.y -= 90.0 * dt;
                transform.look_at(eye, Vec3::Y);
            }
        }
    }
    let strength = hurt
        .time
        .map_or(0.0, |t| (1.0 - (now - t) / 0.65).clamp(0.0, 1.0))
        * hurt.amount;
    let (f, r, _) = movement::angle_vectors(glam::Vec3::new(0.0, local.render_angles.y, 0.0));
    let d = hurt.direction;
    let weights = [
        d.dot(f).max(0.0),
        (-d.dot(f)).max(0.0),
        (-d.dot(r)).max(0.0),
        d.dot(r).max(0.0),
    ];
    for (edge, mut node, mut color) in &mut edges {
        *node = Node {
            position_type: PositionType::Absolute,
            ..default()
        };
        match edge.0 {
            0 => {
                node.top = px(0.0);
                node.width = percent(100);
                node.height = px(18.0);
            }
            1 => {
                node.bottom = px(0.0);
                node.width = percent(100);
                node.height = px(18.0);
            }
            2 => {
                node.left = px(0.0);
                node.width = px(18.0);
                node.height = percent(100);
            }
            _ => {
                node.right = px(0.0);
                node.width = px(18.0);
                node.height = percent(100);
            }
        }
        color.0 = Color::srgba(
            0.65,
            0.025,
            0.015,
            strength * (0.08 + weights[edge.0 as usize] * 0.65),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn surface_sounds_match_material_family() {
        assert_eq!(
            impact_sound("de_dust/metal_door"),
            "SolidMetal.BulletImpact"
        );
        assert_eq!(impact_sound("props/wood_crate"), "Wood.BulletImpact");
        assert_eq!(impact_sound("unknown"), "Concrete.BulletImpact");
    }
}

#[cfg(test)]
mod decal_tests {
    use super::*;

    fn door_mark() -> (DoorImpactMark, collision::CollisionWorld, Vec<sim::doors::State>) {
        let mut map = bsp::Bsp::default();
        let doc = kv::parse(r#"door { classname prop_door_rotating model models/fixture_leaf.mdl
            origin "100 200 32" angles "12 27 9" }"#).unwrap();
        map.entities.push(doc.block("door").unwrap().clone());
        let hull = mdl::ConvexHull {
            points: vec![glam::Vec3::new(-2., 0., 0.), glam::Vec3::new(2., 0., 0.),
                glam::Vec3::new(2., 48., 0.), glam::Vec3::new(-2., 48., 0.),
                glam::Vec3::new(-2., 0., 96.), glam::Vec3::new(2., 0., 96.),
                glam::Vec3::new(2., 48., 96.), glam::Vec3::new(-2., 48., 96.)],
            triangles: vec![[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4],
                [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]],
        };
        let collision = collision::CollisionWorld::from_bsp(&map, &collision::MapCollisionOptions::default(),
            &|_| Some(vec![hull.clone()]));
        let mark = DoorImpactMark {
            map: content::ContentId::csgo(content::ContentKind::Map, "fixture"),
            impact: sim::doors::Impact { door: 0, pos: glam::Vec3::new(-2., 24., 64.), normal: -glam::Vec3::X },
            surface: collision.doors[0].surface,
        };
        (mark, collision, vec![sim::doors::State { angle: 0., target: 0., return_at: None }])
    }

    #[test]
    fn mounted_door_mark_follows_hinge_position_normal_and_texture_roll_both_ways() {
        let (mark, collision, mut states) = door_mark();
        let closed = mark.pose(&mark.map, &collision, &states).unwrap();
        let def = &collision.doors[0];
        let hinge = to_bevy(def.origin);
        for angle in [-37., -90., -37., 0.] {
            states[0].angle = angle;
            let pose = mark.pose(&mark.map, &collision, &states).unwrap();
            let swing = rotation_to_bevy(def.rotation(angle) * def.rotation(0.).transpose());
            assert!((pose.translation - (hinge + swing * (closed.translation - hinge))).length() < 0.001);
            for axis in [Vec3::Z, Vec3::X] {
                assert!((pose.rotation * axis - swing * (closed.rotation * axis)).length() < 0.001);
            }
        }
        assert!((mark.pose(&mark.map, &collision, &states).unwrap().translation - closed.translation).length() < 0.001);
        assert!(mark.pose(&mark.map, &collision, &[]).is_none());
        let new_map = content::ContentId::csgo(content::ContentKind::Map, "another_map");
        assert!(mark.pose(&new_map, &collision, &states).is_none(), "ordinals cannot cross map ContentIds");
        let invalid = DoorImpactMark { surface: mark.surface + 1, ..mark };
        assert!(invalid.pose(&invalid.map, &collision, &states).is_none());
    }

    #[test]
    fn accumulated_impacts_before_latest_round_reset_are_not_respawned() {
        let impact = sim::GameEvent::Impact { player: 1, pos: glam::Vec3::ZERO, normal: glam::Vec3::X,
            surface: 0, door: Some(sim::doors::Impact { door: 0, pos: glam::Vec3::ZERO, normal: glam::Vec3::X }) };
        let mut events = vec![impact.clone(), sim::GameEvent::RoundStart { round: 2 }, impact.clone(),
            sim::GameEvent::RoundStart { round: 3 }, impact.clone()];
        assert!(reset_feedback(&mut events));
        assert_eq!(events, vec![sim::GameEvent::RoundStart { round: 3 }, impact.clone()]);
        events.remove(0);
        assert!(!reset_feedback(&mut events)); assert_eq!(events, vec![impact]);
    }

    #[test]
    fn leaving_match_despawns_door_artwork_and_its_attachment_together() {
        use crate::session::{AppState, MatchEntity};
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
            .insert_state(AppState::Match)
            .add_observer(crate::session::own_entity::<Transform>)
            .add_systems(OnExit(AppState::Match), crate::session::clear_match);
        app.update();
        let (mark, _, _) = door_mark();
        let entity = app.world_mut().spawn((mark, Transform::default(),
            ImpactArtwork(content::ContentId::csgo(content::ContentKind::Material, "impact.fixture")),
            ImpactFx { born: 1., life: 30., velocity: Vec3::ZERO, billboard: false })).id();
        app.world_mut().flush();
        assert!(app.world().get::<MatchEntity>(entity).is_some());
        app.world_mut().resource_mut::<NextState<AppState>>().set(AppState::Menu);
        app.update();
        assert!(app.world().get_entity(entity).is_err());
        assert_eq!(app.world_mut().query::<&DoorImpactMark>().iter(app.world()).count(), 0);
    }

    #[test]
    fn new_impact_marks_replace_distinct_old_effects_at_the_entity_limit() {
        let mut world = World::new();
        let old: Vec<_> = (0..MAX_FEEDBACK_FX).map(|i| world.spawn(ImpactFx {
            born: i as f32, life: 30., velocity: Vec3::ZERO, billboard: false,
        }).id()).collect();
        let mut candidates = world.query::<(Entity, &ImpactFx)>();
        let mut queue = bevy::ecs::world::CommandQueue::default();
        let mut budget = 0;
        let mut retired = HashSet::new();
        let mut added = Vec::new();
        {
            let mut commands = Commands::new(&mut queue, &world);
            for i in 0..32 {
                assert!(reserve_impact_slot(&mut commands, &mut budget, &mut retired,
                    candidates.iter(&world).map(|(e, fx)| (e, fx.born))));
                added.push(commands.spawn(ImpactFx {
                    born: 1000. + i as f32, life: 30., velocity: Vec3::ZERO, billboard: false,
                }).id());
            }
        }
        queue.apply(&mut world);
        assert_eq!(world.query::<&ImpactFx>().iter(&world).count(), MAX_FEEDBACK_FX);
        assert_eq!(retired.len(), 32);
        assert!(old[..32].iter().all(|e| world.get_entity(*e).is_err()));
        assert!(old[32..].iter().chain(&added).all(|e| world.get::<ImpactFx>(*e).is_some()));
        assert_eq!(budget, 0);
    }

    #[test]
    fn impact_mark_does_not_require_three_dust_slots() {
        let mut world = World::new();
        let mut queue = bevy::ecs::world::CommandQueue::default();
        for free in 1..4 {
            let mut budget = free;
            let mut retired = HashSet::new();
            let mut commands = Commands::new(&mut queue, &world);
            assert!(reserve_impact_slot(&mut commands, &mut budget, &mut retired,
                std::iter::empty()));
            assert_eq!(budget, free - 1);
            assert!(retired.is_empty());
        }
        queue.apply(&mut world);
    }

    #[test]
    fn impact_overflow_without_a_retirable_effect_stays_bounded() {
        let world = World::new();
        let mut queue = bevy::ecs::world::CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        assert!(!reserve_impact_slot(&mut commands, &mut 0, &mut HashSet::new(),
            std::iter::empty()));
    }

    #[test]
    fn multiply_white_is_transparent_and_dark_is_covered() {
        let mut white = [255, 255, 255, 255];
        multiply_decal_to_alpha(&mut white);
        assert_eq!(white[3], 0);
        let mut dark = [10, 0, 0, 255];
        multiply_decal_to_alpha(&mut dark);
        assert_eq!(dark, [255, 255, 255, 255]);
        let mut neutral = [128, 128, 128, 255];
        multiply_decal_to_alpha(&mut neutral);
        assert_eq!(neutral[3], 0);
        let mut grey = [60, 60, 60, 128];
        multiply_decal_to_alpha(&mut grey);
        assert_eq!(grey[3], 63);
    }
}

#[cfg(test)]
mod mob_blood_tests {
    use super::*;

    #[test]
    fn damage_update_places_both_blood_marks_on_current_runtime_geometry() {
        use std::{collections::BTreeMap, sync::Arc};
        use collision::{CollisionWorld, Convex, Plane};

        let solid = |mins: glam::Vec3, maxs: glam::Vec3| Convex {
            planes: vec![
                Plane { normal: glam::Vec3::X, dist: maxs.x },
                Plane { normal: -glam::Vec3::X, dist: -mins.x },
                Plane { normal: glam::Vec3::Y, dist: maxs.y },
                Plane { normal: -glam::Vec3::Y, dist: -mins.y },
                Plane { normal: glam::Vec3::Z, dist: maxs.z },
                Plane { normal: -glam::Vec3::Z, dist: -mins.z },
            ],
            mins, maxs, contents: collision::contents::SOLID, surface: 0,
        };
        let mut base = CollisionWorld::new();
        base.add_convex(solid(glam::Vec3::new(-128., -128., -16.), glam::Vec3::new(128., 128., 0.)));
        base.add_convex(solid(glam::Vec3::new(120., -128., 0.), glam::Vec3::new(128., 128., 128.)));
        base.breakables = vec![collision::breakables::Breakable {
            entity: 1, model: 1, origin: glam::Vec3::ZERO, health: 10., material: 0,
            prop_data: String::new(), bullet_scale: 1., club_scale: 1., minimum_damage: 0.,
            trigger_only: false, studio: None,
            solids: vec![
                solid(glam::Vec3::new(32., -16., 48.), glam::Vec3::new(40., 16., 80.)),
                solid(glam::Vec3::new(-16., -16., 24.), glam::Vec3::new(16., 16., 32.)),
            ],
        }].into();
        base.finalize();
        let base = Arc::new(base);

        for destroyed in [false, true] {
            // In-memory empty BSP follows the existing audio combat fixture;
            // no install, graphics plugins or asset files are needed.
            let mut bsp = vec![0u8; 1036];
            bsp[..4].copy_from_slice(b"VBSP");
            bsp[4..8].copy_from_slice(&20i32.to_le_bytes());
            let mut fs = vfs::Vfs::new();
            fs.push_back("blood fixture", vfs::Mount::Memory(BTreeMap::from([
                ("maps/blood_fixture.bsp".into(), Arc::<[u8]>::from(bsp)),
            ])));
            let mut level = level::Level::load_from_vfs(
                content::ContentId::csgo(content::ContentKind::Map, "blood_fixture"),
                "blood_fixture", Default::default(), fs,
            ).unwrap();
            let mut world = sim::World::new(base.clone());
            // Retain the intact overlay as the frozen level geometry. Restore
            // rebuilds only the sim's overlay, as a runtime correction does.
            level.collision = world.collision.clone();
            let pos = glam::Vec3::new(0., 0., 64.);
            let attacker = world.add_player("attacker", sim::Team::Terrorist);
            let victim = world.add_player("victim", sim::Team::CounterTerrorist);
            let shooter = world.player_mut(attacker).unwrap();
            shooter.movement.origin = pos - glam::Vec3::X * 16.
                - glam::Vec3::Z * shooter.movement.view_offset();
            let mut state = world.state();
            if destroyed {
                state.breakables[0] = sim::breakables::State { health: 0., destroyed: true };
            }
            world.restore(state);
            let frozen = level.collision.trace_ray(pos, pos - glam::Vec3::Z * 100., collision::contents::MASK_SOLID);
            assert!(frozen.hit() && frozen.end.z > 32., "level must retain the old blocker");

            let mut effects = Effects::default();
            effects.feedback.push(sim::GameEvent::Damage {
                attacker, victim, health: 24, group: sim::weapon::HitGroup::Chest, pos,
                protection: sim::weapon::HitProtection::Flesh,
            });
            let mut app = App::new();
            app.insert_resource(effects)
                .insert_resource(FeedbackAssets {
                    disc: default(), quad: default(), blood: default(), hole: default(),
                    dust: default(), decals: default(), puffs: default(),
                })
                .insert_resource(SimWorld(world))
                .insert_resource(LocalPlayer::new(attacker, pos - glam::Vec3::X * 16., 0.))
                .insert_resource(crate::LoadedLevel(level))
                .init_resource::<crate::surfaces::SurfaceProps>()
                .init_resource::<crate::settings::Settings>()
                .init_resource::<Hurt>().init_resource::<Time>().init_resource::<Assets<Mesh>>()
                .add_systems(Update, update);
            app.update();
            assert!(app.world().resource::<Effects>().feedback.is_empty());
            let mut query = app.world_mut().query::<(&Transform, &ImpactFx)>();
            let fx: Vec<_> = query.iter(app.world()).collect();
            assert_eq!(fx.iter().filter(|(_, fx)| fx.billboard).count(), 6);
            let marks: Vec<_> = fx.iter().filter(|(_, fx)| !fx.billboard).collect();
            assert_eq!(marks.len(), 2);
            let wall = marks.iter().find(|(pose, _)| pose.scale.x == 16.).unwrap().0;
            let floor = marks.iter().find(|(pose, _)| pose.scale.x == 12.).unwrap().0;
            // Compare against fixture surfaces, allowing the existing clip and
            // decal offsets. Using frozen collision fails both destroyed checks.
            let wall_x = if destroyed { 120. } else { 32. };
            let floor_z = if destroyed { 0. } else { 32. };
            assert!((wall.translation - to_bevy(glam::Vec3::new(wall_x, 0., 64.))).length() < 0.25);
            assert!((floor.translation - to_bevy(glam::Vec3::new(0., 0., floor_z))).length() < 0.25);
        }
    }

    #[test]
    fn shared_blood_respects_setting_budget_and_short_lifetime() {
        let mut world = World::new();
        let assets = FeedbackAssets {
            disc: default(),
            quad: default(),
            blood: default(),
            hole: default(),
            dust: default(),
            decals: default(),
            puffs: default(),
        };
        let collision = collision::CollisionWorld::new();
        let mut queue = bevy::ecs::world::CommandQueue::default();
        let mut budget = 512;
        {
            let mut commands = Commands::new(&mut queue, &world);
            blood_hit(
                &mut commands,
                &assets,
                &collision,
                glam::Vec3::ZERO,
                glam::Vec3::X,
                Vec3::Z * 10.,
                1.,
                false,
                &mut budget,
            );
        }
        queue.apply(&mut world);
        assert_eq!(world.query::<&ImpactFx>().iter(&world).count(), 0);
        assert_eq!(budget, 512);
        {
            let mut commands = Commands::new(&mut queue, &world);
            for _ in 0..100 {
                blood_hit(
                    &mut commands,
                    &assets,
                    &collision,
                    glam::Vec3::ZERO,
                    glam::Vec3::X,
                    Vec3::Z * 10.,
                    1.,
                    true,
                    &mut budget,
                );
            }
        }
        queue.apply(&mut world);
        assert_eq!(budget, 0);
        assert_eq!(world.query::<&ImpactFx>().iter(&world).count(), 384);
        assert!(world
            .query::<&ImpactFx>()
            .iter(&world)
            .all(|fx| fx.billboard && fx.life < 0.5));
    }
}
