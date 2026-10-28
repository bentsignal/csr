//! The front-end sounds in the game (`crate::ui_audio::FrontendSound` → retail's front-end audio
//! object, `skate_audio::frontend`): the `fe` records from the install, played as `sk8_menu`
//! Splice one-shots in the audio pass (after the MixMap tick, once per pass, dt = the pass's host
//! ticks). Requests that arrive on a frame without a pass wait for the next one, as retail's queued
//! message waits for the next audio frame.
//!
//! CSR adaptation: the host is built from the pre-decoded `sk8_menu` tree and fed by the headless
//! audio pass (`crate::board_audio`) instead of Bevy messages; the record logic is unchanged.
use skate_audio::frontend::{FeTable, Frontend};

/// The front-end object and the records it plays.
pub(crate) struct FrontendHost {
    pub(crate) core: Frontend,
    pub(crate) table: FeTable,
    /// Splice sounds started (diagnostics, tests).
    pub(crate) started: Vec<(u64, u32)>,
}

impl FrontendHost {
    /// The `fe` records with their Splice bank, loaded into the runtime's Splice player.
    pub(crate) fn load(
        table: FeTable,
        bank: skate_audio::splice::SpliceBank,
        pcm: Vec<Option<std::sync::Arc<skate_audio::mixer::Pcm>>>,
        rt: &mut skate_audio::runtime::Runtime,
    ) -> Self {
        rt.splice.load_bank(&table.bank, bank, pcm, &mut rt.mixer);
        Self { core: Frontend::default(), table, started: Vec::new() }
    }

    /// Queue one record (an unknown key or a silent record plays nothing).
    pub(crate) fn request(&mut self, key: u64) -> bool {
        match self.table.sounds.get(&key) {
            Some(sound) => self.core.request(key, sound, false),
            None => false,
        }
    }

    /// One audio frame of `dt` seconds.
    pub(crate) fn frame(&mut self, dt: f32, runtime: &mut skate_audio::runtime::Runtime) {
        if !self.core.busy() {
            return;
        }
        let report = self.core.frame(dt, &self.table.bank, &mut runtime.splice_host());
        for &(key, id) in &report.started {
            if super::trace() {
                eprintln!("AUDIO_NATIVE frontend {key:016X} sk8_menu {id}");
            }
        }
        self.started.extend(report.started);
    }
}

/// The front-end volume from the MixMap word (int → single × retail's 1/32767 constant `0x38000100`).
pub(crate) fn volume(word: i32) -> f32 {
    ((word as f64) as f32) * f32::from_bits(0x3800_0100)
}

