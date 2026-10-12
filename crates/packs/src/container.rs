//! Cheap preflight before ZIP's eager central-directory parser allocates.
//! V1 uses ordinary single-disk ZIP, within 32-bit sizes; ZIP64/SFX are not
//! needed by its budgets. Layout reference: PKWARE APPNOTE §§4.3.12/4.3.16,
//! https://pkware.cachefly.net/webdocs/casestudies/APPNOTE.TXT
use crate::Limits;

fn u16_at(bytes: &[u8], offset: usize) -> Result<u16, String> {
    let bytes = bytes
        .get(offset..offset + 2)
        .ok_or("truncated ZIP metadata")?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}
fn u32_at(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let bytes = bytes
        .get(offset..offset + 4)
        .ok_or("truncated ZIP metadata")?;
    Ok(u32::from_le_bytes(
        bytes.try_into().map_err(|_| "truncated ZIP metadata")?,
    ))
}

pub(crate) fn preflight(bytes: &[u8], limits: Limits) -> Result<(), String> {
    if bytes.len() < 22 || bytes.get(..4) != Some(&b"PK\x03\x04"[..]) {
        return Err("pack must be an ordinary nonempty ZIP (no self-extracting prefix)".into());
    }
    let minimum = bytes.len().saturating_sub(65535 + 22);
    let end = (minimum..=bytes.len() - 22)
        .rev()
        .find(|&at| {
            bytes[at..at + 4] == *b"PK\x05\x06"
                && u16_at(bytes, at + 20)
                    .is_ok_and(|comment| at + 22 + comment as usize == bytes.len())
        })
        .ok_or("ZIP end record missing or trailing bytes present")?;
    let count = u16_at(bytes, end + 10)? as usize;
    if count == 0 || count > limits.entries || count == u16::MAX as usize {
        return Err("pack exceeds entry count limit or uses ZIP64".into());
    }
    if u16_at(bytes, end + 4)? != 0
        || u16_at(bytes, end + 6)? != 0
        || u16_at(bytes, end + 8)? as usize != count
    {
        return Err("multi-disk ZIP is not a pack".into());
    }
    let size = u32_at(bytes, end + 12)? as usize;
    let offset = u32_at(bytes, end + 16)? as usize;
    if size > 8 << 20 || offset.checked_add(size) != Some(end) {
        return Err("ZIP directory size/offset exceeds pack metadata limits".into());
    }
    let mut cursor = offset;
    let mut expanded = 0u64;
    for _ in 0..count {
        if cursor.checked_add(46).is_none_or(|n| n > end)
            || bytes.get(cursor..cursor + 4) != Some(&b"PK\x01\x02"[..])
        {
            return Err("ZIP entry count/directory mismatch".into());
        }
        let compressed = u32_at(bytes, cursor + 20)? as u64;
        let raw = u32_at(bytes, cursor + 24)? as u64;
        expanded = expanded
            .checked_add(raw)
            .ok_or("ZIP expanded size overflow")?;
        if compressed > limits.archive || raw > limits.entry || expanded > limits.expanded {
            return Err("expanded pack exceeds size limit".into());
        }
        let name = u16_at(bytes, cursor + 28)? as usize;
        let extra = u16_at(bytes, cursor + 30)? as usize;
        let comment = u16_at(bytes, cursor + 32)? as usize;
        if name == 0
            || name > 241
            || u16_at(bytes, cursor + 34)? != 0
            || u32_at(bytes, cursor + 42)? as usize >= offset
        {
            return Err("invalid ZIP entry metadata".into());
        }
        let next = cursor
            .checked_add(46 + name + extra + comment)
            .ok_or("ZIP directory overflow")?;
        if next > end {
            return Err("truncated ZIP directory".into());
        }
        let mut field = cursor + 46 + name;
        let field_end = field + extra;
        while field < field_end {
            if field + 4 > field_end {
                return Err("truncated ZIP extra field".into());
            }
            if u16_at(bytes, field)? == 1 {
                return Err("ZIP64 is unsupported in pack v1".into());
            }
            field += 4 + u16_at(bytes, field + 2)? as usize;
            if field > field_end {
                return Err("truncated ZIP extra field".into());
            }
        }
        cursor = next;
    }
    if cursor != end {
        return Err("ZIP entry count/directory mismatch".into());
    }
    Ok(())
}
