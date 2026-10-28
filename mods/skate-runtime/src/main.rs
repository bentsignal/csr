// SPDX-License-Identifier: GPL-3.0-only
use mechanics_link::{Request, Response, VERSION};
fn main() {
    if std::env::args().nth(1).as_deref() != Some("--stdio-provider-v1") {
        return;
    }
    // The upstream coordinator contains sizeable fixed physical/input records.
    // Give its owner an explicit stack rather than inheriting Windows' 1 MiB
    // console entry-point stack (the standalone ECS has a different layout).
    std::thread::Builder::new()
        .name("skate-simulation".into())
        .stack_size(16 << 20)
        .spawn(serve)
        .expect("start Skate simulation owner")
        .join()
        .expect("Skate simulation owner panicked");
}
enum Audio {
    Idle,
    Loading(std::thread::JoinHandle<Result<csr_skate_runtime::board_audio::Assets, String>>),
    Ready(csr_skate_runtime::board_audio::Assets),
    Failed,
}
impl Audio {
    /// The decoded assets once the loader has finished (never blocks the request thread).
    fn ready(&mut self) -> Option<&csr_skate_runtime::board_audio::Assets> {
        if matches!(self, Audio::Loading(h) if h.is_finished()) {
            let Audio::Loading(handle) = std::mem::replace(self, Audio::Failed) else {
                unreachable!()
            };
            *self = match handle.join() {
                Ok(Ok(assets)) => Audio::Ready(assets),
                Ok(Err(e)) => {
                    eprintln!("Skate board audio unavailable: {e}");
                    Audio::Failed
                }
                Err(_) => {
                    eprintln!("Skate board audio loader panicked");
                    Audio::Failed
                }
            };
        }
        match self {
            Audio::Ready(assets) => Some(assets),
            _ => None,
        }
    }
}
fn serve() {
    let mut input = std::io::stdin().lock();
    let mut output = std::io::stdout().lock();
    let mut assets = None;
    let mut scene = None;
    let mut active = None;
    let mut runtime: Option<csr_skate_runtime::Runtime> = None;
    // The owner's board audio decodes once per process, off the request thread, while the map
    // prepares; riding stays silent until it is ready (or for good, after an error).
    let mut audio = Audio::Idle;
    let mut host: Option<(u64, csr_skate_runtime::board_audio::Host)> = None;
    loop {
        let request = match mechanics_link::receive::<Request>(&mut input) {
            Ok(r) => r,
            Err(_) => break,
        };
        let result = (|| -> Result<Response, String> {
            Ok(match request {
                Request::Initialize {
                    version,
                    assets: root,
                    scene: geometry,
                } => {
                    if version != VERSION {
                        return Err("unsupported mechanics process protocol".into());
                    }
                    let root = std::path::PathBuf::from(root);
                    // Metadata does not depend on the CS map or initial player.
                    // Compile the collision scene once, during map preparation.
                    let rt = csr_skate_runtime::Runtime::load(&root)?;
                    let rig = rt.rig();
                    let prepared = csr_skate_runtime::PreparedScene::new(&rt, geometry)?;
                    if matches!(audio, Audio::Idle) {
                        let root = root.clone();
                        audio = Audio::Loading(std::thread::spawn(move || {
                            csr_skate_runtime::board_audio::Assets::load(&root)
                        }));
                    }
                    host = None;
                    assets = Some(root);
                    scene = Some(prepared);
                    runtime = Some(rt);
                    active = None;
                    Response::Ready(rig)
                }
                Request::Restore {
                    session,
                    start,
                    frames,
                } => {
                    let geometry = scene.as_ref().ok_or("provider not initialized")?;
                    let state = frames.first().map(|f| f.scene.clone()).unwrap_or_default();
                    if let Some(old) = runtime.take() {
                        old.recycle_scene_world();
                    }
                    let mut rt = csr_skate_runtime::Runtime::load_prepared(
                        assets.as_ref().ok_or("assets not initialized")?,
                        geometry.clone(),
                        &start,
                        &state,
                    )?;
                    for frame in &frames {
                        rt.advance_host(frame)?;
                    }
                    let pose = rt.pose();
                    // The replay's steps were heard (or belong to another session).
                    rt.discard_audio();
                    if host.as_ref().is_some_and(|(s, _)| *s == session) {
                        host.as_mut().unwrap().1.cut(&rt);
                    } else {
                        host = None;
                    }
                    runtime = Some(rt);
                    active = Some((session, frames.len() as u64));
                    Response::Pose(pose,vec![])
                }
                Request::Advance { session, at, frame } => {
                    if active != Some((session, at)) {
                        Response::NeedRestore
                    } else {
                        let rt = runtime.as_mut().ok_or("runtime not initialized")?;
                        let pose = rt.advance_host(&frame)?;
                        active = Some((session, at + 1));
                        let samples = match audio.ready() {
                            Some(assets) => {
                                if host.as_ref().is_none_or(|(s, _)| *s != session) {
                                    let mut fresh = csr_skate_runtime::board_audio::Host::new(assets);
                                    fresh.cut(rt);
                                    host = Some((session, fresh));
                                }
                                host.as_mut().unwrap().1.frame(assets, rt, frame.dt)
                            }
                            None => {
                                rt.discard_audio();
                                vec![]
                            }
                        };
                        Response::Pose(pose, samples)
                    }
                }
            })
        })();
        let response = result.unwrap_or_else(Response::Error);
        if mechanics_link::send(&mut output, &response).is_err() {
            break;
        }
    }
}
