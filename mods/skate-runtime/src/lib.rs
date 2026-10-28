// SPDX-License-Identifier: GPL-3.0-only
//! Full upstream coordinator, exposed as explicit fixed-step calls. No window,
//! renderer, hardware polling, menu schedule or standalone game process loop.
#![allow(dead_code, unused_imports)]
#[path = "../upstream/animation_pose/mod.rs"]
mod animation_pose;
mod asset_cache;
mod session_marker;
mod bridge;
#[path = "../upstream/camera/mod.rs"]
mod camera;
#[path = "../upstream/custom_difficulty.rs"]
mod custom_difficulty;
#[path = "../upstream/difficulty.rs"]
mod difficulty;
mod dynamic;
#[path = "../upstream/graph_host/mod.rs"]
mod graph_host;
#[path = "../upstream/graph_runtime.rs"]
mod graph_runtime;
#[path = "../upstream/grind_world/mod.rs"]
mod grind_world;
#[path = "../upstream/input/mod.rs"]
mod input;
#[path = "../upstream/modding/mod.rs"]
mod modding;
#[path = "../upstream/physics/mod.rs"]
mod physics;
#[path = "../upstream/scoring_runtime/mod.rs"]
mod scoring_runtime;
mod skate_world;
#[path = "../upstream/skater_animation/mod.rs"]
mod skater_animation;
pub use bridge::PreparedScene;

// Scheduling/diagnostic names referenced by the upstream host endpoints. They
// own no simulation logic. The headless API calls the original coordinator.
mod app {
    #[derive(bevy::prelude::SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
    pub enum FrameSet {
        Assets,
        Physics,
        Animation,
        Verification,
    }
    #[derive(bevy::prelude::SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
    pub enum SimulationSet {
        Input,
        Controls,
        Physics,
    }
}
mod world {
    #[derive(bevy::prelude::Component)]
    pub struct PlayerRoot;
}
mod config {
    #[derive(bevy::prelude::Resource)]
    pub struct Config {
        pub asset_root: std::path::PathBuf,
        pub map: Option<std::path::PathBuf>,
    }
}
mod crash_context {
    pub fn requested(
        _: u64,
        _: skate_core::player::state::PhysicalStateId,
        _: skate_core::player::state::PhysicalStateId,
    ) {
    }
}
mod animation {
    pub fn native_matrix(m: skate_core::animation::output::NativeMatrix) -> bevy::math::Mat4 {
        use bevy::math::{Mat4, Vec3};
        Mat4::from_cols(
            Vec3::from_slice(&m[0][..3]).extend(0.),
            Vec3::from_slice(&m[1][..3]).extend(0.),
            Vec3::from_slice(&m[2][..3]).extend(0.),
            Vec3::from_slice(&m[3][..3]).extend(1.),
        )
    }
}
// Original APT trick display: bounded ActionScript VM, movie timeline and
// display-list traversal. The CS host only draws the published triangles.
#[path = "../upstream/apt_display.rs"]
mod apt_display;
#[path = "../upstream/apt_imports.rs"]
mod apt_imports;
#[path = "../upstream/apt_movie.rs"]
mod apt_movie;
#[path = "../upstream/apt_scene.rs"]
mod apt_scene;
#[path = "../upstream/apt_text.rs"]
mod apt_text;
#[path = "../upstream/apt_vm.rs"]
mod apt_vm;
mod hud;
#[path = "../upstream/hud_runtime.rs"]
mod hud_runtime;

mod rolling_audio;
pub struct Runtime {
    audio:rolling_audio::Rolling,
    marker:session_marker::SessionMarker,
    hud: hud::Hud,
    physics: physics::GamePhysics,
    skater: physics::SkaterRuntime,
    controls: physics::PlayerControls,
    graphs: std::sync::Arc<graph_runtime::StockGraphs>,
    camera: camera::CameraRuntime,
    pad: skate_core::input::pad::Pad,
    accumulator: f64,
    scene: Option<std::sync::Arc<PreparedScene>>,
    scene_state: mechanics_link::SceneState,
    bind_pose: Vec<mechanics_link::Matrix>,
    language: std::collections::BTreeMap<String, String>,
}
impl Runtime {
    /// Only immutable scene geometry is lent to the next reconstruction. Its
    /// query methods reset contact scratch; dynamic queries are replaced below.
    pub fn recycle_scene_world(mut self) {
        if let Some(scene) = &self.scene {
            let mut world = self.physics.csr_take_world();
            world.set_external_queries(None);
            *scene.world.lock().unwrap() = Some(world);
        }
    }
    pub fn load(root: &std::path::Path) -> Result<Self, String> {
        let manifest = skate_data::GameAssets::load(root).map_err(|e| e.to_string())?;
        let graphs = std::sync::Arc::new(graph_runtime::StockGraphs::load(root, &manifest)?);
        let physics =
            physics::GamePhysics::load_with_difficulty(root, None, difficulty::Difficulty::Easy)?;
        let skater = physics::SkaterRuntime::load(root, &graphs, &physics, "easy")?;
        let controls = physics::PlayerControls::load(root)?;
        let camera = camera::CameraRuntime::load(root)?;
        let bind_pose = bridge::bind_pose(root, &skater)?;
        let language = bridge::language(root)?;
        Ok(Self {
            audio:rolling_audio::Rolling::new(rolling_audio::Bank::load(root)?),
            marker:session_marker::SessionMarker::load(root,None)?,
            hud: hud::Hud::new(root, skater.scoring.hud_input()),
            physics,
            skater,
            controls,
            graphs,
            camera,
            pad: skate_core::input::pad::Pad::new(),
            accumulator: 0.,
            scene: None,
            scene_state: Default::default(),
            bind_pose,
            language,
        })
    }
    pub fn step(&mut self, values: [f32; 18], available: bool) -> Result<(), String> {
        let mut actions = skate_core::input::gameplay_map::GameplayActions::from_values(values);
        self.controls.update_for_physics(
            &mut actions,
            &self.physics,
            &self.skater,
            &self.camera,
        )?;
        self.controls.publish_gestures(
            self.physics.difficulty_index(),
            self.skater.player_input.physical.state.state_16,
        );
        physics::csr_advance(
            &mut self.physics,
            &mut self.skater,
            &mut self.controls,
            &self.graphs,
            &mut actions,
            available,
            &mut self.camera,
        )
    }
    pub fn diagnostic(&self) -> String {
        let deck = &self.physics.board.bodies()[skate_core::physics::board::BodyId::Deck.index()];
        format!(
            "tick={} deck={:?} velocity={:?} state={:?} pose={} bones={}",
            self.physics.ticks,
            deck.rates.position,
            deck.rates.linear_velocity,
            self.skater.player_state.current(),
            self.skater.pose_generation,
            self.skater.render_pose.len()
        )
    }
}
