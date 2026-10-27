use super::*;
use bevy::math::{Mat4,Vec3,Vec4};
use mechanics_link::{Scene,SceneState,Start,Frame,Pose,Rig};
use skate_data::skate_map::{SkateMap,Geometry,Material,Collision};
const METERS: f32=0.0254;
pub(super) fn bind_pose(root:&std::path::Path,skater:&physics::SkaterRuntime)->Result<Vec<mechanics_link::Matrix>,String>{
    let bytes=std::fs::read(root.join("private/skater.glb")).map_err(|e|e.to_string())?;
    let gltf=gltf::Gltf::from_slice(&bytes).map_err(|e|e.to_string())?;
    let blob=gltf.blob.as_deref().ok_or("skater GLB binary missing")?;
    let mut rest=skater.render_pose.clone();
    let basis=Mat4::from_cols(Vec4::X,-Vec4::Z,Vec4::Y,Vec4::W);
    for skin in gltf.skins(){
        let reader=skin.reader(|buffer|(buffer.index()==0).then_some(blob));
        let matrices=reader.read_inverse_bind_matrices().ok_or("skater inverse binds missing")?;
        for (node,inverse) in skin.joints().zip(matrices){
            if let Some(i)=skater.animation.evaluator.frames.bone_names.iter().position(|name|Some(name.as_str())==node.name()){
                rest[i]=(Mat4::from_cols_array_2d(&inverse).inverse()*basis.inverse()).to_cols_array_2d();
            }
        }
    }Ok(rest)
}
fn native(p:[f32;3])->[f32;3] { [p[0]*METERS,p[2]*METERS,-p[1]*METERS] }
fn source(p:[f32;3])->[f32;3] { [p[0]/METERS,-p[2]/METERS,p[1]/METERS] }
fn map(scene:&Scene,state:&SceneState,start:&Start)->SkateMap {
    let mut triangles=scene.triangles.clone();
    for (i,door) in scene.doors.iter().enumerate() {
        let angle=state.doors.get(i).copied().unwrap_or(0.).to_radians();
        let (s,c)=angle.sin_cos();
        triangles.extend(door.triangles.iter().cloned().map(|mut t| {
            for p in &mut t.points {
                let x=p[0]-door.origin[0];let y=p[1]-door.origin[1];
                p[0]=door.origin[0]+c*x-s*y;p[1]=door.origin[1]+s*x+c*y;
            } t
        }));
    }
    for object in &scene.breakables {
        if !object.killed_by.iter().any(|i| state.broken.get(*i).copied().unwrap_or(false)) {
            triangles.extend(object.triangles.iter().cloned());
        }
    }
    // The upstream collision compiler welds at 1 mm. Remove triangles that
    // collapse at that exact boundary before asking it to compile the host
    // scene; convex sweep bevels can generate submillimetre slivers. Preserve
    // first-vertex identity, winding and the order of every retained surface.
    let mut vertices=std::collections::HashMap::new();
    let mut positions=Vec::new();let mut seen=std::collections::HashSet::new();
    let collision=triangles.into_iter().filter_map(|t|{
        let ids=t.points.map(native).map(|p|{
            let inverse=1.0/f64::from(0.001_f32);
            let key=p.map(|v|(f64::from(v)*inverse).round() as i64);
            *vertices.entry(key).or_insert_with(||{let i=positions.len();positions.push(p);i})
        });
        let points=ids.map(|i|positions[i]);let [a,b,c]=points.map(Vec3::from_array);
        if (b-a).cross(c-a).length_squared()<1e-12{return None;}
        let min=ids.iter().enumerate().min_by_key(|(_,v)|**v).unwrap().0;
        let key=[ids[min],ids[(min+1)%3],ids[(min+2)%3]];
        if !seen.insert(key){return None;}
        Some(Collision{points,surface:0,material:1,native_edges:None})
    }).collect::<Vec<_>>();
    eprintln!("CS collision compiled: {} welded triangles",collision.len());
    SkateMap {version:15,name:"CS map collision".into(),spawn:native(start.position),
        heading:(start.yaw+90.).to_radians(),environment:vec![],materials:vec![Material {
            name:"host".into(),flags:0,friction:1.,restitution:0.,color:[1.;3],roughness:1.,emissive:0.,
        textures:[0;5],indirect_strength:1.,alpha_mode:0,alpha_cutoff:0.5,audio:0,physics:0,pattern:0,
            depth_layer:None,retail_definition:None,
        }],textures:vec![],geometry:Geometry {vertices:vec![],indices:vec![],collision},
        rails:vec![],doors:vec![],lights:vec![],routes:vec![],extensions:vec![]}
}
impl Runtime {
    pub fn load_scene(root:&std::path::Path,scene:Scene,start:&Start,state:&SceneState)->Result<Self,String> {
        let manifest=skate_data::GameAssets::load(root).map_err(|e|e.to_string())?;
        let graphs=graph_runtime::StockGraphs::load(root,&manifest)?;
        let map=map(&scene,state,start);
        let physics=physics::GamePhysics::load_with_difficulty(root,Some(&map),difficulty::Difficulty::Easy)?;
        let mut skater=physics::SkaterRuntime::load(root,&graphs,&physics,"easy")?;
        // Original teleport owner transfers incoming CS momentum to the whole
        // physical assembly, including skeleton/board; no render correction.
        let transform=Mat4::from_rotation_translation(bevy::math::Quat::from_rotation_y(map.heading),Vec3::from_array(map.spawn));
        skater.travel(transform.to_cols_array_2d(),Some(native(start.velocity)))?;
        let controls=physics::PlayerControls::load(root)?;
        let camera=camera::CameraRuntime::load(root)?;
        let bind_pose=bind_pose(root,&skater)?;
        Ok(Self{physics,skater,controls,graphs,camera,pad:skate_core::input::pad::Pad::new(),accumulator:0.,
            scene:Some(scene),scene_state:state.clone(),bind_pose})
    }
    pub fn rig(&self)->Rig {
        let frames=&self.skater.animation.evaluator.frames;
        let basis=Mat4::from_cols(Vec4::X,Vec4::Z,-Vec4::Y,Vec4::W)*Mat4::from_scale(Vec3::splat(METERS.recip()));
        Rig {names:frames.bone_names.clone(),parents:frames.parents.clone(),rest:self.bind_pose.clone(),
            to_source:basis.to_cols_array_2d(),bone_basis:Mat4::from_cols(Vec4::X,-Vec4::Z,Vec4::Y,Vec4::W).to_cols_array_2d()}
    }
    pub fn advance_host(&mut self,frame:&Frame)->Result<Pose,String> {
        if !frame.dt.is_finite() || frame.dt <= 0. || frame.dt > 0.1 {return Err("invalid host timestep".into());}
        if frame.scene != self.scene_state {
            if let Some(scene)=&self.scene {
                let map=map(scene,&frame.scene,&Start{position:[0.;3],velocity:[0.;3],yaw:0.});
                self.physics.csr_replace_world(&map)?;
            }
            self.scene_state=frame.scene.clone();
        }
        self.accumulator+=f64::from(frame.dt);
        while self.accumulator + 1e-10 >= self.physics.period().as_secs_f64() {
            self.accumulator-=self.physics.period().as_secs_f64();
            let report=&frame.input;
            let state=skate_core::input::xbox::XboxState{buttons:report.buttons,triggers:report.triggers,left:report.left,right:report.right};
            let values=skate_core::input::xbox::convert(&state,0);
            self.pad.update(if report.connected {&values} else {&[]});
            let actions=skate_core::input::gameplay_map::GameplayActions::from_pad(&self.pad);
            self.step(*actions.values(),report.connected)?;
        }
        Ok(self.pose())
    }
    pub fn pose(&self)->Pose {
        let deck=&self.physics.board.bodies()[skate_core::physics::board::BodyId::Deck.index()];
        let mut pos=deck.rates.position;let mut vel=deck.rates.linear_velocity;
        if self.skater.player_state.current().category()==500 {
            if let Some(hips)=self.skater.animation.evaluator.frames.bone_names.iter().position(|n|n=="HIPS"){
                let p=self.skater.render_pose[hips][3];let rest=self.bind_pose[hips][3];
                pos=skate_core::math::Vector3::new(p[0],p[1]-rest[1],p[2]);
                vel=self.skater.skeleton.bodies()[0].rates.linear_velocity;
            }
        }
        let camera=self.camera.frame.map(|f| {
            let mut m=Mat4::from_cols_array_2d(&[[f.basis.columns[0][0],f.basis.columns[0][1],f.basis.columns[0][2],0.],
                [f.basis.columns[1][0],f.basis.columns[1][1],f.basis.columns[1][2],0.],
                [f.basis.columns[2][0],f.basis.columns[2][1],f.basis.columns[2][2],0.],f.position]);
            m.w_axis.w=1.;
            // Native camera's +Z is its at vector; consumer selects the same
            // presentation sign convention as the standalone camera renderer.
            let to_source=Mat4::from_cols_array_2d(&self.rig().to_source);
            (to_source*m).to_cols_array_2d()
        });
        Pose {generation:self.skater.pose_generation,position:source([pos.x,pos.y,pos.z]),velocity:source([vel.x,vel.y,vel.z]),
            grounded:matches!(self.skater.player_state.current().category(),100|400|500),
            bones:self.skater.render_pose.clone(),state:self.skater.player_state.current() as u32,camera,
            fov:self.camera.frame.map_or(60.,|f|f.field_of_view_degrees)}
    }
}
