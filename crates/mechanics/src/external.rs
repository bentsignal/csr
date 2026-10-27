//! Trusted application provider, isolated from pack code and the CS executable.
//! Its process is a derived cache: restoring any journal rebuilds the exact
//! coordinator from the original spawn, controller inputs and scene poses.
use mechanics_link::{Request,Response,Rig,Scene,Start,Frame,Pose};
use std::{path::PathBuf,sync::{Mutex,mpsc},process::{Child,ChildStdin,Command,Stdio},time::Duration};
use sha2::{Digest,Sha256};
pub mod geometry;
#[derive(Clone,Debug,Default,PartialEq,serde::Serialize,serde::Deserialize)]
pub struct Journal {chunks:Vec<std::sync::Arc<[Frame]>>,tail:Vec<Frame>,len:usize}
impl Journal {
    pub fn len(&self)->usize{self.len}
    pub fn push(&mut self,frame:Frame){self.tail.push(frame);self.len+=1;if self.tail.len()==128{self.chunks.push(std::mem::take(&mut self.tail).into());}}
    pub fn to_vec(&self)->Vec<Frame>{self.chunks.iter().flat_map(|chunk|chunk.iter()).chain(self.tail.iter()).cloned().collect()}
}
pub fn digest(parent:[u8;32],value:&impl serde::Serialize)->[u8;32]{
    let mut hash=Sha256::new();hash.update(parent);hash.update(serde_json::to_vec(value).expect("finite mechanics journal"));hash.finalize().into()
}

pub struct Program { executable:PathBuf, assets:PathBuf, process:Mutex<Option<Process>>,geometry:Mutex<Option<(std::sync::Arc<collision::CollisionWorld>,Scene)>> }
struct Process { child:Child,input:ChildStdin,responses:mpsc::Receiver<Result<Response,String>>,scene:usize,rig:Rig,expected:Option<(u64,usize,[u8;32])> }
impl Drop for Process {fn drop(&mut self){let _=self.child.kill();let _=self.child.wait();}}
impl Process {
    fn exchange(&mut self,request:&Request)->Result<Response,String>{
        mechanics_link::send(&mut self.input,request)?;
        self.responses.recv_timeout(Duration::from_secs(90)).map_err(|_|"mechanics provider stopped responding")?
    }
}
impl Program {
    /// The caller chooses an application-shipped executable, never a pack path.
    pub fn new(executable:PathBuf,assets:PathBuf)->Result<Self,String>{
        if !executable.is_file() || !assets.join("private/game.json").is_file() {return Err("movement runtime or private assets missing".into());}
        Ok(Self{executable,assets,process:Mutex::new(None),geometry:Mutex::new(None)})
    }
    fn start(&self,scene:&Scene,key:usize)->Result<Process,String>{
        eprintln!("Movement provider scene: {} triangles, {} doors",scene.triangles.len(),scene.doors.len());
        let mut command=Command::new(&self.executable);
        command.arg("--stdio-provider-v1").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit());
        #[cfg(windows)] {use std::os::windows::process::CommandExt;command.creation_flags(0x08000000);}
        let mut child=command.spawn().map_err(|e|e.to_string())?;
        let input=child.stdin.take().ok_or("provider stdin missing")?;
        let mut output=child.stdout.take().ok_or("provider stdout missing")?;
        let (sender,responses)=mpsc::sync_channel(1);
        std::thread::spawn(move||loop{
            let response=mechanics_link::receive(&mut output);let failed=response.is_err();
            if sender.send(response).is_err()||failed {break;}
        });
        let mut p=Process{child,input,responses,scene:key,rig:Rig{names:vec![],parents:vec![],rest:vec![],to_source:[[0.;4];4],bone_basis:[[0.;4];4]},expected:None};
        match p.exchange(&Request::Initialize{version:mechanics_link::VERSION,assets:self.assets.to_string_lossy().into_owned(),scene:scene.clone()})? {
            Response::Ready(rig)=>p.rig=rig,Response::Error(e)=>return Err(e),_=>return Err("provider initialization rejected".into()),
        } Ok(p)
    }
    pub fn advance(&self,scene:&Scene,key:usize,session:u64,start:&Start,history:&Journal,parent:[u8;32],frame:&Frame)->Result<(Pose,Rig),String>{
        let mut guard=self.process.lock().map_err(|_|"provider lock poisoned")?;
        if guard.as_ref().is_none_or(|p|p.scene!=key) {*guard=Some(self.start(scene,key)?);}
        let p=guard.as_mut().unwrap();
        let request=Request::Advance{session,at:history.len() as u64,frame:frame.clone()};
        let result=(||{
            let mut response=if p.expected==Some((session,history.len(),parent)){p.exchange(&request)?}else{Response::NeedRestore};
            if matches!(response,Response::NeedRestore){
                match p.exchange(&Request::Restore{session,start:start.clone(),frames:history.to_vec()})? {
                    Response::Pose(_)=>{},Response::Error(e)=>return Err(e),_=>return Err("provider restore rejected".into())
                }
                response=p.exchange(&request)?;
            }
            match response {Response::Pose(pose)=>{p.expected=Some((session,history.len()+1,digest(parent,frame)));Ok((pose,p.rig.clone()))},Response::Error(e)=>Err(e),_=>Err("invalid provider response".into())}
        })();
        if result.is_err(){*guard=None;}
        result
    }
    pub fn advance_world(&self,world:&std::sync::Arc<collision::CollisionWorld>,session:u64,start:&Start,history:&Journal,parent:[u8;32],frame:&Frame)->Result<(Pose,Rig),String>{
        let map=collision::CollisionWorld::map_arc(world);
        let key=std::sync::Arc::as_ptr(&map) as usize;
        let mut guard=self.geometry.lock().map_err(|_|"provider geometry lock poisoned")?;
        if guard.as_ref().is_none_or(|(cached,_)|!std::sync::Arc::ptr_eq(&map,cached)){
            *guard=Some((map.clone(),geometry::scene(&map)));
        }
        self.advance(&guard.as_ref().unwrap().1,key,session,start,history,parent,frame)
    }
}
