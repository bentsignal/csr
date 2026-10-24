//! Scope presentation. The sniper mask and optic dot are original VTF assets,
//! loaded from the user's installation; no game art is bundled.
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use crate::player::{LocalPlayer, SimWorld};

#[path = "optic_blur.rs"]
mod optic_blur;
pub use optic_blur::OpticBlurPlugin;

#[derive(Component)]
pub struct ScopePart(u8);

// Provisional presentation coefficients, not measured native blur parity. The
// input is the same milliradian inaccuracy used by the authoritative shot path.
const MAX_BLUR: f32 = 8.0;
const BLUR_STEPS: usize = 32;
const RETICLE_SIZE: usize = 96;
// Native stationary first-zoom captures at 1600x900 / 1920x1080 have visible
// openings of 757.5 / 907px, independent of hud_scaling 0.85 versus its 0.95
// maximum. Our mounted quarter arc at full height exposes 881px at 900h.
// This measured layout fit narrows that opening; it is not a HUD/FOV scale or
// a claim about the original's movement-dependent vignette deformation.
const SNIPER_MASK_SCALE: f32 = 0.86;

#[derive(Resource)]
pub struct ReticleImages {
    horizontal: Vec<Handle<Image>>, vertical: Vec<Handle<Image>>, dot: Vec<Handle<Image>>,
    dot_pixels: Vec<Vec<u8>>, clipped_dot: Handle<Image>,
}

/// This frame's posed rear lens in physical viewport pixels. Viewmodel update
/// clears it before early returns so a dot cannot outlive its rendered optic.
#[derive(Resource, Default)]
pub struct OpticAperture(pub Vec4);

#[derive(Default)]
pub struct ScopeBlur { key: Option<String>, sigma: f32 }

fn blur_target(inaccuracy: f32, resting: f32, fov: f32) -> f32 {
    let focal = 540.0 / (crate::viewmodel::vertical_fov(fov.clamp(1.0, 179.0)) * 0.5).tan();
    ((inaccuracy - resting).max(0.0) * 0.001 * focal * 0.08).clamp(0.0, MAX_BLUR)
}

fn smooth_blur(current: f32, target: f32, dt: f32) -> f32 {
    current + (target - current) * (1.0 - (-dt.max(0.0) / 0.06).exp())
}

fn clip_dot(source: &[u8], origin: Vec2, texel: f32, aperture: Vec4) -> Vec<u8> {
    let mut clipped = source.to_vec();
    for (index, pixel) in clipped.chunks_exact_mut(4).enumerate() {
        if pixel[3] == 0 { continue; }
        let point = origin + Vec2::new((index % RETICLE_SIZE) as f32 + 0.5,
            (index / RETICLE_SIZE) as f32 + 0.5) * texel;
        let coverage = ((aperture.z - point.distance(aperture.truncate().truncate())) / texel + 0.5).clamp(0.0, 1.0);
        pixel[3] = (pixel[3] as f32 * coverage).round() as u8;
    }
    clipped
}

// Separable, normalized coverage convolution. Transparent padding prevents
// cropping the bloom; normalization keeps motion from making the dot brighter.
fn blur_coverage(input: &[f32], width: usize, height: usize, sigma: f32) -> Vec<f32> {
    if sigma == 0.0 { return input.to_vec(); }
    let radius = (sigma * 3.0).ceil() as isize;
    let mut kernel: Vec<f32> = (-radius..=radius).map(|x| (-0.5 * (x as f32 / sigma).powi(2)).exp()).collect();
    let sum: f32 = kernel.iter().sum();
    for weight in &mut kernel { *weight /= sum; }
    let mut result = input.to_vec();
    for horizontal in [true, false] {
        if (horizontal && width == 1) || (!horizontal && height == 1) { continue; }
        let source = result.clone();
        for y in 0..height { for x in 0..width {
            result[y * width + x] = kernel.iter().enumerate().map(|(i, weight)| {
                let offset = i as isize - radius;
                let sx = x as isize + if horizontal { offset } else { 0 };
                let sy = y as isize + if horizontal { 0 } else { offset };
                if sx < 0 || sy < 0 || sx >= width as isize || sy >= height as isize { 0.0 }
                else { source[sy as usize * width + sx as usize] * weight }
            }).sum();
        }}
    }
    result
}

fn line_profile(rgba: &[u8], width: usize, height: usize) -> Option<Vec<f32>> {
    if width < 2 || height == 0 || rgba.len() != width.checked_mul(height)?.checked_mul(4)? {
        return None;
    }
    // scope_line_blur is a black translucent strip: its ALPHA is coverage.
    // Its long axis is Y; the central row contains the soft cross-line shape.
    let row = &rgba[(height / 2) * width * 4..(height / 2 + 1) * width * 4];
    let profile: Vec<_> = row.chunks_exact(4).map(|pixel| pixel[3] as f32 / 255.0).collect();
    profile.iter().any(|alpha| *alpha > 0.0).then_some(profile)
}

fn sample_profile(profile: &[f32], position: f32) -> f32 {
    if position < 0.0 || position > (profile.len() - 1) as f32 { return 0.0; }
    let index = position.floor() as usize;
    let next = profile[(index + 1).min(profile.len() - 1)];
    profile[index] + (next - profile[index]) * position.fract()
}

fn sniper_line(profile: Option<&[f32]>, sigma: f32) -> Vec<f32> {
    let mut line = vec![0.0; RETICLE_SIZE + 1];
    line[RETICLE_SIZE / 2] = 1.0;
    if sigma <= 0.0 { return line; }
    // The mounted sprite authors a broad soft line with a legible dark core
    // (about .85 alpha), not the unit-area distribution of a one-pixel impulse.
    // Scale that profile across the line while keeping its authored coverage.
    // Dot bloom deliberately retains the separate energy-normalized kernel.
    let shape = profile.filter(|shape| shape.len() >= 2).map(|shape| {
        let center = (shape.len() - 1) as f32 * 0.5;
        let mass: f32 = shape.iter().sum();
        let variance = shape.iter().enumerate().map(|(x, alpha)|
            *alpha * (x as f32 - center).powi(2)).sum::<f32>() / mass.max(f32::EPSILON);
        (shape, center, variance.sqrt().max(0.5))
    });
    let blend = sigma.min(1.0);
    for (index, alpha) in line.iter_mut().enumerate() {
        let distance = index as f32 - (RETICLE_SIZE / 2) as f32;
        let coverage = if let Some((shape, center, native_sigma)) = shape {
            // Symmetrize about the aim ray; texture compression/asymmetry must
            // not displace the reticle when its width changes.
            let offset = distance * native_sigma / sigma;
            (sample_profile(shape, center + offset) + sample_profile(shape, center - offset)) * 0.5
        } else {
            // Missing mounted art still gets a soft, readable line, never the
            // nearly invisible 1/(sigma*sqrt(2*pi)) impulse peak.
            0.85 * (-0.5 * (distance / sigma).powi(2)).exp()
        };
        *alpha += (coverage - *alpha) * blend;
    }
    line
}

fn coverage_image(images: &mut Assets<Image>, coverage: &[f32], width: usize, height: usize) -> Handle<Image> {
    let rgba = coverage.iter().flat_map(|a| [255, 255, 255, (a.clamp(0.0, 1.0) * 255.0).round() as u8]).collect();
    let mut image = Image::new(
        bevy::render::render_resource::Extent3d { width: width as u32, height: height as u32, depth_or_array_layers: 1 },
        bevy::render::render_resource::TextureDimension::D2, rgba,
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        bevy::asset::RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = bevy::image::ImageSampler::linear();
    images.add(image)
}

pub fn setup(commands: Commands, level: Res<crate::LoadedLevel>, mut images: ResMut<Assets<Image>>) {
    let mut cache = crate::textures::TextureCache::default();
    let arc = cache.load(&level.0.vfs, &mut images, "sprites/scope_arc", true).map(|t| t.handle);
    // The original optic dot uses its brightness as a coverage mask; its grey
    // background is not an opaque rectangle. Decode only the base mip for UI.
    let dot = level.0.vfs.read("materials/models/weapons/shared/scope/scope_dot_white.vtf")
        .and_then(|bytes| vtf::Vtf::parse(&bytes).ok()).and_then(|vtf| {
            let rgba = vtf.rgba8(0, 0, 0)?;
            let mut coverage = vec![0.0; RETICLE_SIZE * RETICLE_SIZE];
            // Preserve the original 32px footprint with room for blur on all sides.
            for y in 0..32 { for x in 0..32 {
                let source = ((y * vtf.height as usize / 32) * vtf.width as usize + x * vtf.width as usize / 32) * 4;
                coverage[(y + 32) * RETICLE_SIZE + x + 32] = ((rgba[source] as f32 / 255.0 - 0.5) * 2.0).clamp(0.0, 1.0);
            }}
            Some(coverage)
        });
    let clipped_dot = coverage_image(&mut images, &vec![0.0; RETICLE_SIZE * RETICLE_SIZE], RETICLE_SIZE, RETICLE_SIZE);
    images.get_mut(&clipped_dot).unwrap().asset_usage |= bevy::asset::RenderAssetUsages::MAIN_WORLD;
    let mut reticles = ReticleImages {
        horizontal: Vec::new(), vertical: Vec::new(), dot: Vec::new(), dot_pixels: Vec::new(), clipped_dot,
    };
    let native_line = level.0.vfs.read("materials/sprites/scope_line_blur.vtf")
        .and_then(|bytes| vtf::Vtf::parse(&bytes).ok()).and_then(|vtf| {
            line_profile(&vtf.rgba8(0, 0, 0)?, vtf.width, vtf.height)
        });
    for step in 0..=BLUR_STEPS {
        let sigma = step as f32 * MAX_BLUR / BLUR_STEPS as f32;
        let line = sniper_line(native_line.as_deref(), sigma);
        reticles.horizontal.push(coverage_image(&mut images, &line, 1, line.len()));
        reticles.vertical.push(coverage_image(&mut images, &line, line.len(), 1));
        if let Some(dot) = &dot {
            let image = coverage_image(&mut images, &blur_coverage(dot, RETICLE_SIZE, RETICLE_SIZE, sigma), RETICLE_SIZE, RETICLE_SIZE);
            reticles.dot_pixels.push(images.get(&image).unwrap().data.as_ref().unwrap().clone());
            reticles.dot.push(image);
        }
    }
    spawn_parts(commands, arc, reticles);
}

fn spawn_parts(mut commands: Commands, arc: Option<Handle<Image>>, reticles: ReticleImages) {
    for part in 0..11 {
        let mut entity = commands.spawn((
            Node { position_type: PositionType::Absolute, ..default() },
            GlobalZIndex(-1), Visibility::Hidden, ScopePart(part),
        ));
        if part < 4 {
            if let Some(image) = &arc {
                // In the original quarter arc (0, 0) is the clear centre.
                entity.insert(ImageNode { image: image.clone(), flip_x: part % 2 == 0, flip_y: part < 2, ..default() });
            }
        } else if part == 10 {
            if let Some(image) = reticles.dot.first() { entity.insert(ImageNode::new(image.clone())); }
        } else if part == 8 || part == 9 {
            let image = if part == 8 { &reticles.horizontal[0] } else { &reticles.vertical[0] };
            // These are 1x97 / 97x1 coverage strips. Auto preserves their
            // aspect ratio, leaving only short marks at the scope centre.
            // Stretch along the line while retaining the blur across it.
            entity.insert(ImageNode { image: image.clone(), color: Color::BLACK, image_mode: NodeImageMode::Stretch, ..default() });
        } else { entity.insert(BackgroundColor(Color::BLACK)); }
    }
    commands.insert_resource(reticles);
    commands.init_resource::<OpticAperture>();
}

pub fn update(
    settings: Res<crate::settings::Settings>,
    (time, fixed): (Res<Time>, Res<Time<Fixed>>),
    (reticles, optic, ui_scale): (Res<ReticleImages>, Res<OpticAperture>, Res<UiScale>),
    mut images: ResMut<Assets<Image>>, mut blur: Local<ScopeBlur>,
    mut optical: ResMut<optic_blur::OpticState>,
    local: Res<LocalPlayer>, world: Res<SimWorld>, window: Single<&Window, With<PrimaryWindow>>,
    mut parts: Query<(&ScopePart, &mut Node, &mut Visibility, Option<&mut ImageNode>)>,
    mut crosshair: Query<(&crate::hud::Crosshair, &mut Visibility, &mut Node, &mut BackgroundColor), (With<crate::hud::Crosshair>, Without<ScopePart>)>,
) {
    let p = world.0.player(local.view_target()).filter(|p| p.alive && !local.noclip);
    let weapon = p.and_then(|p| p.active_weapon());
    let zoomed = weapon.is_some_and(|w| w.zoom_level > 0);
    let sniper = weapon.is_some_and(|w| w.hides_viewmodel());
    let key = weapon.map(|w| w.def.class_name.clone());
    let target = p.zip(weapon).filter(|_| zoomed).map_or(0.0, |(p, w)| {
        let ctx = sim::weapon::AccuracyContext { speed: p.movement.speed_2d(), vertical_speed: p.movement.velocity.z, on_ground: p.movement.on_ground, ducked: p.movement.ducked };
        let resting = if ctx.ducked { w.alt_stat("inaccuracy crouch", w.def.accuracy.inaccuracy_crouch) }
            else { w.alt_stat("inaccuracy stand", w.def.accuracy.inaccuracy_stand) };
        blur_target(sim::weapon::inaccuracy(w, ctx), resting,
            w.rendered_fov(world.0.time() + fixed.overstep_fraction() * world.0.dt()))
    });
    if blur.key != key { blur.key = key; blur.sigma = 0.0; }
    blur.sigma = smooth_blur(blur.sigma, target, time.delta_secs());
    // Sniper lines use their mounted soft coverage profile. Scene defocus is
    // confined to the outer optical region, keeping the world at aim sharp.
    // Rifle optics instead defocus only the close weapon, using its own depth.
    optical.0 = optic_blur::OpticSettings {
        aperture: optic.0,
        params: Vec4::new(if sniper { 1.0 } else { 0.0 }, blur.sigma,
            if zoomed && !sniper && optic.0.z > 0.0 { 7.0 } else { 0.0 }, 1.0),
    };
    let blur_step = (blur.sigma / MAX_BLUR * BLUR_STEPS as f32).round().clamp(0.0, BLUR_STEPS as f32) as usize;
    let line_sigma = if sniper { optic_blur::control("CSR_TEST_SCOPE_BLUR").unwrap_or(blur.sigma) } else { blur.sigma };
    let line_step = (line_sigma / MAX_BLUR * BLUR_STEPS as f32).round().clamp(0.0, BLUR_STEPS as f32) as usize;
    let regular = p.is_some() && !zoomed && !weapon.is_some_and(|w| w.def.class == weapons::WeaponClass::Sniper);
    for (part,mut v,mut node,mut color) in &mut crosshair { *v = if regular && (part.0 != 4 || settings.cross_dot) { Visibility::Inherited } else { Visibility::Hidden }; let (x,y,w,h)=crate::menu::cross_bar(&settings,part.0);node.left=px(x);node.top=px(y);node.width=px(w);node.height=px(h);color.0=settings.crosshair_color(); }
    let (width, height) = (window.width() / ui_scale.0, window.height() / ui_scale.0);
    let size = width.min(height) * SNIPER_MASK_SCALE;
    let radius = size * 0.5;
    let scale = height / 1080.0;
    let line_width = (RETICLE_SIZE + 1) as f32 * scale;
    let dot_width = RETICLE_SIZE as f32 * scale;
    let dot_origin = Vec2::new((width - dot_width) * 0.5, (height - dot_width) * 0.5);
    let aperture = optic.0 / (window.scale_factor() * ui_scale.0);
    let dot_image = reticles.dot.get(blur_step).map(|image| {
        // The dot stays on the aim ray. Crop its bloom against the same moving
        // lens as the weapon cutout instead of painting over the housing during
        // firing/zoom. Fully contained dots keep the precomputed GPU image.
        let center = aperture.truncate().truncate();
        let contained = [dot_origin, dot_origin + Vec2::X * dot_width,
            dot_origin + Vec2::Y * dot_width, dot_origin + Vec2::splat(dot_width)]
            .iter().all(|corner| corner.distance(center) <= aperture.z);
        if contained || aperture.z <= 0.0 { return image.clone(); }
        let clipped = clip_dot(&reticles.dot_pixels[blur_step], dot_origin, scale, aperture);
        if clipped == reticles.dot_pixels[blur_step] { return image.clone(); }
        if images.get(&reticles.clipped_dot).is_some_and(|image| image.data.as_ref() != Some(&clipped)) {
            if let Some(mut image) = images.get_mut(&reticles.clipped_dot) { image.data = Some(clipped); }
        }
        reticles.clipped_dot.clone()
    });
    let (left, top) = ((width - size) * 0.5, (height - size) * 0.5);
    for (part, mut node, mut visibility, image) in &mut parts {
        let shown = if part.0 == 10 { zoomed && !sniper && aperture.z > 0.0 && dot_image.is_some() } else { sniper };
        *visibility = if shown { Visibility::Inherited } else { Visibility::Hidden };
        // Overlap every outer edge by a pixel: linear sampling of the
        // quarter arc can otherwise wrap a transparent texel into a seam.
        let (x, y, w, h) = match part.0 {
            n @ 0..=3 => (left + (n % 2) as f32 * radius, top + (n / 2) as f32 * radius, radius, radius),
            4 => (0.0, 0.0, left + 1.0, height),
            5 => (left + size - 1.0, 0.0, left + 1.0, height),
            6 => (left, 0.0, size, top + 1.0),
            7 => (left, top + size - 1.0, size, top + 1.0),
            8 => (left, height * 0.5 - line_width * 0.5, size, line_width),
            9 => (width * 0.5 - line_width * 0.5, top, line_width, size),
            _ => (dot_origin.x, dot_origin.y, dot_width, dot_width),
        };
        node.left = px(x); node.top = px(y); node.width = px(w); node.height = px(h);
        if let Some(mut image) = image {
            if part.0 == 8 { image.image = reticles.horizontal[line_step].clone(); }
            if part.0 == 9 { image.image = reticles.vertical[line_step].clone(); }
            if part.0 == 10 {
                if let Some(dot) = &dot_image { image.image = dot.clone(); }
                image.color = if weapon.is_some_and(|w| w.def.class_name == "weapon_aug") { Color::srgb(0.1, 1.0, 0.1) } else { Color::srgb(1.0, 0.1, 0.1) };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniper_soft_line_uses_alpha_and_retains_a_readable_center_as_it_defocuses() {
        // Synthetic black strip, deliberately unequal sides: alpha is the
        // native semantic; symmetry/centering belong to our aim presentation.
        let rgba: Vec<u8> = [0, 32, 160, 216, 192, 64, 0].into_iter()
            .flat_map(|alpha| [0, 0, 0, alpha]).collect();
        let profile = line_profile(&rgba, 7, 1).unwrap();
        assert_eq!(profile[3], 216.0 / 255.0);
        let sharp = sniper_line(Some(&profile), 0.0);
        assert_eq!(sharp[RETICLE_SIZE / 2], 1.0);
        assert_eq!(sharp.iter().sum::<f32>(), 1.0);
        let narrow = sniper_line(Some(&profile), 2.0);
        let broad = sniper_line(Some(&profile), MAX_BLUR);
        assert!(broad[RETICLE_SIZE / 2] > 0.8, "motion must not dissolve the dark core");
        assert_eq!(broad[RETICLE_SIZE / 2], narrow[RETICLE_SIZE / 2]);
        assert!(broad.iter().sum::<f32>() > narrow.iter().sum::<f32>() * 3.0);
        for offset in 0..=RETICLE_SIZE / 2 {
            assert_eq!(broad[RETICLE_SIZE / 2 - offset], broad[RETICLE_SIZE / 2 + offset]);
        }
        assert_eq!(broad[0], 0.0);
        assert!(sniper_line(None, MAX_BLUR)[RETICLE_SIZE / 2] > 0.8);
        assert!(line_profile(&rgba[..rgba.len() - 1], 7, 1).is_none());
    }

    #[test]
    fn moving_lens_clips_dot_bloom_without_relocating_aim_or_changing_with_dpi() {
        let source = vec![255; RETICLE_SIZE * RETICLE_SIZE * 4];
        let origin = Vec2::new(912., 492.);
        // The aim stays at (960, 540), while the posed optic has moved left.
        let aperture = Vec4::new(938., 540., 16., 0.);
        let clipped = clip_dot(&source, origin, 1., aperture);
        let alpha = |x: usize, y: usize| clipped[(y * RETICLE_SIZE + x) * 4 + 3];
        assert_eq!(alpha(48, 48), 0, "aim outside the glass must not paint the housing");
        assert_eq!(alpha(26, 48), 255, "coverage inside the moving glass survives");
        assert_eq!(alpha(0, 48), 0);
        assert_eq!(alpha(26, 0), 0);
        assert!(clipped.chunks_exact(4).any(|pixel| pixel[3] > 0 && pixel[3] < 255));
        assert_eq!(clipped, clip_dot(&source, origin / 2., 0.5, aperture / 2.));
    }

    #[test]
    fn spawned_sniper_lines_stretch_the_coverage_strips_across_the_scope() {
        use bevy::ecs::system::RunSystemOnce;

        let mut world = World::new();
        world.run_system_once(|commands: Commands| {
            spawn_parts(commands, None, ReticleImages {
                horizontal: vec![Handle::default()],
                vertical: vec![Handle::default()],
                dot: vec![Handle::default()],
                dot_pixels: Vec::new(), clipped_dot: Handle::default(),
            });
        }).unwrap();
        let mut lines = 0;
        for (part, image) in world.query::<(&ScopePart, &ImageNode)>().iter(&world) {
            if part.0 == 8 || part.0 == 9 {
                assert!(matches!(image.image_mode, NodeImageMode::Stretch),
                    "reticle {} must fill its node rather than fit the strip's aspect ratio", part.0);
                assert_eq!(image.color, Color::BLACK);
                lines += 1;
            } else {
                assert!(matches!(image.image_mode, NodeImageMode::Auto));
            }
        }
        assert_eq!(lines, 2);
    }

    #[test]
    fn accuracy_driven_blur_scales_and_recovers_independently_of_frame_rate() {
        let resting = 2.0;
        let targets = [resting, 12.0, 35.0, 140.0].map(|accuracy| blur_target(accuracy, resting, 40.0));
        assert_eq!(targets[0], 0.0);
        assert!(targets.windows(2).all(|pair| pair[1] > pair[0]));
        let mut sigma = targets[3];
        for _ in 0..120 {
            let next = smooth_blur(sigma, targets[0], 1.0 / 120.0);
            assert!(next >= 0.0 && next < sigma);
            sigma = next;
        }
        assert!(sigma < 0.001);
        let full = smooth_blur(0.0, targets[2], 1.0 / 30.0);
        let half = smooth_blur(0.0, targets[2], 1.0 / 60.0);
        assert!((full - smooth_blur(half, targets[2], 1.0 / 60.0)).abs() < 0.00001);
        assert_eq!(blur_target(10000.0, resting, 15.0), MAX_BLUR);
    }

    #[test]
    fn convolution_softens_reticle_without_brightening_or_losing_coverage() {
        let mut point = vec![0.0; RETICLE_SIZE * RETICLE_SIZE];
        point[48 * RETICLE_SIZE + 48] = 1.0;
        assert_eq!(blur_coverage(&point, RETICLE_SIZE, RETICLE_SIZE, 0.0), point);
        let soft = blur_coverage(&point, RETICLE_SIZE, RETICLE_SIZE, MAX_BLUR);
        assert!(soft[48 * RETICLE_SIZE + 48] < 0.01);
        assert!(soft[48 * RETICLE_SIZE + 52] > 0.0);
        assert!((soft.iter().sum::<f32>() - 1.0).abs() < 0.0001);
    }
}
