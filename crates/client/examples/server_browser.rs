//! Offscreen proof of declared installed layouts. No network requests or windows.
//! Captures go to a caller-supplied directory outside the checkout.
#[path = "../src/frontend_servers.rs"]
mod frontend_servers;
#[path = "../src/frontend_servers_data.rs"]
mod frontend_servers_data;
use bevy::{
    app::ScheduleRunnerPlugin,
    camera::RenderTarget,
    input::{
        ButtonState,
        keyboard::{Key, KeyboardInput},
    },
    prelude::*,
    render::{
        render_resource::{TextureFormat, TextureUsages},
        view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    },
    window::{ExitCondition, WindowPlugin},
    winit::WinitPlugin,
};
use frontend_servers_data::{Definition, Model, ServerKey, ServerRow, Snapshot, Tab};
use std::{path::PathBuf, time::Duration};

#[derive(Resource)]
struct Proof {
    definition: Definition,
    fonts: frontend_servers::Fonts,
    model: Model,
    snapshot: Snapshot,
    view: Option<frontend_servers::View>,
    camera: Entity,
    image: Handle<Image>,
    output: PathBuf,
    frame: u32,
    stage: u8,
    captured: bool,
}
fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: None,
                    exit_condition: ExitCondition::DontExit,
                    ..default()
                })
                .disable::<WinitPlugin>()
                .disable::<bevy::audio::AudioPlugin>(),
        )
        .add_plugins(ScheduleRunnerPlugin::run_loop(Duration::from_millis(16)))
        .insert_resource(ClearColor(Color::NONE))
        .add_systems(Startup, setup)
        .add_systems(Update, capture)
        .run();
}
fn setup(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut assets: ResMut<Assets<Font>>,
) {
    let output = PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("capture directory outside repository required"),
    );
    std::fs::create_dir_all(&output).unwrap();
    let (game_dir, game) = level::mount_game(None).unwrap();
    let mut platform = vfs::Vfs::new();
    platform.push_back(
        "legacy-platform",
        vfs::Mount::Dir(game_dir.parent().unwrap().join("platform")),
    );
    let definition = Definition::load(&game, &platform).unwrap();
    let windows = PathBuf::from(std::env::var_os("WINDIR").expect("Windows font directory"));
    let normal = assets.add(Font::from_bytes(
        std::fs::read(windows.join("Fonts/tahoma.ttf")).unwrap(),
    ));
    let bold = assets.add(Font::from_bytes(
        std::fs::read(windows.join("Fonts/tahomabd.ttf")).unwrap(),
    ));
    let mut fonts = frontend_servers::Fonts::default();
    fonts.insert("Default", normal);
    fonts.insert("UiBold", bold);
    let mut target = Image::new_target_texture(1920, 1080, TextureFormat::Rgba8UnormSrgb, None);
    target.texture_descriptor.usage |= TextureUsages::COPY_SRC;
    let image = images.add(target);
    let camera = commands
        .spawn((Camera2d, RenderTarget::Image(image.clone().into())))
        .id();
    let snapshot = Snapshot {
        generation: 1,
        rows: vec![ServerRow {
            key: ServerKey(1),
            endpoint: "127.0.0.1:27015".into(),
            name: None,
            map: None,
            players: None,
            ping_ms: None,
            favorite: true,
            connectable: true,
        }],
        ..default()
    };
    let model = Model {
        tab: Tab::Favorites,
        selected: Some(ServerKey(1)),
        ..default()
    };
    let view = frontend_servers::spawn(
        &mut commands,
        camera,
        Vec2::new(1920.0, 1080.0),
        &definition,
        &fonts,
        &model,
        &snapshot,
    )
    .unwrap();
    commands.insert_resource(Proof {
        definition,
        fonts,
        model,
        snapshot,
        view: Some(view),
        camera,
        image,
        output,
        frame: 0,
        stage: 0,
        captured: false,
    });
}
fn captured(_: On<ScreenshotCaptured>, mut proof: ResMut<Proof>) {
    proof.captured = true;
}
fn capture(mut commands: Commands, mut proof: ResMut<Proof>, mut exit: MessageWriter<AppExit>) {
    proof.frame += 1;
    assert!(proof.frame < 600, "offscreen screenshot did not complete");
    if proof.frame == 90 {
        let name = match proof.stage {
            0 => "favorites.png",
            1 => "address.png",
            2 => "scrolled.png",
            _ => "context.png",
        };
        commands
            .spawn(Screenshot::image(proof.image.clone()))
            .observe(save_to_disk(proof.output.join(name)))
            .observe(captured);
    }
    if !proof.captured {
        return;
    }
    if proof.stage == 3 {
        exit.write(AppExit::Success);
        return;
    }
    proof.captured = false;
    proof.frame = 0;
    if proof.stage == 0 {
        proof.stage = 1;
        let snapshot = proof.snapshot.clone();
        proof.model.command("AddServerByName", &snapshot);
        let event = KeyboardInput {
            key_code: KeyCode::KeyA,
            logical_key: Key::Character("localhost:27015".into()),
            state: ButtonState::Pressed,
            text: Some("localhost:27015".into()),
            repeat: false,
            window: Entity::PLACEHOLDER,
        };
        frontend_servers::keyboard(&event, false, true, &mut proof.model, &snapshot);
    } else if proof.stage == 1 {
        proof.stage = 2;
        let snapshot = proof.snapshot.clone();
        proof.model.command("Close", &snapshot);
        proof.snapshot.rows = (0..64)
            .map(|n| ServerRow {
                key: ServerKey(n),
                endpoint: format!("localhost:{}", 27015 + n),
                name: None,
                map: None,
                players: None,
                ping_ms: None,
                favorite: true,
                connectable: true,
            })
            .collect();
        proof.model.selected = None;
        let event = KeyboardInput {
            key_code: KeyCode::ArrowDown,
            logical_key: Key::ArrowDown,
            state: ButtonState::Pressed,
            text: None,
            repeat: false,
            window: Entity::PLACEHOLDER,
        };
        let snapshot = proof.snapshot.clone();
        let definition = proof.definition.clone();
        for _ in 0..70 {
            frontend_servers::keyboard_with_layout(
                &event,
                false,
                true,
                &mut proof.model,
                &snapshot,
                &definition,
            );
        }
        assert_eq!(proof.model.selected, Some(ServerKey(63)));
    } else {
        proof.stage = 3;
        let snapshot = proof.snapshot.clone();
        assert!(frontend_servers::context(
            &frontend_servers::UiAction::Select(ServerKey(63), snapshot.generation),
            true,
            &mut proof.model,
            &snapshot
        ));
    }
    if let Some(view) = proof.view.take() {
        view.despawn(&mut commands);
    }
    proof.view = Some(
        frontend_servers::spawn(
            &mut commands,
            proof.camera,
            Vec2::new(1920.0, 1080.0),
            &proof.definition,
            &proof.fonts,
            &proof.model,
            &proof.snapshot,
        )
        .unwrap(),
    );
}
