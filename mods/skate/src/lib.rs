// SPDX-License-Identifier: GPL-3.0-only
// Adapter copyright 2026 CSGO Rewrite contributors.
// Riding calculations: dumbad and Skate 3 Rust Engine contributors; see NOTICE.
//! CS collision/keyboard adapter. The upstream push, braking, steering and
//! ground-jump calculations are reused directly. This is an initial riding
//! integration, not the upstream full seven-body/skeleton/animation simulation.
use glam::Vec3;
use mechanics_api as api;
use skate_core::{
    air::ground_jump,
    math::Vector3,
    point_graph::PointGraph,
    riding::{braking, push, steering},
};

const UNITS: f32 = 1. / 0.0254;
static mut PACKET: api::Packet = [0.; api::WORDS];

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "csr_v1")]
unsafe extern "C" {
    fn trace(pointer: i32) -> i32;
}

#[unsafe(no_mangle)]
pub extern "C" fn csr_packet() -> i32 {
    (&raw mut PACKET) as usize as i32
}

#[unsafe(no_mangle)]
pub extern "C" fn csr_step() -> i32 {
    // Each host call owns a fresh instance. No references to this storage cross
    // the ABI; all retained gameplay state is returned in the explicit packet.
    let packet = unsafe { &mut *(&raw mut PACKET) };
    if packet[api::API] != api::VERSION {
        return 1;
    }
    advance(packet);
    0
}

fn graph<const N: usize>(config: &[f32], at: usize) -> PointGraph<N> {
    PointGraph {
        x: core::array::from_fn(|i| config[at + i]),
        y: core::array::from_fn(|i| config[at + N + i]),
    }
}

fn query(start: Vec3, end: Vec3, crouched: bool, height: f32) -> [f32; api::TRACE_WORDS] {
    let mut q = [0.; api::TRACE_WORDS];
    q[..3].copy_from_slice(&start.to_array());
    q[3..6].copy_from_slice(&end.to_array());
    q[6..9].copy_from_slice(&[-16., -16., 0.]);
    q[9..12].copy_from_slice(&[16., 16., (if crouched { 54. } else { 72. }) + height]);
    #[cfg(target_arch = "wasm32")]
    unsafe {
        trace(q.as_mut_ptr() as usize as i32);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        q[12] = 1.;
        q[13..16].copy_from_slice(&end.to_array());
    }
    q
}
fn endpoint(q: &[f32]) -> Vec3 {
    Vec3::from_slice(&q[13..16])
}
fn normal(q: &[f32]) -> Vec3 {
    Vec3::from_slice(&q[16..19])
}

fn slide(
    mut position: Vec3,
    mut velocity: Vec3,
    dt: f32,
    crouched: bool,
    height: f32,
) -> (Vec3, Vec3) {
    let mut remaining = dt;
    for _ in 0..4 {
        let q = query(position, position + velocity * remaining, crouched, height);
        if q[19] != 0. || q[20] != 0. {
            return (position, Vec3::ZERO);
        }
        position = endpoint(&q);
        if q[12] >= 1. {
            break;
        }
        remaining *= 1. - q[12];
        let n = normal(&q);
        let into = velocity.dot(n);
        if into < 0. {
            velocity -= n * into;
        }
    }
    (position, velocity)
}

fn jump(config: &[f32], velocity: Vec3, forward: Vec3, ground: Vec3, strength: f32) -> Vec3 {
    let convert = |v: Vec3| [v.x / UNITS, v.z / UNITS, -v.y / UNITS, 0.];
    let v = convert(velocity);
    let n = [ground.x, ground.z, -ground.y, 0.];
    let settings = ground_jump::GroundJumpSettings {
        vertical_response: graph(config, 84),
        y_scalar_vs_normal_y: graph(config, 116),
        speed_scalar_vs_angle: graph(config, 132),
        minimum_height_vs_speed: graph(config, 148),
        maximum_height_vs_speed: graph(config, 164),
        speed_response_max_speed: config[75],
        minimum_scalar: config[76],
        maximum_y_bonus: config[77],
        adjust_z_factor: config[78],
        adjust_x_factor: config[79],
        absolute_minimum_height: config[80],
        hippy_minimum_height: config[81],
        hippy_maximum_height: config[82],
    };
    let mode = ground_jump::GroundJumpMode {
        minimum_height_64: config[72],
        minimum_height_68: config[73],
        maximum_height: config[74],
    };
    let f = [forward.x, 0., -forward.y, 0.];
    let result = ground_jump::calculate(
        ground_jump::GroundJumpInput {
            flags_2468: 0x0040_0000,
            flags_2480: 0,
            flags_2484: 0,
            flags_2488: 0,
            effective_forward: f,
            forward: f,
            current_velocity: v,
            prepared_velocity: v,
            ground_reference_position: [0.; 4],
            animation_com_position: [0.; 4],
            reference_up: [0., 1., 0., 0.],
            filtered_ground_normal: n,
            jump_strength: strength,
            jump_controls: [0.; 2],
            gravity_y: -config[0],
            surface_speed: velocity.length() / UNITS,
        },
        mode,
        &settings,
    );
    Vec3::new(result.velocity[0], -result.velocity[2], result.velocity[1]) * UNITS
}

fn advance(p: &mut api::Packet) {
    let config: [f32; api::CONFIG_WORDS] = p[api::CONFIG..].try_into().unwrap();
    let c = &config;
    let dt = p[api::DT];
    if !(0.001..=0.063).contains(&dt) || c[0] <= 0. || c[1] <= 0. || c[9] <= 0. || c[14] <= 0. {
        return;
    }
    let mut state: [f32; api::STATE_WORDS] = p[api::STATE..api::STATE + api::STATE_WORDS]
        .try_into()
        .unwrap();
    let reset = p[api::RESET] != 0.;
    let crouched = p[api::DUCK] != 0.;
    let mut pos = Vec3::from_slice(&p[api::POSITION..api::POSITION + 3]);
    let mut vel = Vec3::from_slice(&p[api::VELOCITY..api::VELOCITY + 3]);
    if reset {
        state = [0.; api::STATE_WORDS];
        state[0] = p[api::YAW];
        state[2] = 1.;
    }
    let ground_query = query(pos + Vec3::Z * 0.1, pos - Vec3::Z * 3., crouched, c[180]);
    let mut grounded = ground_query[12] < 1.
        && ground_query[19] == 0.
        && normal(&ground_query).z > 0.35
        && vel.z < 140.;
    let ground = if grounded {
        normal(&ground_query)
    } else {
        Vec3::Z
    };
    let speed = vel.truncate().length() / UNITS;
    let turn = p[api::SIDE];
    let mut push_scalar = state[2];
    let mut damped_turn = state[3];
    let tilt = steering::calculate_tilt(
        &steering::SteeringSettings {
            hard_turn_increase: c[12],
            damping: c[13],
            speed_graph_max_speed: c[14],
            push_scalar_increment: c[15],
            push_scalar_decrement: c[16],
            push_scalar_min: c[17],
            manual_scalar: c[18],
            general_scalar: c[11],
            tight_trucks_scalar: c[19],
            speed_graph: graph(c, 24),
            input_graph: graph(c, 40),
            tilt_blending: c[20],
        },
        steering::SteeringInput {
            turn,
            hard_turn: 0.,
            absolute_body_speed: speed,
            flipped_controls_scalar: 1.,
            balance: 0.,
            truck_tightness: 0.,
            pushing: p[api::FORWARD] > 0.,
        },
        Some(&mut push_scalar),
        Some(&mut damped_turn),
    );
    state[2] = push_scalar;
    state[3] = damped_turn;
    state[4] += (tilt - state[4]) * (dt * 12.).min(1.);
    let old_heading = state[0].to_radians();
    let mut heading = old_heading;
    if grounded {
        // The crossover uses a kinematic CS hull in place of Skate's seven-body
        // contact solver. Truck tilt drives curvature at the stock wheelbase.
        let steer_angle = (state[4] * 0.5).clamp(-0.6, 0.6);
        heading -= (speed / c[9]) * steer_angle.tan() * dt;
        let rotation = heading - old_heading;
        let (sn, cs) = rotation.sin_cos();
        vel = Vec3::new(cs * vel.x - sn * vel.y, sn * vel.x + cs * vel.y, vel.z);
    } else {
        heading -= turn * 2.4 * dt;
    }
    let forward = Vec3::new(heading.cos(), heading.sin(), 0.);
    state[0] = heading.to_degrees().rem_euclid(360.);
    if grounded {
        pos = endpoint(&ground_query);
        vel -= ground * vel.dot(ground);
        let direction = (forward - ground * forward.dot(ground)).normalize_or_zero();
        let meters = vel / UNITS;
        let along = meters.dot(direction);
        state[5] = (state[5] - dt).max(0.);
        if p[api::FORWARD] > 0. && state[5] == 0. {
            let acceleration = push::calculate_acceleration(
                push::PushInput {
                    flags_2468: 0x0200_0000,
                    flags_2472: 0,
                    target_speed: c[1],
                    current_speed: along,
                    absolute_body_speed: speed,
                    scale: 1.,
                    delta_seconds: dt,
                    direction: Vector3::new(direction.x, direction.z, -direction.y),
                },
                push::PushLimits {
                    maximum_pushable_speed: c[1],
                    low_speed_change: c[2],
                    high_speed_change: c[3],
                },
            );
            vel += Vec3::new(
                acceleration.vector.x,
                -acceleration.vector.z,
                acceleration.vector.y,
            ) * (dt * UNITS);
            state[5] = 0.32; // Keyboard push cadence, an explicit adapter policy.
        }
        if p[api::FORWARD] < 0. || p[api::BRAKE] > 0. {
            let force = braking::calculate_braking(
                braking::BrakeInput {
                    flags_2468: 0x4000_0000,
                    input_2728: 1.,
                    signed_speed: along,
                    absolute_body_speed: speed,
                    surface_factor: c[8],
                    direction: Vector3::new(direction.x, direction.z, -direction.y),
                },
                braking::BrakeSettings {
                    input_force: c[4],
                    override_force: c[5],
                    minimum_speed: c[6],
                },
            );
            let braking = Vec3::new(
                force.force_world.x,
                -force.force_world.z,
                force.force_world.y,
            ) / c[7].max(0.01)
                * (dt * UNITS);
            let next = vel + braking;
            vel = if next.dot(vel) < 0. { Vec3::ZERO } else { next };
        }
        // Conserve rolling momentum; apply the imported surface friction curve
        // and gravity along the actual BSP contact plane.
        vel += (Vec3::NEG_Z - ground * Vec3::NEG_Z.dot(ground)) * (c[0] * UNITS * dt);
        let friction = graph::<8>(c, 56).evaluate(speed).max(0.) * UNITS * dt;
        vel = vel.normalize_or_zero() * (vel.length() - friction).max(0.);
        if p[api::JUMP] != 0. && state[1] == 0. {
            vel = jump(c, vel, forward, ground, if crouched { 1. } else { 0.65 });
            grounded = false;
        }
    }
    state[1] = p[api::JUMP];
    if !grounded {
        vel.z -= c[0] * UNITS * dt;
    }
    let original = pos;
    let initial_velocity = vel;
    let (mut moved, mut clipped) = slide(pos, vel, dt, crouched, c[180]);
    // CS-map stairs and small curbs use the normal 18-inch stepping envelope.
    if grounded
        && (moved - pos).truncate().length_squared() + 0.01 < (vel * dt).truncate().length_squared()
    {
        let up = query(original, original + Vec3::Z * 18., crouched, c[180]);
        if up[19] == 0. && up[20] == 0. && up[12] == 1. {
            let (raised, raised_vel) = slide(endpoint(&up), initial_velocity, dt, crouched, c[180]);
            let down = query(raised, raised - Vec3::Z * 21., crouched, c[180]);
            if down[12] < 1.
                && normal(&down).z > 0.35
                && (endpoint(&down) - original).truncate().length_squared()
                    > (moved - original).truncate().length_squared()
            {
                moved = endpoint(&down);
                clipped = raised_vel;
                clipped -= normal(&down) * clipped.dot(normal(&down));
            }
        }
    }
    pos = moved;
    vel = clipped;
    let floor = query(
        pos + Vec3::Z * 0.1,
        pos - Vec3::Z * if grounded { 3. } else { 0.2 },
        crouched,
        c[180],
    );
    let on_ground = vel.z < 140. && floor[12] < 1. && floor[19] == 0. && normal(&floor).z > 0.35;
    if on_ground {
        pos = endpoint(&floor);
        let n = normal(&floor);
        vel -= n * vel.dot(n);
    }
    p[api::OUT_POSITION..api::OUT_POSITION + 3].copy_from_slice(&pos.to_array());
    p[api::OUT_VELOCITY..api::OUT_VELOCITY + 3].copy_from_slice(
        &vel.clamp(Vec3::splat(-3500.), Vec3::splat(3500.))
            .to_array(),
    );
    p[api::OUT_GROUNDED] = u8::from(on_ground) as f32;
    p[api::OUT_YAW] = state[0];
    p[api::OUT_PITCH] = if on_ground {
        -normal(&floor)
            .dot(forward)
            .atan2(normal(&floor).z)
            .to_degrees()
    } else {
        0.
    };
    p[api::OUT_ROLL] = -state[4].to_degrees().clamp(-30., 30.);
    p[api::OUT_HEIGHT] = c[180];
    p[api::OUT_STATE..api::OUT_STATE + api::STATE_WORDS].copy_from_slice(&state);
}
