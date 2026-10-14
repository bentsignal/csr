//! Bevy-free character poses, built from the installed character animation set.
//!
//! A simplified version of the game's layered animation state:
//! * legs: the 9-way locomotion blend (`move_r`/`move_w`/`move_c`) over
//!   `move_yaw` (movement direction relative to facing), or jump/fall poses
//!   in the air;
//! * upper body (`spine_1` and its children): the weapon's aim pose
//!   (`rifle_aim_idle`, `pistol_aim_run`, ...) blended over look pitch;
//! * gestures on top: reload (full upper-body pose) and fire (additive).
//!
//! Used clips are prepared once from the include model (`animset_t.mdl`)
//! and retargeted by bone name onto the character's skeleton. Evaluation reads
//! only the prepared cache, shared by simulation and presentation.

use mdl::anim::Clip;
use mdl::skeleton::{self, Bone};
use std::collections::HashMap;
use std::sync::Arc;
use weapons::WeaponClass;

type Pose = Vec<(glam::Vec3, glam::Quat)>;

#[derive(Clone)]
pub struct PlayerAnimSet {
    bones: Vec<Bone>,
    upper: Vec<bool>,
    source: Arc<mdl::Model>,
    clips: HashMap<String, Option<Arc<Clip>>>,
    grids: HashMap<String, (Arc<Vec<String>>, [i32; 2])>,
    menu_idle: Option<Clip>,
}

/// Inputs for one character this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimInput {
    pub speed: f32,
    /// Movement direction relative to facing, degrees (0 = forward).
    pub move_yaw: f32,
    pub on_ground: bool,
    pub ducked: bool,
    pub pitch: f32,
    pub class: WeaponClass,
    pub time_since_fire: f32,
    /// 0..1 while reloading.
    pub reload_progress: Option<f32>,
    pub air_time: f32,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AnimState {
    pub move_cycle: f32,
}

impl AnimState {
    /// Advance the existing locomotion phase once, independently of bone sampling.
    pub fn advance(&mut self, input: &AnimInput, dt: f32) {
        if input.on_ground && input.speed > 12.0 {
            let ref_speed = match (input.ducked, input.speed > 150.0) {
                (true, _) => 85.0,
                (false, true) => 250.0,
                (false, false) => 130.0,
            };
            // One cycle per ~2.2 s at the reference speed.
            self.move_cycle = (self.move_cycle + dt * (input.speed / ref_speed) / 2.2).fract();
        }
    }
}

fn prefix(class: WeaponClass) -> &'static str {
    match class {
        WeaponClass::Knife => "knife",
        WeaponClass::Pistol => "pistol",
        WeaponClass::Sniper => "sniper",
        _ => "rifle",
    }
}

impl PlayerAnimSet {
    /// `character` provides the skeleton; animations come from its include
    /// models, loaded through `fs`.
    pub fn load(character: &mdl::Model, fs: &vfs::Vfs) -> Option<Self> {
        let include = character.include_models.first()?;
        let source = mdl::Model::load_header(&fs.read(include)?).ok()?;
        let bones = character.bones.clone();
        // Upper body: spine_1 and everything below it in the hierarchy.
        let root = character.bone_index("spine_1");
        let mut upper = vec![false; bones.len()];
        for (i, b) in bones.iter().enumerate() {
            upper[i] = Some(i) == root || (b.parent >= 0 && upper[b.parent as usize]);
        }
        let grids = source
            .sequences
            .iter()
            .map(|s| {
                let names = s
                    .anims
                    .iter()
                    .map(|&i| {
                        source
                            .animations
                            .get(i as usize)
                            .map(|a| a.name.clone())
                            .unwrap_or_default()
                    })
                    .collect();
                (s.name.clone(), (Arc::new(names), s.group_size))
            })
            .collect();
        let mut set = Self {
            grids,
            bones,
            upper,
            source: Arc::new(source),
            clips: HashMap::new(),
            menu_idle: None,
        };
        if let Some(source) = fs
            .read("models/player/custom_player/uiplayer/animset_uiplayer_loadout.mdl")
            .and_then(|bytes| mdl::Model::load_header(&bytes).ok())
        {
            if let Some(mut clip) = source
                .animation_index("@ct_loadout_rifle_idle_handrepo_m4")
                .and_then(|index| source.decode_animation(index, None).ok())
                .map(|clip| mdl::anim::retarget(&clip, &source.bones, &set.bones))
            {
                clip.looping = true;
                set.menu_idle = Some(clip);
            }
        }
        // Decode the clips used by the current animation controller during
        // loading, so a first encounter/reload/jump cannot stall a frame.
        let mut names = std::collections::HashSet::new();
        for (sequence, (grid, _)) in &set.grids {
            if sequence.starts_with("move_")
                || ["rifle", "pistol", "knife", "sniper"]
                    .iter()
                    .any(|p| sequence.starts_with(&format!("{p}_aim_")))
            {
                names.extend(grid.iter().cloned());
            }
        }
        for prefix in ["rifle", "pistol", "knife", "sniper"] {
            for action in ["fire", "fire_crouch", "reload", "reload_crouch"] {
                names.insert(format!("@{prefix}_{action}"));
            }
        }
        names.extend(["a_jump_onspot", "a_jump_onspot_crouch", "a_fall"].map(str::to_string));
        names.insert("@deathpose_lowviolence".to_string());
        for name in names {
            set.prepare_clip(&name);
        }
        Some(set)
    }

    pub fn bones(&self) -> &[Bone] {
        &self.bones
    }

    /// Installed full-body death sequence, played once and held at its end.
    /// This is the model's low-violence animation, not a ragdoll simulation.
    pub fn death_pose(&mut self, seconds: f32) -> Option<Vec<glam::Mat4>> {
        let clip = self.cached_clip("@deathpose_lowviolence")?;
        if clip.delta || clip.looping || clip.frames.len() < 2 {
            return None;
        }
        let mut pose = skeleton::rest_pose(&self.bones);
        clip.sample(seconds.clamp(0., clip.duration()), &mut pose);
        Some(skeleton::model_space(&self.bones, &pose))
    }

    /// Original Panorama loadout idle, using real time independently of a paused match.
    pub fn menu_pose(&self, time: f32) -> Option<Vec<glam::Mat4>> {
        let clip = self.menu_idle.as_ref()?;
        let mut pose = skeleton::rest_pose(&self.bones);
        clip.sample(time, &mut pose);
        Some(skeleton::model_space(&self.bones, &pose))
    }

    /// Decode/retarget only during loading; absent clips are cached as well.
    fn prepare_clip(&mut self, name: &str) -> Option<Arc<Clip>> {
        if let Some(c) = self.clips.get(name) {
            return c.clone();
        }
        let decoded = self
            .source
            .animation_index(name)
            .and_then(|i| self.source.decode_animation(i, None).ok())
            .map(|clip| {
                let map: Vec<Option<usize>> = self
                    .bones
                    .iter()
                    .map(|b| self.source.bone_index(&b.name))
                    .collect();
                let frames = clip
                    .frames
                    .iter()
                    .map(|f| {
                        self.bones
                            .iter()
                            .zip(&map)
                            .map(|(b, m)| {
                                m.and_then(|i| f.get(i).copied()).unwrap_or(if clip.delta {
                                    (glam::Vec3::ZERO, glam::Quat::IDENTITY)
                                } else {
                                    (b.pos, b.quat)
                                })
                            })
                            .collect()
                    })
                    .collect();
                let animated = map
                    .iter()
                    .map(|m| m.is_some_and(|i| clip.animated.get(i).copied().unwrap_or(false)))
                    .collect();
                Arc::new(Clip {
                    frames,
                    animated,
                    ..clip
                })
            });
        self.clips.insert(name.to_string(), decoded.clone());
        decoded
    }

    /// Evaluation never decodes or mutates the prepared animation cache.
    fn cached_clip(&self, name: &str) -> Option<Arc<Clip>> {
        self.clips.get(name).cloned().flatten()
    }

    /// Animation names of a sequence's blend grid.
    fn sequence_anims(&self, seq: &str) -> Option<(Arc<Vec<String>>, [i32; 2])> {
        self.grids.get(seq).cloned()
    }

    fn sample(&self, name: &str, t: f32, out: &mut Pose) -> bool {
        match self.cached_clip(name) {
            Some(c) => {
                c.sample(t, out);
                true
            }
            None => false,
        }
    }

    fn sample_normalized(&self, name: &str, cycle: f32, out: &mut Pose) -> bool {
        match self.cached_clip(name) {
            Some(c) => {
                c.sample(cycle * c.duration(), out);
                true
            }
            None => false,
        }
    }

    /// Aim pose for a state (`idle`, `walk`, `run`, `crouch_idle`,
    /// `crouch_moving`) blended over pitch along the grid's centre column.
    fn aim_pose(&self, class: WeaponClass, state: &str, pitch: f32) -> Option<Pose> {
        let (names, size) = self.sequence_anims(&format!("{}_aim_{state}", prefix(class)))?;
        let cols = size[0].max(1) as usize;
        let rows = size[1].max(1) as usize;
        let col = cols / 2;
        // Row 0 looks up (pitch -90), the last row down (pitch +90).
        let x = ((pitch + 90.0) / 180.0).clamp(0.0, 1.0) * (rows - 1) as f32;
        let r0 = x.floor() as usize;
        let r1 = (r0 + 1).min(rows - 1);
        let s = x - r0 as f32;
        let mut a = skeleton::rest_pose(&self.bones);
        let mut b = a.clone();
        self.sample(&names[r0 * cols + col], 0.0, &mut a);
        self.sample(&names[r1 * cols + col], 0.0, &mut b);
        Some(blend(&a, &b, s))
    }

    /// Locomotion blend over the 9-way `move_yaw` grid.
    fn move_pose(&self, seq: &str, move_yaw: f32, cycle: f32) -> Option<Pose> {
        let (names, size) = self.sequence_anims(seq)?;
        let n = (size[0].max(1) as usize).min(names.len());
        if n < 2 {
            return None;
        }
        // Grid spans -180..180; entry 0 is "N" (forward) at -180.
        let yaw = (move_yaw + 180.0).rem_euclid(360.0) - 180.0;
        let x = (yaw + 180.0) / 360.0 * (n - 1) as f32;
        let i0 = (x.floor() as usize).min(n - 1);
        let i1 = (i0 + 1).min(n - 1);
        let s = x - i0 as f32;
        let mut a = skeleton::rest_pose(&self.bones);
        let mut b = a.clone();
        self.sample_normalized(&names[i0], cycle, &mut a);
        self.sample_normalized(&names[i1], cycle, &mut b);
        Some(blend(&a, &b, s))
    }

    /// Model-space (Source) bone matrices, retaining the existing client API.
    pub fn evaluate(&mut self, st: &mut AnimState, input: &AnimInput, dt: f32) -> Vec<glam::Mat4> {
        self.evaluate_prepared(st, input, dt)
    }

    /// Evaluate only prepared clips, without VFS reads or animation decoding.
    /// The caller owns cycle advancement: use a fixed dt for a simulation tick,
    /// or dt = 0.0 to sample an existing replicated move_cycle for rendering.
    pub fn evaluate_prepared(
        &self,
        st: &mut AnimState,
        input: &AnimInput,
        dt: f32,
    ) -> Vec<glam::Mat4> {
        st.advance(input, dt);
        let moving = input.on_ground && input.speed > 12.0;
        let aim_state = match (input.ducked, moving) {
            (true, true) => "crouch_moving",
            (true, false) => "crouch_idle",
            (false, true) if input.speed > 150.0 => "run",
            (false, true) => "walk",
            (false, false) => "idle",
        };
        let upper = self
            .aim_pose(input.class, aim_state, input.pitch)
            .or_else(|| self.aim_pose(WeaponClass::Rifle, aim_state, input.pitch))
            .unwrap_or_else(|| skeleton::rest_pose(&self.bones));

        let knife = input.class == WeaponClass::Knife;
        let lower = if !input.on_ground {
            let mut p = upper.clone();
            let name = if input.air_time < 0.45 {
                if input.ducked {
                    "a_jump_onspot_crouch"
                } else {
                    "a_jump_onspot"
                }
            } else {
                "a_fall"
            };
            self.sample(name, input.air_time.min(1.0), &mut p);
            p
        } else if moving {
            let seq = match (input.ducked, input.speed > 150.0) {
                (true, _) => {
                    if knife {
                        "move_knife_c"
                    } else {
                        "move_c"
                    }
                }
                (false, true) => {
                    if knife {
                        "move_knife_r"
                    } else {
                        "move_r"
                    }
                }
                (false, false) => {
                    if knife {
                        "move_knife_w"
                    } else {
                        "move_w"
                    }
                }
            };
            self.move_pose(seq, input.move_yaw, st.move_cycle)
                .unwrap_or_else(|| upper.clone())
        } else {
            upper.clone()
        };

        let mut local: Pose = self
            .upper
            .iter()
            .enumerate()
            .map(|(i, &u)| if u { upper[i] } else { lower[i] })
            .collect();

        let p = prefix(input.class);
        if let Some(progress) = input.reload_progress {
            let mut r = local.clone();
            let name = if input.ducked {
                format!("@{p}_reload_crouch")
            } else {
                format!("@{p}_reload")
            };
            if self.sample_normalized(&name, progress, &mut r)
                || self.sample_normalized(&format!("@{p}_reload"), progress, &mut r)
            {
                for (i, slot) in local.iter_mut().enumerate() {
                    if self.upper[i] {
                        *slot = r[i];
                    }
                }
            }
        } else if input.time_since_fire < 0.6 {
            let name = if input.ducked {
                format!("@{p}_fire_crouch")
            } else {
                format!("@{p}_fire")
            };
            if let Some(clip) = self
                .cached_clip(&name)
                .or_else(|| self.cached_clip(&format!("@{p}_fire")))
            {
                if clip.delta {
                    let mut d = vec![(glam::Vec3::ZERO, glam::Quat::IDENTITY); self.bones.len()];
                    clip.sample(input.time_since_fire, &mut d);
                    for (i, slot) in local.iter_mut().enumerate() {
                        if self.upper[i] {
                            slot.0 += d[i].0;
                            slot.1 = (slot.1 * d[i].1).normalize();
                        }
                    }
                }
            }
        }
        skeleton::model_space(&self.bones, &local)
    }
}

fn blend(a: &Pose, b: &Pose, t: f32) -> Pose {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x.0.lerp(y.0, t), x.1.slerp(y.1, t)))
        .collect()
}

#[cfg(test)]
mod death_tests {
    use super::*;
    #[test]
    fn death_clip_finishes_once_and_preserves_its_final_pose() {
        let bone = Bone {
            name: "root".into(),
            parent: -1,
            pos: glam::Vec3::ZERO,
            quat: glam::Quat::IDENTITY,
            rot: glam::Vec3::ZERO,
            pos_scale: glam::Vec3::ONE,
            rot_scale: glam::Vec3::ONE,
            pose_to_bone: glam::Mat4::IDENTITY,
            flags: 0,
        };
        let clip = Clip {
            name: "@deathpose_lowviolence".into(),
            fps: 1.,
            looping: false,
            delta: false,
            frames: vec![
                vec![(glam::Vec3::ZERO, glam::Quat::IDENTITY)],
                vec![(glam::Vec3::X, glam::Quat::IDENTITY)],
            ],
            animated: vec![true],
        };
        let mut anims = PlayerAnimSet {
            bones: vec![bone],
            upper: vec![true],
            source: Arc::new(mdl::Model::default()),
            clips: HashMap::from([("@deathpose_lowviolence".into(), Some(Arc::new(clip)))]),
            grids: HashMap::new(),
            menu_idle: None,
        };
        assert_eq!(anims.death_pose(0.).unwrap()[0].w_axis.x, 0.);
        assert_eq!(anims.death_pose(0.5).unwrap()[0].w_axis.x, 0.5);
        assert_eq!(
            anims.death_pose(1.).unwrap(),
            anims.death_pose(20.).unwrap()
        );
        assert_eq!(anims.death_pose(20.).unwrap()[0].w_axis.x, 1.);
    }
}

#[cfg(test)]
mod prepared_tests {
    use super::*;

    fn input() -> AnimInput {
        AnimInput {
            speed: 130.0,
            move_yaw: 0.0,
            on_ground: true,
            ducked: false,
            pitch: 0.0,
            class: WeaponClass::Rifle,
            time_since_fire: 1.0,
            reload_progress: None,
            air_time: 0.0,
        }
    }

    #[test]
    fn replicated_cycle_samples_prepared_bones_without_advancing_or_mutating_cache() {
        fn assert_send_sync_clone<T: Send + Sync + Clone>() {}
        assert_send_sync_clone::<PlayerAnimSet>();
        let bone = Bone {
            name: "root".into(),
            parent: -1,
            pos: glam::Vec3::ZERO,
            quat: glam::Quat::IDENTITY,
            rot: glam::Vec3::ZERO,
            pos_scale: glam::Vec3::ONE,
            rot_scale: glam::Vec3::ONE,
            pose_to_bone: glam::Mat4::IDENTITY,
            flags: 0,
        };
        let clip = Arc::new(Clip {
            name: "walk".into(),
            fps: 1.0,
            looping: true,
            delta: false,
            frames: vec![
                vec![(glam::Vec3::ZERO, glam::Quat::IDENTITY)],
                vec![(glam::Vec3::X, glam::Quat::IDENTITY)],
            ],
            animated: vec![true],
        });
        let set = PlayerAnimSet {
            bones: vec![bone],
            upper: vec![false],
            source: Arc::new(mdl::Model::default()),
            clips: HashMap::from([("walk".into(), Some(clip))]),
            grids: HashMap::from([("move_w".into(), (Arc::new(vec!["walk".into(); 2]), [2, 1]))]),
            menu_idle: None,
        };
        let original = AnimState { move_cycle: 0.25 };
        let bytes = postcard::to_allocvec(&original).unwrap();
        let mut restored: AnimState = postcard::from_bytes(&bytes).unwrap();
        let pose = set.evaluate_prepared(&mut restored, &input(), 0.0);
        assert_eq!(restored, original);
        assert_eq!(pose[0].w_axis.x, 0.25);
        let count = set.clips.len();
        assert_eq!(set.evaluate_prepared(&mut restored, &input(), 0.0), pose);
        assert_eq!(set.clips.len(), count);
        let mut compatibility = set.clone();
        assert_eq!(compatibility.evaluate(&mut restored, &input(), 0.0), pose);
        assert_eq!(restored, original);
    }

    #[test]
    fn cycle_advance_matches_reference_stride_at_both_tick_rates_and_stops_in_air() {
        for rate in [64, 128] {
            for (speed, ducked) in [(85.0, true), (130.0, false), (250.0, false)] {
                let mut input = input();
                input.speed = speed;
                input.ducked = ducked;
                let mut state = AnimState::default();
                for _ in 0..rate {
                    state.advance(&input, 1.0 / rate as f32);
                }
                assert!((state.move_cycle - 1.0 / 2.2).abs() < 0.00001);
                let moving = state.clone();
                input.on_ground = false;
                state.advance(&input, 1.0);
                assert_eq!(state, moving);
                input.on_ground = true;
                input.speed = 12.0;
                state.advance(&input, 1.0);
                assert_eq!(state, moving);
            }
        }
    }
}
