//! Original Skate HUD, drawn from the movement provider's published triangles:
//! the trick display stretched over the window like the standalone game, and
//! the session-marker panel (uniform scale, top-left) while LB is held.
//! Layout, fonts and animation all come from the provider and the owner's
//! private pack; this module only rasterizes `mechanics::link::HudFrame`s.
use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::{NoFrustumCulling, RenderLayers},
    core_pipeline::tonemapping::{DebandDither, Tonemapping},
    image::ImageSampler,
    mesh::{Mesh2d, PrimitiveTopology},
    prelude::*,
    render::render_resource::{AsBindGroup, Extent3d, ShaderType, TextureDimension, TextureFormat},
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dPlugin, MeshMaterial2d},
    window::PrimaryWindow,
};
use mechanics::link::{HUD_CANVAS, HudFit, HudFrame};
use std::sync::Arc;

const LAYER: usize = 30;

pub(crate) fn install(app: &mut App) {
    bevy::asset::embedded_asset!(app, "shaders/skate_hud.wgsl");
    app.add_plugins(Material2dPlugin::<HudMaterial>::default());
}

#[derive(Clone, Copy, Debug, PartialEq, ShaderType)]
struct ColorTransform {
    multiply: Vec4,
    add: Vec4,
}
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub(crate) struct HudMaterial {
    #[uniform(0)]
    transform: ColorTransform,
    #[texture(1)]
    #[sampler(2)]
    texture: Handle<Image>,
}
impl Material2d for HudMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://client/shaders/skate_hud.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

#[derive(Component)]
pub(crate) struct HudCamera;
#[derive(Component)]
pub(crate) struct HudSlot;
struct Slot {
    entity: Entity,
    mesh: Handle<Mesh>,
    material: Handle<HudMaterial>,
}
#[derive(Default)]
pub(crate) struct State {
    table: Option<Arc<Vec<mechanics::external::hud::Image>>>,
    textures: Vec<Handle<Image>>,
    white: Option<Handle<Image>>,
    slots: Vec<Slot>,
    used: usize,
    drawn: Option<(Arc<HudFrame>, [f32; 2])>,
}

/// Canvas point to 2D camera space (origin centre, y up, logical pixels).
fn place(fit: HudFit, window: [f32; 2], x: f32, y: f32) -> [f32; 3] {
    let [w, h] = window;
    let scale = match fit {
        HudFit::Stretch => [w / HUD_CANVAS[0], h / HUD_CANVAS[1]],
        HudFit::ContainTopLeft => [(w / HUD_CANVAS[0]).min(h / HUD_CANVAS[1]); 2],
    };
    [x * scale[0] - w * 0.5, h * 0.5 - y * scale[1], 0.]
}

fn image(width: u32, height: u32, rgba: Vec<u8>) -> Image {
    let mut image = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    // Linear, clamp-to-edge: the standalone HUD's sampler.
    image.sampler = ImageSampler::linear();
    image
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update(
    mut commands: Commands,
    mut state: Local<State>,
    world: Res<crate::player::SimWorld>,
    local: Res<crate::player::LocalPlayer>,
    menu: Res<crate::menu::Menu>,
    playtest: Option<Res<crate::playtest::Panel>>,
    window: Query<&Window, With<PrimaryWindow>>,
    view: Query<&Msaa, (With<crate::player::ViewModelCamera>, Without<HudCamera>)>,
    mut camera: Query<(&mut Camera, &mut Msaa), With<HudCamera>>,
    mut slots: Query<&mut Visibility, With<HudSlot>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<HudMaterial>>,
) {
    let Ok(window) = window.single() else { return };
    let size = [window.width(), window.height()];
    let Ok((mut camera, mut msaa)) = camera.single_mut() else {
        // The overlay belongs to the match; a new match starts from scratch.
        *state = State::default();
        commands.spawn((
            crate::session::MatchEntity,
            HudCamera,
            Camera2d,
            Camera {
                order: 3,
                clear_color: ClearColorConfig::None,
                is_active: false,
                ..default()
            },
            Tonemapping::None,
            DebandDither::Disabled,
            view.single().copied().unwrap_or(Msaa::Off),
            RenderLayers::layer(LAYER),
        ));
        return;
    };
    // Cameras sharing the window's colour target must agree on sampling.
    if let Ok(view) = view.single() {
        if *msaa != *view {
            *msaa = *view;
        }
    }
    let shown = world
        .0
        .player(local.id)
        .filter(|p| {
            p.alive
                && !menu.open()
                && !playtest.as_ref().is_some_and(|p| p.open)
                && local.spectating.is_none()
        })
        .and_then(|p| {
            let mechanic = p.mechanic.as_ref()?;
            let session = mechanic.external.as_ref()?.session;
            let provider = world
                .0
                .mechanics
                .iter()
                .find(|m| m.id == mechanic.id)?
                .external
                .as_ref()?;
            Some((provider.hud(session)?, provider.hud_textures()))
        });
    let Some((frame, table)) = shown else {
        if camera.is_active {
            camera.is_active = false;
        }
        return;
    };
    if !camera.is_active {
        camera.is_active = true;
    }
    if !state.table.as_ref().is_some_and(|t| Arc::ptr_eq(t, &table)) {
        state.textures = table
            .iter()
            .map(|t| images.add(image(t.width, t.height, t.rgba.clone())))
            .collect();
        state.table = Some(table);
        state.drawn = None;
    }
    if state
        .drawn
        .as_ref()
        .is_some_and(|(f, s)| Arc::ptr_eq(f, &frame) && *s == size)
    {
        return;
    }
    let white = state
        .white
        .get_or_insert_with(|| images.add(image(1, 1, vec![255; 4])))
        .clone();
    let mut used = 0;
    for layer in &frame.layers {
        for draw in &layer.draws {
            let texture = match draw.texture {
                Some(t) => match state.textures.get(t as usize) {
                    Some(handle) => handle.clone(),
                    None => continue,
                },
                None => white.clone(),
            };
            let range = draw.first as usize..(draw.first + draw.count) as usize;
            let vertices = &layer.vertices[range];
            let mut mesh = Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::RENDER_WORLD,
            );
            mesh.insert_attribute(
                Mesh::ATTRIBUTE_POSITION,
                vertices
                    .iter()
                    .map(|v| place(layer.fit, size, v[0], v[1]))
                    .collect::<Vec<_>>(),
            );
            mesh.insert_attribute(
                Mesh::ATTRIBUTE_UV_0,
                vertices.iter().map(|v| [v[2], v[3]]).collect::<Vec<_>>(),
            );
            let material = HudMaterial {
                transform: ColorTransform {
                    multiply: Vec4::from_array(draw.multiply),
                    add: Vec4::from_array(draw.add),
                },
                texture,
            };
            if let Some(slot) = state.slots.get(used) {
                if let Some(mut old) = meshes.get_mut(&slot.mesh) {
                    *old = mesh;
                }
                if materials.get(&slot.material).is_none_or(|m| {
                    m.transform != material.transform || m.texture != material.texture
                }) {
                    if let Some(mut old) = materials.get_mut(&slot.material) {
                        *old = material;
                    }
                }
                if let Ok(mut visibility) = slots.get_mut(slot.entity) {
                    *visibility = Visibility::Visible;
                }
            } else {
                let mesh = meshes.add(mesh);
                let material = materials.add(material);
                // Painter's order: the 2D transparent phase sorts by z.
                let entity = commands
                    .spawn((
                        crate::session::MatchEntity,
                        HudSlot,
                        Mesh2d(mesh.clone()),
                        MeshMaterial2d(material.clone()),
                        Transform::from_xyz(0., 0., used as f32 * 0.001),
                        Visibility::Visible,
                        NoFrustumCulling,
                        RenderLayers::layer(LAYER),
                    ))
                    .id();
                state.slots.push(Slot {
                    entity,
                    mesh,
                    material,
                });
            }
            used += 1;
        }
    }
    for slot in &state.slots[used..state.used.max(used)] {
        if let Ok(mut visibility) = slots.get_mut(slot.entity) {
            *visibility = Visibility::Hidden;
        }
    }
    state.used = used;
    state.drawn = Some((frame, size));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn layers_map_the_original_canvas_onto_the_window() {
        let window = [1600., 900.];
        assert_eq!(place(HudFit::Stretch, window, 0., 0.), [-800., 450., 0.]);
        assert_eq!(place(HudFit::Stretch, window, 1280., 720.), [800., -450., 0.]);
        // A wider window keeps the marker panel's aspect and top-left anchor.
        let wide = [2560., 1080.];
        assert_eq!(place(HudFit::ContainTopLeft, wide, 0., 0.), [-1280., 540., 0.]);
        assert_eq!(place(HudFit::ContainTopLeft, wide, 1280., 720.), [640., -540., 0.]);
        assert_eq!(place(HudFit::Stretch, wide, 1280., 720.), [1280., -540., 0.]);
    }
}
