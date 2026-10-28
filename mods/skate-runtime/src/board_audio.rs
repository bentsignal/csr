// SPDX-License-Identifier: GPL-3.0-only
//! Headless host of the public rewrite's native board audio (`upstream/game_audio`): retail's
//! AEMS patch programs, Splice one-shots, MixMap mixer and granular rolling bed, rendered to
//! interleaved 48 kHz stereo for the CS client's cosmetic stream.
//!
//! [`Assets`] reads and decodes the owner's private audio once per provider process (the boot of
//! the upstream `native::Native::start_from` and `load_player_banks`). [`Host`] is that boot's
//! runtime for one riding session, driven once per admitted host frame exactly as the upstream
//! frame schedule drives it: `native::mixmap_frame` (seams on every frame; on frames that took
//! physics steps the inputs and the components' process), `mixmap_tick` (the console
//! evaluations, the components' update, the reverb owner), `grain_bed::update` and the
//! front-end object (`frontend::frame`). The audio device's pull is replaced by rendering the
//! frame's elapsed host time. Physics never reads anything here.
use crate::game_audio::{
    frontend::FrontendHost,
    grain_bed::{self, Bed},
    native::{self, HostClock, MIX_STEP},
    player_audio::{self, PlayerAudio},
    Library,
};
use skate_audio::{
    formats::{Bank, Project},
    mixer::{GROUP_PLAYER, Pcm},
    mixmap::{MixMap, MixMapFile, keys},
    runtime::Runtime,
    splice::SpliceBank,
};
use std::{collections::HashMap, sync::Arc};

type BankPcm = Vec<Option<Arc<Pcm>>>;

/// The owner's decoded board audio, immutable and shared by every session's [`Host`].
pub struct Assets {
    library: Library,
    projects: Vec<Project>,
    mixmap: MixMapFile,
    utility: (Bank, BankPcm),
    common: Option<(Bank, BankPcm)>,
    /// `player_audio::BANKS`: every component needs all of them (upstream turns all off otherwise).
    player: Vec<(&'static str, Bank, BankPcm)>,
    /// `player_audio::OPTIONAL_BANKS`: per component, its banks present in the install.
    optional: [Vec<(&'static str, Bank, BankPcm)>; 5],
    splice: Vec<(&'static str, SpliceBank, BankPcm)>,
    wheels: BankPcm,
    frontend: Option<(skate_audio::frontend::FeTable, SpliceBank, BankPcm)>,
    bed: Bed,
}

impl Assets {
    pub fn load(root: &std::path::Path) -> Result<Self, String> {
        static DSP: std::sync::Once = std::sync::Once::new();
        DSP.call_once(|| {
            let _ = skate_audio::dsp::init_fma();
        });
        let started = std::time::Instant::now();
        let library = Library::load(root)?;
        let files = library.aems();
        let projects = files
            .projects
            .iter()
            .map(|file| {
                let bytes = library.read(file).map_err(|e| format!("{file}: {e}"))?;
                Project::parse(file, &bytes).map_err(|e| format!("{file}: {e}"))
            })
            .collect::<Result<Vec<_>, String>>()?;
        if projects.is_empty() {
            return Err("the Skate audio has no AEMS projects".into());
        }
        let mixmap = {
            let file = files.mixmap.as_ref().ok_or("the Skate audio has no MixMapSK8.mxb")?;
            let bytes = library.read(file).map_err(|e| format!("{file}: {e}"))?;
            MixMapFile::parse(&bytes).map_err(|e| format!("{file}: {e}"))?
        };
        let utility = library.bank("emitter_utility")?;
        let common = library.bank(skate_audio::player::seams::UTILITY_BANK).ok();
        let player = player_audio::BANKS
            .iter()
            .map(|&stem| library.bank(stem).map(|(b, p)| (stem, b, p)))
            .collect::<Result<Vec<_>, String>>()?;
        let optional = player_audio::OPTIONAL_BANKS.map(|banks| {
            banks
                .iter()
                .filter_map(|&stem| library.bank(stem).ok().map(|(b, p)| (stem, b, p)))
                .collect::<Vec<_>>()
        });
        let splice = player_audio::SPLICE_BANKS
            .iter()
            .filter_map(|&stem| library.splice_bank(stem).map(|(b, p)| (stem, b, p)))
            .collect::<Vec<_>>();
        let wheels = player_audio::WHEEL_STREAMS.iter().map(|n| library.wheels_pcm(n)).collect();
        let frontend = library.frontend_sounds().and_then(|table| {
            let (bank, pcm) = library.splice_bank(&table.bank)?;
            Some((table, bank, pcm))
        });
        let bed = Bed::new(&library).ok_or("the Skate audio lacks grain recordings or their tuning")?;
        if crate::game_audio::trace() || std::env::var_os("CSR_SKATE_DIAGNOSTICS").is_some() {
            eprintln!(
                "SKATE_AUDIO assets {} projects, {} player banks, optional {:?}, splice {:?}, frontend {}, {:?}",
                projects.len(),
                player.len(),
                optional.each_ref().map(|v| v.len()),
                splice.iter().map(|s| s.0).collect::<Vec<_>>(),
                frontend.is_some(),
                started.elapsed()
            );
        }
        Ok(Self { library, projects, mixmap, utility, common, player, optional, splice, wheels, frontend, bed })
    }
}

/// The free-skate globals the upstream host writes every pass (`native::mixmap_frame`): Master
/// in1..3 = the retail option volumes at full scale (the CS client applies its own volume to the
/// stream), in4 = 32767; Music in1/2/5.
fn globals(m: &mut MixMap) {
    for id in 1..=4 {
        m.set_input(keys::MASTER, id, 32767);
    }
    for id in [1, 2, 5] {
        m.set_input(keys::MUSIC, id, 32767);
    }
}

/// `CSR_SKATE_AUDIO_TRACE=2`: also every sounding AEMS / Splice voice after each pass.
fn trace_voices() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("CSR_SKATE_AUDIO_TRACE").is_ok_and(|v| v == "2"))
}

/// One riding session's native audio runtime.
pub struct Host {
    rt: Runtime,
    mixmap: MixMap,
    player: PlayerAudio,
    bed: Bed,
    frontend: Option<FrontendHost>,
    clock: HostClock,
    holds: bool,
    /// A camera cut / restore: no seam interpolation or camera velocity across it.
    cut_wheels: Option<[[f32; 3]; 4]>,
    marker: (u64, u64),
    /// Owed output frames (48 kHz) for the elapsed host time.
    owed: f64,
    names: HashMap<usize, &'static str>,
}

impl Host {
    /// The upstream boot order (`native::Native::start_from`): projects, MixMap, buses, the
    /// utility bank, the player's banks (required, optional, Splice, wheel streams), then the boot
    /// posts c_emitter_utility → Start_up_Play_ctl → c_foley_utility (one shared generator).
    pub fn new(assets: &Assets) -> Self {
        let library = &assets.library;
        let mut rt = Runtime::new();
        for project in &assets.projects {
            rt.install_project(project);
        }
        let mut mixmap = MixMap::new(&assets.mixmap, &skate_audio::mixmap::RETAIL_INSTANCES);
        native::hold_flag_inputs(&mut mixmap);
        let mut player = PlayerAudio::new(library.player_tuning(), true);
        let (presets, eq) = library.bus_tuning();
        rt.mixer.buses.env.presets = presets;
        rt.mixer.buses.eq.set_records(&eq);
        // No Skate map region: the default environment preset (reverb01), as upstream without one.
        rt.mixer.buses.env.request(skate_audio::bus::env::DEFAULT_PRESET);
        if let Some([a, b]) = library.flange_presets() {
            rt.mixer.buses.flange.set_presets(a, b);
        }
        rt.mixer.buses.submix.enabled = true;
        let mut names = HashMap::new();
        let mut load = |rt: &mut Runtime, stem: &'static str, bank: &Bank, pcm: &BankPcm, group: bool| {
            let id = rt.load_bank(bank.clone(), pcm.clone());
            if group {
                rt.mixer.set_bank_group(id, GROUP_PLAYER);
            }
            names.insert(id, stem);
        };
        load(&mut rt, "emitter_utility", &assets.utility.0, &assets.utility.1, false);
        for (stem, bank, pcm) in &assets.player {
            load(&mut rt, stem, bank, pcm, true);
        }
        let mut optional = [false; 5];
        for (k, banks) in assets.optional.iter().enumerate() {
            for (stem, bank, pcm) in banks {
                load(&mut rt, stem, bank, pcm, true);
                optional[k] |= *stem == player_audio::OPTIONAL_BANKS[k][0];
            }
        }
        let mut contacts = false;
        for (stem, bank, pcm) in &assets.splice {
            rt.splice.load_bank(stem, bank.clone(), pcm.clone(), &mut rt.mixer);
            contacts |= *stem == player_audio::SPLICE_BANKS[0];
        }
        let wheels = assets.wheels.iter().all(Option::is_some);
        rt.load_streams(assets.wheels.clone());
        (player.rolling_on, player.rattle_on, player.slide_on, player.tricks_on, player.treatment_on) =
            (optional[0], optional[1], optional[2], optional[3], optional[4]);
        player.contacts_on = contacts;
        player.set_footstep_materials(library.footstep_materials());
        player.footsteps_on = contacts;
        player.contact_tuning = library.contacts_tuning();
        player.wheels_on = wheels;
        if let Some(class) = rt.eval.class_id("c_emitter_utility") {
            rt.post(class, &[]);
        }
        if let Some((bank, pcm)) = &assets.common {
            load(&mut rt, skate_audio::player::seams::UTILITY_BANK, bank, pcm, false);
            if let Some(class) = rt.eval.class_id(skate_audio::player::seams::UTILITY) {
                rt.post(class, &[]);
            }
        }
        if player.components && player.tricks_on {
            if let Some(id) = rt.eval.class_id(skate_audio::player::tricks::FOLEY_UTILITY) {
                rt.post(id, &[]);
            }
        }
        let frontend = assets
            .frontend
            .as_ref()
            .map(|(table, bank, pcm)| FrontendHost::load(table.clone(), bank.clone(), pcm.clone(), &mut rt));
        if crate::game_audio::trace() {
            eprintln!(
                "AUDIO_NATIVE boot rolling={} rattle={} slide={} tricks={} treatment={} contacts={} wheels={} frontend={}",
                player.rolling_on, player.rattle_on, player.slide_on, player.tricks_on, player.treatment_on,
                player.contacts_on, player.wheels_on, frontend.is_some()
            );
        }
        Self {
            rt,
            mixmap,
            player,
            bed: assets.bed.fresh(),
            frontend,
            clock: HostClock::default(),
            holds: true,
            cut_wheels: None,
            marker: (0, 0),
            owed: 0.,
            names,
        }
    }

    /// The physics restarted from its journal (a replay): its steps were discarded; treat the
    /// next frame as a camera cut.
    pub fn cut(&mut self, runtime: &crate::Runtime) {
        self.cut_wheels = Some(runtime.audio_cues.riding.audio.wheel_position);
        self.player.reset_listener();
        self.marker = (runtime.marker.ui.event, runtime.marker.refused);
    }

    /// One admitted host frame of `dt` seconds after `runtime` advanced: returns the frame's
    /// interleaved stereo.
    pub fn frame(&mut self, assets: &Assets, runtime: &mut crate::Runtime, dt: f32) -> Vec<f32> {
        let library = &assets.library;
        let cues = &mut runtime.audio_cues;
        let m = &mut self.mixmap;
        let rt = &mut self.rt;
        let player = &mut self.player;
        // The session marker's front-end records (PlayerUI `sub_82898FC8`), queued until a pass.
        if let Some(host) = &mut self.frontend {
            use skate_audio::frontend::{key, marker};
            let ui = &runtime.marker.ui;
            if ui.event != self.marker.0 {
                let record = match ui.action {
                    1 => Some(marker::ACTIVATE),
                    2 => Some(marker::PLACE),
                    3 => Some(marker::GOTO),
                    _ => None,
                };
                if let Some(record) = record {
                    host.request(key(record));
                }
            }
            if runtime.marker.refused != self.marker.1 {
                host.request(key(marker::ERROR));
            }
        }
        self.marker = (runtime.marker.ui.event, runtime.marker.refused);
        let discontinuity = runtime.camera.frame.is_some_and(|f| f.discontinuity);
        let now = cues.riding.audio.wheel_position;
        if discontinuity {
            self.cut_wheels = Some(now);
            player.reset_listener();
        }
        if self.cut_wheels.is_some_and(|w| w != now) {
            self.cut_wheels = None;
        }
        let before = if self.cut_wheels.is_some() { now } else { cues.riding.wheels_before };
        // Class_Seams on the console cadence at the rendered board's wheels, every frame.
        let period = runtime.physics.period().as_secs_f64();
        player.seam_alpha = Some((runtime.accumulator / period).clamp(0., 1.) as f32);
        player.step_wheels(before, now);
        player.seam_frame(m, &cues.riding.audio, dt, rt);
        if let Some(pass) = self.clock.pass(cues, false) {
            globals(m);
            for (id, x) in rt.mixer.buses.env.reverb_inputs().into_iter().enumerate() {
                m.set_input(keys::REVERB, id, x);
            }
            m.set_input(keys::PAUSE, 0, 0);
            let (ticks, calls) = (pass.ticks, pass.calls);
            self.bed.slew_calls = Some(calls);
            if !self.holds {
                native::hold_flag_inputs(m);
                self.holds = true;
            }
            if pass.clear_eq {
                rt.mixer.buses.eq.clear(player.eq_jitter());
            }
            let riding = cues.riding;
            let s = riding.audio;
            let pass_dt = ticks as f32 * MIX_STEP;
            // The listener follows the native gameplay camera (presentation's camera transform:
            // native At is the view direction), as upstream's `follow_camera`.
            let (camera, view) = runtime.camera.frame.map_or(
                ([s.com_position[0], s.com_position[1] + 1.5, s.com_position[2]], [0., 0., 1.]),
                |f| ([f.position[0], f.position[1], f.position[2]], f.basis.columns[2]),
            );
            let l = player.listener(camera, view, pass_dt, &s);
            player.jitter_steps = Some(calls);
            player.teleport_effect = (runtime.marker.ui.progress > 0.).then_some(runtime.marker.ui.progress);
            player.write_inputs(m, &s, Some(&l));
            let routing = player.components && player.rolling_on;
            self.bed.write_inputs(m, &s, !routing);
            let speed_scale = self.bed.push_scale();
            let loose = PlayerAudio::loose_board(&s, &riding);
            player.process(m, &s, rt, speed_scale, loose);
            for _ in 0..calls {
                m.tick(skate_audio::mixmap::cadence::CONSOLE_DT);
            }
            player.update(m, &s, rt, speed_scale, loose);
            rt.mixer.buses.flange.frame(std::array::from_fn(|i| m.level(keys::REVERB, i)));
            rt.mixer.buses.env.scale_frame(m.level(keys::REVERB, 4));
            let routed = routing.then(|| (std::mem::take(&mut player.routed.grains), player.routed.primary));
            grain_bed::step_with(&mut self.bed, library, m, &riding, pass_dt, &player.tuning, routed, |apply| apply(rt));
            if let Some(host) = &mut self.frontend {
                // sub_824958F0: sk8_menu starts scale by the Master controller's output 0.
                host.core.volume = crate::game_audio::frontend::volume(m.level(keys::MASTER, 0));
                host.frame(pass_dt, rt);
            }
            if trace_voices() {
                for v in rt.mixer.snapshot().iter().filter(|v| v.gain > 0.0005) {
                    let bank = self.names.get(&v.bank).copied().unwrap_or("splice/stream");
                    eprintln!("AUDIO_VOICE bank={bank} slot={} gain={:.4} pitch={:.3}", v.slot, v.gain, v.pitch);
                }
            }
        }
        self.owed += f64::from(dt) * f64::from(mechanics_link::AUDIO_RATE);
        let frames = self.owed.floor().min(8192.) as usize;
        self.owed -= frames as f64;
        let mut out = vec![0.; frames * 2];
        rt.fill_stereo(&mut out);
        for x in &mut out {
            *x = x.clamp(-1., 1.);
        }
        out
    }
}
