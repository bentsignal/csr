// vmprobe2 <model> <activity> [--sight-rois]: installed animation geometry.
// The optional bounded ROI report is specific to the observed M4A4 idle,
// FOV60/offset1,1,-1 at1600x900; it does not establish visible-vertex admission.
fn main() {
    let (_, fs) = level::mount_game(None).unwrap();
    let args: Vec<String> = std::env::args().collect();
    let path = &args[1];
    let act = &args[2];
    let base = path.trim_end_matches(".mdl");
    let m = mdl::Model::load(&fs.read(path).unwrap(), &fs.read(&format!("{base}.vvd")).unwrap(), &fs.read(&format!("{base}.dx90.vtx")).unwrap()).unwrap();
    let ani = fs.read(&m.anim_block_file);
    let mut worst = (0.0f32, String::new());
    for b in &m.bones {
        let e = mdl::skeleton::euler_to_quat(b.rot);
        let d = b.quat.dot(e).abs();
        let ang = 2.0 * d.min(1.0).acos().to_degrees();
        if ang > worst.0 { worst = (ang, b.name.clone()); }
    }
    println!("max quat-vs-euler rest mismatch: {:.2} deg at {}", worst.0, worst.1);
    let sight_rois = args.iter().any(|s| s == "--sight-rois");
    if sight_rois { assert!(path == "models/weapons/v_rif_m4a1.mdl" && act == "ACT_VM_IDLE",
        "sight ROI framing was observed only for M4A4 idle"); }
    let mut reported_rois = false;
    for seq in m.sequences.iter().filter(|s| &s.activity == act) {
        let desc = &m.animations[seq.anims[0] as usize];
        let clip = m.decode_animation(seq.anims[0] as usize, ani.as_deref()).unwrap();
        let parents: Vec<String> = m.bones.iter().enumerate().filter(|(_, b)| b.name.to_lowercase().contains("parent")).map(|(i, b)| format!("{}:{}", b.name, clip.animated[i])).collect();
        println!("  parent bones animated: {parents:?}");
        println!("{} anim {} frames {} sectionframes {} sections {:?} animated {}", seq.name, desc.name, desc.num_frames, desc.section_frames, desc.sections, clip.animated.iter().filter(|a| **a).count());
        let flash = std::env::var("BONE").ok().and_then(|n| m.bone_index(&n)).unwrap_or_else(|| m.bones.iter().position(|b| b.name.to_lowercase().contains("flash") || b.name.to_lowercase().contains("muzzle")).unwrap_or(0));
        println!("  tracking bone {} animated in this clip: {}", m.bones[flash].name, clip.animated[flash]);
        let hand = m.bone_index("v_weapon.Bip01_R_Hand").unwrap_or(0);
        for k in 0..=5 {
            let t = clip.duration() * k as f32 / 5.0;
            let mut local = mdl::skeleton::rest_pose(&m.bones);
            clip.sample(t, &mut local);
            let pose = mdl::skeleton::model_space(&m.bones, &local);
            if sight_rois && !reported_rois {
                report_sight_rois(&m, &pose);
                reported_rois = true;
            }
            println!("  t {:.2}: muzzle {:?} hand {:?}", t, pose[flash].transform_point3(glam::Vec3::ZERO), pose[hand].transform_point3(glam::Vec3::ZERO));
        }
    }
}
#[allow(dead_code)]
fn unused() {}

/// Print runtime-only vertex candidates; ROI overlap alone does not prove
/// visibility, anatomical identity, or native animation correspondence.
fn report_sight_rois(model: &mdl::Model, pose: &[glam::Mat4]) {
    let focal = 600. / 30_f32.to_radians().tan();
    let offset = glam::Vec3::new(1., -1., -1.);
    println!("sight ROI candidates:1600x900 fov60 offsets1,1,-1; model={} checksum={}; raw installed geometry, not visibility admission",
        model.name, model.checksum);
    for (label, x0, y0, x1, y1) in [
        ("front-post", 977., 498., 1008., 548.),
        ("rear-aperture", 1193., 558., 1221., 585.),
        ("receiver-carry-handle", 1285., 682., 1358., 735.),
        ("receiver-side", 1340., 760., 1450., 840.),
    ] {
        let mut vertices = Vec::new();
        for (mi, mesh) in model.meshes.iter().enumerate() {
            for (vi, vertex) in mesh.vertices.iter().enumerate() {
                let point = mdl::skeleton::skin_vertex(vertex, pose, &model.bones) + offset;
                if point.x <= 0. { continue; }
                let pixel = glam::Vec2::new(800. - focal * point.y / point.x,
                    450. - focal * point.z / point.x);
                if pixel.x >= x0 && pixel.x <= x1 && pixel.y >= y0 && pixel.y <= y1 {
                    vertices.push((mi, vi, point, pixel));
                }
            }
        }
        let raw_count = vertices.len();
        vertices.sort_by(|a, b| a.2.x.total_cmp(&b.2.x)
            .then(a.2.y.total_cmp(&b.2.y)).then(a.2.z.total_cmp(&b.2.z)));
        vertices.dedup_by(|a, b| a.2 == b.2);
        println!("ROI {label}: {raw_count} raw, {} unique candidates; bounded first128 by depth (offset already included)", vertices.len());
        for (mesh, vertex, point, pixel) in vertices.iter().take(128) {
            println!("  mesh={mesh} vertex={vertex} eye_model={point:?} pixel={pixel:?}");
        }
    }
}
