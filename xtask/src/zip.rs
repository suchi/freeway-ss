//! A minimal zip writer that stores files without compression.
//!
//! Why: the release archive holds one small executable and two text files,
//! so compression would save little. Storing the files keeps the writer to a
//! few dozen lines with no dependency (docs/plans/0006-phase6-release.md).
//! Windows Explorer and common tools extract stored zips.

use crate::crc32::crc32;

const LOCAL_HEADER: u32 = 0x0403_4b50;
const CENTRAL_HEADER: u32 = 0x0201_4b50;
const END_OF_CENTRAL_DIRECTORY: u32 = 0x0605_4b50;
/// Bit 11: names are UTF-8.
const FLAG_UTF8: u16 = 0x0800;
/// Version 1.0, enough for stored entries.
const VERSION: u16 = 10;
/// 1980-01-01 00:00, the earliest zip timestamp. A fixed value makes the same
/// input produce the same archive (and the same checksum).
const DOS_DATE: u16 = (1 << 5) | 1;
const DOS_TIME: u16 = 0;

pub struct Entry<'a> {
    pub name: &'a str,
    pub data: &'a [u8],
}

pub fn write_stored(entries: &[Entry]) -> Result<Vec<u8>, String> {
    let count = u16::try_from(entries.len()).map_err(|_| "too many zip entries".to_owned())?;
    let mut out = Vec::new();
    let mut central = Vec::new();
    for entry in entries {
        let name = entry.name.as_bytes();
        let name_len = u16::try_from(name.len())
            .map_err(|_| format!("zip entry name too long: {}", entry.name))?;
        let size = u32::try_from(entry.data.len())
            .map_err(|_| format!("zip entry too large: {}", entry.name))?;
        let offset = u32::try_from(out.len()).map_err(|_| "zip archive too large".to_owned())?;
        let crc = crc32(entry.data);

        put_u32(&mut out, LOCAL_HEADER);
        put_u16(&mut out, VERSION);
        put_u16(&mut out, FLAG_UTF8);
        put_u16(&mut out, 0); // method: stored
        put_u16(&mut out, DOS_TIME);
        put_u16(&mut out, DOS_DATE);
        put_u32(&mut out, crc);
        put_u32(&mut out, size); // compressed size
        put_u32(&mut out, size); // uncompressed size
        put_u16(&mut out, name_len);
        put_u16(&mut out, 0); // extra field length
        out.extend_from_slice(name);
        out.extend_from_slice(entry.data);

        put_u32(&mut central, CENTRAL_HEADER);
        put_u16(&mut central, VERSION); // version made by (FAT host)
        put_u16(&mut central, VERSION); // version needed
        put_u16(&mut central, FLAG_UTF8);
        put_u16(&mut central, 0);
        put_u16(&mut central, DOS_TIME);
        put_u16(&mut central, DOS_DATE);
        put_u32(&mut central, crc);
        put_u32(&mut central, size);
        put_u32(&mut central, size);
        put_u16(&mut central, name_len);
        put_u16(&mut central, 0); // extra field length
        put_u16(&mut central, 0); // comment length
        put_u16(&mut central, 0); // disk number
        put_u16(&mut central, 0); // internal attributes
        put_u32(&mut central, 0); // external attributes
        put_u32(&mut central, offset);
        central.extend_from_slice(name);
    }
    let central_offset =
        u32::try_from(out.len()).map_err(|_| "zip archive too large".to_owned())?;
    let central_size =
        u32::try_from(central.len()).map_err(|_| "zip archive too large".to_owned())?;
    out.extend_from_slice(&central);
    put_u32(&mut out, END_OF_CENTRAL_DIRECTORY);
    put_u16(&mut out, 0); // this disk
    put_u16(&mut out, 0); // disk with the central directory
    put_u16(&mut out, count);
    put_u16(&mut out, count);
    put_u32(&mut out, central_size);
    put_u32(&mut out, central_offset);
    put_u16(&mut out, 0); // comment length
    Ok(out)
}

fn put_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

/// Reads back an archive written by `write_stored`, checking every CRC.
#[cfg(test)]
pub fn read_stored(archive: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    fn u16_at(bytes: &[u8], at: usize) -> Result<u16, String> {
        let slice = bytes.get(at..at + 2).ok_or("truncated zip")?;
        Ok(u16::from_le_bytes([slice[0], slice[1]]))
    }
    fn u32_at(bytes: &[u8], at: usize) -> Result<u32, String> {
        let slice = bytes.get(at..at + 4).ok_or("truncated zip")?;
        Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
    }

    let end = archive.len().checked_sub(22).ok_or("truncated zip")?;
    if u32_at(archive, end)? != END_OF_CENTRAL_DIRECTORY {
        return Err("end of central directory not found".into());
    }
    let count = usize::from(u16_at(archive, end + 10)?);
    let mut at = u32_at(archive, end + 16)? as usize;
    let mut files = Vec::new();
    for _ in 0..count {
        if u32_at(archive, at)? != CENTRAL_HEADER {
            return Err("bad central directory entry".into());
        }
        let method = u16_at(archive, at + 10)?;
        let crc = u32_at(archive, at + 16)?;
        let size = u32_at(archive, at + 24)? as usize;
        let name_len = usize::from(u16_at(archive, at + 28)?);
        let local = u32_at(archive, at + 42)? as usize;
        let name = archive
            .get(at + 46..at + 46 + name_len)
            .ok_or("truncated zip")?;
        let name = String::from_utf8(name.to_vec()).map_err(|e| e.to_string())?;
        at += 46 + name_len;

        if method != 0 {
            return Err(format!("{name}: not stored"));
        }
        if u32_at(archive, local)? != LOCAL_HEADER {
            return Err(format!("{name}: bad local header"));
        }
        let local_name_len = usize::from(u16_at(archive, local + 26)?);
        let local_extra_len = usize::from(u16_at(archive, local + 28)?);
        let start = local + 30 + local_name_len + local_extra_len;
        let data = archive.get(start..start + size).ok_or("truncated zip")?;
        if crc32(data) != crc {
            return Err(format!("{name}: CRC mismatch"));
        }
        files.push((name, data.to_vec()));
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_entries() {
        let big: Vec<u8> = (0..100_000u32).map(|i| (i % 251) as u8).collect();
        let archive = write_stored(&[
            Entry {
                name: "a.scr",
                data: &big,
            },
            Entry {
                name: "LICENSE",
                data: b"MIT",
            },
            Entry {
                name: "empty",
                data: b"",
            },
        ])
        .unwrap();
        let files = read_stored(&archive).unwrap();
        assert_eq!(
            files,
            vec![
                ("a.scr".to_owned(), big),
                ("LICENSE".to_owned(), b"MIT".to_vec()),
                ("empty".to_owned(), Vec::new()),
            ]
        );
    }

    #[test]
    fn output_is_deterministic() {
        let entries = [Entry {
            name: "x",
            data: b"data",
        }];
        assert_eq!(
            write_stored(&entries).unwrap(),
            write_stored(&entries).unwrap()
        );
    }

    #[test]
    fn corruption_is_detected() {
        let mut archive = write_stored(&[Entry {
            name: "x",
            data: b"data",
        }])
        .unwrap();
        let at = archive.iter().position(|&b| b == b'd').unwrap();
        archive[at] = b'D';
        assert!(read_stored(&archive).unwrap_err().contains("CRC"));
    }
}
