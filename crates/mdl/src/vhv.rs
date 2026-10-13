//! Bounded, borrowed VHV v2 metadata from installed Source1 map pakfiles.
//!
//! The observed header is 40 bytes followed by 28-byte mesh descriptors.
//! Dust2 stores opaque 12-byte records. No color, directional basis, transfer
//! function or payload-to-vertex ordering is inferred here. This is an additive
//! data seam; the renderer does not consume it.

use crate::{HardwareMesh, Model};
use content::{ContentId, ContentKind};
use std::io;

pub const MAX_FILE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_MESHES: usize = 128;
pub const RECORD_BYTES: usize = 12;
const HEADER_BYTES: usize = 40;
const MESH_BYTES: usize = 28;

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn word(bytes: &[u8], at: usize) -> io::Result<u32> {
    let raw = bytes
        .get(at..at + 4)
        .ok_or_else(|| invalid("truncated VHV metadata"))?;
    Ok(u32::from_le_bytes(raw.try_into().unwrap()))
}
fn reserved(bytes: &[u8], at: usize) -> io::Result<[u32; 4]> {
    Ok([
        word(bytes, at)?,
        word(bytes, at + 4)?,
        word(bytes, at + 8)?,
        word(bytes, at + 12)?,
    ])
}

#[derive(Clone, Debug)]
pub struct MeshSpan<'a> {
    pub lod: u32,
    pub vertex_count: u32,
    pub offset: u32,
    pub reserved: [u32; 4],
    records: &'a [u8],
}
impl<'a> MeshSpan<'a> {
    pub fn opaque_bytes(&self) -> &'a [u8] {
        self.records
    }
    pub fn opaque_records(&self) -> impl ExactSizeIterator<Item = &'a [u8; RECORD_BYTES]> {
        self.records
            .chunks_exact(RECORD_BYTES)
            .map(|bytes| bytes.try_into().unwrap())
    }
}

#[derive(Clone, Debug)]
pub struct File<'a> {
    pub checksum: u32,
    /// Exact header word, deliberately not interpreted as a color/basis mode.
    pub raw_flags: u32,
    pub vertex_count: u32,
    pub reserved: [u32; 4],
    meshes: Vec<MeshSpan<'a>>,
}
impl<'a> File<'a> {
    pub fn parse(bytes: &'a [u8]) -> io::Result<Self> {
        if bytes.len() > MAX_FILE_BYTES {
            return Err(invalid("VHV exceeds byte bound"));
        }
        if bytes.len() < HEADER_BYTES {
            return Err(invalid("truncated VHV header"));
        }
        if word(bytes, 0)? != 2 {
            return Err(invalid("unsupported VHV version"));
        }
        if word(bytes, 12)? as usize != RECORD_BYTES {
            return Err(invalid("unsupported opaque VHV record size"));
        }
        let count = word(bytes, 20)? as usize;
        if count > MAX_MESHES {
            return Err(invalid("VHV exceeds mesh bound"));
        }
        let metadata_end = HEADER_BYTES + count * MESH_BYTES;
        if metadata_end > bytes.len() {
            return Err(invalid("truncated VHV mesh table"));
        }
        let mut meshes = Vec::with_capacity(count);
        let mut spans = Vec::with_capacity(count);
        let mut sum = 0u32;
        for index in 0..count {
            let at = HEADER_BYTES + index * MESH_BYTES;
            let vertex_count = word(bytes, at + 4)?;
            let offset = word(bytes, at + 8)?;
            let start = offset as usize;
            let size = (vertex_count as usize)
                .checked_mul(RECORD_BYTES)
                .ok_or_else(|| invalid("VHV payload size overflow"))?;
            let end = start
                .checked_add(size)
                .ok_or_else(|| invalid("VHV payload offset overflow"))?;
            if start < metadata_end {
                return Err(invalid("VHV payload overlaps metadata"));
            }
            let records = bytes
                .get(start..end)
                .ok_or_else(|| invalid("VHV payload past end"))?;
            sum = sum
                .checked_add(vertex_count)
                .ok_or_else(|| invalid("VHV vertex total overflow"))?;
            if size != 0 {
                spans.push((start, end));
            }
            meshes.push(MeshSpan {
                lod: word(bytes, at)?,
                vertex_count,
                offset,
                reserved: reserved(bytes, at + 12)?,
                records,
            });
        }
        spans.sort_unstable();
        if spans.windows(2).any(|pair| pair[0].1 > pair[1].0) {
            return Err(invalid("overlapping VHV mesh payloads"));
        }
        let vertex_count = word(bytes, 16)?;
        if sum != vertex_count {
            return Err(invalid("VHV vertex total differs from mesh spans"));
        }
        Ok(Self {
            checksum: word(bytes, 4)?,
            raw_flags: word(bytes, 8)?,
            vertex_count,
            reserved: reserved(bytes, 24)?,
            meshes,
        })
    }
    pub fn meshes(&self) -> &[MeshSpan<'a>] {
        &self.meshes
    }
}

/// A selected immutable map and a static-prop ordinal, never just a model name.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct InstanceKey {
    map: ContentId,
    bsp_sha1: [u8; 20],
    ordinal: u32,
}
impl InstanceKey {
    pub fn new(map: ContentId, bsp_sha1: [u8; 20], ordinal: u32) -> io::Result<Self> {
        if map.kind != ContentKind::Map {
            return Err(invalid("VHV scope requires a map ContentId"));
        }
        Ok(Self {
            map,
            bsp_sha1,
            ordinal,
        })
    }
    pub fn map(&self) -> &ContentId {
        &self.map
    }
    pub fn bsp_sha1(&self) -> &[u8; 20] {
        &self.bsp_sha1
    }
    pub fn ordinal(&self) -> u32 {
        self.ordinal
    }
    pub fn hdr_name(&self) -> String {
        format!("sp_hdr_{}.vhv", self.ordinal)
    }
}

/// Caller supplies bytes from the selected map's scoped mount (e.g. read_from
/// the map pak), not generic VFS fallback. This key cannot attest filesystem
/// provenance by itself; it binds and checks the provenance supplied by that
/// owner. The byte borrow prevents a later archive lookup changing this file.
#[derive(Clone, Debug)]
pub struct ScopedFile<'a> {
    instance: InstanceKey,
    file: File<'a>,
}
impl<'a> ScopedFile<'a> {
    pub fn parse(instance: InstanceKey, name: &str, bytes: &'a [u8]) -> io::Result<Self> {
        if name != instance.hdr_name() {
            return Err(invalid("VHV filename differs from instance ordinal"));
        }
        Ok(Self {
            instance,
            file: File::parse(bytes)?,
        })
    }
    pub fn instance(&self) -> &InstanceKey {
        &self.instance
    }
    pub fn file(&self) -> &File<'a> {
        &self.file
    }

    /// Validate a complete candidate binding before returning any matched
    /// spans. Matching count sequences/checksums do NOT prove lighting-record
    /// ordering. Current model output covers only LOD0/default submodels;
    /// unsupported spans are rejected rather than silently discarded.
    pub fn candidate<'s, 'm>(
        &'s self,
        expected: &InstanceKey,
        model: &'m Model,
    ) -> io::Result<Candidate<'s, 'm, 'a>> {
        if &self.instance != expected {
            return Err(invalid("VHV map/instance scope differs"));
        }
        if self.file.checksum != model.checksum {
            return Err(invalid("VHV model checksum differs"));
        }
        if self.file.meshes.len() != model.meshes.len() {
            return Err(invalid("VHV/model mesh count differs"));
        }
        let mut previous: Option<HardwareMesh> = None;
        for (span, mesh) in self.file.meshes.iter().zip(&model.meshes) {
            let identity = mesh
                .hardware
                .ok_or_else(|| invalid("model mesh has no hardware identity"))?;
            if span.lod != 0 || identity.lod != span.lod || identity.sub_model != 0 {
                return Err(invalid("unsupported VHV/model LOD or submodel"));
            }
            if previous.is_some_and(|old| old >= identity) {
                return Err(invalid("model hardware mesh order differs"));
            }
            previous = Some(identity);
            if span.vertex_count as usize != mesh.vertices.len()
                || mesh.source_vertex_indices.len() != mesh.vertices.len()
            {
                return Err(invalid("VHV/model vertex count or source identity differs"));
            }
        }
        Ok(Candidate {
            scoped: self,
            model,
        })
    }
}

/// A checked correspondence candidate, never decoded colors or proof of
/// individual record-to-vertex order. No renderer consumes this structure.
pub struct Candidate<'s, 'm, 'a> {
    scoped: &'s ScopedFile<'a>,
    model: &'m Model,
}
impl<'s, 'm, 'a> Candidate<'s, 'm, 'a> {
    pub fn instance(&self) -> &InstanceKey {
        self.scoped.instance()
    }
    pub fn matching_spans(&self) -> impl ExactSizeIterator<Item = (&MeshSpan<'a>, &crate::Mesh)> {
        self.scoped.file.meshes.iter().zip(self.model.meshes.iter())
    }
}

#[cfg(test)]
mod tests;
