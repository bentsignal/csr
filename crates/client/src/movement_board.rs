//! Render the imported board with its original skin, inverse binds, textures,
//! and the coordinator's completed deck/truck/wheel bones. No baked orientation.
use super::Visuals;
use crate::{coords::to_bevy, player::SimWorld};
use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::NoFrustumCulling,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use std::{path::Path, sync::Arc};
struct Part {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uv: Vec<[f32; 2]>,
    indices: Vec<u32>,
    weights: Vec<[f32; 4]>,
    joints: Vec<[u16; 4]>,
    inverse: Vec<Mat4>,
    names: Vec<String>,
    color: [f32; 4],
    texture: Option<(u32, u32, Vec<u8>)>,
}
pub struct Model {
    parts: Vec<Part>,
    center: Vec3,
}
impl Model {
    pub fn load(root: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(root.join("private/skater.glb")).map_err(|e| e.to_string())?;
        let gltf = gltf::Gltf::from_slice(&bytes).map_err(|e| e.to_string())?;
        let blob = gltf.blob.as_deref().ok_or("board GLB binary missing")?;
        let buffer = |b: gltf::Buffer<'_>| (b.index() == 0).then_some(blob);
        let mut parts = vec![];
        for node in gltf.nodes() {
            let (Some(mesh), Some(skin)) = (node.mesh(), node.skin()) else {
                continue;
            };
            let names: Vec<_> = skin
                .joints()
                .map(|n| n.name().unwrap_or("").to_string())
                .collect();
            let board: Vec<_> = names
                .iter()
                .map(|n| n == "SKATEBOARD_ROOT" || n.contains("TRUCK") || n.contains("WHEEL"))
                .collect();
            let inverse: Vec<_> = skin
                .reader(buffer)
                .read_inverse_bind_matrices()
                .ok_or("board inverse binds missing")?
                .map(|m| Mat4::from_cols_array_2d(&m))
                .collect();
            if inverse.len() != names.len() || inverse.iter().any(|m| !m.is_finite()) {
                return Err("invalid board inverse binds".into());
            }
            for primitive in mesh.primitives() {
                let reader = primitive.reader(buffer);
                let positions: Vec<_> = reader
                    .read_positions()
                    .ok_or("board positions missing")?
                    .collect();
                let normals: Vec<_> = reader
                    .read_normals()
                    .ok_or("board normals missing")?
                    .collect();
                let weights: Vec<_> = reader
                    .read_weights(0)
                    .ok_or("board weights missing")?
                    .into_f32()
                    .collect();
                let joints: Vec<_> = reader
                    .read_joints(0)
                    .ok_or("board joints missing")?
                    .into_u16()
                    .collect();
                let uv: Vec<_> = reader
                    .read_tex_coords(0)
                    .map(|u| u.into_f32().collect())
                    .unwrap_or(vec![[0.; 2]; positions.len()]);
                let indices: Vec<_> = reader
                    .read_indices()
                    .ok_or("board indices missing")?
                    .into_u32()
                    .collect();
                if positions.len() > 100_000
                    || [normals.len(), weights.len(), joints.len(), uv.len()]
                        .iter()
                        .any(|n| *n != positions.len())
                    || indices.len() % 3 != 0
                    || indices.iter().any(|i| *i as usize >= positions.len())
                    || joints.iter().flatten().any(|i| *i as usize >= names.len())
                    || positions
                        .iter()
                        .flatten()
                        .chain(normals.iter().flatten())
                        .chain(uv.iter().flatten())
                        .any(|v| !v.is_finite())
                    || weights
                        .iter()
                        .flatten()
                        .any(|w| !w.is_finite() || *w < 0. || *w > 1.)
                {
                    return Err("invalid board skin arrays".into());
                }
                let on_board = |i: usize| {
                    (0..4)
                        .filter(|j| board.get(joints[i][*j] as usize).copied().unwrap_or(false))
                        .map(|j| weights[i][j])
                        .sum::<f32>()
                        > 0.99
                };
                let indices: Vec<_> = indices
                    .chunks_exact(3)
                    .filter(|t| t.iter().all(|i| on_board(*i as usize)))
                    .flatten()
                    .copied()
                    .collect();
                if indices.is_empty() {
                    continue;
                }
                // Compact the selected board vertices. The GLB also contains a
                // skater; neither its body nor its vertices are rendered here.
                let mut selected = std::collections::BTreeMap::new();
                for i in &indices {
                    let count = selected.len() as u32;
                    selected.entry(*i).or_insert(count);
                }
                let mut old: Vec<_> = selected.iter().map(|(a, b)| (*b, *a as usize)).collect();
                old.sort_by_key(|v| v.0);
                let old: Vec<_> = old.into_iter().map(|v| v.1).collect();
                let pbr = primitive.material().pbr_metallic_roughness();
                let texture = if let Some(info) = pbr.base_color_texture() {
                    let source = info.texture().source();
                    let gltf::image::Source::View { view, .. } = source.source() else {
                        return Err("board textures must be embedded".into());
                    };
                    let bytes = blob
                        .get(view.offset()..view.offset() + view.length())
                        .ok_or("board texture range invalid")?;
                    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
                        .with_guessed_format()
                        .map_err(|e| e.to_string())?;
                    let mut limits = image::Limits::default();
                    limits.max_image_width = Some(4096);
                    limits.max_image_height = Some(4096);
                    limits.max_alloc = Some(64 << 20);
                    reader.limits(limits);
                    let image = reader.decode().map_err(|e| e.to_string())?.to_rgba8();
                    Some((image.width(), image.height(), image.into_raw()))
                } else {
                    None
                };
                parts.push(Part {
                    positions: old.iter().map(|i| positions[*i]).collect(),
                    normals: old.iter().map(|i| normals[*i]).collect(),
                    uv: old.iter().map(|i| uv[*i]).collect(),
                    weights: old.iter().map(|i| weights[*i]).collect(),
                    joints: old.iter().map(|i| joints[*i]).collect(),
                    indices: indices.iter().map(|i| selected[i]).collect(),
                    inverse: inverse.clone(),
                    names: names.clone(),
                    color: pbr.base_color_factor(),
                    texture,
                });
            }
        }
        if parts.is_empty() {
            return Err("no skinned board found".into());
        }
        let low = parts
            .iter()
            .flat_map(|p| &p.positions)
            .map(|p| Vec3::from_array(*p))
            .fold(Vec3::splat(f32::INFINITY), Vec3::min);
        let high = parts
            .iter()
            .flat_map(|p| &p.positions)
            .map(|p| Vec3::from_array(*p))
            .fold(Vec3::splat(f32::NEG_INFINITY), Vec3::max);
        Ok(Self {
            parts,
            center: (low + high) * 0.5,
        })
    }
}
#[derive(Component)]
pub struct Instance {
    player: sim::PlayerId,
    mode: String,
    part: usize,
    model: Arc<Model>,
    mesh: Handle<Mesh>,
    display: u8,
}
pub fn draw(
    mut commands: Commands,
    world: Res<SimWorld>,
    visuals: Option<Res<Visuals>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    time: Res<Time<Fixed>>,
    local: Res<crate::player::LocalPlayer>,
    mut instances: Query<(Entity, &mut Instance)>,
) {
    for (entity, mut instance) in &mut instances {
        let Some(player) = world.0.player(instance.player).filter(|p| p.alive) else {
            commands.entity(entity).despawn();
            continue;
        };
        let state = player
            .mechanic
            .as_ref()
            .filter(|s| s.id == instance.mode)
            .and_then(|s| s.external.as_ref());
        let held = player.mechanic.is_none()
            && player
                .equipped_mechanic
                .as_ref()
                .is_some_and(|item| item.id == instance.mode);
        if state.is_none() && !held {
            if instance.display != 0 {
                if let Some(mut mesh) = meshes.get_mut(&instance.mesh) {
                    mesh.insert_attribute(
                        Mesh::ATTRIBUTE_POSITION,
                        vec![[0.; 3]; instance.model.parts[instance.part].positions.len()],
                    );
                }
            }
            instance.display = 0;
            continue;
        }
        let part = &instance.model.parts[instance.part];
        let transforms = state.map(|state| {
            let sampled = mechanics::presentation::bones(
                &state.previous_pose,
                &state.pose,
                &state.rig,
                world.0.dt() * time.overstep_fraction(),
            );
            let conversion = Mat4::from_cols_array_2d(&state.rig.to_source);
            let basis = Mat4::from_cols_array_2d(&state.rig.bone_basis);
            part.names
                .iter()
                .zip(&part.inverse)
                .map(|(name, inverse)| {
                    state
                        .rig
                        .names
                        .iter()
                        .position(|n| n == name)
                        .and_then(|i| sampled.get(i))
                        .map(|m| conversion * *m * basis * *inverse)
                        .unwrap_or(Mat4::IDENTITY)
                })
                .collect::<Vec<_>>()
        });
        // An equipment preview uses the imported rest mesh. Native bone output
        // still owns the board whenever the player is actually riding it.
        let angles = if player.id == local.id {
            local.render_angles
        } else {
            player.view_angles
        };
        let eye = if player.id == local.id {
            local.render_eye
        } else {
            player.eye()
        };
        let (forward, right, up) = movement::angle_vectors(angles);
        let held_basis = Mat3::from_cols(-right, -forward, up);
        let center = instance.model.center;
        let mut positions = Vec::with_capacity(part.positions.len());
        let mut normals = Vec::with_capacity(part.positions.len());
        for i in 0..part.positions.len() {
            let mut p = Vec3::ZERO;
            let mut n = Vec3::ZERO;
            if let Some(transforms) = &transforms {
                for j in 0..4 {
                    let w = part.weights[i][j];
                    if w == 0. {
                        continue;
                    }
                    let m = transforms[part.joints[i][j] as usize];
                    p += m.transform_point3(Vec3::from_array(part.positions[i])) * w;
                    n += m.transform_vector3(Vec3::from_array(part.normals[i])) * w;
                }
            } else {
                p = eye + forward * 25. + right * 12. - up * 12.
                    + held_basis * (Vec3::from_array(part.positions[i]) - center) / 0.0254;
                n = held_basis * Vec3::from_array(part.normals[i]);
            }
            positions.push(to_bevy(p).to_array());
            normals.push(to_bevy(n.normalize_or_zero()).to_array());
        }
        if let Some(mut mesh) = meshes.get_mut(&instance.mesh) {
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        }
        instance.display = if held { 1 } else { 2 };
    }
    let Some(visuals) = visuals else { return };
    // Allocate and queue board materials with the map, including before any
    // mode switch. Inactive meshes have degenerate triangles: no pixels, but
    // their real material specialization is prepared by the renderer already.
    for player in world
        .0
        .players
        .iter()
        .filter(|p| p.alive && (!p.is_bot || p.mechanic.is_some()))
    {
        for (mode, model) in &visuals.1 {
            if instances
                .iter()
                .any(|(_, i)| i.player == player.id && i.mode == *mode)
            {
                continue;
            }
            for (i, part) in model.parts.iter().enumerate() {
                let mesh = Mesh::new(
                    PrimitiveTopology::TriangleList,
                    RenderAssetUsages::default(),
                )
                .with_inserted_attribute(
                    Mesh::ATTRIBUTE_POSITION,
                    vec![[0.; 3]; part.positions.len()],
                )
                .with_inserted_attribute(
                    Mesh::ATTRIBUTE_NORMAL,
                    vec![[0., 1., 0.]; part.positions.len()],
                )
                .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, part.uv.clone())
                .with_inserted_indices(Indices::U32(part.indices.clone()));
                let texture = part.texture.as_ref().map(|(width, height, data)| {
                    images.add(Image::new(
                        Extent3d {
                            width: *width,
                            height: *height,
                            depth_or_array_layers: 1,
                        },
                        TextureDimension::D2,
                        data.clone(),
                        TextureFormat::Rgba8UnormSrgb,
                        RenderAssetUsages::default(),
                    ))
                });
                let material = materials.add(StandardMaterial {
                    base_color: Color::srgba(
                        part.color[0],
                        part.color[1],
                        part.color[2],
                        part.color[3],
                    ),
                    base_color_texture: texture,
                    perceptual_roughness: 0.7,
                    cull_mode: None,
                    unlit: true,
                    ..Default::default()
                });
                let mesh = meshes.add(mesh);
                commands.spawn((
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(material),
                    Transform::default(),
                    NoFrustumCulling,
                    Instance {
                        player: player.id,
                        mode: mode.clone(),
                        part: i,
                        model: model.clone(),
                        mesh,
                        display: 0,
                    },
                ));
            }
        }
    }
}
