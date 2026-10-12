//! Minimal in-memory ZIP reader for BSP pakfile lumps.
//!
//! Map pakfiles are ordinary ZIPs. Most entries are stored; CS:GO also uses
//! LZMA (method 14) and occasionally deflate (method 8).

use std::collections::HashMap;
use std::io::{self, Write};

// A BSP can carry another ZIP inside a validated pack. Apply budgets here too
// so a small outer archive cannot hide an unbounded inner decompression.
const MAX_ENTRY_BYTES: u64 = 256 << 20;
const MAX_EXPANDED_BYTES: u64 = 512 << 20;

const EOCD_SIG: u32 = 0x0605_4B50;
const CENTRAL_SIG: u32 = 0x0201_4B50;
const LOCAL_SIG: u32 = 0x0403_4B50;

#[derive(Clone, Debug)]
struct ZipEntry {
    method: u16,
    compressed_size: u32,
    uncompressed_size: u32,
    local_header_offset: u32,
}

pub struct ZipArchive {
    data: Vec<u8>,
    entries: HashMap<String, ZipEntry>,
}

impl std::fmt::Debug for ZipArchive {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ZipArchive")
            .field("entries", &self.entries.len())
            .finish()
    }
}

fn invalid(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.into())
}

fn u16_at(d: &[u8], o: usize) -> io::Result<u16> {
    d.get(o..o + 2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .ok_or_else(|| invalid("truncated zip"))
}

fn u32_at(d: &[u8], o: usize) -> io::Result<u32> {
    d.get(o..o + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or_else(|| invalid("truncated zip"))
}

impl ZipArchive {
    pub fn from_bytes(data: Vec<u8>) -> io::Result<Self> {
        if data.is_empty() {
            return Ok(Self {
                data,
                entries: HashMap::new(),
            });
        }
        // The end-of-central-directory record sits in the last 64 KiB + 22 bytes.
        let min = data.len().saturating_sub(0xFFFF + 22);
        let mut eocd = None;
        let mut i = data.len().saturating_sub(22);
        loop {
            if u32_at(&data, i).ok() == Some(EOCD_SIG) {
                eocd = Some(i);
                break;
            }
            if i == min {
                break;
            }
            i -= 1;
        }
        let eocd = eocd.ok_or_else(|| invalid("zip end record not found"))?;
        let count = u16_at(&data, eocd + 10)? as usize;
        let cd_offset = u32_at(&data, eocd + 16)? as usize;

        let mut entries = HashMap::with_capacity(count);
        let mut expanded = 0u64;
        let mut p = cd_offset;
        for _ in 0..count {
            if u32_at(&data, p)? != CENTRAL_SIG {
                return Err(invalid("bad zip central directory entry"));
            }
            let method = u16_at(&data, p + 10)?;
            let compressed_size = u32_at(&data, p + 20)?;
            let uncompressed_size = u32_at(&data, p + 24)?;
            expanded = expanded.checked_add(uncompressed_size as u64).ok_or_else(|| invalid("zip expanded size overflow"))?;
            if uncompressed_size as u64 > MAX_ENTRY_BYTES || expanded > MAX_EXPANDED_BYTES {
                return Err(invalid("zip expanded size limit exceeded"));
            }
            let name_len = u16_at(&data, p + 28)? as usize;
            let extra_len = u16_at(&data, p + 30)? as usize;
            let comment_len = u16_at(&data, p + 32)? as usize;
            let local_header_offset = u32_at(&data, p + 42)?;
            let name = data
                .get(p + 46..p + 46 + name_len)
                .ok_or_else(|| invalid("truncated zip name"))?;
            let name = String::from_utf8_lossy(name);
            if !name.ends_with('/') {
                entries.insert(
                    vpk::normalize(&name),
                    ZipEntry {
                        method,
                        compressed_size,
                        uncompressed_size,
                        local_header_offset,
                    },
                );
            }
            p += 46 + name_len + extra_len + comment_len;
        }
        Ok(Self { data, entries })
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn contains(&self, path: &str) -> bool {
        self.entries.contains_key(&vpk::normalize(path))
    }

    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }

    pub fn read(&self, path: &str) -> io::Result<Option<Vec<u8>>> {
        let Some(e) = self.entries.get(&vpk::normalize(path)) else {
            return Ok(None);
        };
        let lh = e.local_header_offset as usize;
        if u32_at(&self.data, lh)? != LOCAL_SIG {
            return Err(invalid("bad zip local header"));
        }
        let name_len = u16_at(&self.data, lh + 26)? as usize;
        let extra_len = u16_at(&self.data, lh + 28)? as usize;
        let start = lh + 30 + name_len + extra_len;
        let raw = self
            .data
            .get(start..start + e.compressed_size as usize)
            .ok_or_else(|| invalid("zip entry runs past end"))?;
        let out = match e.method {
            0 => raw.to_vec(),
            8 => miniz_oxide::inflate::decompress_to_vec_with_limit(raw, e.uncompressed_size as usize)
                .map_err(|err| invalid(format!("deflate: {err:?}")))?,
            14 => {
                // ZIP LZMA: u8 major, u8 minor, u16 props size, props, raw stream.
                let props_len = u16_at(raw, 2)? as usize;
                let stream = raw
                    .get(4..)
                    .ok_or_else(|| invalid("truncated zip lzma header"))?;
                if props_len != 5 || stream.len() < 5 {
                    return Err(invalid("unexpected zip lzma props"));
                }
                decode_lzma_raw(stream, e.uncompressed_size as u64)?
            }
            m => return Err(invalid(format!("unsupported zip method {m}"))),
        };
        if out.len() != e.uncompressed_size as usize { return Err(invalid("zip expanded size mismatch")); }
        Ok(Some(out))
    }
}

/// Decodes `props(5) + stream` with a known output size.
pub fn decode_lzma_raw(props_and_stream: &[u8], size: u64) -> io::Result<Vec<u8>> {
    if size > MAX_ENTRY_BYTES { return Err(invalid("lzma expanded size limit exceeded")); }
    let mut input = io::BufReader::new(props_and_stream);
    struct Bounded { data: Vec<u8>, limit: usize }
    impl Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.len() > self.limit.saturating_sub(self.data.len()) { return Err(invalid("lzma expanded size mismatch")); }
            self.data.extend_from_slice(bytes); Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> { Ok(()) }
    }
    let mut out = Bounded { data: Vec::new(), limit: size as usize };
    let opts = lzma_rs::decompress::Options {
        unpacked_size: lzma_rs::decompress::UnpackedSize::UseProvided(Some(size)),
        ..Default::default()
    };
    lzma_rs::lzma_decompress_with_options(&mut input, &mut out, &opts)
        .map_err(|e| invalid(format!("lzma: {e:?}")))?;
    if out.data.len() != size as usize { return Err(invalid("lzma expanded size mismatch")); }
    Ok(out.data)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a stored-only zip in memory.
    pub fn build_stored(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for (name, data) in files {
            let offset = out.len() as u32;
            out.extend_from_slice(&LOCAL_SIG.to_le_bytes());
            out.extend_from_slice(&[20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            out.extend_from_slice(&0u32.to_le_bytes()); // crc (unchecked)
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(data);

            central.extend_from_slice(&CENTRAL_SIG.to_le_bytes());
            central.extend_from_slice(&[20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            central.extend_from_slice(&0u32.to_le_bytes());
            central.extend_from_slice(&(data.len() as u32).to_le_bytes());
            central.extend_from_slice(&(data.len() as u32).to_le_bytes());
            central.extend_from_slice(&(name.len() as u16).to_le_bytes());
            central.extend_from_slice(&[0; 12]);
            central.extend_from_slice(&offset.to_le_bytes());
            central.extend_from_slice(name.as_bytes());
        }
        let cd_offset = out.len() as u32;
        out.extend_from_slice(&central);
        out.extend_from_slice(&EOCD_SIG.to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(central.len() as u32).to_le_bytes());
        out.extend_from_slice(&cd_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    #[test]
    fn stored_entries() {
        let zip = build_stored(&[("materials/maps/de_test/a.vmt", b"patch {}"), ("b.txt", b"hi")]);
        let z = ZipArchive::from_bytes(zip).unwrap();
        assert_eq!(z.len(), 2);
        assert_eq!(z.read("MATERIALS/maps/de_test/A.vmt").unwrap().unwrap(), b"patch {}");
        assert!(z.read("nope").unwrap().is_none());
    }

    #[test]
    fn nested_archives_reject_oversized_and_dishonest_expansion() {
        let mut bytes = build_stored(&[("x", b"test")]);
        let central = bytes.windows(4).position(|w| w == CENTRAL_SIG.to_le_bytes()).unwrap();
        bytes[central + 24..central + 28].copy_from_slice(&((MAX_ENTRY_BYTES + 1) as u32).to_le_bytes());
        assert!(ZipArchive::from_bytes(bytes).is_err());
        let mut bytes = build_stored(&[("x", b"test")]);
        let central = bytes.windows(4).position(|w| w == CENTRAL_SIG.to_le_bytes()).unwrap();
        bytes[central + 24..central + 28].copy_from_slice(&1u32.to_le_bytes());
        assert!(ZipArchive::from_bytes(bytes).unwrap().read("x").is_err());
        let compressed = miniz_oxide::deflate::compress_to_vec(&[0; 4096], 6);
        let mut bytes = build_stored(&[("x", &compressed)]);
        let central = bytes.windows(4).position(|w| w == CENTRAL_SIG.to_le_bytes()).unwrap();
        bytes[central + 10..central + 12].copy_from_slice(&8u16.to_le_bytes());
        bytes[central + 24..central + 28].copy_from_slice(&16u32.to_le_bytes());
        assert!(ZipArchive::from_bytes(bytes).unwrap().read("x").is_err());
        assert!(decode_lzma_raw(&[], MAX_ENTRY_BYTES + 1).is_err());
    }
}
