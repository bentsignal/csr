//! Local board foley from the owner's private pack. Simulation poses/inputs
//! select presentation sounds; playback never changes gameplay or physics.
use bevy::{
    audio::{AudioPlayer, AudioSink, AudioSinkPlayback, AudioSource, PlaybackSettings, Volume},
    prelude::*,
};
use std::collections::HashMap;
#[derive(Component)]
pub(crate) struct Loop(&'static str);
#[derive(Resource, Default)]
pub(crate) struct BoardAudio {
    native_rolling:bool,
    marker_event:u64,
    stream:crate::board_stream::Stream,
    key: Option<(u32, u64, String)>,
    clips: HashMap<String, Handle<AudioSource>>,
    previous: Option<(u64, bool, f32, u16)>,
    last_tick: u64,
    last_tick_time: f32,
    push_at: f32,
    logged: std::collections::HashSet<&'static str>,
}
pub(crate) fn update(
    mut commands: Commands,
    world: Res<crate::player::SimWorld>,
    local: Res<crate::player::LocalPlayer>,
    visuals: Option<Res<crate::movement_modes::Visuals>>,
    mut state: ResMut<BoardAudio>,
    mut sources: ResMut<Assets<AudioSource>>,
    mut loops: Query<(Entity, &Loop, Option<&mut AudioSink>),Without<crate::board_stream::Playing>>,
    settings: Res<crate::settings::Settings>,
    time: Res<Time>,
    windows: Query<&Window>,
    mut streams:ResMut<Assets<crate::board_stream::Stream>>,
    mut playing:Query<(Entity,Option<&mut AudioSink>),(With<crate::board_stream::Playing>,Without<Loop>)>,
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
        for (e,_) in &playing{commands.entity(e).despawn();}
        for (e, _, _) in &loops {
            commands.entity(e).despawn();
        }
        state.key = key;
        state.clips.clear();
        state.previous = None;
        state.marker_event=0;
        state.logged.clear();
        if let Some((_, m, _)) = active {
            commands.spawn((crate::session::MatchEntity,AudioPlayer(streams.add(state.stream.clone())),PlaybackSettings::default(),crate::board_stream::Playing));
            if let Some(model) = visuals.as_ref().and_then(|v| v.1.get(&m.id)) {
                state.native_rolling=model.native_audio;
                for (name, bytes) in &model.audio {
                    if let Some((bytes, _)) = crate::audio::decoded_audio(bytes.to_vec()) {
                        state.clips.insert(
                            name.clone(),
                            sources.add(AudioSource {
                                bytes: bytes.into(),
                            }),
                        );
                    } else {
                        warn!("Private Skate audio could not decode: {name}");
                    }
                }
                for name in ["roll", "grind", "brake"] {
                    if let Some(clip) = state.clips.get(name) {
                        commands.spawn((
                            crate::session::MatchEntity,
                            AudioPlayer(clip.clone()),
                            PlaybackSettings::LOOP.with_volume(Volume::Linear(0.)),
                            Loop(name),
                        ));
                    }
                }
            }
        }
        return;
    }
    let master = settings
        .audio_options
        .effective_volume(settings.volume, windows.iter().any(|w| w.focused));
    let running = now - state.last_tick_time < 0.15;
    if !running {state.stream.clear();}
    for (_,sink) in &mut playing {if let Some(mut sink)=sink {sink.set_volume(Volume::Linear(if running{master}else{0.}));}}
    if let Some((_,m,s))=active {
        if let Some(provider)=world.0.mechanics.iter().find(|v|v.id==m.id).and_then(|v|v.external.as_ref()) {
            let pcm=provider.take_audio(s.session);
            if std::env::var_os("CSR_SKATE_AUDIO_TRACE").is_some() && pcm.iter().any(|v|v.abs()>0.0001) && state.logged.insert("native PCM"){
                info!("SKATE_AUDIO native granular stream: {} samples, peak={:.5}",pcm.len(),pcm.iter().fold(0f32,|p,v|p.max(v.abs())));
            }
            state.stream.push(pcm);
        }
    }
    let (rolling, grinding, braking, speed) =
        active.map_or((false, false, false, 0.), |(p, _, s)| {
            let speed = p.movement.velocity.truncate().length();
            let grind = matches!(s.pose.state, 400..=405);
            (
                s.pose.grounded && matches!(s.pose.state, 100..=105) && speed > 8.,
                grind,
                p.last_input.controller.buttons & 0x2000 != 0 && s.pose.grounded,
                speed,
            )
        });
    for (_, kind, sink) in &mut loops {
        let Some(mut sink) = sink else { continue };
        let on = match kind.0 {
            "roll" => !state.native_rolling && rolling && !braking,
            "grind" => grinding,
            "brake" => braking && speed > 15.,
            _ => false,
        };
        let gain = if on && running {
            master * (speed / 450.).sqrt().clamp(0., 1.) * 0.28
        } else {
            0.
        };
        sink.set_volume(Volume::Linear(gain));
        if gain > 0.
            && std::env::var_os("CSR_SKATE_AUDIO_TRACE").is_some()
            && state.logged.insert(kind.0)
        {
            info!(
                "SKATE_AUDIO loop={} gain={gain:.4} speed={speed:.2}",
                kind.0
            );
        }
        sink.set_speed((0.75 + speed / 700.).clamp(0.75, 1.8));
        if on && running {
            sink.play();
        } else {
            sink.pause();
        }
    }
    if let Some((p, _, s)) = active {
        if state.marker_event!=s.pose.marker.event {
            state.marker_event=s.pose.marker.event;
            let name=match s.pose.marker.action {1=>"marker-open",2=>"marker-set",3=>"marker-return",_=>""};
            if let Some(clip)=state.clips.get(name){commands.spawn((crate::session::MatchEntity,AudioPlayer(clip.clone()),PlaybackSettings::DESPAWN.with_volume(Volume::Linear(master*0.45))));}
        }
        let generation = s.pose.generation;
        if state.previous.is_some_and(|v| v.0 == generation) {
            return;
        }
        let mut event = None;
        if let Some((_, grounded, _, buttons)) = state.previous {
            if grounded && !s.pose.grounded && p.movement.velocity.z > 60. {
                event = Some(("pop", 0.50));
            } else if !grounded && s.pose.grounded {
                event = Some(("land", 0.45));
            } else if rolling
                && p.last_input.controller.buttons & 0x5000 != 0
                && (buttons & 0x5000 == 0 || now - state.push_at > 0.45)
            {
                event = Some(("push", 0.25));
                state.push_at = now;
            }
        }
        state.previous = Some((
            generation,
            s.pose.grounded,
            speed,
            p.last_input.controller.buttons,
        ));
        if running {
            if let Some((name, gain)) = event {
                if let Some(clip) = state.clips.get(name) {
                    if std::env::var_os("CSR_SKATE_AUDIO_TRACE").is_some() {
                        info!("SKATE_AUDIO event={name} gain={:.4}", master * gain);
                    }
                    commands.spawn((
                        crate::session::MatchEntity,
                        AudioPlayer(clip.clone()),
                        PlaybackSettings::DESPAWN.with_volume(Volume::Linear(master * gain)),
                    ));
                }
            }
        }
    }
}
