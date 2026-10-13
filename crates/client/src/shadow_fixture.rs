//! Synthetic material/depth/factor proof, not installed-map admission or fidelity.
use super::{CASTER_LAYER, enabled};
use crate::{
    material::{WorldMaterial, WorldParams, flags},
    remote_render_gate::{self, GateStatus, GateTicket, RenderGate},
};
use bevy::{
    asset::{AssetId, RenderAssetUsages, embedded_asset},
    audio::AudioPlugin,
    camera::{
        ScalingMode,
        primitives::{Aabb, Frustum},
        visibility::{CascadesVisibleEntities, RenderLayers},
    },
    core_pipeline::tonemapping::Tonemapping,
    image::ImageSampler,
    light::{CascadeShadowConfigBuilder, DirectionalLight, NotShadowCaster},
    mesh::Indices,
    pbr::{Material, MaterialPlugin},
    prelude::*,
    render::{
        render_resource::{Extent3d, PrimitiveTopology, TextureDimension, TextureFormat},
        view::screenshot::{Screenshot, ScreenshotCaptured},
    },
    window::WindowResolution,
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

const WIDTH: u32 = 960;
const HEIGHT: u32 = 640;
const MAX_FRAMES: u32 = 3600;
// Synthetic layout unit =10 Source inches. Twenty-inch mask holes remain
// larger than the unchanged Bevy bias/filter footprint; this is not tuning.
const SCALE: f32 = 10.0;

#[derive(Resource)]
struct Proof {
    output: PathBuf,
    start: Instant,
    frames: u32,
    ticket: Option<GateTicket>,
    ready: Option<u32>,
    requested: bool,
    camera: Option<Entity>,
    light: Option<Entity>,
    offview: Option<Entity>,
    meshes: Vec<AssetId<Mesh>>,
    images: Vec<AssetId<Image>>,
    // Real camera-projected receiver rectangles, prepared after viewport setup.
    rectangles: Vec<[u32; 4]>,
    completed: bool,
}

/// CLI: `shadow_fixture --output <fresh absolute PNG>`. Set the immutable
/// startup env externally to1 for on; leave it absent for the off comparison.
/// No game data, controls, native processes or normal CsgoPlugin are involved.
pub fn run() -> i32 {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 || args[0] != "--output" {
        eprintln!("Expected --output <fresh absolute PNG>");
        return 2;
    }
    let output = PathBuf::from(&args[1]);
    if !output.is_absolute()
        || output.exists()
        || output.extension().and_then(|s| s.to_str()) != Some("png")
        || !output.parent().is_some_and(|p| p.is_dir())
    {
        eprintln!("Output must be a fresh absolute PNG in an existing directory");
        return 2;
    }
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "CSR synthetic shadow pipeline".into(),
                    resolution: WindowResolution::new(WIDTH, HEIGHT)
                        .with_scale_factor_override(1.0),
                    ..default()
                }),
                ..default()
            })
            .disable::<AudioPlugin>(),
    )
    .add_plugins(MaterialPlugin::<WorldMaterial>::default());
    embedded_asset!(&mut app, "shaders/world.wgsl");
    if enabled() {
        embedded_asset!(&mut app, "shaders/world_shadow.wgsl");
    }
    if remote_render_gate::install(&mut app).is_err() {
        return 2;
    }
    app.insert_resource(Proof {
        output,
        start: Instant::now(),
        frames: 0,
        ticket: None,
        ready: None,
        requested: false,
        camera: None,
        light: None,
        offview: None,
        meshes: Vec::new(),
        images: Vec::new(),
        rectangles: Vec::new(),
        completed: false,
    })
    .add_systems(Startup, setup)
    .add_systems(Last, advance);
    let exit = app.run();
    if exit.is_success() && app.world().resource::<Proof>().completed {
        0
    } else {
        1
    }
}

fn quad(reverse: bool) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![[-1., 0., -1.], [-1., 0., 1.], [1., 0., 1.], [1., 0., -1.]],
    );
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_NORMAL,
        vec![[0., if reverse { -1. } else { 1. }, 0.]; 4],
    );
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_UV_0,
        vec![[0., 0.], [0., 1.], [1., 1.], [1., 0.]],
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[1.; 4]; 4]);
    mesh.insert_indices(Indices::U32(if reverse {
        vec![0, 2, 1, 0, 3, 2]
    } else {
        vec![0, 1, 2, 0, 2, 3]
    }));
    mesh
}

fn setup(world: &mut World) {
    let on = enabled();
    info!(
        "shadow-fixture mode={} material_casting={} world_shader={} depth_shader={} production_map_scope=NOT_INSTALLED",
        if on { "on" } else { "off" },
        <WorldMaterial as Material>::enable_shadows(),
        crate::material::SHADER_PATH,
        if on { super::SHADOW_SHADER } else { "default" }
    );
    let white = world
        .resource_mut::<Assets<Image>>()
        .add(crate::textures::solid([255; 4]));
    let mut checker = Vec::with_capacity(8 * 8 * 4);
    for y in 0..8 {
        for x in 0..8 {
            checker.extend_from_slice(&[
                255,
                255,
                255,
                if (x / 4 + y / 4) % 2 == 0 { 255 } else { 0 },
            ]);
        }
    }
    let mut mask = Image::new(
        Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        checker,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mask.sampler = ImageSampler::nearest();
    let mask = world.resource_mut::<Assets<Image>>().add(mask);
    let array = world.resource_mut::<Assets<Image>>().add(Image::new(
        Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 2,
        },
        TextureDimension::D2,
        vec![255; 8],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    ));
    let material = |base: Handle<Image>, masked: bool, cull: bool| WorldMaterial {
        params: WorldParams {
            flags: flags::UNLIT
                | if masked { flags::ALPHA_TEST } else { 0 }
                | if on { flags::SHADOW_DIAGNOSTIC } else { 0 },
            ..default()
        },
        base,
        base2: white.clone(),
        lightmap: array.clone(),
        modulate: white.clone(),
        normal: white.clone(),
        normal2: white.clone(),
        prop_light: white.clone(),
        phong_map: white.clone(),
        skin_surface: white.clone(),
        skin_position: white.clone(),
        alpha_mode: if masked {
            AlphaMode::Mask(0.5)
        } else {
            AlphaMode::Opaque
        },
        cull,
    };
    let opaque =
        world
            .resource_mut::<Assets<WorldMaterial>>()
            .add(material(white.clone(), false, true));
    let front =
        world
            .resource_mut::<Assets<WorldMaterial>>()
            .add(material(mask.clone(), true, true));
    let back =
        world
            .resource_mut::<Assets<WorldMaterial>>()
            .add(material(mask.clone(), true, false));
    let forward = world.resource_mut::<Assets<Mesh>>().add(quad(false));
    let reversed = world.resource_mut::<Assets<Mesh>>().add(quad(true));
    let layers = if on {
        RenderLayers::default().with(CASTER_LAYER)
    } else {
        RenderLayers::default()
    };
    let camera = world
        .spawn((
            Camera3d::default(),
            Camera {
                clear_color: ClearColorConfig::Custom(Color::srgb(0.12, 0.25, 0.5)),
                ..default()
            },
            Projection::Orthographic(OrthographicProjection {
                scaling_mode: ScalingMode::FixedVertical {
                    viewport_height: 20. * SCALE,
                },
                near: 0.1,
                far: 100. * SCALE,
                ..OrthographicProjection::default_3d()
            }),
            Transform::from_xyz(0., 20. * SCALE, 16. * SCALE).looking_at(Vec3::ZERO, Vec3::Y),
            Tonemapping::None,
            layers.clone(),
        ))
        .id();
    let light = on.then(|| {
        world
            .spawn((
                DirectionalLight {
                    illuminance: 0.,
                    shadow_maps_enabled: true,
                    ..default()
                },
                CascadeShadowConfigBuilder {
                    maximum_distance: 100. * SCALE,
                    ..default()
                }
                .build(),
                Transform::IDENTITY.looking_to(Vec3::new(1., -1., 0.).normalize(), Vec3::Z),
                RenderLayers::layer(CASTER_LAYER),
            ))
            .id()
    });
    world.spawn((
        Mesh3d(forward.clone()),
        MeshMaterial3d(opaque.clone()),
        Transform::from_scale(Vec3::new(15. * SCALE, 1., 12. * SCALE)),
        layers.clone(),
        NotShadowCaster,
    ));
    for (x, mesh, material) in [
        (-5., forward.clone(), opaque.clone()),
        (0., forward.clone(), front),
        (5., reversed.clone(), back),
    ] {
        world.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::from_xyz(x * SCALE, 4. * SCALE, -2. * SCALE).with_scale(Vec3::new(
                2. * SCALE,
                1.,
                2. * SCALE,
            )),
            layers.clone(),
        ));
    }
    let offview = world
        .spawn((
            Mesh3d(forward.clone()),
            MeshMaterial3d(opaque),
            Transform::from_xyz(-20. * SCALE, 8. * SCALE, 3. * SCALE).with_scale(Vec3::new(
                2. * SCALE,
                1.,
                2. * SCALE,
            )),
            layers,
        ))
        .id();
    let mut proof = world.resource_mut::<Proof>();
    proof.camera = Some(camera);
    proof.light = light;
    proof.offview = Some(offview);
    proof.meshes = vec![forward.id(), reversed.id()];
    proof.images = vec![white.id(), mask.id(), array.id()];
    info!(
        "shadow-fixture scene cameras=1 receivers=1 casters=4 materials=3 meshes=2 images=3 lights={} mask_cutoff=0.5 reverse_mask_cull=false",
        usize::from(on)
    );
}

fn advance(world: &mut World) {
    let mut proof = world.remove_resource::<Proof>().unwrap();
    proof.frames += 1;
    if proof.frames > MAX_FRAMES || proof.start.elapsed() > Duration::from_secs(30) {
        error!(
            "shadow-fixture deadline frame={} capture_requested={}",
            proof.frames, proof.requested
        );
        world.write_message(AppExit::error());
    } else if !proof.requested {
        if proof.ticket.is_none() {
            proof.ticket = world
                .resource_mut::<RenderGate>()
                .begin(&proof.meshes, &proof.images)
                .ok();
        }
        match proof.ticket.as_ref().map(GateTicket::status) {
            Some(GateStatus::Failed | GateStatus::Cancelled) | None => {
                error!("shadow-fixture render gate failed");
                world.write_message(AppExit::error());
            }
            Some(GateStatus::Pending) => {}
            Some(GateStatus::Ready) => {
                let ready = *proof.ready.get_or_insert(proof.frames);
                if proof.frames >= ready + 5 {
                    let camera_entity = proof.camera.unwrap();
                    let camera = world.get::<Camera>(camera_entity).unwrap();
                    let transform = world.get::<GlobalTransform>(camera_entity).unwrap();
                    let offview = proof.offview.unwrap();
                    let outside = world
                        .get::<Frustum>(camera_entity)
                        .zip(world.get::<Aabb>(offview))
                        .zip(world.get::<GlobalTransform>(offview))
                        .is_some_and(|((frustum, aabb), transform)| {
                            !frustum.intersects_obb(aabb, &transform.affine(), true, true)
                        });
                    let visible_caster = proof
                        .light
                        .and_then(|e| world.get::<CascadesVisibleEntities>(e))
                        .is_some_and(|v| {
                            v.entities
                                .values()
                                .flatten()
                                .any(|v| v.entities.contains(&offview))
                        });
                    if !outside || (enabled() && !visible_caster) {
                        error!(
                            "shadow-fixture offview admission failed outside={outside} cascade_member={visible_caster}"
                        );
                        world.write_message(AppExit::error());
                    } else {
                        for (x, z) in [(-1., -2.), (4., -2.), (9., -2.), (-12., 3.)] {
                            let mut lo = Vec2::splat(f32::INFINITY);
                            let mut hi = Vec2::splat(f32::NEG_INFINITY);
                            for dx in [-1.5, 1.5] {
                                for dz in [-1.5, 1.5] {
                                    if let Ok(p) = camera.world_to_viewport(
                                        transform,
                                        Vec3::new((x + dx) * SCALE, 0., (z + dz) * SCALE),
                                    ) {
                                        lo = lo.min(p);
                                        hi = hi.max(p);
                                    }
                                }
                            }
                            if lo.is_finite()
                                && hi.is_finite()
                                && lo.min_element() >= 0.
                                && hi.x <= WIDTH as f32
                                && hi.y <= HEIGHT as f32
                            {
                                proof.rectangles.push([
                                    lo.x.ceil() as u32,
                                    lo.y.ceil() as u32,
                                    hi.x.floor() as u32,
                                    hi.y.floor() as u32,
                                ]);
                            }
                        }
                        if proof.rectangles.len() != 4 {
                            error!("shadow-fixture receiver projections missing");
                            world.write_message(AppExit::error());
                        } else {
                            info!(
                                "shadow-fixture rendergate=Ready generation={} frame={} offview_outside_camera=true offview_in_light={visible_caster} receiver_rectangles={:?}",
                                proof.ticket.as_ref().unwrap().generation(),
                                proof.frames,
                                proof.rectangles
                            );
                            world.spawn(Screenshot::primary_window()).observe(captured);
                            proof.requested = true;
                        }
                    }
                }
            }
        }
    }
    world.insert_resource(proof);
}

fn captured(
    event: On<ScreenshotCaptured>,
    mut proof: ResMut<Proof>,
    mut exit: MessageWriter<AppExit>,
) {
    let image = match event.image.clone().try_into_dynamic() {
        Ok(i) => i.to_rgb8(),
        Err(e) => {
            error!("shadow-fixture capture conversion: {e}");
            exit.write(AppExit::error());
            return;
        }
    };
    let mut failures = usize::from(image.dimensions() != (WIDTH, HEIGHT));
    for (index, &[x0, y0, x1, y1]) in proof.rectangles.iter().enumerate() {
        let (mut dark, mut white, mut magenta, mut count) = (0usize, 0usize, 0usize, 0usize);
        for y in y0..y1 {
            for x in x0..x1 {
                if let Some(p) = image.get_pixel_checked(x, y) {
                    count += 1;
                    dark += usize::from(p.0.iter().all(|&c| c < 48));
                    white += usize::from(p.0.iter().all(|&c| c > 224));
                    magenta += usize::from(p[0] > 224 && p[1] < 48 && p[2] > 224);
                }
            }
        }
        let valid = count > 0
            && magenta == 0
            && if !enabled() {
                dark == 0
            } else if index == 1 || index == 2 {
                dark * 10 > count && white * 10 > count
            } else {
                dark * 2 > count
            };
        info!(
            "shadow-fixture pixels roi={index} count={count} dark={dark} white={white} magenta={magenta} valid={valid}"
        );
        failures += usize::from(!valid);
    }
    if let Err(e) = image.save(&proof.output) {
        error!("shadow-fixture PNG save: {e}");
        failures += 1;
    }
    proof.completed = failures == 0;
    info!(
        "shadow-fixture screenshot_ACK=true frame={} dimensions={:?} failures={failures} output={}",
        proof.frames,
        image.dimensions(),
        proof.output.display()
    );
    exit.write(if proof.completed {
        AppExit::Success
    } else {
        AppExit::error()
    });
}
