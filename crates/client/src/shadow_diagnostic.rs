//! Opt-in shadow plumbing, not a Source lighting/composition implementation.
//!
//! `CSR_SHADOW_DIAGNOSTIC=1` visualizes the shadow factor on static world/props.
//! Normal startup installs no systems, lights or material variants. Units stay
//! in Source inches. Cascade defaults/bias and prop shadow flags are not claimed
//! to match the native renderer; no baked irradiance or exposure is adjusted.

use crate::{
    LoadedLevel, coords,
    material::{WorldMaterial, flags},
    session::{AppState, MatchEntity, Persistent},
    visibility::MapClusters,
};
use bevy::{
    asset::{AssetId, embedded_asset},
    camera::visibility::{RenderLayers, VisibilitySystems},
    light::{CascadeShadowConfigBuilder, DirectionalLight, SimulationLightSystems},
    prelude::*,
    transform::TransformSystems,
};
use std::{
    collections::{HashMap, HashSet},
    sync::OnceLock,
};

pub const ENVIRONMENT: &str = "CSR_SHADOW_DIAGNOSTIC";
pub const SHADOW_SHADER: &str = "embedded://client/shaders/world_shadow.wgsl";
#[path = "shadow_fixture.rs"]
pub mod fixture;
/// Reserved only while this developer diagnostic is active. Existing layers:
/// main=0, sky=1, viewmodel=2. No ordinary scene asset uses layer31.
const CASTER_LAYER: usize = 31;
/// Diagnostic coverage only. The native status's raw400 is NOT a calibration
/// of Bevy cascade splits, projection, caster reach or shadow bias.
const COVERAGE: f32 = 400.0;
const CASTER_REACH: f32 = 400.0;
const MAX_CAMERAS: usize = 8;
const MAX_MESHES: usize = 8192;
const MAX_MATERIALS: usize = 1024;
type OwnedScene = (With<MatchEntity>, Without<Persistent>);

pub fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var(ENVIRONMENT).as_deref() == Ok("1"))
}

pub fn install(app: &mut App) {
    if !enabled() {
        return;
    }
    embedded_asset!(app, "shaders/world_shadow.wgsl");
    app.init_resource::<DiagnosticState>().add_systems(
        PostUpdate,
        update_scope
            .after(TransformSystems::Propagate)
            .before(VisibilitySystems::VisibilityPropagate)
            .before(SimulationLightSystems::UpdateDirectionalLightCascades),
    );
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Bounds {
    pub min: glam::Vec3,
    pub max: glam::Vec3,
}

impl Bounds {
    pub(crate) fn new(min: glam::Vec3, max: glam::Vec3) -> Option<Self> {
        (min.is_finite() && max.is_finite() && min.cmple(max).all()).then_some(Self { min, max })
    }
    fn intersects(self, other: Self) -> bool {
        self.min.cmple(other.max).all() && other.min.cmple(self.max).all()
    }
    fn transformed(self, transform: &GlobalTransform) -> Option<Self> {
        let mut lo = glam::Vec3::splat(f32::INFINITY);
        let mut hi = glam::Vec3::splat(f32::NEG_INFINITY);
        for x in [self.min.x, self.max.x] {
            for y in [self.min.y, self.max.y] {
                for z in [self.min.z, self.max.z] {
                    let p = coords::to_source(
                        transform.transform_point(coords::to_bevy(glam::Vec3::new(x, y, z))),
                    );
                    if !p.is_finite() {
                        return None;
                    }
                    lo = lo.min(p);
                    hi = hi.max(p);
                }
            }
        }
        Self::new(lo, hi)
    }
}

fn caster_region(
    eye: glam::Vec3,
    towards_sun: glam::Vec3,
    coverage: f32,
    reach: f32,
) -> Option<Bounds> {
    if !eye.is_finite()
        || !towards_sun.is_finite()
        || towards_sun.length_squared() < 0.5
        || !coverage.is_finite()
        || !(0.0..=4096.0).contains(&coverage)
        || coverage == 0.0
        || !reach.is_finite()
        || !(0.0..=4096.0).contains(&reach)
    {
        return None;
    }
    let displacement = towards_sun.normalize() * reach;
    let extent = glam::Vec3::splat(coverage);
    Bounds::new(
        eye - extent + displacement.min(glam::Vec3::ZERO),
        eye + extent + displacement.max(glam::Vec3::ZERO),
    )
}

fn main_layers(layers: Option<&RenderLayers>) -> bool {
    let layers = layers.cloned().unwrap_or_default();
    layers.intersects(&RenderLayers::layer(0))
        && !layers.intersects(&crate::sky::sky_layer())
        && !layers.intersects(&crate::viewmodel::layer())
}

fn caster_material(alpha: AlphaMode) -> bool {
    matches!(alpha, AlphaMode::Opaque | AlphaMode::Mask(_))
}

fn camera_candidates(world: &mut World) -> Vec<(Entity, GlobalTransform, Option<RenderLayers>)> {
    world
        .query_filtered::<(
            Entity,
            &Camera,
            &GlobalTransform,
            Option<&RenderLayers>,
            Option<&Override>,
        ), (With<Camera3d>, OwnedScene)>()
        .iter(world)
        .filter(|(_, camera, _, layers, saved)| {
            camera.is_active && main_layers(saved.map_or(*layers, |s| s.layers.as_ref()))
        })
        .take(MAX_CAMERAS + 1)
        .map(|(entity, _, transform, layers, saved)| {
            (
                entity,
                *transform,
                saved.map_or_else(|| layers.cloned(), |s| s.layers.clone()),
            )
        })
        .collect()
}

#[derive(Resource, Default)]
struct DiagnosticState {
    level_tick: Option<u32>,
    light: Option<Entity>,
    variants: HashMap<AssetId<WorldMaterial>, Handle<WorldMaterial>>,
    unavailable: Option<&'static str>,
    status: Status,
}

/// CPU scope receipt, not evidence of GPU submission or native agreement.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Status {
    #[default]
    Inactive,
    Unavailable(&'static str),
    Preparing {
        cameras: usize,
        casters: usize,
        variants: usize,
    },
    Active {
        light: Entity,
        cameras: usize,
        casters: usize,
        variants: usize,
    },
}

/// Absent entirely on ordinary startup. Counts refresh without per-frame logs.
pub fn status(world: &World) -> Option<&Status> {
    world.get_resource::<DiagnosticState>().map(|s| &s.status)
}

fn publish(state: &mut DiagnosticState, status: Status) {
    let transition = std::mem::discriminant(&state.status) != std::mem::discriminant(&status)
        || matches!((&state.status, &status), (Status::Unavailable(a), Status::Unavailable(b)) if a != b);
    if transition {
        info!("Shadow diagnostic CPU scope: {status:?}");
    }
    state.status = status;
}

#[derive(Component)]
struct DiagnosticLight;

#[derive(Component, Clone)]
struct Override {
    layers: Option<RenderLayers>,
    material: Option<Handle<WorldMaterial>>,
    visibility: Option<Visibility>,
}

fn restore_entity(world: &mut World, entity: Entity) {
    let Some(saved) = world.get::<Override>(entity).cloned() else {
        return;
    };
    let mut entity = world.entity_mut(entity);
    if let Some(layers) = saved.layers {
        entity.insert(layers);
    } else {
        entity.remove::<RenderLayers>();
    }
    if let Some(material) = saved.material {
        entity.insert(MeshMaterial3d(material));
    }
    if let Some(visibility) = saved.visibility {
        entity.insert(visibility);
    }
    entity.remove::<Override>();
}

fn clear_scope(world: &mut World, state: &mut DiagnosticState) {
    let entities: Vec<_> = world
        .query_filtered::<Entity, With<Override>>()
        .iter(world)
        .collect();
    for entity in entities {
        restore_entity(world, entity);
    }
    if let Some(light) = state.light.take() {
        let _ = world.despawn(light);
    }
    state.variants.clear();
    state.level_tick = None;
}

fn unavailable(world: &mut World, state: &mut DiagnosticState, reason: &'static str) {
    if state.unavailable != Some(reason) {
        warn!("Shadow diagnostic unavailable: {reason}");
    }
    // Preserve this scene's identity: do not reset the warning every frame.
    let tick = state.level_tick;
    clear_scope(world, state);
    state.level_tick = tick;
    state.unavailable = Some(reason);
    publish(state, Status::Unavailable(reason));
}

/// Runs before inherited-visibility propagation. It changes only its recorded
/// overrides and uses a dedicated light layer, never an unowned caster mesh.
fn update_scope(world: &mut World) {
    let Some(mut state) = world.remove_resource::<DiagnosticState>() else {
        return;
    };
    let active = world
        .get_resource::<State<AppState>>()
        .is_some_and(|s| *s.get() == AppState::Match);
    let context = world.get_resource_ref::<LoadedLevel>().and_then(|level| {
        let sun = crate::props::Sun::from_bsp(&level.0.bsp)?;
        (sun.to_sun.is_finite() && sun.to_sun.length_squared() > 0.5)
            .then_some((level.last_changed().get(), sun.to_sun))
    });
    if !active || context.is_none() {
        clear_scope(world, &mut state);
        publish(&mut state, Status::Inactive);
        world.insert_resource(state);
        return;
    }
    let (tick, to_sun) = context.unwrap();
    if state.level_tick != Some(tick) {
        clear_scope(world, &mut state);
        publish(&mut state, Status::Inactive);
        state.level_tick = Some(tick);
        state.unavailable = None;
    }
    // Refuse collisions rather than letting another layer31 scene become a caster.
    let collision = world
        .query::<(
            Entity,
            &RenderLayers,
            Option<&Override>,
            Option<&DiagnosticLight>,
        )>()
        .iter(world)
        .any(|(_, layers, saved, light)| {
            layers.intersects(&RenderLayers::layer(CASTER_LAYER))
                && saved.is_none()
                && light.is_none()
        });
    if collision {
        unavailable(world, &mut state, "reserved caster layer already in use");
        world.insert_resource(state);
        return;
    }
    // The factor shader deliberately reads index0. Refuse an ambiguous light
    // list instead of claiming another light's shadow map is this diagnostic.
    if world
        .query_filtered::<Entity, (With<DirectionalLight>, Without<DiagnosticLight>)>()
        .iter(world)
        .next()
        .is_some()
    {
        unavailable(world, &mut state, "another directional light is present");
        world.insert_resource(state);
        return;
    }
    if state
        .light
        .is_none_or(|light| world.get_entity(light).is_err())
    {
        let transform = Transform::IDENTITY.looking_to(coords::to_bevy(-to_sun), Vec3::Y);
        state.light = Some(
            world
                .spawn((
                    DirectionalLight {
                        illuminance: 0.0,
                        shadow_maps_enabled: true,
                        affects_lightmapped_mesh_diffuse: false,
                        ..Default::default()
                    },
                    CascadeShadowConfigBuilder {
                        maximum_distance: COVERAGE,
                        ..Default::default()
                    }
                    .build(),
                    transform,
                    // Created after transform propagation: admit the actual
                    // parentless direction to this frame's cascade preparation.
                    GlobalTransform::from(transform),
                    RenderLayers::layer(CASTER_LAYER),
                    DiagnosticLight,
                    MatchEntity,
                ))
                .id(),
        );
    }
    let cameras = camera_candidates(world);
    if cameras.len() > MAX_CAMERAS {
        unavailable(world, &mut state, "too many owned world cameras");
        world.insert_resource(state);
        return;
    }
    let mut touched = HashSet::new();
    let mut regions = Vec::new();
    for (entity, transform, layers) in cameras {
        if !main_layers(layers.as_ref()) {
            continue;
        }
        let Some(region) = caster_region(
            coords::to_source(transform.translation()),
            to_sun,
            COVERAGE,
            CASTER_REACH,
        ) else {
            continue;
        };
        if world.get::<Override>(entity).is_none() {
            world.entity_mut(entity).insert(Override {
                layers: layers.clone(),
                material: None,
                visibility: None,
            });
        }
        world
            .entity_mut(entity)
            .insert(layers.unwrap_or_default().with(CASTER_LAYER));
        regions.push(region);
        touched.insert(entity);
    }
    let camera_count = regions.len();
    let mut caster_count = 0;
    // Recompute the original camera-PVS result each diagnostic frame. Otherwise
    // within-cluster motion could leave last frame's extra caster visible forever.
    let pvs = world
        .get_resource::<crate::player::LocalPlayer>()
        .and_then(|local| {
            let level = &world.resource::<LoadedLevel>().0;
            let cluster = level
                .bsp
                .leafs
                .get(level.bsp.point_leaf(local.render_eye))
                .map_or(-1, |l| l.cluster);
            if std::env::var_os("CSR_NO_PVS").is_some() {
                None
            } else {
                level.bsp.cluster_pvs(cluster)
            }
        });
    let meshes: Vec<_> = world
        .query_filtered::<(
            Entity,
            &MeshMaterial3d<WorldMaterial>,
            &MapClusters,
            &GlobalTransform,
            Option<&RenderLayers>,
            Option<&Override>,
        ), (With<Mesh3d>, OwnedScene)>()
        .iter(world)
        .take(MAX_MESHES + 1)
        .map(|(entity, material, clusters, transform, layers, saved)| {
            (
                entity,
                saved
                    .and_then(|s| s.material.clone())
                    .unwrap_or_else(|| material.0.clone()),
                clusters.bounds().and_then(|b| b.transformed(transform)),
                clusters.visible(pvs.as_deref()),
                saved.map_or_else(|| layers.cloned(), |s| s.layers.clone()),
            )
        })
        .collect();
    if meshes.len() > MAX_MESHES {
        unavailable(world, &mut state, "too many owned static meshes");
        world.insert_resource(state);
        return;
    }
    for (entity, original, bounds, pvs_visible, layers) in meshes {
        let eligible = main_layers(layers.as_ref())
            && bounds.is_some_and(|b| regions.iter().any(|r| r.intersects(b)));
        let baseline = if pvs_visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        let material = world
            .get_resource::<Assets<WorldMaterial>>()
            .and_then(|assets| assets.get(&original));
        if !eligible || material.is_none_or(|m| !caster_material(m.alpha_mode)) {
            if let Some(mut saved) = world.get_mut::<Override>(entity) {
                saved.visibility = Some(baseline);
            }
            restore_entity(world, entity);
            continue;
        }
        if !state.variants.contains_key(&original.id()) && state.variants.len() >= MAX_MATERIALS {
            unavailable(world, &mut state, "too many owned static material variants");
            world.insert_resource(state);
            return;
        }
        // Clone only on first admission, retaining existing texture handles.
        let new_material =
            (!state.variants.contains_key(&original.id())).then(|| material.unwrap().clone());
        let variant = state
            .variants
            .entry(original.id())
            .or_insert_with(|| {
                let mut material = new_material.unwrap();
                material.params.flags |= flags::SHADOW_DIAGNOSTIC;
                world.resource_mut::<Assets<WorldMaterial>>().add(material)
            })
            .clone();
        world.entity_mut(entity).insert((
            Override {
                layers: layers.clone(),
                material: Some(original),
                visibility: Some(baseline),
            },
            layers.unwrap_or_default().with(CASTER_LAYER),
            MeshMaterial3d(variant),
            Visibility::Inherited,
        ));
        touched.insert(entity);
        caster_count += 1;
    }
    let stale: Vec<_> = world
        .query_filtered::<Entity, With<Override>>()
        .iter(world)
        .filter(|e| !touched.contains(e))
        .collect();
    for entity in stale {
        restore_entity(world, entity);
    }
    let variants = state.variants.len();
    let status = if camera_count == 0 || caster_count == 0 {
        Status::Preparing {
            cameras: camera_count,
            casters: caster_count,
            variants,
        }
    } else {
        Status::Active {
            light: state.light.unwrap(),
            cameras: camera_count,
            casters: caster_count,
            variants,
        }
    };
    state.unavailable = None;
    publish(&mut state, status);
    world.insert_resource(state);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actual_main_camera_spawn_receives_match_ownership_via_existing_transform_observer() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
            .insert_state(AppState::Match)
            .add_observer(crate::session::own_entity::<Transform>);
        app.update();
        let camera = app
            .world_mut()
            .spawn((
                Camera3d::default(),
                Transform::default(),
                crate::player::PlayerCamera,
            ))
            .id();
        app.world_mut().flush();
        assert!(app.world().get::<MatchEntity>(camera).is_some());
        assert_eq!(
            camera_candidates(app.world_mut())
                .iter()
                .map(|c| c.0)
                .collect::<Vec<_>>(),
            vec![camera]
        );
    }

    #[test]
    fn camera_and_mesh_admission_excludes_inactive_unowned_and_both_marked_persistent() {
        let mut world = World::new();
        let active = world.spawn((Camera3d::default(), MatchEntity)).id();
        world.spawn((Camera3d::default(), MatchEntity, Persistent));
        world.spawn(Camera3d::default());
        world.spawn((
            Camera3d::default(),
            MatchEntity,
            Camera {
                is_active: false,
                ..Default::default()
            },
        ));
        world.spawn((Camera3d::default(), MatchEntity, crate::sky::sky_layer()));
        world.spawn((Camera3d::default(), MatchEntity, crate::viewmodel::layer()));
        assert_eq!(
            camera_candidates(&mut world)
                .iter()
                .map(|c| c.0)
                .collect::<Vec<_>>(),
            vec![active]
        );

        let mesh = world.spawn((Mesh3d(Handle::default()), MatchEntity)).id();
        world.spawn((Mesh3d(Handle::default()), MatchEntity, Persistent));
        world.spawn(Mesh3d(Handle::default()));
        let owners = world
            .query_filtered::<Entity, (With<Mesh3d>, OwnedScene)>()
            .iter(&world)
            .collect::<Vec<_>>();
        assert_eq!(owners, vec![mesh]);

        for _ in 0..MAX_CAMERAS + 4 {
            world.spawn((Camera3d::default(), MatchEntity));
        }
        assert_eq!(camera_candidates(&mut world).len(), MAX_CAMERAS + 1);
        // The bounded sentinel causes update_scope to refuse the whole scope.
    }

    #[test]
    fn receiver_sweep_includes_nearby_off_view_caster_and_rejects_invalid_bounds() {
        let region = caster_region(glam::Vec3::ZERO, glam::Vec3::Z, 10.0, 20.0).unwrap();
        assert!(
            region.intersects(
                Bounds::new(
                    glam::Vec3::new(-1.0, -1.0, 25.0),
                    glam::Vec3::new(1.0, 1.0, 26.0)
                )
                .unwrap()
            )
        );
        assert!(
            !region.intersects(
                Bounds::new(glam::Vec3::splat(100.0), glam::Vec3::splat(101.0)).unwrap()
            )
        );
        assert!(caster_region(glam::Vec3::NAN, glam::Vec3::Z, 10.0, 20.0).is_none());
        assert!(caster_region(glam::Vec3::ZERO, glam::Vec3::ZERO, 10.0, 20.0).is_none());
        assert!(Bounds::new(glam::Vec3::ONE, glam::Vec3::ZERO).is_none());
        let moved =
            caster_region(glam::Vec3::new(50.0, 0.0, 0.0), glam::Vec3::Z, 10.0, 20.0).unwrap();
        assert!(!moved.intersects(Bounds::new(glam::Vec3::ZERO, glam::Vec3::ONE).unwrap()));
    }

    #[test]
    fn reserved_layer_does_not_extend_sky_viewmodel_or_translucent_materials() {
        assert!(main_layers(None));
        assert!(main_layers(Some(&RenderLayers::from_layers(&[0, 5]))));
        assert!(!main_layers(Some(&crate::sky::sky_layer())));
        assert!(!main_layers(Some(&crate::viewmodel::layer())));
        assert!(!main_layers(Some(&RenderLayers::from_layers(&[0, 1]))));
        assert!(caster_material(AlphaMode::Mask(0.37)));
        assert!(!caster_material(AlphaMode::Blend));
        assert!(!caster_material(AlphaMode::Add));
    }

    #[test]
    fn teardown_restores_recorded_masks_material_and_visibility_without_touching_other_entities() {
        let mut world = World::new();
        let original = Handle::<WorldMaterial>::default();
        let layers = RenderLayers::from_layers(&[0, 5]);
        let owner = world
            .spawn((
                MatchEntity,
                layers.clone().with(CASTER_LAYER),
                Visibility::Inherited,
                Override {
                    layers: Some(layers.clone()),
                    material: Some(original.clone()),
                    visibility: Some(Visibility::Hidden),
                },
            ))
            .id();
        let persistent = world
            .spawn((Persistent, RenderLayers::layer(5), Visibility::Hidden))
            .id();
        let mut state = DiagnosticState::default();
        state.light = Some(world.spawn(DiagnosticLight).id());
        let light = state.light.unwrap();
        clear_scope(&mut world, &mut state);
        assert_eq!(world.get::<RenderLayers>(owner), Some(&layers));
        assert_eq!(world.get::<Visibility>(owner), Some(&Visibility::Hidden));
        assert_eq!(
            world.get::<MeshMaterial3d<WorldMaterial>>(owner).unwrap().0,
            original
        );
        assert!(world.get::<Override>(owner).is_none());
        assert_eq!(
            world.get::<RenderLayers>(persistent),
            Some(&RenderLayers::layer(5))
        );
        assert!(world.get_entity(light).is_err());
        clear_scope(&mut world, &mut state); // repeated cancellation is safe
    }

    #[test]
    fn shadow_uniform_prefix_matches_actual_world_material_serialization() {
        use bevy::render::render_resource::encase::UniformBuffer;
        let params = crate::material::WorldParams {
            color: Vec4::new(0.1, 0.2, 0.3, 0.4),
            flags: flags::ALPHA_TEST | flags::BLEND | flags::SHADOW_DIAGNOSTIC,
            alpha_ref: 0.37,
            exposure: 2.5,
            lightmap_scale: 0.75,
            base2_uv: Vec4::new(2.0, 2.0, 0.0, 0.0),
            normal2_uv: Vec4::new(3.0, 4.0, 0.1, 0.2),
            modulate_uv: Vec4::new(0.65, 0.65, 0.5, -0.5),
            ..Default::default()
        };
        let mut buffer = UniformBuffer::new(Vec::new());
        buffer.write(&params).unwrap();
        let bytes = buffer.into_inner();
        let f = |offset| f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        assert_eq!([f(0), f(4), f(8), f(12)], params.color.to_array());
        assert_eq!(
            u32::from_le_bytes(bytes[16..20].try_into().unwrap()),
            params.flags
        );
        assert_eq!(
            [f(20), f(24), f(28)],
            [params.alpha_ref, params.exposure, params.lightmap_scale]
        );
        for (offset, expected) in [(32, params.base2_uv), (48, params.normal2_uv), (64, params.modulate_uv)] {
            assert_eq!([f(offset), f(offset + 4), f(offset + 8), f(offset + 12)], expected.to_array());
        }
        // Forward and shadow-depth alpha sampling share this80-byte prefix.
        assert!(bytes.len() > 80);
    }
}
