//! Trusted application provider, isolated from pack code and the CS executable.
//! Its process is a derived cache: restoring any journal rebuilds the exact
//! coordinator from the original spawn, controller inputs and scene poses.
use mechanics_link::{Frame, Pose, Request, Response, Rig, Scene, Start};
use sha2::{Digest, Sha256};
use std::{
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{Mutex, mpsc},
    time::Duration,
};
pub mod geometry;
pub mod hud;
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Journal {
    chunks: Vec<std::sync::Arc<[Frame]>>,
    tail: Vec<Frame>,
    len: usize,
}
impl Journal {
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn push(&mut self, frame: Frame) {
        self.tail.push(frame);
        self.len += 1;
        if self.tail.len() == 128 {
            self.chunks.push(std::mem::take(&mut self.tail).into());
        }
    }
    pub fn to_vec(&self) -> Vec<Frame> {
        self.chunks
            .iter()
            .flat_map(|chunk| chunk.iter())
            .chain(self.tail.iter())
            .cloned()
            .collect()
    }
}
pub fn digest(parent: [u8; 32], value: &impl serde::Serialize) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(parent);
    hash.update(serde_json::to_vec(value).expect("finite mechanics journal"));
    hash.finalize().into()
}

pub struct Program {
    audio:Mutex<(u64,std::collections::VecDeque<f32>)>,
    hud: hud::Overlay,
    executable: PathBuf,
    assets: PathBuf,
    fingerprint: String,
    process: Mutex<Option<Process>>,
    geometry: Mutex<Option<(std::sync::Arc<collision::CollisionWorld>, Scene)>>,
}
struct Process {
    child: Child,
    input: ChildStdin,
    responses: mpsc::Receiver<Result<Response, String>>,
    scene: usize,
    rig: Rig,
    expected: Option<(u64, usize, [u8; 32])>,
}
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Process {
    fn exchange(&mut self, request: &Request) -> Result<Response, String> {
        mechanics_link::send(&mut self.input, request)?;
        self.responses
            .recv_timeout(Duration::from_secs(90))
            .map_err(|_| "mechanics provider stopped responding")?
    }
}
impl Program {
    /// The caller chooses an application-shipped executable, never a pack path.
    pub fn new(executable: PathBuf, assets: PathBuf) -> Result<Self, String> {
        if !executable.is_file() || !assets.join("private/game.json").is_file() {
            return Err("movement runtime or private assets missing".into());
        }
        let fingerprint = format!(
            "{:x}",
            Sha256::digest(std::fs::read(&executable).map_err(|e| e.to_string())?)
        );
        Ok(Self {
            audio:Mutex::new((0,Default::default())),
            hud: Default::default(),
            executable,
            assets,
            fingerprint,
            process: Mutex::new(None),
            geometry: Mutex::new(None),
        })
    }
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
    fn start(&self, scene: &Scene, key: usize) -> Result<Process, String> {
        eprintln!(
            "Movement provider scene: {} triangles, {} doors",
            scene.triangles.len(),
            scene.doors.len()
        );
        let mut command = Command::new(&self.executable);
        command
            .arg("--stdio-provider-v1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn().map_err(|e| e.to_string())?;
        let input = child.stdin.take().ok_or("provider stdin missing")?;
        let mut output = child.stdout.take().ok_or("provider stdout missing")?;
        let (sender, responses) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            loop {
                let response = mechanics_link::receive(&mut output);
                let failed = response.is_err();
                if sender.send(response).is_err() || failed {
                    break;
                }
            }
        });
        let mut p = Process {
            child,
            input,
            responses,
            scene: key,
            rig: Rig {
                names: vec![],
                parents: vec![],
                rest: vec![],
                to_source: [[0.; 4]; 4],
                bone_basis: [[0.; 4]; 4],
            },
            expected: None,
        };
        match p.exchange(&Request::Initialize {
            version: mechanics_link::VERSION,
            assets: self.assets.to_string_lossy().into_owned(),
            scene: scene.clone(),
        })? {
            Response::Ready(rig, textures) => {
                p.rig = rig;
                self.hud.textures(&self.assets, &textures);
            }
            Response::Error(e) => return Err(e),
            _ => return Err("provider initialization rejected".into()),
        }
        Ok(p)
    }
    pub fn advance(
        &self,
        scene: &Scene,
        key: usize,
        session: u64,
        start: &Start,
        history: &Journal,
        parent: [u8; 32],
        frame: &Frame,
    ) -> Result<(Pose, Rig), String> {
        let mut guard = self.process.lock().map_err(|_| "provider lock poisoned")?;
        if guard.as_ref().is_none_or(|p| p.scene != key) {
            *guard = Some(self.start(scene, key)?);
        }
        let p = guard.as_mut().unwrap();
        let request = Request::Advance {
            session,
            at: history.len() as u64,
            frame: frame.clone(),
        };
        let result = (|| {
            let mut response = if p.expected == Some((session, history.len(), parent)) {
                p.exchange(&request)?
            } else {
                Response::NeedRestore
            };
            if matches!(response, Response::NeedRestore) {
                match p.exchange(&Request::Restore {
                    session,
                    start: start.clone(),
                    frames: history.to_vec(),
                })? {
                    Response::Pose(_,_,_) => {if let Ok(mut a)=self.audio.lock(){a.1.clear();}}
                    Response::Error(e) => return Err(e),
                    _ => return Err("provider restore rejected".into()),
                }
                response = p.exchange(&request)?;
            }
            match response {
                Response::Pose(pose,audio,hud) => {
                    if let Some(frame) = hud {
                        self.hud.publish(session, frame);
                    }
                    if audio.len()>16384 || audio.len()%2!=0 || audio.iter().any(|s|!s.is_finite()||s.abs()>1.) {return Err("invalid provider audio".into());}
                    if let Ok(mut a)=self.audio.lock(){
                        if a.0!=session {a.0=session;a.1.clear();}
                        // Interleaved stereo: keep at most 250 ms, dropping whole frames.
                        a.1.extend(audio);let excess=a.1.len().saturating_sub(24000);a.1.drain(..excess);
                    }
                    p.expected = Some((session, history.len() + 1, digest(parent, frame)));
                    Ok((pose, p.rig.clone()))
                }
                Response::Error(e) => Err(e),
                _ => Err("invalid provider response".into()),
            }
        })();
        if result.is_err() {
            *guard = None;
        }
        result
    }
    pub fn advance_world(
        &self,
        world: &std::sync::Arc<collision::CollisionWorld>,
        session: u64,
        start: &Start,
        history: &Journal,
        parent: [u8; 32],
        frame: &Frame,
    ) -> Result<(Pose, Rig), String> {
        self.with_world_scene(world, |scene, key| {
            self.advance(scene, key, session, start, history, parent, frame)
        })
    }
    /// Latest provider HUD for this session. Cosmetic only, like the PCM
    /// stream: never retained in snapshots or replay hashes.
    pub fn hud(&self, session: u64) -> Option<std::sync::Arc<mechanics_link::HudFrame>> {
        self.hud.frame(session)
    }
    /// Decoded texture table that the current HUD frames index.
    pub fn hud_textures(&self) -> std::sync::Arc<Vec<hud::Image>> {
        self.hud.images()
    }
    /// Cosmetic stream only; never retained in snapshots or replay hashes.
    pub fn take_audio(&self,session:u64)->Vec<f32>{
        self.audio.lock().ok().filter(|a|a.0==session).map(|mut a|a.1.drain(..).collect()).unwrap_or_default()
    }
    /// Prepare immutable collision and the trusted provider while the map is
    /// loading. This does not advance or select any player's gameplay state.
    pub fn prepare_world(
        &self,
        world: &std::sync::Arc<collision::CollisionWorld>,
    ) -> Result<(), String> {
        self.with_world_scene(world, |scene, key| {
            let mut guard = self.process.lock().map_err(|_| "provider lock poisoned")?;
            if guard.as_ref().is_none_or(|p| p.scene != key) {
                *guard = Some(self.start(scene, key)?);
            }
            Ok(())
        })
    }
    fn with_world_scene<T>(
        &self,
        world: &std::sync::Arc<collision::CollisionWorld>,
        run: impl FnOnce(&Scene, usize) -> Result<T, String>,
    ) -> Result<T, String> {
        let map = collision::CollisionWorld::map_arc(world);
        let key = std::sync::Arc::as_ptr(&map) as usize;
        let mut guard = self
            .geometry
            .lock()
            .map_err(|_| "provider geometry lock poisoned")?;
        if guard
            .as_ref()
            .is_none_or(|(cached, _)| !std::sync::Arc::ptr_eq(&map, cached))
        {
            *guard = Some((map.clone(), geometry::scene(&map)));
        }
        run(&guard.as_ref().unwrap().1, key)
    }
}
