//! Numeric targets from qualified Legacy standing P2000 runs at 64/128 Hz.
//! Raw game receipts stay private. This analytic floor does not replay the map
//! or establish internal command phase, historical parity, crouch or surfing.
use super::*;

const FLOOR_Z: f32 = -319.96875;
struct FlatFloor;
impl TraceWorld for FlatFloor {
    fn trace_hull(&self, start: Vec3, end: Vec3, mins: Vec3, _maxs: Vec3, _mask: i32) -> Trace {
        let a = start.z + mins.z - FLOOR_Z;
        let b = end.z + mins.z - FLOOR_Z;
        if a < -0.0001 {
            return Trace { fraction: 0., end: start, normal: Vec3::Z,
                plane_dist: FLOOR_Z, start_solid: true, all_solid: b < -0.0001,
                contents: contents::SOLID, surface: 0 };
        }
        if b < 0. && a >= 0. {
            let fraction = a / (a - b);
            return Trace { fraction, end: start + (end - start) * fraction, normal: Vec3::Z,
                plane_dist: FLOOR_Z, start_solid: false, all_solid: false,
                contents: contents::SOLID, surface: 0 };
        }
        Trace::empty(end)
    }
}

struct Run {
    rate: u32,
    forward: usize,
    jump: usize,
    jump_end: usize,
    landing: usize,
    release: usize,
    takeoff_vz: f32,
    jump_stamina: f32,
    landing_height: f32,
    landing_stamina: f32,
    runup: [f32; 12],
    recovery_speed: [f32; 32],
    recovery_stamina: [f32; 32],
    coast: [f32; 32],
}

fn near(actual: f32, target: f32, tolerance: f32, label: &str, rate: u32, ordinal: usize) {
    assert!((actual - target).abs() <= tolerance,
        "{label} at {rate}Hz command {ordinal}: {actual} != {target}");
}

#[test]
fn measured_standing_running_landing_recovery_and_coast_64_128() {
    for run in runs() {
        // Native interval/input edges, not host sleep translated to ticks.
        // First forward wish is actually 225; later held wishes are 450.
        for (ground_ticks, last_fall_speed) in [(0, 0.), (128, 1234.5)] {
            let cfg = MoveConfig::default();
            let dt = 1. / run.rate as f32;
            let mut pm = PlayerMove {
                origin: Vec3::new(0., 0., FLOOR_Z),
                on_ground: true, ground_ticks, last_fall_speed,
                ..Default::default()
            };
            let mut jumps = 0;
            let mut landings = 0;
            for ordinal in 0..512 {
                let held = (run.forward..run.release).contains(&ordinal);
                let jump = (run.jump..=run.jump_end).contains(&ordinal);
                let input = MoveInput {
                    forward_move: if ordinal == run.forward {225.} else if held {450.} else {0.},
                    buttons: if jump {buttons::JUMP} else {0},
                    weapon_max_speed: 240., ..Default::default()
                };
                let events = player_move(&mut pm, &input, &cfg, &FlatFloor, dt);
                jumps += usize::from(events.jumped);
                landings += usize::from(events.landed.is_some());
                assert_eq!(events.jumped, ordinal == run.jump);
                assert_eq!(events.landed.is_some(), ordinal == run.landing);
                assert_eq!(pm.on_ground, !(run.jump..run.landing).contains(&ordinal));
                assert_eq!(pm.velocity.y, 0.);
                assert!(!pm.ducked);
                assert_eq!(pm.duck_amount, 0.);
                if ordinal < run.forward {
                    assert_eq!(pm.velocity, Vec3::ZERO);
                    assert_eq!(pm.stamina, 0.);
                    assert_eq!(pm.origin.z, FLOOR_Z);
                }
                if (run.forward..run.forward + 12).contains(&ordinal) {
                    near(pm.speed_2d(), run.runup[ordinal - run.forward], 0.0002, "runup", run.rate, ordinal);
                }
                if (run.jump - 8..run.jump).contains(&ordinal) {
                    near(pm.speed_2d(), 240., 0.0002, "plateau", run.rate, ordinal);
                    assert_eq!(pm.stamina, 0.);
                }
                if ordinal == run.jump {
                    near(pm.velocity.z, run.takeoff_vz, 0.0001, "takeoff VZ", run.rate, ordinal);
                    near(pm.stamina, run.jump_stamina, 0.0001, "jump stamina", run.rate, ordinal);
                }
                if ordinal == run.landing {
                    near(pm.speed_2d(), 240., 0.0002, "first support speed", run.rate, ordinal);
                    near(pm.origin.z - FLOOR_Z, run.landing_height, 0.0001, "first support height", run.rate, ordinal);
                    near(pm.stamina, run.landing_stamina, 0.0001, "landing stamina", run.rate, ordinal);
                    assert_eq!(pm.velocity.z, 0.);
                }
                if (run.landing + 1..run.landing + 33).contains(&ordinal) {
                    let i = ordinal - run.landing - 1;
                    near(pm.speed_2d(), run.recovery_speed[i], 0.0002, "held recovery", run.rate, ordinal);
                    near(pm.stamina, run.recovery_stamina[i], 0.0001, "recovered stamina", run.rate, ordinal);
                    near(pm.origin.z, FLOOR_Z, 0.0001, "settled support", run.rate, ordinal);
                }
                if (run.release..run.release + 32).contains(&ordinal) {
                    near(pm.speed_2d(), run.coast[ordinal - run.release], 0.0002, "coast", run.rate, ordinal);
                    assert_eq!(pm.stamina, 0.);
                }
            }
            assert_eq!((jumps, landings), (1, 1));
            assert_eq!(pm.velocity, Vec3::ZERO);
            assert_eq!(pm.stamina, 0.);
        }
    }
}

#[test]
fn disabled_weapon_acceleration_option_preserves_partial_wish_base() {
    let cfg = MoveConfig { accelerate_use_weapon_speed: false, ..Default::default() };
    let input = MoveInput { forward_move: 225., weapon_max_speed: 240., ..Default::default() };
    for rate in [64, 128] {
        let mut pm = PlayerMove { origin: Vec3::Z * FLOOR_Z, on_ground: true, ..Default::default() };
        player_move(&mut pm, &input, &cfg, &FlatFloor, 1. / rate as f32);
        near(pm.speed_2d(), 225. * 5.5 / rate as f32, 0.0001, "wish-based option", rate, 0);
    }
}

// Independent observed edge tests make each baseline defect visible even
// when an earlier runup assertion rejects the complete replay first.
fn observed_landing_step(rate: u32) -> PlayerMove {
    let (z, vz) = if rate == 64 { (-314.043273925, -279.256622314) }
        else { (-316.210815429, -288.631622314) };
    let mut pm = PlayerMove { origin: Vec3::new(0., 0., z),
        velocity: Vec3::new(240., 0., vz), ..Default::default() };
    let input = MoveInput { forward_move: 450., weapon_max_speed: 240., ..Default::default() };
    player_move(&mut pm, &input, &MoveConfig::default(), &FlatFloor, 1. / rate as f32);
    assert!(pm.on_ground);
    pm
}

#[test]
fn measured_first_support_preserves_height_at_both_rates() {
    for run in runs() {
        let pm = observed_landing_step(run.rate);
        near(pm.origin.z - FLOOR_Z, run.landing_height, 0.0001, "observed support height", run.rate, run.landing);
    }
}

#[test]
fn measured_landing_stamina_uses_incoming_command_vz_at_both_rates() {
    for run in runs() {
        let pm = observed_landing_step(run.rate);
        near(pm.stamina, run.landing_stamina, 0.0001, "observed landing cost", run.rate, run.landing);
    }
}

#[test]
fn measured_supported_recovery_caps_from_pre_recovery_stamina_at_both_rates() {
    for run in runs() {
        let mut pm = PlayerMove { origin: Vec3::new(0., 0., FLOOR_Z + run.landing_height),
            velocity: Vec3::new(240., 0., 0.), on_ground: true,
            stamina: run.landing_stamina, ..Default::default() };
        let input = MoveInput { forward_move: 450., weapon_max_speed: 240., ..Default::default() };
        player_move(&mut pm, &input, &MoveConfig::default(), &FlatFloor, 1. / run.rate as f32);
        near(pm.speed_2d(), run.recovery_speed[0], 0.0001, "observed recovery cap", run.rate, run.landing + 1);
        near(pm.stamina, run.recovery_stamina[0], 0.0001, "recovery after cap", run.rate, run.landing + 1);
        near(pm.origin.z, FLOOR_Z, 0.0001, "next support snaps", run.rate, run.landing + 1);
    }
}


// Observed post-state numbers, not outputs generated by the Rust mover.
fn runs() -> [Run; 2] {
    [
        Run { rate: 64, forward: 13, jump: 74, jump_end: 84, landing: 120, release: 191,
            takeoff_vz: 283.243377685, jump_stamina: 23.659469604,
            landing_height: 1.464447022, landing_stamina: 13.962831497,
            runup: [
                20.625001907, 34.750003814, 48.875007629, 63.000007629,
                77.125007629, 91.250007629, 104.460945129, 116.598495483,
                127.749870300, 137.995193481, 147.408081054, 156.056182861,
            ],
            recovery_speed: [
                177.657485961, 181.550231933, 185.485183715, 189.462326049,
                193.481658935, 197.543182373, 201.646865844, 205.792755126,
                209.697097778, 213.284210205, 216.579864501, 219.607757568,
                222.389633178, 224.945480346, 227.293655395, 229.451049804,
                231.433151245, 233.254211425, 234.927307128, 236.464462280,
                237.876724243, 239.174240112, 240.000000000, 240.000000000,
                240.000000000, 240.000000000, 240.000000000, 240.000000000,
                240.000000000, 240.000000000, 240.000000000, 240.000000000,
            ],
            recovery_stamina: [
                13.025331497, 12.087831497, 11.150331497, 10.212831497,
                9.275331497, 8.337831497, 7.400331497, 6.462831497,
                5.525331497, 4.587831497, 3.650331497, 2.712831497,
                1.775331497, 0.837831497, 0.000000000, 0.000000000,
                0.000000000, 0.000000000, 0.000000000, 0.000000000,
                0.000000000, 0.000000000, 0.000000000, 0.000000000,
                0.000000000, 0.000000000, 0.000000000, 0.000000000,
                0.000000000, 0.000000000, 0.000000000, 0.000000000,
            ],
            coast: [
                220.500000000, 202.584381103, 186.124404907, 171.001800537,
                157.107910156, 144.342895507, 132.615036010, 121.840065002,
                111.940559387, 102.845390319, 94.489204406, 86.811958312,
                79.758483886, 73.258483886, 66.758483886, 60.258483886,
                53.758483886, 47.258483886, 40.758483886, 34.258483886,
                27.758483886, 21.258483886, 14.758483886, 8.258483886,
                1.758483886, 0.000000000, 0.000000000, 0.000000000,
                0.000000000, 0.000000000, 0.000000000, 0.000000000,
            ],
        },
        Run { rate: 128, forward: 30, jump: 165, jump_end: 176, landing: 259, release: 378,
            takeoff_vz: 292.618377685, jump_stamina: 23.909469604,
            landing_height: 1.478576661, landing_stamina: 14.431581497,
            runup: [
                10.312500953, 17.375001907, 24.437503814, 31.500003814,
                38.562503814, 45.625003814, 52.687503814, 59.750003814,
                66.812507629, 73.875007629, 80.937507629, 87.961921691,
            ],
            recovery_speed: [
                175.726898193, 177.657485961, 179.598587036, 181.550231933,
                183.512435913, 185.485183715, 187.468490600, 189.462326049,
                191.466720581, 193.481658935, 195.507125854, 197.543182373,
                199.589752197, 201.646865844, 203.714553833, 205.751144409,
                207.705001831, 209.579483032, 211.377822875, 213.103103637,
                214.758285522, 216.346237182, 217.869674682, 219.331222534,
                220.733398437, 222.078598022, 223.369155883, 224.607284545,
                225.795120239, 226.934692382, 228.027969360, 229.076828002,
            ],
            recovery_stamina: [
                13.962831497, 13.494081497, 13.025331497, 12.556581497,
                12.087831497, 11.619081497, 11.150331497, 10.681581497,
                10.212831497, 9.744081497, 9.275331497, 8.806581497,
                8.337831497, 7.869081497, 7.400331497, 6.931581497,
                6.462831497, 5.994081497, 5.525331497, 5.056581497,
                4.587831497, 4.119081497, 3.650331497, 3.181581497,
                2.712831497, 2.244081497, 1.775331497, 1.306581497,
                0.837831497, 0.369081497, 0.000000000, 0.000000000,
            ],
            coast: [
                230.250000000, 220.896087646, 211.922180175, 203.312835693,
                195.053253173, 187.129211425, 179.527084350, 172.233795166,
                165.236801147, 158.524063110, 152.084030151, 145.905609130,
                139.978195190, 134.291580200, 128.835983276, 123.602020263,
                118.580688476, 113.763351440, 109.141716003, 104.707832336,
                100.454078674, 96.373130798, 92.457969665, 88.701866149,
                85.098350524, 81.641227722, 78.324554443, 75.074554443,
                71.824554443, 68.574554443, 65.324554443, 62.074554443,
            ],
        },
    ]
}
