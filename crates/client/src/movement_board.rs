//! Render the imported board with its original skin, inverse binds, textures,
//! and the coordinator's completed deck/truck/wheel bones. No baked orientation.
use super::Visuals;
use crate::{coords::to_bevy,player::SimWorld};
use bevy::{prelude::*,asset::RenderAssetUsages,mesh::{Indices,PrimitiveTopology},
    render::render_resource::{Extent3d,TextureDimension,TextureFormat},camera::visibility::NoFrustumCulling};
use std::{path::Path,sync::Arc};
struct Part {
    positions:Vec<[f32;3]>,normals:Vec<[f32;3]>,uv:Vec<[f32;2]>,indices:Vec<u32>,
    weights:Vec<[f32;4]>,joints:Vec<[u16;4]>,inverse:Vec<Mat4>,names:Vec<String>,
    color:[f32;4],texture:Option<(u32,u32,Vec<u8>)>,
}
pub struct Model {parts:Vec<Part>}
impl Model {
    pub fn load(root:&Path)->Result<Self,String>{
        let bytes=std::fs::read(root.join("private/skater.glb")).map_err(|e|e.to_string())?;
        let gltf=gltf::Gltf::from_slice(&bytes).map_err(|e|e.to_string())?;
        let blob=gltf.blob.as_deref().ok_or("board GLB binary missing")?;
        let buffer=|b:gltf::Buffer<'_>|(b.index()==0).then_some(blob);
        let mut parts=vec![];
        for node in gltf.nodes(){
            let (Some(mesh),Some(skin))=(node.mesh(),node.skin())else{continue};
            let names:Vec<_>=skin.joints().map(|n|n.name().unwrap_or("").to_string()).collect();
            let board:Vec<_>=names.iter().map(|n|n=="SKATEBOARD_ROOT"||n.contains("TRUCK")||n.contains("WHEEL")).collect();
            let inverse:Vec<_>=skin.reader(buffer).read_inverse_bind_matrices().ok_or("board inverse binds missing")?
                .map(|m|Mat4::from_cols_array_2d(&m)).collect();
            for primitive in mesh.primitives(){
                let reader=primitive.reader(buffer);
                let positions:Vec<_>=reader.read_positions().ok_or("board positions missing")?.collect();
                let normals:Vec<_>=reader.read_normals().ok_or("board normals missing")?.collect();
                let weights:Vec<_>=reader.read_weights(0).ok_or("board weights missing")?.into_f32().collect();
                let joints:Vec<_>=reader.read_joints(0).ok_or("board joints missing")?.into_u16().collect();
                let uv:Vec<_>=reader.read_tex_coords(0).map(|u|u.into_f32().collect()).unwrap_or(vec![[0.;2];positions.len()]);
                let indices:Vec<_>=reader.read_indices().ok_or("board indices missing")?.into_u32().collect();
                let on_board=|i:usize| (0..4).filter(|j|board.get(joints[i][*j] as usize).copied().unwrap_or(false)).map(|j|weights[i][j]).sum::<f32>()>0.99;
                let indices:Vec<_>=indices.chunks_exact(3).filter(|t|t.iter().all(|i|on_board(*i as usize))).flatten().copied().collect();
                if indices.is_empty(){continue;}
                // Compact the selected board vertices. The GLB also contains a
                // skater; neither its body nor its vertices are rendered here.
                let mut selected=std::collections::BTreeMap::new();
                for i in &indices{let count=selected.len() as u32;selected.entry(*i).or_insert(count);}
                let mut old:Vec<_>=selected.iter().map(|(a,b)|(*b,*a as usize)).collect();old.sort_by_key(|v|v.0);
                let old:Vec<_>=old.into_iter().map(|v|v.1).collect();
                let pbr=primitive.material().pbr_metallic_roughness();
                let texture=if let Some(info)=pbr.base_color_texture(){
                    let source=info.texture().source();
                    let gltf::image::Source::View{view,..}=source.source() else{return Err("board textures must be embedded".into())};
                    let image=image::load_from_memory(blob.get(view.offset()..view.offset()+view.length()).ok_or("board texture range invalid")?).map_err(|e|e.to_string())?.to_rgba8();
                    if image.width()>4096||image.height()>4096{return Err("board texture exceeds budget".into());}
                    Some((image.width(),image.height(),image.into_raw()))
                }else{None};
                parts.push(Part{positions:old.iter().map(|i|positions[*i]).collect(),normals:old.iter().map(|i|normals[*i]).collect(),
                    uv:old.iter().map(|i|uv[*i]).collect(),weights:old.iter().map(|i|weights[*i]).collect(),joints:old.iter().map(|i|joints[*i]).collect(),
                    indices:indices.iter().map(|i|selected[i]).collect(),inverse:inverse.clone(),names:names.clone(),color:pbr.base_color_factor(),texture});
            }
        }
        if parts.is_empty(){return Err("no skinned board found".into());}Ok(Self{parts})
    }
}
#[derive(Component)]
pub struct Instance {player:sim::PlayerId,mode:String,part:usize,model:Arc<Model>,mesh:Handle<Mesh>,generation:u64}
pub fn draw(mut commands:Commands,world:Res<SimWorld>,visuals:Option<Res<Visuals>>,mut meshes:ResMut<Assets<Mesh>>,
    mut materials:ResMut<Assets<StandardMaterial>>,mut images:ResMut<Assets<Image>>,mut instances:Query<(Entity,&mut Instance)>) {
    for (entity,mut instance) in &mut instances {
        let state=world.0.player(instance.player).filter(|p|p.alive).and_then(|p|p.mechanic.as_ref())
            .filter(|s|s.id==instance.mode).and_then(|s|s.external.as_ref());
        let Some(state)=state else{commands.entity(entity).despawn();continue};
        if state.pose.generation==instance.generation{continue;}
        let part=&instance.model.parts[instance.part];
        let conversion=Mat4::from_cols_array_2d(&state.rig.to_source);
        let basis=Mat4::from_cols_array_2d(&state.rig.bone_basis);
        let transforms:Vec<_>=part.names.iter().zip(&part.inverse).map(|(name,inverse)|{
            state.rig.names.iter().position(|n|n==name).and_then(|i|state.pose.bones.get(i))
                .map(|m|conversion*Mat4::from_cols_array_2d(m)*basis* *inverse).unwrap_or(Mat4::IDENTITY)
        }).collect();
        let mut positions=Vec::with_capacity(part.positions.len());let mut normals=Vec::with_capacity(part.normals.len());
        for i in 0..part.positions.len(){let mut p=Vec3::ZERO;let mut n=Vec3::ZERO;
            for j in 0..4{let w=part.weights[i][j];if w==0.{continue;}let m=transforms[part.joints[i][j] as usize];
                p+=m.transform_point3(Vec3::from_array(part.positions[i]))*w;n+=m.transform_vector3(Vec3::from_array(part.normals[i]))*w;}
            positions.push(to_bevy(glam::Vec3::from_array(p.to_array())).to_array());normals.push(to_bevy(glam::Vec3::from_array(n.normalize_or_zero().to_array())).to_array());
        }
        if let Some(mut mesh)=meshes.get_mut(&instance.mesh){mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION,positions);mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL,normals);}
        instance.generation=state.pose.generation;
    }
    let Some(visuals)=visuals else{return};
    for player in world.0.players.iter().filter(|p|p.alive){
        let Some(state)=player.mechanic.as_ref().filter(|s|s.external.is_some())else{continue};
        if instances.iter().any(|(_,i)|i.player==player.id&&i.mode==state.id){continue;}
        let Some(model)=visuals.1.get(&state.id)else{continue};
        for (i,part) in model.parts.iter().enumerate(){
            let mesh=Mesh::new(PrimitiveTopology::TriangleList,RenderAssetUsages::default())
                .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION,vec![[0.;3];part.positions.len()])
                .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL,vec![[0.,1.,0.];part.positions.len()])
                .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0,part.uv.clone()).with_inserted_indices(Indices::U32(part.indices.clone()));
            let texture=part.texture.as_ref().map(|(width,height,data)|images.add(Image::new(Extent3d{width:*width,height:*height,depth_or_array_layers:1},TextureDimension::D2,data.clone(),TextureFormat::Rgba8UnormSrgb,RenderAssetUsages::default())));
            let material=materials.add(StandardMaterial{base_color:Color::srgba(part.color[0],part.color[1],part.color[2],part.color[3]),base_color_texture:texture,
                perceptual_roughness:0.7,cull_mode:None,..Default::default()});
            let mesh=meshes.add(mesh);
            commands.spawn((Mesh3d(mesh.clone()),MeshMaterial3d(material),Transform::default(),NoFrustumCulling,
                Instance{player:player.id,mode:state.id.clone(),part:i,model:model.clone(),mesh,generation:u64::MAX}));
        }
    }
}
