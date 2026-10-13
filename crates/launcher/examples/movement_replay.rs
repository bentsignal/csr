//! Developer-only replay of bounded, explicit movement inputs on an installed map.
//! A successful replay is not a claim of native CS:GO parity.
use movement::{MoveConfig, PlayerMove};
use serde_json::{Value, json};
use sim::{TickInput, World};
use std::{error::Error, io::Read, path::Path};

const MAX_BYTES: u64 = 2 * 1024 * 1024;
const MAX_COMMANDS: usize = 4096;
const FIELDS: &[&str] = &[
    "schema", "map", "map_sha1", "catalog_sha1", "tick_rate", "config", "weapon",
    "initial_movement", "commands",
];

fn read_fixture(path: &Path) -> Result<Value, Box<dyn Error>> {
    let metadata = path.symlink_metadata()?;
    if !metadata.is_file() || metadata.len() > MAX_BYTES {
        return Err("fixture must be an ordinary file of at most 2 MiB".into());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)?.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_BYTES { return Err("fixture exceeds byte bound".into()); }
    let value: Value = serde_json::from_slice(&bytes)?;
    let object = value.as_object().ok_or("fixture must be an object")?;
    if object.len() != FIELDS.len() || FIELDS.iter().any(|key| !object.contains_key(*key)) {
        return Err("unexpected schema1 fixture fields".into());
    }
    if value["schema"].as_u64() != Some(1) { return Err("unsupported fixture schema".into()); }
    Ok(value)
}

fn validate_config(value: &Value) -> Result<MoveConfig, Box<dyn Error>> {
    let config: MoveConfig = serde_json::from_value(value.clone())?;
    let fields = value.as_object().ok_or("config must be an object")?;
    let expected = serde_json::to_value(MoveConfig::default())?;
    let expected = expected.as_object().unwrap();
    if fields.len() != expected.len() || fields.keys().any(|key| !expected.contains_key(key)) {
        return Err("config must explicitly specify every movement field".into());
    }
    for (key, value) in fields {
        if expected[key].is_boolean() { continue; }
        let number = value.as_f64().ok_or("movement field is not numeric")?;
        if !number.is_finite() || !(0. ..=1_000_000.).contains(&number) {
            return Err(format!("invalid movement field: {key}").into());
        }
    }
    if config.max_speed <= 0. || config.max_velocity <= 0. {
        return Err("speed bounds must be positive".into());
    }
    Ok(config)
}

fn validate_movement(state: &PlayerMove, cfg: &MoveConfig) -> Result<(), Box<dyn Error>> {
    if !state.origin.is_finite() || !state.velocity.is_finite()
        || state.origin.abs().max_element() > 1_000_000.
        || state.velocity.abs().max_element() > 1_000_000.
        || !state.duck_amount.is_finite() || !(0. ..=1.).contains(&state.duck_amount)
        || !state.stamina.is_finite() || !(0. ..=cfg.stamina_max).contains(&state.stamina)
        || !state.last_fall_speed.is_finite() || state.last_fall_speed < 0.
    {
        return Err("movement state is nonfinite or outside explicit state bounds".into());
    }
    Ok(())
}

fn validate_input(input: &TickInput) -> Result<(), Box<dyn Error>> {
    let movement_mask = movement::buttons::JUMP | movement::buttons::DUCK | movement::buttons::WALK;
    if !input.view_angles.is_finite() || !input.forward_move.is_finite() || !input.side_move.is_finite()
        || input.forward_move.abs() > 450. || input.side_move.abs() > 450.
        || input.buttons & !movement_mask != 0 || input.weapon_slot != 0
        || input.buy != sim::Purchase::None || input.drop || input.utility != 0
        || input.tracked.is_some() || input.aim.is_some() || input.rewind_tick.is_some()
    {
        return Err("fixture accepts finite movement-only TickInput commands".into());
    }
    Ok(())
}

fn replay(world: &mut World, actor: sim::PlayerId, inputs: &[TickInput]) -> Result<Vec<Value>, Box<dyn Error>> {
    let mut samples = Vec::with_capacity(inputs.len());
    for (ordinal, input) in inputs.iter().enumerate() {
        validate_input(input)?;
        let producer_tick = world.tick;
        world.step(&[(actor, *input)]);
        let player = world.player(actor).ok_or("replay actor disappeared")?;
        if !player.alive { return Err("actor died; movement-only segment is invalid".into()); }
        validate_movement(&player.movement, &world.config)?;
        samples.push(json!({"ordinal": ordinal, "producer_tick": producer_tick,
            "published_tick": world.tick, "movement": player.movement,
            "movement_events": player.last_events}));
    }
    Ok(samples)
}

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 1 { return Err("usage: cargo run -p launcher --example movement_replay -- fixture.json".into()); }
    let fixture = read_fixture(Path::new(&args[0]))?;
    let map = fixture["map"].as_str().ok_or("map must be a string")?;
    if !packs::valid_bsp_name(map) { return Err("invalid map name".into()); }
    // Never trigger a Workshop fetch as a side effect of reference replay.
    let (_, fs) = level::mount_game(None)?;
    if !fs.exists(&format!("maps/{map}.bsp")) { return Err("map is not already installed/mounted".into()); }
    drop(fs);
    let level = level::Level::load(map, None)?;
    let actual_hash: String = level.bsp_sha1().iter().map(|byte| format!("{byte:02x}")).collect();
    if fixture["map_sha1"].as_str() != Some(actual_hash.as_str()) {
        return Err("loaded BSP digest differs from fixture identity".into());
    }
    let catalog_hash: String = level.catalog_sha1().ok_or("selected catalog has no parsed input identity")?
        .iter().map(|byte| format!("{byte:02x}")).collect();
    if fixture["catalog_sha1"].as_str() != Some(catalog_hash.as_str()) {
        return Err("parsed catalog digest differs from fixture identity".into());
    }
    let rate = fixture["tick_rate"].as_u64().ok_or("tick_rate must be an integer")?;
    if !matches!(rate, 64 | 128) { return Err("reference replay requires explicit 64 or 128 tick".into()); }
    let config = validate_config(&fixture["config"])?;
    let initial: PlayerMove = serde_json::from_value(fixture["initial_movement"].clone())?;
    validate_movement(&initial, &config)?;
    let commands: Vec<TickInput> = serde_json::from_value(fixture["commands"].clone())?;
    if commands.is_empty() || commands.len() > MAX_COMMANDS { return Err("commands must contain 1..4096 entries".into()); }
    for input in &commands { validate_input(input)?; }
    let weapon = fixture["weapon"].as_str().ok_or("weapon must be a string")?;
    let definition = level.catalog.get(weapon).ok_or("fixture weapon is not in selected catalog")?.clone();
    let mut world = World::with_catalog(level.collision.clone(), level.catalog.clone());
    world.set_tick_rate(rate as u32);
    world.config = config;
    world.load_spawns(&level.bsp);
    let actor = world.add_player("movement reference", sim::Team::CounterTerrorist);
    let player = world.player_mut(actor).unwrap();
    player.movement = initial;
    player.weapons = vec![sim::weapon::WeaponState::new(std::sync::Arc::new(definition))];
    player.active = 0;
    player.last_active = 0;
    let samples = replay(&mut world, actor, &commands)?;
    println!("{}", serde_json::to_string(&json!({"schema":1,"map":map,"map_sha1":actual_hash,
        "catalog_sha1":catalog_hash,"tick_rate":rate,"weapon":weapon,"config":world.config,"initial_movement":initial,
        "scope":"movement-only sim replay; native phase, hull, surface, triggers and2015 parity unverified",
        "samples":samples}))?);
    Ok(())
}

fn main() {
    if let Err(error) = run() { eprintln!("movement replay failed: {error}"); std::process::exit(1); }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;
    #[test]
    fn malformed_and_gameplay_commands_cannot_become_movement_receipts() {
        for input in [TickInput { buttons: movement::buttons::ATTACK, ..Default::default() },
            TickInput { forward_move: f32::INFINITY, ..Default::default() },
            TickInput { weapon_slot: 3, ..Default::default() }] {
            assert!(validate_input(&input).is_err());
        }
        assert!(validate_input(&TickInput { forward_move:450.,buttons:movement::buttons::JUMP,..Default::default() }).is_ok());
        assert!(validate_config(&json!({})).is_err());
        let mut cfg = serde_json::to_value(MoveConfig::default()).unwrap();
        cfg["gravity"] = json!(-1.);
        assert!(validate_config(&cfg).is_err());
    }
    #[test]
    fn sim_replay_records_producing_tick_and_actual_grounded_jump_at_both_rates() {
        for rate in [64,128] {
            let mut collision = collision::CollisionWorld::new();
            let z = Vec3::ZERO;
            collision.add_triangle([z,Vec3::new(4096.,0.,0.),Vec3::new(0.,4096.,0.)],collision::contents::SOLID,0);
            collision.finalize();
            let mut world = World::new(std::sync::Arc::new(collision));
            world.set_tick_rate(rate);
            let actor = world.add_player("receipt test",sim::Team::CounterTerrorist);
            let origin = Vec3::new(256.,256.,0.0625);
            assert!(!world.collision.trace_box(origin,origin,movement::STAND_MINS,movement::STAND_MAXS,
                collision::contents::MASK_PLAYERSOLID).start_solid);
            world.player_mut(actor).unwrap().movement = PlayerMove::spawn_at(origin);
            let samples = replay(&mut world,actor,&[TickInput { buttons:movement::buttons::JUMP,..Default::default() },TickInput::default()]).unwrap();
            assert_eq!(samples[0]["producer_tick"],0);
            assert_eq!(samples[0]["published_tick"],1);
            assert_eq!(samples[1]["producer_tick"],1);
            assert_eq!(samples[0]["movement_events"]["jumped"],true);
            assert!(!world.player(actor).unwrap().movement.on_ground);
        }
    }
}
