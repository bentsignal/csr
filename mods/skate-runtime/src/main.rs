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
fn serve() {
    let mut input = std::io::stdin().lock();
    let mut output = std::io::stdout().lock();
    let mut assets = None;
    let mut scene = None;
    let mut active = None;
    let mut runtime: Option<csr_skate_runtime::Runtime> = None;
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
                    // Compile that large collision scene once, on Restore.
                    let rt = csr_skate_runtime::Runtime::load(&root)?;
                    let rig = rt.rig();
                    let prepared = csr_skate_runtime::PreparedScene::new(&rt, geometry)?;
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
                    runtime = Some(rt);
                    active = Some((session, frames.len() as u64));
                    Response::Pose(pose)
                }
                Request::Advance { session, at, frame } => {
                    if active != Some((session, at)) {
                        Response::NeedRestore
                    } else {
                        let pose = runtime
                            .as_mut()
                            .ok_or("runtime not initialized")?
                            .advance_host(&frame)?;
                        active = Some((session, at + 1));
                        Response::Pose(pose)
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
