//! Original CS:GO front-end music, read from the player's mounted content.
//! Music has its own gain; it never goes through weapon/ambient gain or panning.
use crate::{audio::{SoundGain, SoundLibrary}, session::{AppState, Persistent}, settings::Settings};
use bevy::{audio::{AudioPlayer, AudioSink, AudioSinkPlayback, AudioSource, PlaybackSettings, Volume}, prelude::*};
use content::{ContentId, ContentKind};

const DECLARATION: &str = "sound/music/valve_csgo_01/game_sounds_music.txt";
const ENTRY: &str = "Musix.HalfTime.valve_csgo_01";

#[derive(Clone, Debug)]
struct Track {
    id: ContentId,
    wave: String,
    volume: f32,
}
impl Track {
    fn parse(text: &str) -> Option<Self> {
        let document = kv::parse(text).ok()?;
        let entry = document.block(ENTRY)?;
        let update = entry.block("operator_stacks")?.block("update_stack")?;
        if update.block("snd_cuevolume_convar")?.str("convar")? != "snd_menumusic_volume" {
            return None;
        }
        let wave = entry.str("wave")?.trim_start_matches('*').replace('\\', "/");
        // Explicit original CS:GO namespace. Never select the first music kit,
        // a modern default kit ID, a loose absolute file or a CS2-named track.
        if wave != "music/valve_csgo_01/mainmenu.mp3" { return None; }
        let volume = entry.f32("volume")?;
        if !volume.is_finite() || !(0.0..=1.0).contains(&volume) { return None; }
        Some(Self { id: ContentId::csgo(ContentKind::Sound, "music_valve_csgo_01_mainmenu"), wave: format!("sound/{wave}"), volume })
    }
}

#[derive(Resource, Default)]
pub(crate) struct MenuMusic {
    track: Option<Track>,
    entity: Option<Entity>,
    unavailable: bool,
}
impl MenuMusic {
    pub(crate) fn load(fs: &vfs::Vfs) -> Self {
        Self { track: fs.read_string(DECLARATION).and_then(|text| Track::parse(&text)), ..default() }
    }
}

#[derive(Component)]
pub(crate) struct FrontendMusic;

pub(crate) fn update(
    mut commands: Commands,
    state: Res<State<AppState>>,
    settings: Res<Settings>,
    data: Res<crate::GameData>,
    mut music: ResMut<MenuMusic>,
    mut sounds: ResMut<SoundLibrary>,
    mut sources: ResMut<Assets<AudioSource>>,
    players: Query<(), With<FrontendMusic>>,
) {
    let front = matches!(state.get(), AppState::Menu | AppState::Browser);
    if !front {
        if let Some(entity) = music.entity.take() {
            commands.entity(entity).try_despawn();
            info!("menu_music stop for {:?}", state.get());
        }
        return;
    }
    let Some(track) = music.track.clone() else { return; };
    // SelectedMusic's authored default mixer gain relative to Weapons1.
    // Full music operator/DSP/dashboard transitions remain outside this slice.
    let gain = track.volume * (0.6 / 0.7) * settings.audio_options.menu_music_volume;
    if let Some(entity) = music.entity {
        if players.contains(entity) { return; }
        music.entity = None;
    }
    if music.unavailable { return; }
    let Some((source, _)) = sounds.source(&data.vfs, &mut sources, &track.wave) else {
        music.unavailable = true;
        warn!("Original menu music {} unavailable; keeping the front end silent", track.id);
        return;
    };
    let entity = commands.spawn((
        Persistent, FrontendMusic, AuthoredMusicGain(track.volume * (0.6 / 0.7)), AudioPlayer(source), SoundGain(gain),
        PlaybackSettings::LOOP.with_volume(Volume::Linear(sounds.master_volume * gain)),
    )).id();
    music.entity = Some(entity);
    info!("menu_music start {} gain {:.6}", track.id, gain);
}

#[derive(Component)]
pub(crate) struct AuthoredMusicGain(f32);

pub(crate) fn update_gain(
    settings: Res<Settings>,
    sounds: Res<SoundLibrary>,
    mut players: Query<(&AuthoredMusicGain, &mut SoundGain, &mut PlaybackSettings, Option<&mut AudioSink>), With<FrontendMusic>>,
) {
    for (authored, mut gain, mut playback, sink) in &mut players {
        gain.0 = authored.0 * settings.audio_options.menu_music_volume;
        let volume = Volume::Linear(sounds.master_volume * gain.0);
        // Keep deferred starts and already-created sinks on the same value.
        playback.volume = volume;
        if let Some(mut sink) = sink { sink.set_volume(volume); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_track_requires_original_namespace_and_matching_gain_control() {
        let declaration = |wave: &str, control: &str, volume: &str| format!(
            "\"{ENTRY}\" {{ wave \"{wave}\" volume \"{volume}\" operator_stacks {{ update_stack {{ snd_cuevolume_convar {{ convar \"{control}\" }} }} }} }}"
        );
        let original = declaration("*music/valve_csgo_01/mainmenu.mp3", "snd_menumusic_volume", "0.8");
        let track = Track::parse(&original).unwrap();
        assert_eq!(track.wave, "sound/music/valve_csgo_01/mainmenu.mp3");
        assert_eq!(track.id.to_string(), "csgo_sound_music_valve_csgo_01_mainmenu");
        for invalid in [
            declaration("*music/valve_cs2_01/mainmenu.mp3", "snd_menumusic_volume", "0.8"),
            declaration("C:/external.mp3", "snd_menumusic_volume", "0.8"),
            declaration("*music/valve_csgo_01/mainmenu.mp3", "snd_roundend_volume", "0.8"),
            declaration("*music/valve_csgo_01/mainmenu.mp3", "snd_menumusic_volume", "NaN"),
        ] { assert!(Track::parse(&invalid).is_none()); }
    }

    #[test]
    fn music_slider_updates_a_real_sink_and_pending_start_without_scaling_weapon_gain() {
        let mut app = App::new();
        app.init_resource::<Settings>().init_resource::<SoundLibrary>()
            .add_systems(Update, update_gain);
        app.world_mut().resource_mut::<SoundLibrary>().master_volume = 0.5;
        let (player, _queue) = rodio::Player::new();
        let music = app.world_mut().spawn((FrontendMusic, AuthoredMusicGain(0.8), SoundGain(0.0), PlaybackSettings::LOOP, AudioSink::new(player))).id();
        let ordinary = app.world_mut().spawn(SoundGain(0.35)).id();
        app.world_mut().resource_mut::<Settings>().set_convar_number("snd_menumusic_volume", 0.25);
        app.update();
        assert!((app.world().get::<AudioSink>(music).unwrap().volume().to_linear() - 0.1).abs() < 1e-6);
        assert_eq!(app.world().get::<PlaybackSettings>(music).unwrap().volume, Volume::Linear(0.1));
        assert_eq!(app.world().get::<SoundGain>(ordinary).unwrap().0, 0.35);
        app.world_mut().resource_mut::<Settings>().set_convar_number("snd_menumusic_volume", 0.0);
        app.update();
        assert_eq!(app.world().get::<AudioSink>(music).unwrap().volume().to_linear(), 0.0);
        app.world_mut().resource_mut::<Settings>().set_convar_number("snd_menumusic_volume", 1.0);
        app.world_mut().resource_mut::<SoundLibrary>().master_volume = 0.0;
        app.update();
        assert_eq!(app.world().get::<AudioSink>(music).unwrap().volume().to_linear(), 0.0);
        app.world_mut().resource_mut::<SoundLibrary>().master_volume = 0.5;
        app.update();
        assert!((app.world().get::<AudioSink>(music).unwrap().volume().to_linear() - 0.4).abs() < 1e-6);
        let settings = app.world().resource::<Settings>();
        assert_eq!(Settings::parse(&settings.encode()), *settings);
    }
}
