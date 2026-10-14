//! Bounded readers for installed PCF (DMX binary 5) and VTF particle sheets.
//! Format-only clean-room implementation; no engine implementation is used.
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub enum Value {
    Number(f32),
    Text(String),
    Vector([f32; 3]),
    Color([u8; 4]),
    Ref(usize),
    Array(Vec<Value>),
    Other,
}
#[derive(Debug, Default)]
pub struct Element {
    pub kind: String,
    pub name: String,
    pub attrs: HashMap<String, Value>,
}
impl Element {
    pub fn number(&self, name: &str, fallback: f32) -> f32 {
        match self.attrs.get(&name.to_ascii_lowercase()) {
            Some(Value::Number(v)) => *v,
            _ => fallback,
        }
    }
    pub fn text(&self, name: &str) -> &str {
        match self.attrs.get(&name.to_ascii_lowercase()) {
            Some(Value::Text(v)) => v,
            _ => "",
        }
    }
    pub fn vector(&self, name: &str, fallback: [f32; 3]) -> [f32; 3] {
        match self.attrs.get(&name.to_ascii_lowercase()) {
            Some(Value::Vector(v)) => *v,
            _ => fallback,
        }
    }
    pub fn color(&self, name: &str, fallback: [u8; 4]) -> [u8; 4] {
        match self.attrs.get(&name.to_ascii_lowercase()) {
            Some(Value::Color(v)) => *v,
            _ => fallback,
        }
    }
    pub fn refs<'a>(&self, name: &str, all: &'a [Element]) -> Vec<&'a Element> {
        match self.attrs.get(&name.to_ascii_lowercase()) {
            Some(Value::Array(v)) => v
                .iter()
                .filter_map(|v| {
                    if let Value::Ref(i) = v {
                        all.get(*i)
                    } else {
                        None
                    }
                })
                .collect(),
            _ => Vec::new(),
        }
    }
}
struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.at.checked_add(n)?;
        let v = self.data.get(self.at..end)?;
        self.at = end;
        Some(v)
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn f32(&mut self) -> Option<f32> {
        let v = f32::from_bits(self.u32()?);
        v.is_finite().then_some(v)
    }
    fn count(&mut self, max: usize) -> Option<usize> {
        let n = self.u32()? as usize;
        (n <= max).then_some(n)
    }
    fn string(&mut self) -> Option<String> {
        let n = self.data.get(self.at..)?.iter().position(|v| *v == 0)?;
        if n > 65536 {
            return None;
        };
        let v = String::from_utf8_lossy(self.take(n)?).into_owned();
        self.take(1)?;
        Some(v)
    }
    fn indexed(&mut self, strings: &[String]) -> Option<String> {
        strings.get(self.u32()? as usize).cloned()
    }
    fn value(&mut self, ty: u8, strings: &[String], array: bool) -> Option<Value> {
        Some(match ty {
            1 => {
                let i = self.u32()?;
                if i == u32::MAX {
                    Value::Other
                } else if i == u32::MAX - 1 {
                    self.string()?;
                    Value::Other
                } else {
                    Value::Ref(i as usize)
                }
            }
            2 => Value::Number(self.u32()? as i32 as f32),
            3 => Value::Number(self.f32()?),
            4 => Value::Number((self.take(1)?[0] != 0) as u8 as f32),
            5 => Value::Text(if array {
                self.string()?
            } else {
                self.indexed(strings)?
            }),
            6 => {
                let n = self.count(16 * 1024 * 1024)?;
                self.take(n)?;
                Value::Other
            }
            7 => {
                self.take(4)?;
                Value::Other
            }
            8 => Value::Color(self.take(4)?.try_into().ok()?),
            10 | 12 => Value::Vector([self.f32()?, self.f32()?, self.f32()?]),
            9 => {
                self.take(8)?;
                Value::Other
            }
            11 | 13 => {
                self.take(16)?;
                Value::Other
            }
            14 => {
                self.take(64)?;
                Value::Other
            }
            _ => return None,
        })
    }
}
pub fn pcf(data: &[u8]) -> Option<Vec<Element>> {
    let mut r = Reader { data, at: 0 };
    if r.string()?.trim() != "<!-- dmx encoding binary 5 format pcf 2 -->" {
        return None;
    }
    let count = r.count(100_000)?;
    let mut strings = Vec::with_capacity(count);
    for _ in 0..count {
        strings.push(r.string()?);
    }
    let count = r.count(100_000)?;
    let mut elements = Vec::with_capacity(count);
    for _ in 0..count {
        let kind = r.indexed(&strings)?;
        let name = r.indexed(&strings)?;
        r.take(16)?;
        elements.push(Element {
            kind,
            name,
            attrs: HashMap::new(),
        });
    }
    for e in &mut elements {
        for _ in 0..r.count(4096)? {
            let key = r.indexed(&strings)?.to_ascii_lowercase();
            let ty = r.take(1)?[0];
            let value = if ty > 14 {
                let n = r.count(100_000)?;
                let mut a = Vec::with_capacity(n);
                for _ in 0..n {
                    a.push(r.value(ty - 14, &strings, true)?)
                }
                Value::Array(a)
            } else {
                r.value(ty, &strings, false)?
            };
            e.attrs.insert(key, value);
        }
    }
    Some(elements)
}

#[derive(Clone, Debug)]
pub struct Sheet {
    pub frames: Vec<[f32; 4]>,
    pub durations: Vec<f32>,
    pub duration: f32,
    pub clamp: bool,
}
pub fn sheets(vtf: &[u8]) -> Option<HashMap<usize, Sheet>> {
    if vtf.get(..4)? != b"VTF\0" || vtf.len() < 80 {
        return None;
    }
    let mut dir = Reader { data: vtf, at: 68 };
    let count = dir.count(128)?;
    for i in 0..count {
        let mut d = Reader {
            data: vtf,
            at: 80 + i * 8,
        };
        let tag = d.take(4)?;
        let offset = d.u32()? as usize;
        if tag[..3] != [0x10, 0, 0] {
            continue;
        }
        if tag[3] & 2 != 0 {
            return None;
        }
        let mut r = Reader {
            data: vtf,
            at: offset,
        };
        let len = r.count(16 * 1024 * 1024)?;
        let bytes = r.take(len)?;
        let mut r = Reader { data: bytes, at: 0 };
        let version = r.u32()?;
        if version > 1 {
            return None;
        }
        let count = r.count(64)?;
        let mut result = HashMap::new();
        for _ in 0..count {
            let id = r.u32()? as usize;
            let clamp = r.u32()? != 0;
            let count = r.count(4096)?;
            let duration = r.f32()?;
            let mut frames = Vec::new();
            let mut durations = Vec::new();
            for _ in 0..count {
                durations.push(r.f32()?);
                let uv = [r.f32()?, r.f32()?, r.f32()?, r.f32()?];
                if uv.iter().any(|v| !(0.0..=1.0).contains(v)) {
                    return None;
                };
                frames.push(uv);
                if version == 1 {
                    r.take(48)?;
                }
            }
            if frames.is_empty() || duration <= 0.0 {
                return None;
            };
            result.insert(
                id,
                Sheet {
                    frames,
                    durations,
                    duration,
                    clamp,
                },
            );
        }
        return Some(result);
    }
    Some(HashMap::new())
}
impl Sheet {
    pub fn frame(&self, age: f32, life: f32, rate: f32, fps: bool) -> usize {
        let total = if fps {
            age * rate
        } else {
            age / life.max(0.001) * rate * self.duration
        };
        let mut t = if self.clamp {
            total.clamp(0.0, self.duration)
        } else {
            total.rem_euclid(self.duration)
        };
        for (i, d) in self.durations.iter().enumerate() {
            if t < *d {
                return i;
            }
            t -= *d;
        }
        self.frames.len() - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_assets_fail_closed() {
        for len in 0..128 {
            assert!(pcf(&vec![0; len]).is_none());
            assert!(sheets(&vec![0; len]).is_none());
        }
    }
    #[test]
    fn sequence_uses_authored_durations_and_clamp() {
        let mut s = Sheet {
            frames: vec![[0.; 4]; 3],
            durations: vec![1., 2., 1.],
            duration: 4.,
            clamp: true,
        };
        assert_eq!(s.frame(1.5, 1., 1., true), 1);
        assert_eq!(s.frame(20., 1., 1., true), 2);
        s.clamp = false;
        assert_eq!(s.frame(4.1, 1., 1., true), 0);
    }
}
