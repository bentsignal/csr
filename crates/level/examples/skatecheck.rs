//! Focused private-pack proof. No original assets or private pack fixture in CI.
use glam::Vec3;
use sim::{Team, TickInput};
use std::{path::Path, sync::Arc};

fn load_module(pack: &packs::Pack) -> Result<Arc<sim::mechanics::Module>, String> {
    let def = pack
        .manifest()
        .mechanics
        .first()
        .ok_or("pack has no mechanics")?;
    Ok(Arc::new(sim::mechanics::Module {
        id: def.id.clone(),
        name: def.name.clone(),
        archive_hash: pack.archive_hash().into(),
        program: if def.api_version == 1 {
            Some(mechanics::Program::compile(
                &pack.file(&def.module).ok_or("missing module")?,
            )?)
        } else {
            None
        },
        config: mechanics::parse_config(&pack.file(&def.config).ok_or("missing configuration")?)?,
        external: if def.api_version == 2 {
            Some(Arc::new(mechanics::external::Program::new(
                std::env::var_os("CSR_SKATE_RUNTIME")
                    .map(std::path::PathBuf::from)
                    .ok_or("CSR_SKATE_RUNTIME missing")?,
                mechanics::data::materialize(
                    &pack.file(&def.module).ok_or("missing runtime data")?,
                    &std::env::temp_dir().join("csr-skatecheck-cache"),
                )?,
            )?))
        } else {
            None
        },
    }))
}

fn run() -> Result<(), String> {
    let path = std::env::args()
        .nth(1)
        .ok_or("Usage: skatecheck <private pack.csrpack> [map]")?;
    let pack = packs::Pack::open(Path::new(&path))?;
    let module = load_module(&pack)?;
    if std::env::args().any(|a|a=="--ramps"){return rampcheck(module);}
    if std::env::args().any(|a|a=="--route"){return coursecheck(module);}
    if std::env::args().any(|a| a == "--audio") {
        return audioproof(module);
    }
    let full = pack.manifest().mechanics[0].api_version == 2;
    if full {
        momentumcheck(&module)?;
        if pack.manifest().version.as_str() >= "0.5.0" {
            audiocheck(module.clone())?;
        }
        grindcheck(module.clone())?;
    }
    let mut collision = collision::CollisionWorld::new();
    let floor = [
        Vec3::new(-5000., -5000., 0.),
        Vec3::new(5000., -5000., 0.),
        Vec3::new(5000., 5000., 0.),
        Vec3::new(-5000., 5000., 0.),
    ];
    for tri in [[0, 1, 2], [0, 2, 3]] {
        collision.add_triangle(tri.map(|i| floor[i]), collision::contents::SOLID, 0);
    }
    collision.finalize();
    let collision = Arc::new(collision);
    if full {
        combatcheck(module.clone(),collision.clone())?;
        markercheck(module.clone(),collision.clone())?;
        offboardcheck(module.clone(), collision.clone())?;
        flipcheck(module.clone(), collision.clone())?;
        dynamiccheck(&module)?;
    }
    for rate in [64, 128] {
        let mut world = sim::World::new(collision.clone());
        world.set_tick_rate(rate);
        world.mechanics.push(module.clone());
        let id = world.add_player("Skate check", Team::CounterTerrorist);
        world.player_mut(id).unwrap().movement.origin = Vec3::Z * 0.1;
        for _ in 0..4 {
            world.step(&[(id, TickInput::default())]);
        }
        let before = world.player(id).unwrap().weapons.len();
        world.step(&[(
            id,
            TickInput {
                toggle_mode: true,
                ..Default::default()
            },
        )]);
        if world.player(id).unwrap().mechanic.is_none() {
            return Err("module activation failed".into());
        }
        let mut peak = 0f32;
        for tick in 0..rate * 3 {
            let input = TickInput {
                controller: if full {
                    controller(tick as f32 / rate as f32)
                } else {
                    Default::default()
                },
                forward_move: 450.,
                buttons: if tick == rate {
                    movement::buttons::JUMP
                } else {
                    0
                },
                ..Default::default()
            };
            world.step(&[(id, input)]);
            peak = peak.max(world.player(id).unwrap().movement.origin.z);
        }
        let p = world.player(id).unwrap();
        println!(
            "{rate} Hz: position={:?} speed={:.2} peak={peak:.2} mode={:?}",
            p.movement.origin,
            p.movement.speed_2d(),
            p.mechanic.as_ref().map(|s| &s.id)
        );
        if p.mechanic.is_none() || p.movement.origin.x < 30. || peak < 8. {
            return Err("riding/ollie did not advance".into());
        }
        let state = world.state();
        let mut replay = world.clone();
        replay.restore(state);
        let mut future = Vec::new();
        for tick in 0..rate {
            let input = TickInput {
                controller: if full {
                    controller(3. + tick as f32 / rate as f32)
                } else {
                    Default::default()
                },
                forward_move: 450.,
                side_move: if tick < rate / 2 { 450. } else { 0. },
                ..Default::default()
            };
            world.step(&[(id, input)]);
            future.push((input, world.state_hash()));
        }
        for (input, hash) in future {
            replay.step(&[(id, input)]);
            if hash != replay.state_hash() {
                return Err("movement diverged after snapshot restore".into());
            }
        }
        world.step(&[(
            id,
            TickInput {
                toggle_mode: true,
                ..Default::default()
            },
        )]);
        if world.player(id).unwrap().mechanic.is_some()
            || world.player(id).unwrap().weapons.len() != before
        {
            return Err("mode switching changed inventory or failed".into());
        }
    }
    let map = std::env::args().nth(2).unwrap_or("de_dust2".into());
    let level = level::Level::load(&map, None).map_err(|e| e.to_string())?;
    let mut world = sim::World::with_catalog(level.collision.clone(), level.catalog.clone());
    world.set_character_profiles(level.character_profiles.clone());
    world.load_map(&level.bsp, level.overview().as_deref());
    if let Some(provider)=&module.external {
        let started=std::time::Instant::now();provider.prepare_world(&world.collision)?;
        println!("Map provider preparation: {:?}",started.elapsed());
    }
    world.mechanics.push(module);
    let id = world.add_player("Map skate check", Team::CounterTerrorist);
    world.step(&[(id, TickInput {
        weapon_slot: sim::SLOT_PREVIOUS,
        ..Default::default()
    })]);
    if world.player(id).unwrap().equipped_mechanic.is_none() {
        return Err("map inventory did not select the board".into());
    }
    let activation=std::time::Instant::now();
    world.step(&[(
        id,
        TickInput {
            buttons: movement::buttons::ATTACK,
            controller: mechanics::link::Controller {
                connected: true,
                triggers: [0, 255],
                ..Default::default()
            },
            ..Default::default()
        },
    )]);
    println!("Prepared first activation: {:?}",activation.elapsed());
    let started = std::time::Instant::now();
    let mut foot_error = Vec3::ZERO;
    let mut max_foot_error = 0f32;
    println!(
        "Map entry: origin={:?} yaw={}",
        world.player(id).unwrap().movement.origin,
        world.player(id).unwrap().view_angles.y
    );
    for tick in 0..512 {
        let input = TickInput {
            controller: if full {
                controller(tick as f32 / 128.)
            } else {
                Default::default()
            },
            view_angles: Vec3::ZERO,
            forward_move: 450.,
            side_move: if (128..256).contains(&tick) { 450. } else { 0. },
            buttons: if tick == 300 {
                movement::buttons::JUMP
            } else {
                0
            },
            ..Default::default()
        };
        world.step(&[(id, input)]);
        if tick == 128 {
            let p = world.player(id).unwrap();
            if let (Some(state), Some(profile)) = (
                p.mechanic.as_ref().and_then(|s| s.external.as_ref()),
                world.character_profiles().for_team(p.team),
            ) {
                let bones = profile.animations.bones();
                let mut pose = mdl::skeleton::model_space(bones, &mdl::skeleton::rest_pose(bones));
                sim::model_pose::apply_movement_pose(p, bones, &mut pose);
                let frame = glam::Mat4::from_rotation_translation(
                    glam::Quat::from_rotation_z(p.view_angles.y.to_radians()),
                    p.movement.origin,
                );
                let conversion = glam::Mat4::from_cols_array_2d(&state.rig.to_source);
                for (cs, skate) in [("ankle_L", "LEFTFOOT"), ("ankle_R", "RIGHTFOOT")] {
                    if let (Some(a), Some(b)) = (
                        bones.iter().position(|b| b.name == cs),
                        state.rig.names.iter().position(|n| n == skate),
                    ) {
                        let target = conversion.transform_point3(
                            glam::Mat4::from_cols_array_2d(&state.pose.bones[b])
                                .w_axis
                                .truncate(),
                        );
                        let actual = frame.transform_point3(pose[a].w_axis.truncate());
                        foot_error += (actual - target) * 0.5;
                        max_foot_error = max_foot_error.max(actual.distance(target));
                    }
                }
            }
        }
    }
    let player = world.player(id).unwrap();
    if let Some(state) = player.mechanic.as_ref().and_then(|s| s.external.as_ref()) {
        println!(
            "native state={} position={:?}",
            state.pose.state, state.pose.position
        );
        if let Some(profile) = world.character_profiles().for_team(player.team) {
            let bones = profile.animations.bones();
            let mut pose = mdl::skeleton::model_space(bones, &mdl::skeleton::rest_pose(bones));
            sim::model_pose::apply_movement_pose(player, bones, &mut pose);
            if pose.iter().any(|m| !m.is_finite()) {
                return Err("retarget produced a nonfinite CS pose".into());
            }
        }
    }
    if player.mechanic.is_none() || !player.movement.origin.is_finite() {
        return Err("real-map movement module failed".into());
    }
    println!(
        "{map}: 512 ticks in {:?}, origin={:?}, archive={}",
        started.elapsed(),
        player.movement.origin,
        pack.archive_hash()
    );
    println!("Retarget average ankle offset: {foot_error:?}");
    if max_foot_error > 0.1 {
        return Err(format!(
            "CS feet miss the native board pose by {max_foot_error:.2} inches"
        ));
    }
    world.step(&[(id,TickInput{toggle_mode:true,..Default::default()})]);
    let actor=world.player(id).unwrap();
    let (mins,maxs)=actor.movement.hull();
    let hit=world.collision.trace_box(actor.movement.origin,actor.movement.origin,mins,maxs,collision::contents::MASK_PLAYERSOLID);
    println!("Real-map CS return: mode={}, position={:?}, start_solid={}, all_solid={}",actor.mechanic.is_some(),actor.movement.origin,hit.start_solid,hit.all_solid);
    if actor.mechanic.is_some()||hit.start_solid||hit.all_solid{return Err("real-map CS exit did not produce a clear player hull".into());}
    println!("failures: 0");
    Ok(())
}
fn controller(t: f32) -> mechanics::link::Controller {
    let mut report = mechanics::link::Controller {
        connected: true,
        ..Default::default()
    };
    if t < 1.5 {
        report.buttons = 0x1000;
    }
    if (1.5..2.).contains(&t) {
        report.right = [0, -32768];
    }
    if (2.0..2.2).contains(&t) {
        report.right = [0, 32767];
    }
    if (3.0..3.5).contains(&t) {
        report.left = [18000, 0];
    }
    report
}
fn momentumcheck(module:&sim::mechanics::Module)->Result<(),String>{
    use mechanics::link::{Scene,Start,Frame,Controller};
    let program=module.external.as_ref().unwrap();
    let scene=Scene::default();
    for (case,vertical) in [450.,-700.].into_iter().enumerate(){
        let start=Start{grounded:false,position:[0.,0.,1200.],velocity:[1400.,240.,vertical],yaw:0.,checkpoint:None};
        let mut journal=mechanics::external::Journal::default();let mut hash=mechanics::external::digest([0;32],&start);
        for tick in 0..32 {
            let frame=Frame{input:Controller{connected:true,..Default::default()},dt:1./128.,scene:Default::default()};
            let (pose,_)=program.advance(&scene,900+case,900+case as u64,&start,&journal,hash,&frame)?;
            if pose.velocity[0]<1250. || pose.velocity[1]<180. || pose.velocity[2].signum()!=vertical.signum(){return Err(format!("air transfer lost momentum at {tick}: {:?}",pose.velocity));}
            if tick==31{println!("Air transfer {case}: position={:?} velocity={:?}",pose.position,pose.velocity);}
            hash=mechanics::external::digest(hash,&frame);journal.push(frame);
        }
    }
    Ok(())
}
fn combatcheck(module:Arc<sim::mechanics::Module>,collision:Arc<collision::CollisionWorld>)->Result<(),String>{
    let catalog=course_level()?.catalog;
    let mut w=sim::World::with_catalog(collision,catalog);w.mechanics.push(module);let id=w.add_player("Riding combat",Team::CounterTerrorist);
    w.player_mut(id).unwrap().movement.origin=Vec3::Z*0.1;
    let mut shots=0;let mut selected=std::collections::BTreeSet::new();
    for tick in 0..512 {
        let slot=if tick==128||tick==256{sim::SLOT_NEXT}else{0};
        w.step(&[(id,TickInput{toggle_mode:tick==0,weapon_slot:slot,buttons:if tick%32==0 && tick>64{movement::buttons::ATTACK}else{0},controller:mechanics::link::Controller{connected:true,buttons:if slot!=0{1}else if tick%32==0 && tick>64{8}else{0},..Default::default()},..Default::default()})]);
        let p=w.player(id).unwrap();if p.mechanic.is_none()||p.equipped_mechanic.is_some(){return Err("riding weapon cycle selected a board tool or left Skate".into());}
        if let Some(gun)=p.active_weapon(){selected.insert(gun.def.class_name.clone());}
        shots+=w.events.iter().filter(|e|matches!(e,sim::GameEvent::Fire{player,..} if *player==id)).count();
    }
    println!("Riding combat: shots={shots} weapons={selected:?}");
    if shots==0||selected.len()<2{return Err("riding fire / weapon switching failed".into());}Ok(())
}
const STEER:(f32,f32)=(60.,40.);
fn rampcheck(module:Arc<sim::mechanics::Module>)->Result<(),String>{
    use mechanics::link::{Start,Frame,Controller};
    use pack_authoring::surf_course::*;
    let level=course_level()?;
    let scene=mechanics::external::geometry::scene(&level.collision);
    let program=module.external.as_ref().unwrap();
    let cases: [(&str,[f32;3],f32,f32);14]=[("funbox",[-3200.,360.,DECK_Z+3.5],0.,-2700.),
        ("rail launch",[-1800.,270.,DECK_Z+3.5],0.,-1400.),
        ("launch box",[0.,360.,DECK_Z+3.5],0.,500.),
        ("small gap",[1100.,360.,DECK_Z+3.5],0.,1500.),
        ("stair approach",[-260.,-1100.,DECK_Z+3.5],0.,250.),
        ("bank",[-1400.,700.,DECK_Z+3.5],90.,1250.),
        ("quarter pipe",[1200.,700.,DECK_Z+3.5],90.,1150.),
        ("big gap",[GAP_LAUNCH[0]-200.,0.,DECK_Z+3.5],0.,GAP_LAUNCH[1]+100.),
        ("exit kicker",[KICKER_X[0]-200.,LANE_Y,DECK_Z+3.5],0.,KICKER_X[1]+100.),
        ("funbox return",[-1900.,360.,DECK_Z+3.5],180.,-2400.),
        ("rail return",[-440.,270.,DECK_Z+3.5],180.,-850.),
        ("launch box return",[1140.,360.,DECK_Z+3.5],180.,650.),
        ("small gap return",[2160.,360.,DECK_Z+3.5],180.,1720.),
        ("big gap return",[GAP_LANDING[1]+200.,0.,DECK_Z+3.5],180.,GAP_LANDING[0]-100.)];
    for (case,(name,position,yaw,target)) in cases.into_iter().enumerate(){
        for (rate_index,speed) in [240.,500.,850.].into_iter().enumerate(){
            let heading=yaw.to_radians();
            let start=Start{grounded:true,position,velocity:[speed*heading.cos(),speed*heading.sin(),0.],yaw,checkpoint:None};
            let mut journal=mechanics::external::Journal::default();let mut hash=mechanics::external::digest([0;32],&start);
            let session=1000+(case*10+rate_index) as u64;
            let mut reached=false;let mut last=Default::default();
            for tick in 0..2048 {
                let frame=Frame{input:Controller{connected:true,buttons:if speed<300. && tick%128<32{0x1000}else{0},..Default::default()},dt:1./128.,scene:Default::default()};
                let (pose,_)=program.advance(&scene,501,session,&start,&journal,hash,&frame)?;
                let along=pose.position[if yaw==90.{1}else{0}];
                if pose.state==300 || (500..600).contains(&pose.state) {return Err(format!("{name} speed={speed} bailed tick={tick} state={} at {:?}",pose.state,pose.position));}
                hash=mechanics::external::digest(hash,&frame);journal.push(frame);
                last=pose;
                if (yaw==180. && along<target)||(yaw!=180. && along>target) {reached=true;break;}
            }
            println!("Ramp {name} speed={speed}: reached={reached} state={} position={:?} velocity={:?}",last.state,last.position,last.velocity);
            if !reached {return Err(format!("{name} speed={speed} failed to ride through the transition"));}
        }
    }
    let start=Start{grounded:true,position:[-8100.,-128.,START_Z+3.5],velocity:[350.,0.,0.],yaw:0.,checkpoint:None};
    let mut journal=mechanics::external::Journal::default();let mut hash=mechanics::external::digest([0;32],&start);
    let mut air_ticks=0;
    let mut dropped=false;
    for tick in 0..1024 {
        let mut input=Controller{connected:true,..Default::default()};
        if (100..150).contains(&tick){input.right=[0,-32768];}
        if (150..175).contains(&tick){input.right=[30000,30000];}
        let frame=Frame{input,dt:1./128.,scene:Default::default()};
        let (pose,_)=program.advance(&scene,501,2000,&start,&journal,hash,&frame)?;
        if !pose.grounded {air_ticks+=1;}
        if pose.state==300 {return Err("starting drop bailed before the surf handoff".into());}
        hash=mechanics::external::digest(hash,&frame);journal.push(frame);
        if pose.position[2]<1800. {
            println!("Starting drop: air_time={:.2}s position={:?} velocity={:?} trick={:?}",air_ticks as f32/128.,pose.position,pose.velocity,pose.trick);
            dropped=true;break;
        }
    }
    if !dropped||air_ticks<256 {return Err("starting drop did not allow two seconds of airborne skating".into());}
    Ok(())
}
fn coursecheck(module:Arc<sim::mechanics::Module>)->Result<(),String>{
    let level=course_level()?;
    let mut w=sim::World::with_catalog(level.collision.clone(),level.catalog.clone());
    w.load_map(&level.bsp,None);w.config.air_accelerate=1000.;w.config.accelerate=10.;w.mechanics.push(module);
    let id=w.add_player("Clip route",Team::CounterTerrorist);
    let mut stage=0;let mut landed_at=None;let mut max_speed=0f32;let mut last_state=0;let mut switched=0;
    let mut trick=String::new();let mut score=0;
    for tick in 0..12288 {
        let p=w.player(id).unwrap();let pos=p.movement.origin;let vel=p.movement.velocity;
        if let Some(s)=p.mechanic.as_ref().and_then(|s|s.external.as_ref()){
            if s.pose.state==300 || (500..600).contains(&s.pose.state) {return Err(format!("course rider bailed at {pos:?} state={}",s.pose.state));}
            if !s.pose.trick.is_empty(){trick=s.pose.trick.clone();score=score.max(s.pose.score);}
        }
        let mut input=TickInput{view_angles:Vec3::ZERO,..Default::default()};
        if stage==0 {
            use pack_authoring::surf_course::RAMP1_X;
            if pos.x<RAMP1_X[0] {input.forward_move=450.;}
            else if pos.z>pack_authoring::surf_course::ramp_peak(false,pos.x)+150. {
                input.forward_move=450.;input.side_move=((pos.y+100.)*5.+vel.y*2.).clamp(-450.,450.);
            }else{input.side_move=-450.;}
            if pos.x>RAMP1_X[1]+50. {input.toggle_mode=true;stage=1;}
        }else if stage==1 {
            input.controller.connected=true;
            // Hold the clear lane: steer back toward its centre line.
            let steer=((pos.y-pack_authoring::surf_course::LANE_Y)*STEER.0+vel.y*STEER.1).clamp(-24000.,24000.);
            if p.movement.on_ground {input.controller.left[0]=steer as i16;}
            if p.movement.on_ground && pos.x>pack_authoring::surf_course::DECK_X[0] && landed_at.is_none(){landed_at=Some(tick);println!("Trick deck landing tick={tick} position={pos:?} velocity={vel:?}");}
            if let Some(at)=landed_at {
                if (at+40..at+100).contains(&tick){input.controller.right=[0,-32768];}
                if (at+100..at+126).contains(&tick){input.controller.right=[30000,30000];}
            }
            if landed_at.is_some_and(|at|tick>at+200) && p.movement.on_ground && pos.x<pack_authoring::surf_course::KICKER_X[0]-600. && (tick/16)%4==0 {input.controller.buttons|=0x1000;}
            if pos.x>pack_authoring::surf_course::KICKER_X[1]+15. {input.toggle_mode=true;stage=2;}
        }else {
            let face=pack_authoring::surf_course::ramp_peak(true,pos.x)-1.6*(pos.y-pack_authoring::surf_course::RAMP2_Y).abs();
            input.side_move=if pos.z>face+100. {((pos.y+100.)*5.+vel.y*2.).clamp(-450.,450.)}else{-450.};
            if pos.x>pack_authoring::surf_course::FINISH_X[0]+100. && p.movement.on_ground {println!("Final runout reached tick={tick} position={pos:?} velocity={vel:?} peak={max_speed} trick={trick} score={score}");return if landed_at.is_some() && !trick.is_empty(){Ok(())}else{Err("missed trick deck or trick recognition".into())};}
        }
        max_speed=max_speed.max(vel.truncate().length());
        if stage!=last_state{switched=tick;}
        if tick%128==0||stage!=last_state||tick<switched+8{println!("Route tick={tick} stage={stage} position={pos:?} velocity={vel:?}");last_state=stage;}
        w.step(&[(id,input)]);
        if input.toggle_mode {
            let after=w.player(id).unwrap().movement.velocity;
            println!("Mode transfer stage={stage}: before={vel:?} after={after:?}");
            if after.truncate().length()<vel.truncate().length()*0.95 || (after.z-vel.z).abs()>20. {return Err("course handoff discarded airborne velocity".into());}
        }
    }
    Err(format!("route incomplete at {:?}",w.player(id).unwrap().movement.origin))
}
fn markercheck(module:Arc<sim::mechanics::Module>,collision:Arc<collision::CollisionWorld>)->Result<(),String>{
    use mechanics::link::Controller;
    let mut w=sim::World::new(collision.clone());w.mechanics.push(module);
    let id=w.add_player("Session marker",Team::CounterTerrorist);
    w.player_mut(id).unwrap().movement.origin=Vec3::Z*0.1;
    w.step(&[(id,TickInput{toggle_mode:true,..Default::default()})]);
    for _ in 0..128 {w.step(&[(id,TickInput{controller:Controller{connected:true,..Default::default()},..Default::default()})]);}
    w.step(&[(id,TickInput{controller:Controller{connected:true,buttons:0x102,..Default::default()},..Default::default()})]);
    let marker=w.player(id).unwrap().mechanic_checkpoint.clone().ok_or("grounded marker placement failed")?;
    let ui=&w.player(id).unwrap().mechanic.as_ref().unwrap().external.as_ref().unwrap().pose.marker;
    if !ui.visible||!ui.can_return||ui.action!=2{return Err("marker placement did not publish its contextual UI / sound event".into());}
    let p=marker.target.transform[3];let expected=Vec3::new(p[0],-p[2],p[1])/0.0254;
    let saved=w.state();let mut replay=w.clone();replay.restore(saved);
    let mut future=Vec::new();
    for _ in 0..256 {
        let input=TickInput{controller:Controller{connected:true,buttons:0x1000,..Default::default()},..Default::default()};
        w.step(&[(id,input)]);future.push((input,w.state_hash()));
    }
    for (input,hash) in future {replay.step(&[(id,input)]);if hash!=replay.state_hash(){return Err("marker diverged after restore".into());}}
    let distance=w.player(id).unwrap().movement.origin.truncate().distance(expected.truncate());
    if distance<40. {return Err("marker test did not travel away".into());}
    w.step(&[(id,TickInput{toggle_mode:true,..Default::default()})]);
    let actor=w.player(id).unwrap();let (mins,maxs)=actor.movement.hull();let hit=collision.trace_box(actor.movement.origin,actor.movement.origin,mins,maxs,collision::contents::MASK_PLAYERSOLID);
    if actor.mechanic.is_some()||hit.start_solid||hit.all_solid{return Err("grounded Skate exit embedded CS hull".into());}
    if actor.mechanic_checkpoint.as_ref()!=Some(&marker){return Err("CS exit erased session marker".into());}
    w.step(&[(id,TickInput::default())]);
    w.step(&[(id,TickInput{toggle_mode:true,..Default::default()})]);
    let mut progress=false;
    for _ in 0..256 {w.step(&[(id,TickInput{controller:Controller{connected:true,buttons:0x101,..Default::default()},..Default::default()})]);let ui=&w.player(id).unwrap().mechanic.as_ref().unwrap().external.as_ref().unwrap().pose.marker;progress|=ui.progress>0. && ui.progress<1.;}
    let returned=w.player(id).unwrap().movement.origin.truncate().distance(expected.truncate());
    println!("Session marker: travelled={distance:.2}in, return error={returned:.2}in; CS exit hull clear and marker preserved");
    if returned>20. {return Err("session marker did not return through the native actor reset".into());}
    if !progress||w.player(id).unwrap().mechanic.as_ref().unwrap().external.as_ref().unwrap().pose.marker.action!=3{return Err("marker return did not publish progress / return sound event".into());}
    Ok(())
}
fn grindcheck(module: Arc<sim::mechanics::Module>) -> Result<(), String> {
    let mut collision = collision::CollisionWorld::new();
    let floor = [
        Vec3::new(-5000., -5000., 0.),
        Vec3::new(5000., -5000., 0.),
        Vec3::new(5000., 5000., 0.),
        Vec3::new(-5000., 5000., 0.),
    ];
    for tri in [[0, 1, 2], [0, 2, 3]] {
        collision.add_triangle(tri.map(|i| floor[i]), collision::contents::SOLID, 0);
    }
    let low = Vec3::new(0., 20., 0.);
    let high = Vec3::new(1000., 80., 32.);
    collision.add_convex(collision::Convex {
        planes: vec![
            collision::Plane {
                normal: Vec3::X,
                dist: high.x,
            },
            collision::Plane {
                normal: -Vec3::X,
                dist: -low.x,
            },
            collision::Plane {
                normal: Vec3::Y,
                dist: high.y,
            },
            collision::Plane {
                normal: -Vec3::Y,
                dist: -low.y,
            },
            collision::Plane {
                normal: Vec3::Z,
                dist: high.z,
            },
            collision::Plane {
                normal: -Vec3::Z,
                dist: -low.z,
            },
        ],
        mins: low,
        maxs: high,
        contents: collision::contents::SOLID,
        surface: 0,
        plane_surfaces: vec![],
    });
    collision.finalize();
    let collision = Arc::new(collision);
    for side in [20., 19., 21.] {
        let mut world = sim::World::new(collision.clone());
        world.mechanics.push(module.clone());
        let id = world.add_player("Ledge check", Team::CounterTerrorist);
        let player = world.player_mut(id).unwrap();
        player.movement.origin = Vec3::new(100., side, 32.1);
        player.movement.velocity = Vec3::X * 180.;
        let mut acquired = false;
        for tick in 0..256 {
            world.step(&[(
                id,
                TickInput {
                    toggle_mode: tick == 0,
                    controller: mechanics::link::Controller {
                        connected: true,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )]);
            let state = world
                .player(id)
                .unwrap()
                .mechanic
                .as_ref()
                .and_then(|s| s.external.as_ref())
                .ok_or("grind check runtime failed")?;
            if (400..406).contains(&state.pose.state) {
                acquired = true;
            }
        }
        println!("Ledge side={side}: native grind acquired={acquired}");
        if acquired {
            return Ok(());
        }
    }
    Err("native grind acquisition did not occur on the exported ledge".into())
}
fn offboardcheck(
    module: Arc<sim::mechanics::Module>,
    collision: Arc<collision::CollisionWorld>,
) -> Result<(), String> {
    let mut world = sim::World::new(collision);
    world.mechanics.push(module);
    let id = world.add_player("Walk/remount check", Team::CounterTerrorist);
    world.player_mut(id).unwrap().movement.origin = Vec3::Z * 0.1;
    let mut walked = false;
    let mut remounted = false;
    let mut start = None;
    let mut distance = 0f32;
    let mut look_baseline=None;
    let mut look_up=None;
    for tick in 0..128 * 7 {
        let t = tick as f32 / 128.;
        let mut report = mechanics::link::Controller {
            connected: true,
            ..Default::default()
        };
        if (0.75..0.9).contains(&t) || (4.0..4.15).contains(&t) {
            report.buttons = 0x8000;
        }
        if (1.5..3.5).contains(&t) {
            report.left = [0, 25000];
        }
        if (2.5..3.5).contains(&t){report.right=[0,24000];}
        world.step(&[(
            id,
            TickInput {
                toggle_mode: tick == 0,
                controller: report,
                ..Default::default()
            },
        )]);
        let p = world.player(id).unwrap();
        let state = p
            .mechanic
            .as_ref()
            .and_then(|s| s.external.as_ref())
            .ok_or("offboard runtime failed")?;
        if (500..600).contains(&state.pose.state) {
            walked = true;
            let start = *start.get_or_insert(p.movement.origin);
            distance = distance.max(p.movement.origin.distance(start));
            if (2.45..2.5).contains(&t){look_baseline=state.pose.camera.map(|m|Vec3::from_slice(&m[2][..3]).normalize_or_zero().z);}
            if (3.45..3.5).contains(&t){look_up=state.pose.camera.map(|m|Vec3::from_slice(&m[2][..3]).normalize_or_zero().z);}
        }
        if t > 4.5 && (100..200).contains(&state.pose.state) {
            remounted = true;
        }
    }
    println!("Offboard: walked={walked}, distance={distance:.2}, remounted={remounted}");
    if !walked || !remounted || distance < 24. {
        return Err("native walking/remount did not complete".into());
    }
    println!("Offboard vertical look: before={look_baseline:?}, stick-up={look_up:?}");
    if !matches!((look_baseline,look_up),(Some(before),Some(after)) if after>before+0.03){return Err("walking stick-up did not raise the camera aim".into());}
    Ok(())
}
fn dynamiccheck(module: &sim::mechanics::Module) -> Result<(), String> {
    use mechanics::link::*;
    let program = module.external.as_ref().unwrap();
    let floor = [
        [-5000., -5000., 0.],
        [5000., -5000., 0.],
        [5000., 5000., 0.],
        [-5000., 5000., 0.],
    ];
    let points = |x: f32| {
        [x, x + 4.]
            .into_iter()
            .flat_map(|x| {
                [-64., 64.]
                    .into_iter()
                    .flat_map(move |y| [0., 120.].map(move |z| [x, y, z]))
            })
            .collect::<Vec<_>>()
    };
    let mut distances = vec![];
    for (case, (door, removed)) in [(true, false), (true, true), (false, false), (false, true)]
        .into_iter()
        .enumerate()
    {
        let part = points(100.);
        let mut scene = Scene {
            triangles: [[0, 1, 2], [0, 2, 3]]
                .map(|ids| Triangle {
                    points: ids.map(|i| floor[i]),
                    surface: 0,
                })
                .to_vec(),
            ..Default::default()
        };
        if door {
            scene.doors.push(Door {
                origin: [100., -64., 0.],
                triangles: vec![],
                parts: vec![part],
            });
        } else {
            scene.breakables.push(Breakable {
                killed_by: vec![0],
                triangles: vec![],
                parts: vec![part],
            });
        }
        let start = Start {
            grounded:true,
            checkpoint:None,
            position: [0., 0., 3.5],
            velocity: [240., 0., 0.],
            yaw: 0.,
        };
        let mut journal = mechanics::external::Journal::default();
        let mut hash = mechanics::external::digest([0; 32], &start);
        let mut x = 0.;
        for _ in 0..512 {
            let frame = Frame {
                input: Controller {
                    connected: true,
                    buttons: 0x1000,
                    ..Default::default()
                },
                dt: 1. / 128.,
                scene: SceneState {
                    ground_material:0,
                    doors: vec![if removed { 90. } else { 0. }],
                    broken: vec![removed],
                },
            };
            let (pose, _) = program.advance(
                &scene,
                100 + case,
                100 + case as u64,
                &start,
                &journal,
                hash,
                &frame,
            )?;
            x = pose.position[0];
            hash = mechanics::external::digest(hash, &frame);
            journal.push(frame);
        }
        distances.push(x);
    }
    println!(
        "Dynamic contacts: door closed/open x={:.1}/{:.1}, breakable intact/broken x={:.1}/{:.1}",
        distances[0], distances[1], distances[2], distances[3]
    );
    if distances[0] > 130. || distances[2] > 130. || distances[1] < 250. || distances[3] < 250. {
        return Err("native dynamic contact did not match live CS geometry".into());
    }
    Ok(())
}
fn flipcheck(
    module: Arc<sim::mechanics::Module>,
    collision: Arc<collision::CollisionWorld>,
) -> Result<(), String> {
    let mut world = sim::World::new(collision);
    world.mechanics.push(module);
    let id = world.add_player("Flick check", Team::CounterTerrorist);
    world.player_mut(id).unwrap().movement.origin = Vec3::Z * 0.1;
    let mut inverted = false;
    let mut trick = String::new();
    for tick in 0..384 {
        let t = tick as f32 / 128.;
        let mut report = mechanics::link::Controller {
            connected: true,
            ..Default::default()
        };
        if (0.8..1.3).contains(&t) {
            report.right = [0, -32768];
        }
        if (1.3..1.5).contains(&t) {
            report.right = [30000, 30000];
        }
        world.step(&[(
            id,
            TickInput {
                toggle_mode: tick == 0,
                controller: report,
                ..Default::default()
            },
        )]);
        let state = world
            .player(id)
            .unwrap()
            .mechanic
            .as_ref()
            .and_then(|s| s.external.as_ref())
            .ok_or("flick runtime failed")?;
        if let Some(i) = state.rig.names.iter().position(|n| n == "SKATEBOARD_ROOT") {
            inverted |= state.pose.bones[i][1][1] < -0.5;
        }
        if state.pose.trick.to_ascii_lowercase().contains("flip") {
            trick = state.pose.trick.clone();
        }
    }
    println!("Flick: inverted={inverted}, trick={trick}");
    if !inverted || trick.is_empty() {
        return Err("right-stick flick did not produce an articulated flip trick".into());
    }
    Ok(())
}

/// A flat floor of one host rolling family (`surfaces` char), optionally with a ledge of
/// another family, exported to the provider like a CS map.
fn audio_world(floor: u8, ledge: Option<u8>) -> Arc<collision::CollisionWorld> {
    let mut collision = collision::CollisionWorld::new();
    let corners = [
        Vec3::new(-20000., -5000., 0.),
        Vec3::new(20000., -5000., 0.),
        Vec3::new(20000., 5000., 0.),
        Vec3::new(-20000., 5000., 0.),
    ];
    for tri in [[0, 1, 2], [0, 2, 3]] {
        collision.add_triangle(tri.map(|i| corners[i]), collision::contents::SOLID, 0);
    }
    if ledge.is_some() {
        let (low, high) = (Vec3::new(0., 20., 0.), Vec3::new(1500., 80., 32.));
        let plane = |normal: Vec3, dist: f32| collision::Plane { normal, dist };
        collision.add_convex(collision::Convex {
            planes: vec![
                plane(Vec3::X, high.x),
                plane(-Vec3::X, -low.x),
                plane(Vec3::Y, high.y),
                plane(-Vec3::Y, -low.y),
                plane(Vec3::Z, high.z),
                plane(-Vec3::Z, -low.z),
            ],
            mins: low,
            maxs: high,
            contents: collision::contents::SOLID,
            surface: 1,
            plane_surfaces: vec![],
        });
    }
    collision.rolling_surfaces = vec![floor, ledge.unwrap_or(floor)];
    collision.finalize();
    Arc::new(collision)
}

fn write_stereo_wav(path: &Path, samples: &[f32]) -> Result<(), String> {
    let pcm: Vec<u8> = samples
        .iter()
        .flat_map(|s| ((s.clamp(-1., 1.) * 32767.) as i16).to_le_bytes())
        .collect();
    let mut wav = b"RIFF".to_vec();
    wav.extend((pcm.len() as u32 + 36).to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(2u16.to_le_bytes());
    wav.extend(48000u32.to_le_bytes());
    wav.extend((48000u32 * 4).to_le_bytes());
    wav.extend(4u16.to_le_bytes());
    wav.extend(16u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend((pcm.len() as u32).to_le_bytes());
    wav.extend(pcm);
    std::fs::write(path, wav).map_err(|e| e.to_string())
}

/// RMS and peak of interleaved stereo between two times (s).
fn level(samples: &[f32], from: f32, to: f32) -> (f32, f32) {
    let at = |t: f32| ((t.max(0.) * 48000.) as usize * 2).min(samples.len());
    let s = &samples[at(from)..at(to)];
    let rms = (s.iter().map(|v| v * v).sum::<f32>() / s.len().max(1) as f32).sqrt();
    (rms, s.iter().fold(0f32, |p, v| p.max(v.abs())))
}

/// A scripted Xbox ride: (seconds, label, report) segments in order.
struct Ride {
    name: &'static str,
    floor: u8,
    ledge: Option<u8>,
    origin: Vec3,
    velocity: Vec3,
    seconds: f32,
    script: fn(f32) -> (&'static str, mechanics::link::Controller),
}

/// Rolling, pushes (A), an ollie and a kickflip (right-stick flicks), a foot brake (B) and a
/// powerslide (left stick past 0.9) on one surface.
fn street(t: f32) -> (&'static str, mechanics::link::Controller) {
    let mut c = mechanics::link::Controller {
        connected: true,
        ..Default::default()
    };
    let label = match t {
        t if t < 1.5 => "roll",
        t if t < 3.5 => {
            c.buttons = 0x1000;
            "push"
        }
        t if t < 4.5 => "roll-fast",
        t if t < 4.8 => {
            c.right = [0, -32768];
            "ollie-crouch"
        }
        t if t < 4.95 => {
            c.right = [0, 32767];
            "ollie-pop"
        }
        t if t < 6.5 => "ollie-air-land",
        t if t < 6.8 => {
            c.right = [0, -32768];
            "kickflip-crouch"
        }
        t if t < 6.95 => {
            c.right = [30000, 30000];
            "kickflip-flick"
        }
        t if t < 8.5 => "kickflip-air-land",
        t if t < 9.5 => {
            // Past 0.9 deflection, 0.6 rad right of forward: RightSlideStart.
            c.left = [18500, 27000];
            "powerslide"
        }
        t if t < 10. => "roll-after-slide",
        t if t < 11.5 => {
            c.buttons = 0x2000;
            "brake"
        }
        _ => "stop",
    };
    (label, c)
}

/// Onto the ledge's top edge and along it.
fn ledge(_: f32) -> (&'static str, mechanics::link::Controller) {
    (
        "grind",
        mechanics::link::Controller {
            connected: true,
            ..Default::default()
        },
    )
}

/// Drive one ride through the sim and the provider; returns its stereo PCM and segment spans.
fn record(
    module: &Arc<sim::mechanics::Module>,
    ride: &Ride,
) -> Result<(Vec<f32>, Vec<(&'static str, f32, f32)>), String> {
    let collision = audio_world(ride.floor, ride.ledge);
    let program = module.external.as_ref().ok_or("no provider")?;
    // As map loading does: start the provider for this scene, then give its background audio
    // decode time to finish (the sim below runs much faster than real time).
    program.prepare_world(&collision)?;
    std::thread::sleep(std::time::Duration::from_secs(3));
    let mut world = sim::World::new(collision);
    world.set_tick_rate(128);
    world.mechanics.push(module.clone());
    let id = world.add_player("Audio proof", Team::CounterTerrorist);
    let player = world.player_mut(id).unwrap();
    player.movement.origin = ride.origin;
    player.movement.velocity = ride.velocity;
    let (mut audio, mut segments) = (Vec::new(), Vec::<(&'static str, f32, f32)>::new());
    let mut last = (0u32, false, String::new());
    let ticks = (ride.seconds * 128.) as u32;
    for tick in 0..ticks {
        let t = tick as f32 / 128.;
        let (label, controller) = (ride.script)(t);
        if segments.last().is_none_or(|s| s.0 != label) {
            if let Some(s) = segments.last_mut() {
                s.2 = t;
            }
            segments.push((label, t, ride.seconds));
            eprintln!("PROOF {} t={t:.3} segment={label}", ride.name);
        }
        world.step(&[(
            id,
            TickInput {
                toggle_mode: tick == 0,
                controller,
                ..Default::default()
            },
        )]);
        let p = world.player(id).unwrap();
        let state = p
            .mechanic
            .as_ref()
            .and_then(|s| s.external.as_ref())
            .ok_or("audio proof runtime failed")?;
        audio.extend(program.take_audio(state.session));
        let now = (state.pose.state, state.pose.grounded, state.pose.trick.clone());
        if now != last {
            eprintln!(
                "PROOF {} t={t:.3} state={} grounded={} trick={:?} speed={:.0}",
                ride.name,
                now.0,
                now.1,
                now.2,
                p.movement.speed_2d()
            );
            last = now;
        }
    }
    Ok((audio, segments))
}

/// Native audio smoke check: every floor family rolls audibly and differently.
fn audiocheck(module: Arc<sim::mechanics::Module>) -> Result<(), String> {
    let mut hashes = std::collections::BTreeSet::new();
    for floor in [b'C', b'W', b'M'] {
        let ride = Ride {
            name: "roll",
            floor,
            ledge: None,
            origin: Vec3::Z * 0.1,
            velocity: Vec3::X * 300.,
            seconds: 3.,
            script: |_| ("roll", mechanics::link::Controller { connected: true, ..Default::default() }),
        };
        let (audio, _) = record(&module, &ride)?;
        let (rms, peak) = level(&audio, 1., 3.);
        println!("Native rolling floor={} frames={} rms={rms:.5} peak={peak:.4}", floor as char, audio.len() / 2);
        if !rms.is_finite() || rms < 0.0005 {
            return Err(format!("silent native rolling on {}", floor as char));
        }
        hashes.insert(mechanics::external::digest([0; 32], &audio));
    }
    if hashes.len() != 3 {
        return Err("different floor families produced identical rolling audio".into());
    }
    Ok(())
}

/// `--audio`: scripted rides on concrete, wood and metal plus a metal ledge grind. Writes
/// `<CSR_SKATE_AUDIO_PROOF>/<ride>.wav` (48 kHz stereo) and prints per-segment RMS / peak; run
/// with `CSR_SKATE_AUDIO_TRACE=1` for the provider's posts and Splice ids on stderr.
fn audioproof(module: Arc<sim::mechanics::Module>) -> Result<(), String> {
    let dir = std::env::var_os("CSR_SKATE_AUDIO_PROOF")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("csr-skate-audio-proof"));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut rides: Vec<Ride> = [(b'C', "street-concrete"), (b'W', "street-wood"), (b'M', "street-metal")]
        .into_iter()
        .map(|(floor, name)| Ride {
            name,
            floor,
            ledge: None,
            origin: Vec3::Z * 0.1,
            velocity: Vec3::X * 250.,
            seconds: 12.,
            script: street,
        })
        .collect();
    rides.push(Ride {
        name: "grind-metal-ledge",
        floor: b'C',
        ledge: Some(b'M'),
        origin: Vec3::new(100., 20., 32.1),
        velocity: Vec3::X * 220.,
        seconds: 6.,
        script: ledge,
    });
    rides.push(Ride {
        name: "grind-concrete-ledge",
        floor: b'C',
        ledge: Some(b'C'),
        origin: Vec3::new(100., 20., 32.1),
        velocity: Vec3::X * 220.,
        seconds: 6.,
        script: ledge,
    });
    for ride in &rides {
        let (audio, segments) = record(&module, ride)?;
        let path = dir.join(format!("{}.wav", ride.name));
        write_stereo_wav(&path, &audio)?;
        println!("{} -> {} ({} frames)", ride.name, path.display(), audio.len() / 2);
        for (label, from, to) in segments {
            let (rms, peak) = level(&audio, from, to);
            println!("  {label:<20} {from:6.2}-{to:6.2}s rms={rms:.5} peak={peak:.4}");
        }
        let mut windows = String::from("t\trms\tpeak\n");
        for w in 0..(audio.len() / 2 / 2400) {
            let (rms, peak) = level(&audio, w as f32 * 0.05, (w + 1) as f32 * 0.05);
            windows += &format!("{:.2}\t{rms:.5}\t{peak:.5}\n", w as f32 * 0.05);
        }
        std::fs::write(dir.join(format!("{}.tsv", ride.name)), windows).map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn course_level()->Result<level::Level,String>{
 let pack=pack_authoring::surf_course::pack()?;
 let (dir,fs)=level::mount_game(None).map_err(|e|e.to_string())?;
 level::Level::load_pack(&pack,&pack.manifest().maps[0],dir,fs).map_err(|e|e.to_string())
}
