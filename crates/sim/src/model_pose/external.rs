//! Retarget a provider's solved pose onto the installed CS rig. The reference
//! frames come from both real skins; target limb lengths remain those of CS.
use crate::Player;
use glam::{Mat4,Quat};
pub(super) fn apply(player:&Player,bones:&[mdl::skeleton::Bone],pose:&mut [Mat4])->bool{
    let Some(state)=player.mechanic.as_ref().and_then(|s|s.external.as_ref()) else{return false};
    let rig=&state.rig;let native=&state.pose.bones;
    if native.is_empty()||native.len()!=rig.rest.len(){return false;}
    let conversion=Mat4::from_cols_array_2d(&rig.to_source);
    let (_,basis,_)=conversion.to_scale_rotation_translation();
    let align=Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2);
    let frame=Mat4::from_rotation_translation(Quat::from_rotation_z(player.view_angles.y.to_radians()),player.movement.origin);
    let inverse=frame.inverse();
    let rest=mdl::skeleton::model_space(bones,&mdl::skeleton::rest_pose(bones));
    let mut globals=vec![Mat4::IDENTITY;bones.len()];
    for (i,bone) in bones.iter().enumerate(){
        let name=bone.name.to_ascii_lowercase();
        let suffix=name.rsplit('.').next().unwrap_or(&name);
        let mapped=match suffix {
            "bip01_pelvis"=>Some("HIPS"),"bip01_spine"=>Some("SPINE"),"bip01_spine1"=>Some("SPINE1"),
            "bip01_spine2"=>Some("SPINE2"),"bip01_spine3"=>Some("SPINE3"),"bip01_neck1"=>Some("NECK1"),"bip01_head1"=>Some("HEAD"),
            "bip01_l_clavicle"=>Some("LEFTSHOULDER"),"bip01_l_upperarm"=>Some("LEFTARM"),"bip01_l_forearm"=>Some("LEFTFOREARM"),"bip01_l_hand"=>Some("LEFTHAND"),
            "bip01_r_clavicle"=>Some("RIGHTSHOULDER"),"bip01_r_upperarm"=>Some("RIGHTARM"),"bip01_r_forearm"=>Some("RIGHTFOREARM"),"bip01_r_hand"=>Some("RIGHTHAND"),
            "bip01_l_thigh"=>Some("LEFTUPLEG"),"bip01_l_calf"=>Some("LEFTLEG"),"bip01_l_foot"=>Some("LEFTFOOT"),"bip01_l_toe0"=>Some("LEFTTOEBASE"),
            "bip01_r_thigh"=>Some("RIGHTUPLEG"),"bip01_r_calf"=>Some("RIGHTLEG"),"bip01_r_foot"=>Some("RIGHTFOOT"),"bip01_r_toe0"=>Some("RIGHTTOEBASE"),_=>None
        }.and_then(|name|rig.names.iter().position(|n|n==name));
        let (position,mut rotation)=if bone.parent>=0&&(bone.parent as usize)<i {
            let parent=globals[bone.parent as usize];
            let (_,q,_)=parent.to_scale_rotation_translation();
            (parent.transform_point3(bone.pos),q*bone.quat)
        }else{let global=frame*rest[i];let (_,q,p)=global.to_scale_rotation_translation();(p,q)};
        let mut position=position;
        if let Some(source)=mapped{
            let actual=Mat4::from_cols_array_2d(&native[source]);
            let reference=Mat4::from_cols_array_2d(&rig.rest[source]);
            let (_,a,_)=actual.to_scale_rotation_translation();let (_,r,_)=reference.to_scale_rotation_translation();
            let (_,target,_)=rest[i].to_scale_rotation_translation();
            rotation=(basis*a*r.inverse()*basis.inverse()*align*target).normalize();
            if rig.names[source]=="HIPS"{position=conversion.transform_point3(actual.w_axis.truncate());}
        }
        globals[i]=Mat4::from_rotation_translation(rotation,position);
    }
    for (out,global) in pose.iter_mut().zip(globals){*out=inverse*global;}
    true
}
