use super::*;

#[test]
fn first_spawn_jump_and_landing_replay_through_complete_tick_input_at_both_rates() {
    for rate in [64, 128] {
        let mut authority = world();
        authority.set_tick_rate(rate);
        authority.set_mode(GameMode::Deathmatch, rules());
        let actor = authority.add_player("first-command mover", Team::CounterTerrorist);
        assert!(!authority.player(actor).unwrap().movement.on_ground);
        // Restoration replicates state, not the installed map's spawn/zone
        // definitions. Both sides must retain the same map-owned fixture.
        let mut predicted = world();
        predicted.restore(authority.state());
        let mut saw_landing = false;
        for tick in 0..rate * 2 {
            let t = tick as f32 / rate as f32;
            let buttons = if tick == 0 {
                buttons::JUMP
            } else if (0.25..0.375).contains(&t) {
                buttons::DUCK
            } else {
                0
            };
            let input = TickInput {
                view_angles: Vec3::new(0., -30. * t, 0.),
                forward_move: if t < 1. { 200. } else { 0. },
                side_move: if (0.125..0.5).contains(&t) { 450. } else { 0. },
                buttons,
                ..Default::default()
            };
            authority.step(&[(actor, input)]);
            predicted.step_prediction(&[(actor, input)]);
            assert_eq!(
                authority.state(),
                predicted.state(),
                "replay diverged at {rate}Hz tick{tick}"
            );
            assert!(predicted.events.is_empty());
            if tick == 0 {
                assert!(
                    authority
                        .events
                        .iter()
                        .any(|e| matches!(e, GameEvent::Jump { player } if *player == actor))
                );
            }
            saw_landing |= authority.events.iter().any(|e| matches!(e, GameEvent::Land { player, speed } if *player == actor && *speed > 0.));
            // Exercise restoration mid-flight/duck transition, not just replay
            // from a fresh world or a hand-forced grounded movement state.
            if tick == rate / 4 {
                predicted.restore(authority.state());
            }
        }
        assert!(saw_landing);
        assert!(authority.player(actor).unwrap().movement.on_ground);
    }
}
