//! Presentation and session loading for generic movement packs.
use crate::{
    coords::{rotation_to_bevy, to_bevy},
    player::SimWorld,
};
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use std::{collections::HashMap, sync::Arc};
#[path="movement_board.rs"]
pub mod full_board;

pub struct Visual {
    pub mesh: Arc<mechanics::VisualMesh>,
}
#[derive(Resource, Default)]
pub struct Visuals(pub HashMap<String, Visual>,pub HashMap<String,Arc<full_board::Model>>);
#[derive(Component)]
pub struct Board {
    player: sim::PlayerId,
    mode: String,
}

pub fn third_person(player: &sim::Player) -> bool {
    player.alive
        && player.mechanic.is_some()
        && player.active_weapon().is_none_or(|w| w.zoom_level == 0)
}
pub fn native_camera(player:&sim::Player)->Option<(glam::Vec3,glam::Vec3,glam::Vec3,f32)>{
    let state=player.mechanic.as_ref()?.external.as_ref()?;
    let camera=glam::Mat4::from_cols_array_2d(state.pose.camera.as_ref()?);
    Some((camera.w_axis.truncate(),camera.z_axis.truncate().normalize_or_zero(),camera.y_axis.truncate().normalize_or_zero(),state.pose.fov))
}

pub fn load() -> (Vec<Arc<sim::mechanics::Module>>, Visuals) {
    let mut modules = Vec::new();
    let mut visuals = Visuals::default();
    let (packs, errors) = packs::Installed::current().load();
    for error in errors {
        warn!("Movement pack: {error}");
    }
    for pack in packs {
        for def in &pack.manifest().mechanics {
            let result = (|| -> Result<_, String> {
                let bytes = pack.file(&def.module).ok_or("missing mechanics module")?;
                let config = mechanics::parse_config(
                    &pack.file(&def.config).ok_or("missing mechanics config")?,
                )?;
                let (program,external) = if def.api_version==2 {
                    let root=mechanics::data::materialize(&bytes,std::path::Path::new("mechanics-cache"))?;
                    visuals.1.insert(def.id.clone(),Arc::new(full_board::Model::load(&root)?));
                    let executable=std::env::current_exe().map_err(|e|e.to_string())?.with_file_name("Skate-Runtime.exe");
                    (None,Some(Arc::new(mechanics::external::Program::new(executable,root)?)))
                } else {(Some(mechanics::Program::compile(&bytes)?),None)};
                if let Some(path) = &def.model {
                    let mesh = mechanics::VisualMesh::parse(
                        &pack.file(path).ok_or("missing mechanics model")?,
                    )?;
                    visuals.0.insert(
                        def.id.clone(),
                        Visual {
                            mesh: Arc::new(mesh),
                        },
                    );
                }
                Ok(Arc::new(sim::mechanics::Module {
                    id: def.id.clone(),
                    name: def.name.clone(),
                    archive_hash: pack.archive_hash().into(),
                    program,
                    config,
                    external,
                }))
            })();
            match result {
                Ok(module) => modules.push(module),
                Err(error) => warn!("{}: {error}", def.name),
            }
        }
    }
    (modules, visuals)
}

pub fn camera_eye(
    eye: glam::Vec3,
    angles: glam::Vec3,
    collision: &collision::CollisionWorld,
) -> glam::Vec3 {
    let (forward, _, _) = movement::angle_vectors(angles);
    let target = eye - forward * 140. + glam::Vec3::Z * 18.;
    let hit = collision.trace_box(
        eye,
        target,
        glam::Vec3::splat(-5.),
        glam::Vec3::splat(5.),
        collision::contents::MASK_PLAYERSOLID,
    );
    if hit.start_solid || hit.all_solid {
        eye
    } else {
        hit.end
    }
}

pub fn draw(
    mut commands: Commands,
    world: Res<SimWorld>,
    visuals: Option<Res<Visuals>>,
    poses: Res<crate::interpolation::RenderPoses>,
    time: Res<Time<Fixed>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut boards: Query<(Entity, &Board, &mut Transform)>,
) {
    for (entity, board, mut transform) in &mut boards {
        let player = world.0.player(board.player).filter(|p| p.alive);
        let state = player
            .and_then(|p| p.mechanic.as_ref())
            .filter(|s| s.id == board.mode);
        let Some((player, state)) = player.zip(state) else {
            commands.entity(entity).despawn();
            continue;
        };
        let pose = poses.sample(player, time.overstep_fraction());
        *transform = Transform::from_translation(to_bevy(pose.origin))
            .with_rotation(rotation_to_bevy(mdl::angles_to_mat3(state.angles)));
    }
    let Some(visuals) = visuals else {
        return;
    };
    for player in world
        .0
        .players
        .iter()
        .filter(|p| p.alive && p.mechanic.is_some())
    {
        let state = player.mechanic.as_ref().unwrap();
        if boards
            .iter()
            .any(|(_, board, _)| board.player == player.id && board.mode == state.id)
        {
            continue;
        }
        let Some(visual) = visuals.0.get(&state.id) else {
            continue;
        };
        let pose = poses.sample(player, time.overstep_fraction());
        let root = commands
            .spawn((
                Board {
                    player: player.id,
                    mode: state.id.clone(),
                },
                Transform::from_translation(to_bevy(pose.origin))
                    .with_rotation(rotation_to_bevy(mdl::angles_to_mat3(state.angles))),
                Visibility::Inherited,
            ))
            .id();
        for part in &visual.mesh.parts {
            let positions: Vec<[f32; 3]> = part
                .positions
                .iter()
                .map(|p| to_bevy(glam::Vec3::from_array(*p)).to_array())
                .collect();
            let normals: Vec<[f32; 3]> = part
                .normals
                .iter()
                .map(|p| to_bevy(glam::Vec3::from_array(*p)).to_array())
                .collect();
            let mesh = Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::default(),
            )
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
            .with_inserted_indices(Indices::U32(part.indices.clone()));
            commands.spawn((
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::srgba(
                        part.color[0],
                        part.color[1],
                        part.color[2],
                        part.color[3],
                    ),
                    perceptual_roughness: 0.8,
                    unlit: true,
                    ..Default::default()
                })),
                Transform::default(),
                ChildOf(root),
            ));
        }
    }
}
