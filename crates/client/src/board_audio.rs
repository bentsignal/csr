//! Board audio from the trusted Skate provider: the public rewrite's native
//! board/player sounds (rolling, pops, flips, landings, grinds, pushes, foot
//! brakes, session marker), rendered by the provider and streamed here as
//! interleaved 48 kHz stereo. Presentation only: playback never changes
//! gameplay or physics, and no audio enters simulation state.
use bevy::{
    audio::{AudioPlayer, AudioSink, AudioSinkPlayback, PlaybackSettings, Volume},
    prelude::*,
};
#[derive(Resource, Default)]
pub(crate) struct BoardAudio {
    stream: crate::board_stream::Stream,
    key: Option<(u32, u64, String)>,
    last_tick: u64,
    last_tick_time: f32,
    logged: bool,
}
pub(crate) fn update(
    mut commands: Commands,
    world: Res<crate::player::SimWorld>,
    local: Res<crate::player::LocalPlayer>,
    mut state: ResMut<BoardAudio>,
    settings: Res<crate::settings::Settings>,
    time: Res<Time>,
    windows: Query<&Window>,
    mut streams: ResMut<Assets<crate::board_stream::Stream>>,
    mut playing: Query<(Entity, Option<&mut AudioSink>), With<crate::board_stream::Playing>>,
) {
    let now = time.elapsed_secs();
    if state.last_tick != world.0.tick {
        state.last_tick = world.0.tick;
        state.last_tick_time = now;
    }
    let active = world.0.player(local.id).filter(|p| p.alive).and_then(|p| {
        p.mechanic
            .as_ref()
            .and_then(|m| m.external.as_ref().map(|s| (p, m, s)))
    });
    let key = active.map(|(p, m, _)| (p.id, p.life_epoch, m.archive_hash.clone()));
    if state.key != key {
        state.stream.clear();
        for (e, _) in &playing {
            commands.entity(e).despawn();
        }
        state.key = key;
        state.logged = false;
        if active.is_some() {
            commands.spawn((
                crate::session::MatchEntity,
                AudioPlayer(streams.add(state.stream.clone())),
                PlaybackSettings::default().with_volume(Volume::Linear(0.)),
                crate::board_stream::Playing,
            ));
        }
        return;
    }
    let master = settings
        .audio_options
        .effective_volume(settings.volume, windows.iter().any(|w| w.focused));
    // A paused or stalled simulation stops producing audio; drop what is queued.
    let running = now - state.last_tick_time < 0.15;
    if !running {
        state.stream.clear();
    }
    for (_, sink) in &mut playing {
        if let Some(mut sink) = sink {
            sink.set_volume(Volume::Linear(if running { master } else { 0. }));
        }
    }
    let Some((_, m, s)) = active else { return };
    let Some(provider) = world
        .0
        .mechanics
        .iter()
        .find(|v| v.id == m.id)
        .and_then(|v| v.external.as_ref())
    else {
        return;
    };
    let pcm = provider.take_audio(s.session);
    if std::env::var_os("CSR_SKATE_AUDIO_TRACE").is_some()
        && !state.logged
        && pcm.iter().any(|v| v.abs() > 0.0001)
    {
        state.logged = true;
        info!(
            "SKATE_AUDIO native board stream: {} stereo frames, peak={:.5}, volume={master:.3}",
            pcm.len() / 2,
            pcm.iter().fold(0f32, |p, v| p.max(v.abs()))
        );
    }
    state.stream.push(pcm);
}
