// SPDX-License-Identifier: GPL-3.0-only
//! Native two-truck grain synthesis, using the owner's complete recordings.
use skate_audio::{
    BLOCK,
    grain::{
        bed::GrainBed,
        board::{position, speed_fraction},
        player::{GrainParams, GrainSource, Record},
    },
    mixer::Pcm,
};
use std::{collections::BTreeMap, path::Path, sync::Arc};
struct Surface {
    source: Arc<GrainSource>,
    params: [GrainParams; 2],
    curve: [f32; 4],
    max_kmh: f32,
}
pub struct Bank(BTreeMap<String, Surface>);
impl Bank {
    pub fn load(root: &Path) -> Result<Option<Arc<Self>>, String> {
        let root = root.join("private/crossover-audio");
        if !root.join("grains.json").is_file() {
            return Ok(None);
        }
        let json: serde_json::Value = serde_json::from_slice(
            &std::fs::read(root.join("grains.json")).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let mut bank = BTreeMap::new();
        for name in [
            "concrete_smooth_hard",
            "concrete_rough_hard",
            "asphalt_rough_hard",
            "wood_ramp_hard",
            "metal_smooth_hard",
        ] {
            let t = &json[name];
            let array = |v: &serde_json::Value| -> Result<Vec<f32>, String> {
                v.as_array()
                    .ok_or("invalid grain tuning")?
                    .iter()
                    .map(|v| {
                        v.as_f64()
                            .map(|v| v as f32)
                            .filter(|v| v.is_finite())
                            .ok_or("invalid grain number".into())
                    })
                    .collect()
            };
            let params = [
                GrainParams::from_slice(&array(&t["params"][0])?).ok_or("grain parameters")?,
                GrainParams::from_slice(&array(&t["params"][1])?).ok_or("grain parameters")?,
            ];
            if params.iter().any(|p| {
                [p.attack, p.sustain, p.release]
                    .iter()
                    .any(|v| !(0.001..=2.).contains(v))
                    || p.window < p.length()
                    || p.window > 16.
                    || !(0.001..=1.).contains(&p.drift)
            }) {
                return Err("out-of-range grain parameters".into());
            }
            let curve = array(&t["bezier"])?;
            let curve: [f32; 4] = curve.try_into().map_err(|_| "grain curve")?;
            let mut wav = hound::WavReader::open(root.join(format!("{name}.wav")))
                .map_err(|e| e.to_string())?;
            let spec = wav.spec();
            if spec.channels != 1
                || spec.bits_per_sample != 16
                || !(8000..=192000).contains(&spec.sample_rate)
            {
                return Err("unsupported grain PCM".into());
            }
            let samples = wav
                .samples::<i16>()
                .map(|s| s.map(|v| v as f32 / 32768.).map_err(|e| e.to_string()))
                .collect::<Result<Vec<_>, _>>()?;
            let duration = samples.len() as f32 / spec.sample_rate as f32;
            let max_kmh = t["max_kmh"].as_f64().ok_or("grain speed")? as f32;
            if !(1.0..=300.).contains(&max_kmh)
                || curve.iter().any(|v| v.abs() > 2.)
                || params.iter().any(|p| duration < p.window + p.length())
            {
                return Err("invalid grain recording/tuning".into());
            }
            bank.insert(
                name.into(),
                Surface {
                    source: Arc::new(GrainSource {
                        name: name.into(),
                        duration,
                        pcm: Arc::new(Pcm {
                            rate: spec.sample_rate,
                            channels: vec![samples],
                        }),
                    }),
                    params,
                    curve,
                    max_kmh,
                },
            );
        }
        Ok(Some(Arc::new(Self(bank))))
    }
}
pub struct Rolling {
    bank: Option<Arc<Bank>>,
    bed: GrainBed,
    current: &'static str,
    clock: f64,
    output: Vec<f32>,
}
impl Rolling {
    pub fn bank(&self) -> Option<Arc<Bank>> {
        self.bank.clone()
    }
    pub fn new(bank: Option<Arc<Bank>>) -> Self {
        Self {
            bank,
            bed: GrainBed::new(),
            current: "",
            clock: 0.,
            output: vec![],
        }
    }
    pub fn advance(&mut self, dt: f32, material: u8, grounded: bool, speed: f32) {
        let Some(bank) = &self.bank else {
            return;
        };
        let name = match material {
            b'W' => "wood_ramp_hard",
            b'M' | b'V' | b'G' => "metal_smooth_hard",
            b'R' | b'D' | b'T' => "concrete_rough_hard",
            b'A' => "asphalt_rough_hard",
            _ => "concrete_smooth_hard",
        };
        let t = &bank.0[name];
        let gain = if grounded {
            (speed / 3.).clamp(0., 1.) * 0.18
        } else {
            0.
        };
        let pos = position(&t.curve, speed_fraction(speed, t.max_kmh));
        for truck in 0..2 {
            let records = [
                Record {
                    gain,
                    pitch: 1.,
                    position: pos,
                },
                Record {
                    gain: gain * 0.65,
                    pitch: 1.,
                    position: pos - 0.1,
                },
            ];
            if self.current != name {
                self.bed
                    .bind_truck(truck, t.source.clone(), t.params, records);
            } else {
                self.bed.set_records(truck, records);
            }
        }
        self.current = name;
        self.clock += f64::from(dt) * 48000.;
        while self.clock >= BLOCK as f64 {
            self.clock -= BLOCK as f64;
            self.bed.render_block();
            let mut bus = [[0.; BLOCK]; 6];
            self.bed.add_to(&mut bus);
            self.output.extend(
                bus[0]
                    .iter()
                    .zip(&bus[1])
                    .map(|(a, b)| ((a + b) * 0.5).clamp(-1., 1.)),
            );
        }
        if self.output.len() > 24000 {
            self.output.drain(..self.output.len() - 12000);
        }
    }
    pub fn take(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.output)
    }
}
