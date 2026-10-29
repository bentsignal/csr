//! The native board/player audio of the public rewrite (`skate-game/src/game_audio`), hosted
//! headless. `skate_events`, `player_audio`, `grain_bed`, `frontend` and `library` are the
//! upstream modules with their Bevy systems, tests and mod-content paths removed (see README).
//! `native` keeps only the host clock and constants of the upstream `native.rs`; `mod_audio` /
//! `mod_rules` are pass-through stand-ins for the upstream mod event and rule layers (no mods
//! run here). `crate::board_audio` drives them per host frame exactly as `native::mixmap_frame`,
//! `mixmap_tick` and `grain_bed::update` do.

/// `CSR_SKATE_AUDIO_TRACE=1` (or the upstream `SKATE_AUDIO_TRACE=1`): log every component post,
/// release, Splice start, grain bind and front-end sound on stderr.
pub(crate) fn trace() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        ["CSR_SKATE_AUDIO_TRACE", "SKATE_AUDIO_TRACE"]
            .iter()
            .any(|k| std::env::var_os(k).is_some_and(|v| v != "0"))
    })
}

/// The upstream modules' Bevy log lines: traced events on stderr.
macro_rules! info {
    ($($t:tt)*) => {
        if $crate::game_audio::trace() {
            eprintln!($($t)*);
        }
    };
}
macro_rules! warn {
    ($($t:tt)*) => {
        eprintln!($($t)*)
    };
}

pub(crate) mod frontend;
pub(crate) mod grain_bed;
pub(crate) mod library;
pub(crate) mod player_audio;
pub(crate) mod skate_events;

pub(crate) use library::Library;

/// The host clock and constants of the upstream `native.rs` (verbatim).
pub(crate) mod native {
    use skate_audio::mixmap::{MixMap, keys};

    /// The host's step: one 60 Hz physics step. The host ticks once per physics step ([`HostClock`]);
    /// the MixMap evaluates on every second one (the console's 30 Hz, `skate_audio::mixmap::cadence`).
    pub(crate) const MIX_STEP: f32 = 1.0 / 60.0;
    /// The most host ticks one frame takes (the steps beyond are dropped; their pulses stay latched).
    pub(crate) const MAX_STEPS_PER_FRAME: u32 = 4;

    /// What a frame's host pass runs ([`HostClock::pass`]).
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) struct Pass {
        /// Host ticks: the physics steps taken (capped at [`MAX_STEPS_PER_FRAME`]).
        pub ticks: usize,
        /// Console evaluations they complete (the console cadence).
        pub calls: usize,
        /// `sub_82491180` (the eEQChain clear) runs in this pass.
        pub clear_eq: bool,
    }

    /// The audio host's clock: the physics steps `skate_events::observe` published since the last
    /// frame.
    #[derive(Default)]
    pub(crate) struct HostClock {
        /// The console's 30 Hz evaluation grid over the steps.
        cadence: skate_audio::mixmap::cadence::Cadence,
    }

    impl HostClock {
        /// Take the frame's steps. `None` (no pass: no inputs, process, tick or update) when nothing
        /// stepped, or while the game is silenced: those steps and their pulses are dropped.
        pub(crate) fn pass(&mut self, cues: &mut super::skate_events::Cues, silenced: bool) -> Option<Pass> {
            let steps = cues.take_steps();
            if steps == 0 || silenced {
                return None;
            }
            let ticks = steps.min(MAX_STEPS_PER_FRAME) as usize;
            let calls = self.cadence.advance(ticks);
            let clear_eq = calls > 0;
            Some(Pass { ticks, calls, clear_eq })
        }
    }

    /// The one-frame flag inputs the console's writers set for a whole evaluation, held between our 60
    /// Hz writes and the 30 Hz evaluations ([`MixMap::hold_input`]).
    pub(crate) fn hold_flag_inputs(m: &mut MixMap) {
        for (key, id) in [(keys::contacts(0), 1), (keys::contacts(0), 6), (keys::rail(0), 1), (keys::skateboard(0), 0), (keys::skateboard(0), 4), (keys::cracks(0), 0)] {
            m.hold_input(key, id);
        }
    }
}

/// Stand-in for the upstream mod event layer: no subscribers, no rules. [`Observed`] forwards
/// every Splice call unchanged and traces the starts.
pub(crate) mod mod_audio {
    use skate_audio::player::components::Slot;
    use skate_audio::player::contacts::SpliceHost;

    pub(crate) type EventBuf = Option<Vec<EventRow>>;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) enum EventKind {
        Post,
        Release,
        Splice,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) enum Source {
        Player,
    }

    #[derive(Clone, Copy, Debug)]
    pub(crate) struct EventRow {
        pub kind: EventKind,
        pub source: Source,
        pub class: &'static str,
        pub slot: &'static str,
        pub id: i32,
        pub owner: u64,
    }

    pub(crate) fn record(_rows: &mut EventBuf, _row: EventRow) {}

    pub(crate) fn player_slot(_slot: &Slot) -> (&'static str, i32) {
        ("", 0)
    }

    pub(crate) struct Observed<'a, 'b> {
        inner: &'a mut dyn SpliceHost,
        _rows: &'b mut EventBuf,
    }

    impl<'a, 'b> Observed<'a, 'b> {
        pub(crate) fn new(inner: &'a mut dyn SpliceHost, rows: &'b mut EventBuf, _source: Source, _owner: u64) -> Self {
            Self { inner, _rows: rows }
        }

        pub(crate) fn rules(self, _rules: Option<&'b super::mod_rules::RuleSet>) -> Self {
            self
        }
    }

    impl SpliceHost for Observed<'_, '_> {
        fn set_route(&mut self, route: skate_audio::bus::Route) {
            self.inner.set_route(route);
        }
        fn set_submix(&mut self, submix: Option<skate_audio::player::footsteps::Submix>) {
            self.inner.set_submix(submix);
        }
        fn start(&mut self, bank: &str, id: u32, block: [f32; 6]) -> Option<skate_audio::splice::SoundId> {
            let sound = self.inner.start(bank, id, block);
            info!("AUDIO_NATIVE splice {bank}:{id} block={block:?} started={}", sound.is_some());
            sound
        }
        fn update(&mut self, sound: skate_audio::splice::SoundId, block: [f32; 6]) {
            self.inner.update(sound, block);
        }
        fn alive(&self, sound: skate_audio::splice::SoundId) -> bool {
            self.inner.alive(sound)
        }
        fn release(&mut self, sound: skate_audio::splice::SoundId) {
            self.inner.release(sound);
        }
    }
}

/// Stand-in for the upstream mod rule set: nothing is muted.
pub(crate) mod mod_rules {
    pub(crate) struct RuleSet;

    impl RuleSet {
        pub(crate) fn mutes(&self, _row: &super::mod_audio::EventRow) -> bool {
            false
        }
    }
}
