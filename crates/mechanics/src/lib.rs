//! Sandboxed mechanics. No filesystem, network, clocks, native libraries or
//! WASI. A fresh instance per tick makes the explicit packet the entire state.
use glam::Vec3;
use mechanics_api as api;
use movement::TraceWorld;
use wasmi::{Caller, Config, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder};
pub mod external;
pub mod data;
pub use mechanics_link as link;
#[cfg(test)]
mod tests;

pub const MAX_MODULE_BYTES: usize = 2 * 1024 * 1024;
const FUEL: u64 = 500_000;
const MAX_QUERIES: u32 = 64;

/// A baked rigid mesh, using Source axes/inches relative to the board's centre.
/// Conversion happens outside the runtime; no GLB, scripts or external paths
/// are interpreted by the renderer.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisualMesh {
    pub parts: Vec<MeshPart>,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeshPart {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    pub color: [f32; 4],
}
impl VisualMesh {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > 8 * 1024 * 1024 {
            return Err("mechanics mesh exceeds 8 MiB".into());
        }
        let mesh: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        let mut vertices = 0;
        if mesh.parts.is_empty() || mesh.parts.len() > 32 {
            return Err("invalid mesh part count".into());
        }
        for p in &mesh.parts {
            vertices += p.positions.len();
            if vertices > 100_000
                || p.positions.len() != p.normals.len()
                || p.indices.len() > 600_000
                || p.indices.len() % 3 != 0
                || p.indices.iter().any(|i| *i as usize >= p.positions.len())
                || p.positions
                    .iter()
                    .flatten()
                    .any(|v| !v.is_finite() || v.abs() > 128.)
                || p.normals
                    .iter()
                    .flatten()
                    .any(|v| !v.is_finite() || v.abs() > 1.01)
                || p.color
                    .iter()
                    .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
            {
                return Err("invalid mechanics visual mesh".into());
            }
        }
        Ok(mesh)
    }
}

pub struct Program {
    engine: Engine,
    module: Module,
}

pub fn parse_config(bytes: &[u8]) -> Result<[f32; api::CONFIG_WORDS], String> {
    if bytes.len() > 16 * 1024 {
        return Err("mechanics config too large".into());
    }
    let values: Vec<f32> = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if values
        .iter()
        .any(|v| !v.is_finite() || v.abs() > 1_000_000.)
    {
        return Err("invalid mechanics config".into());
    }
    values
        .try_into()
        .map_err(|_| "mechanics config requires 256 numbers".into())
}

struct Host<'a> {
    world: &'a dyn TraceWorld,
    queries: u32,
    limits: StoreLimits,
}

impl Program {
    pub fn compile(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_MODULE_BYTES || !bytes.starts_with(b"\0asm\x01\0\0\0") {
            return Err("mechanics requires a bounded WebAssembly v1 binary".into());
        }
        let mut config = Config::default();
        config.consume_fuel(true);
        let engine = Engine::new(&config);
        let module = Module::new(&engine, bytes).map_err(|e| e.to_string())?;
        for import in module.imports() {
            if import.module() != "csr_v1" || import.name() != "trace" {
                return Err(format!(
                    "unsupported mechanics import: {}.{}",
                    import.module(),
                    import.name()
                ));
            }
        }
        let program = Self { engine, module };
        // Instantiation/ABI validation, without running the module's step.
        let empty = collision::CollisionWorld::new();
        program.instantiate(&empty)?;
        Ok(program)
    }

    fn instantiate<'a>(
        &self,
        world: &'a dyn TraceWorld,
    ) -> Result<(Store<Host<'a>>, wasmi::Instance, usize), String> {
        let limits = StoreLimitsBuilder::new()
            .memory_size(4 * 1024 * 1024)
            .memories(1)
            .tables(1)
            .table_elements(4096)
            .instances(1)
            .trap_on_grow_failure(true)
            .build();
        let mut store = Store::new(
            &self.engine,
            Host {
                world,
                queries: 0,
                limits,
            },
        );
        store.limiter(|host| &mut host.limits);
        store.set_fuel(FUEL).map_err(|e| e.to_string())?;
        let mut linker = Linker::new(&self.engine);
        linker
            .func_wrap("csr_v1", "trace", trace)
            .map_err(|e| e.to_string())?;
        let instance = linker
            .instantiate(&mut store, &self.module)
            .map_err(|e| e.to_string())?
            .ensure_no_start(&mut store)
            .map_err(|e| e.to_string())?;
        let pointer = instance
            .get_typed_func::<(), i32>(&store, "csr_packet")
            .map_err(|e| e.to_string())?
            .call(&mut store, ())
            .map_err(|e| e.to_string())?;
        let pointer = usize::try_from(pointer).map_err(|_| "negative mechanics packet pointer")?;
        instance
            .get_typed_func::<(), i32>(&store, "csr_step")
            .map_err(|e| e.to_string())?;
        let memory = instance
            .get_memory(&store, "memory")
            .ok_or("mechanics memory missing")?;
        let mut bytes = [0; api::BYTES];
        memory
            .read(&store, pointer, &mut bytes)
            .map_err(|e| e.to_string())?;
        Ok((store, instance, pointer))
    }

    pub fn step(&self, input: &api::Packet, world: &dyn TraceWorld) -> Result<api::Packet, String> {
        let (mut store, instance, pointer) = self.instantiate(world)?;
        let memory = instance
            .get_memory(&store, "memory")
            .ok_or("mechanics memory missing")?;
        memory
            .write(&mut store, pointer, &api::encode(input))
            .map_err(|e| e.to_string())?;
        let status = instance
            .get_typed_func::<(), i32>(&store, "csr_step")
            .map_err(|e| e.to_string())?
            .call(&mut store, ())
            .map_err(|e| e.to_string())?;
        if status != 0 {
            return Err(format!("mechanics step failed ({status})"));
        }
        let mut bytes = [0; api::BYTES];
        memory
            .read(&store, pointer, &mut bytes)
            .map_err(|e| e.to_string())?;
        let output = api::decode(&bytes);
        if output[api::OUT_POSITION..api::OUT_STATE + api::STATE_WORDS]
            .iter()
            .any(|v| !v.is_finite())
        {
            return Err("mechanics returned a non-finite state".into());
        }
        let vector = |at| Vec3::from_slice(&output[at..at + 3]);
        if vector(api::OUT_VELOCITY).abs().max_element() > 3500.
            || output[api::OUT_YAW..=api::OUT_ROLL]
                .iter()
                .any(|v| v.abs() > 360_000.)
            || !matches!(output[api::OUT_GROUNDED], 0.0 | 1.0)
            || vector(api::OUT_POSITION).abs().max_element() > 1_000_000.
            || !(0.0..=16.0).contains(&output[api::OUT_HEIGHT])
            || (vector(api::OUT_POSITION)
                - Vec3::from_slice(&input[api::POSITION..api::POSITION + 3]))
            .length()
                > 3500. * input[api::DT] + 20.
        {
            return Err("mechanics returned an out-of-bounds movement".into());
        }
        Ok(output)
    }
}

fn trace(mut caller: Caller<'_, Host<'_>>, pointer: i32) -> Result<i32, wasmi::Error> {
    if caller.data().queries >= MAX_QUERIES {
        return Err(wasmi::Error::new("mechanics trace budget exceeded"));
    }
    caller.data_mut().queries += 1;
    let memory = caller
        .get_export("memory")
        .and_then(|e| e.into_memory())
        .ok_or_else(|| wasmi::Error::new("missing trace memory"))?;
    let pointer =
        usize::try_from(pointer).map_err(|_| wasmi::Error::new("negative trace pointer"))?;
    let mut bytes = [0u8; api::TRACE_WORDS * 4];
    memory
        .read(&caller, pointer, &mut bytes)
        .map_err(|e| wasmi::Error::new(e.to_string()))?;
    let mut values = [0.; api::TRACE_WORDS];
    for (v, b) in values.iter_mut().zip(bytes.chunks_exact(4)) {
        *v = f32::from_le_bytes(b.try_into().unwrap());
    }
    if values[..12]
        .iter()
        .any(|v| !v.is_finite() || v.abs() > 1_000_000.)
    {
        return Err(wasmi::Error::new("invalid trace input"));
    }
    let v = |at| Vec3::from_slice(&values[at..at + 3]);
    if (v(3) - v(0)).abs().max_element() > 1024. {
        return Err(wasmi::Error::new(
            "mechanics trace exceeds query distance budget",
        ));
    }
    if v(6).cmpgt(v(9)).any() || (v(9) - v(6)).max_element() > 256. {
        return Err(wasmi::Error::new("invalid trace hull"));
    }
    let hit = caller.data().world.trace_hull(
        v(0),
        v(3),
        v(6),
        v(9),
        collision::contents::MASK_PLAYERSOLID,
    );
    values[12] = hit.fraction;
    values[13..16].copy_from_slice(&hit.end.to_array());
    values[16..19].copy_from_slice(&hit.normal.to_array());
    values[19] = u8::from(hit.start_solid) as f32;
    values[20] = u8::from(hit.all_solid) as f32;
    for (b, v) in bytes.chunks_exact_mut(4).zip(values) {
        b.copy_from_slice(&v.to_le_bytes());
    }
    memory
        .write(&mut caller, pointer, &bytes)
        .map_err(|e| wasmi::Error::new(e.to_string()))?;
    Ok(0)
}
