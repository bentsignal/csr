use super::*;
use mechanics_api as api;

fn module(body: &str) -> Vec<u8> {
    wat::parse_str(format!(
        r#"(module (memory (export "memory") 1 64)
        (func (export "csr_packet") (result i32) i32.const 0)
        (func (export "csr_step") (result i32) {body} i32.const 0))"#
    ))
    .unwrap()
}
fn packet() -> api::Packet {
    let mut p = [0.; api::WORDS];
    p[api::API] = api::VERSION;
    p[api::DT] = 1. / 128.;
    p
}

#[test]
fn fuel_pointer_import_start_and_memory_budgets_are_enforced() {
    let world = collision::CollisionWorld::new();
    let infinite = Program::compile(&module("(loop $forever br $forever) ")).unwrap();
    assert!(
        infinite
            .step(&packet(), &world)
            .unwrap_err()
            .contains("fuel")
    );
    let bad_pointer = wat::parse_str(
        r#"(module (memory (export "memory") 1)
        (func (export "csr_packet") (result i32) i32.const -1)
        (func (export "csr_step") (result i32) i32.const 0))"#,
    )
    .unwrap();
    assert!(Program::compile(&bad_pointer).is_err());
    let import =
        wat::parse_str(r#"(module (import "wasi_snapshot_preview1" "fd_write" (func)))"#).unwrap();
    assert!(Program::compile(&import).is_err());
    let start = wat::parse_str(r#"(module (func $start) (start $start))"#).unwrap();
    assert!(Program::compile(&start).is_err());
    let memory = wat::parse_str(
        r#"(module (memory (export "memory") 65)
        (func (export "csr_packet") (result i32) i32.const 0)
        (func (export "csr_step") (result i32) i32.const 0))"#,
    )
    .unwrap();
    assert!(Program::compile(&memory).is_err());
}

#[test]
fn hidden_memory_resets_and_only_packet_state_survives_replay() {
    let wasm = module(
        r#"
        i32.const 256 i32.const 8 i32.const 24 memory.copy
        i32.const 320 i32.const 80 i32.const 128 memory.copy
        i32.const 320 i32.const 80 f32.load f32.const 1 f32.add f32.store
        i32.const 2048 i32.const 2048 f32.load f32.const 1 f32.add f32.store
        i32.const 324 i32.const 2048 f32.load f32.store
    "#,
    );
    let program = Program::compile(&wasm).unwrap();
    let world = collision::CollisionWorld::new();
    let mut input = packet();
    for tick in 1..10 {
        let out = program.step(&input, &world).unwrap();
        assert_eq!(out[api::OUT_STATE], tick as f32);
        assert_eq!(out[api::OUT_STATE + 1], 1.);
        assert_eq!(program.step(&input, &world).unwrap(), out);
        input[api::STATE..api::STATE + api::STATE_WORDS]
            .copy_from_slice(&out[api::OUT_STATE..api::OUT_STATE + api::STATE_WORDS]);
    }
}

#[test]
fn nonfinite_and_teleport_results_are_rejected() {
    let world = collision::CollisionWorld::new();
    for value in ["nan", "inf", "100000"] {
        let wasm = module(&format!("i32.const 256 f32.const {value} f32.store"));
        assert!(
            Program::compile(&wasm)
                .unwrap()
                .step(&packet(), &world)
                .is_err()
        );
    }
}

#[test]
fn collision_queries_are_bounded_and_use_the_host_world() {
    let wasm = wat::parse_str(
        r#"(module
        (import "csr_v1" "trace" (func $trace (param i32) (result i32)))
        (memory (export "memory") 1)
        (func (export "csr_packet") (result i32) i32.const 0)
        (func (export "csr_step") (result i32)
            (local $n i32)
            (loop $again
                i32.const 2048 call $trace drop
                local.get $n i32.const 1 i32.add local.tee $n
                i32.const 65 i32.lt_s br_if $again)
            i32.const 0))"#,
    )
    .unwrap();
    let program = Program::compile(&wasm).unwrap();
    assert!(
        program
            .step(&packet(), &collision::CollisionWorld::new())
            .unwrap_err()
            .contains("trace budget")
    );
}
