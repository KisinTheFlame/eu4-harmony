//! Incremental ZIP writing, with ZIP64 for unknown or large sizes.
use super::{crc32_update, Entry, DATA_DESCRIPTOR, DEFLATED, DESCRIPTOR, DIRECTORY, END, LOCAL, ZIP64_END, ZIP64_LOCATOR};
use std::io::{self, Read, Write};

/// Writes entries incrementally, retaining only central-directory metadata.
/// [`Self::deflate`] reads and compresses through fixed-size buffers. After
/// any error, discard the unfinished archive.
pub struct Stream<W> {
    output: W,
    directory: Vec<u8>,
    position: u64,
    count: u64,
}

fn extra(values: &[u64]) -> Vec<u8> {
    if values.is_empty() {
        return Vec::new();
    }
    let mut extra = Vec::with_capacity(4 + 8 * values.len());
    extra.extend_from_slice(&1u16.to_le_bytes());
    extra.extend_from_slice(&((8 * values.len()) as u16).to_le_bytes());
    for value in values {
        extra.extend_from_slice(&value.to_le_bytes());
    }
    extra
}

fn header(entry: &Entry, extra: &[u8], sizes64: bool) -> Vec<u8> {
    let mut out = Vec::new();
    for v in [if sizes64 { 45u16 } else { 20 }, entry.flags, entry.method, entry.time, entry.date] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&entry.crc.to_le_bytes());
    for size in [entry.packed, entry.unpacked] {
        out.extend_from_slice(&(if sizes64 { u32::MAX } else { size as u32 }).to_le_bytes());
    }
    out.extend_from_slice(&(entry.name.len() as u16).to_le_bytes());
    out.extend_from_slice(&(extra.len() as u16).to_le_bytes());
    out
}

impl<W: Write> Stream<W> {
    pub fn new(output: W) -> Self {
        Self { output, directory: Vec::new(), position: 0, count: 0 }
    }

    fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.output.write_all(bytes)?;
        self.position = self.position.checked_add(bytes.len() as u64).ok_or_else(|| io::Error::other("zip length overflow"))?;
        Ok(())
    }

    fn check_name(name: &[u8]) -> io::Result<()> {
        if name.len() > u16::MAX as usize {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "zip entry name too long"));
        }
        Ok(())
    }

    fn central(&mut self, entry: &Entry, sizes64: bool) {
        let offset64 = entry.local >= u64::from(u32::MAX);
        let mut values = Vec::new();
        if sizes64 {
            values.extend([entry.unpacked as u64, entry.packed as u64]);
        }
        if offset64 {
            values.push(entry.local);
        }
        let extra = extra(&values);
        self.directory.extend_from_slice(DIRECTORY);
        self.directory.extend_from_slice(&if sizes64 || offset64 { 45u16 } else { 20 }.to_le_bytes());
        // An offset-only extra needs version 4.5 but leaves the ordinary
        // size fields intact.
        let mut common = header(entry, &extra, sizes64);
        if offset64 {
            common[..2].copy_from_slice(&45u16.to_le_bytes());
        }
        self.directory.extend_from_slice(&common);
        for v in [0u16, 0, 0] {
            self.directory.extend_from_slice(&v.to_le_bytes());
        }
        self.directory.extend_from_slice(&entry.attributes.to_le_bytes());
        self.directory.extend_from_slice(&(if offset64 { u32::MAX } else { entry.local as u32 }).to_le_bytes());
        self.directory.extend_from_slice(&entry.name);
        self.directory.extend_from_slice(&extra);
        self.count += 1;
    }

    /// Adds data already stored or compressed by the caller, like the
    /// in-memory ZIP writer.
    pub fn entry(&mut self, entry: &Entry, data: &[u8]) -> io::Result<()> {
        Self::check_name(&entry.name)?;
        let entry = Entry { flags: entry.flags & !DATA_DESCRIPTOR, local: self.position, packed: data.len(), ..entry.clone() };
        let sizes64 = entry.packed >= u32::MAX as usize || entry.unpacked >= u32::MAX as usize;
        let extra = extra(&if sizes64 { vec![entry.unpacked as u64, entry.packed as u64] } else { Vec::new() });
        let mut local = LOCAL.to_vec();
        local.extend_from_slice(&header(&entry, &extra, sizes64));
        local.extend_from_slice(&entry.name);
        local.extend_from_slice(&extra);
        self.write(&local)?;
        self.write(data)?;
        self.central(&entry, sizes64);
        Ok(())
    }

    /// Deflates a reader without retaining its contents or compressed output.
    /// Returns the sizes and CRC actually written, not a prior file length.
    pub fn deflate(&mut self, name: &[u8], mut reader: impl Read, when: (u16, u16)) -> io::Result<Entry> {
        use miniz_oxide::deflate::core::CompressorOxide;
        use miniz_oxide::deflate::stream::deflate;
        use miniz_oxide::{DataFormat, MZFlush, MZStatus};
        Self::check_name(name)?;
        let mut entry = Entry { flags: DATA_DESCRIPTOR, method: DEFLATED, time: when.0, date: when.1, local: self.position, ..Entry::stored(name, &[]) };
        // Reserve ZIP64 sizes before reading: neither size is known yet.
        let extra = extra(&[0, 0]);
        let mut local = LOCAL.to_vec();
        local.extend_from_slice(&header(&entry, &extra, true));
        local.extend_from_slice(name);
        local.extend_from_slice(&extra);
        self.write(&local)?;
        let data_at = self.position;
        let mut compressor = Box::<CompressorOxide>::default();
        compressor.set_format_and_level(DataFormat::Raw, 6);
        let mut input = vec![0; 64 << 10];
        let mut output = vec![0; 64 << 10];
        let mut crc = !0;
        loop {
            let len = match reader.read(&mut input) {
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                result => result?,
            };
            entry.unpacked = entry.unpacked.checked_add(len).ok_or_else(|| io::Error::other("zip entry length overflow"))?;
            crc = crc32_update(crc, &input[..len]);
            let flush = if len == 0 { MZFlush::Finish } else { MZFlush::None };
            let mut consumed = 0;
            loop {
                let result = deflate(&mut compressor, &input[consumed..len], &mut output, flush);
                consumed += result.bytes_consumed;
                self.write(&output[..result.bytes_written])?;
                match result.status {
                    Ok(MZStatus::StreamEnd) => break,
                    Ok(_) if consumed == len && len != 0 => break,
                    Ok(_) if result.bytes_consumed != 0 || result.bytes_written != 0 => (),
                    _ => return Err(io::Error::other("deflate failed to make progress")),
                }
            }
            if len == 0 {
                break;
            }
        }
        entry.crc = !crc;
        entry.packed = usize::try_from(self.position - data_at).map_err(|_| io::Error::other("zip entry length overflow"))?;
        let mut descriptor = DESCRIPTOR.to_vec();
        descriptor.extend_from_slice(&entry.crc.to_le_bytes());
        descriptor.extend_from_slice(&(entry.packed as u64).to_le_bytes());
        descriptor.extend_from_slice(&(entry.unpacked as u64).to_le_bytes());
        self.write(&descriptor)?;
        self.central(&entry, true);
        Ok(entry)
    }

    pub fn finish(mut self) -> io::Result<W> {
        let directory_at = self.position;
        let directory_len = self.directory.len() as u64;
        self.output.write_all(&self.directory)?;
        self.position += directory_len;
        if self.count >= u64::from(u16::MAX) || directory_at >= u64::from(u32::MAX) || directory_len >= u64::from(u32::MAX) {
            let end_at = self.position;
            let mut end64 = ZIP64_END.to_vec();
            end64.extend_from_slice(&44u64.to_le_bytes());
            for v in [45u16, 45] {
                end64.extend_from_slice(&v.to_le_bytes());
            }
            for v in [0u32, 0] {
                end64.extend_from_slice(&v.to_le_bytes());
            }
            for v in [self.count, self.count, directory_len, directory_at] {
                end64.extend_from_slice(&v.to_le_bytes());
            }
            end64.extend_from_slice(ZIP64_LOCATOR);
            end64.extend_from_slice(&0u32.to_le_bytes());
            end64.extend_from_slice(&end_at.to_le_bytes());
            end64.extend_from_slice(&1u32.to_le_bytes());
            self.write(&end64)?;
        }
        let mut end = END.to_vec();
        for v in [0u16, 0, self.count.min(u64::from(u16::MAX)) as u16, self.count.min(u64::from(u16::MAX)) as u16] {
            end.extend_from_slice(&v.to_le_bytes());
        }
        end.extend_from_slice(&(directory_len.min(u64::from(u32::MAX)) as u32).to_le_bytes());
        end.extend_from_slice(&(directory_at.min(u64::from(u32::MAX)) as u32).to_le_bytes());
        end.extend_from_slice(&0u16.to_le_bytes());
        self.write(&end)?;
        self.output.flush()?;
        Ok(self.output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zip::{directory, read, u16_at, u32_at, u64_at};
    use std::io::{Cursor, Seek, SeekFrom};

    #[test]
    fn deflate_reads_in_chunks_retries_interruptions_and_preserves_every_byte() {
        struct Chunks<'a> {
            data: &'a [u8],
            interrupted: bool,
        }
        impl Read for Chunks<'_> {
            fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
                assert!(out.len() <= 64 << 10);
                if !self.interrupted {
                    self.interrupted = true;
                    return Err(io::ErrorKind::Interrupted.into());
                }
                let len = self.data.len().min(out.len()).min(7919);
                out[..len].copy_from_slice(&self.data[..len]);
                self.data = &self.data[len..];
                Ok(len)
            }
        }
        let mut seed = 1u64;
        let data: Vec<_> = (0..300_001)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                seed as u8
            })
            .collect();
        let mut stream = Stream::new(Vec::new());
        let entry = stream.deflate(b"minidump.dmp", Chunks { data: &data, interrupted: false }, (0x1234, 0x5678)).unwrap();
        assert_eq!(entry.unpacked, data.len());
        assert_eq!(entry.crc, crate::zip::crc32(&data));
        assert!(entry.packed > 64 << 10);
        stream.deflate(b"empty", io::empty(), (0, 0x21)).unwrap();
        let mut archive = Cursor::new(stream.finish().unwrap());
        let entries = directory(&mut archive, 4096).unwrap();
        assert_eq!(entries[0], entry);
        assert_eq!(read(&mut archive, &entries[0], 1 << 20).unwrap(), data);
        assert!(read(&mut archive, &entries[1], 4096).unwrap().is_empty());
    }

    #[test]
    fn deflate_propagates_input_and_output_errors() {
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::other("source read failed"))
            }
        }
        let mut stream = Stream::new(Vec::new());
        assert_eq!(stream.deflate(b"a", Broken, (0, 0x21)).unwrap_err().to_string(), "source read failed");
        // The local header fits; writing the compressed data or descriptor
        // must still surface an output failure.
        let mut output = [0; 52];
        let mut stream = Stream::new(Cursor::new(output.as_mut_slice()));
        assert!(stream.deflate(b"a", b"data".as_slice(), (0, 0x21)).is_err());
    }

    #[test]
    fn zip64_sizes_are_written_and_read_at_the_zip32_sentinel() {
        let mut stream = Stream::new(Vec::new());
        let entry = Entry { packed: u32::MAX as usize, unpacked: u32::MAX as usize + 1, ..Entry::stored(b"large", &[]) };
        stream.central(&entry, true);
        let mut archive = Cursor::new(stream.finish().unwrap());
        assert_eq!(directory(&mut archive, 4096).unwrap(), [entry]);
    }

    #[test]
    fn zip64_counts_preserve_more_than_65535_entries() {
        let mut stream = Stream::new(Vec::new());
        let entry = Entry::stored(b"empty", &[]);
        for _ in 0..=u16::MAX {
            stream.entry(&entry, &[]).unwrap();
        }
        let bytes = stream.finish().unwrap();
        let end = bytes.len() - 22;
        assert_eq!(u16_at(&bytes, end + 10).unwrap(), u16::MAX);
        assert_eq!(&bytes[end - 20..end - 16], ZIP64_LOCATOR);
        let end64 = u64_at(&bytes, end - 12).unwrap() as usize;
        assert_eq!(&bytes[end64..end64 + 4], ZIP64_END);
        assert_eq!(u64_at(&bytes, end64 + 32).unwrap(), 65_536);
        let entries = directory(&mut Cursor::new(bytes), 8 << 20).unwrap();
        assert_eq!(entries.len(), 65_536);
    }

    /// A large logical prefix without allocating or writing its zero bytes.
    struct Padded {
        prefix: u64,
        position: u64,
        tail: Vec<u8>,
    }

    impl Write for Padded {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            assert_eq!(self.position, self.prefix + self.tail.len() as u64);
            self.tail.extend_from_slice(bytes);
            self.position += bytes.len() as u64;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Read for Padded {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            let len = bytes.len().min((self.prefix + self.tail.len() as u64).saturating_sub(self.position) as usize);
            bytes[..len].fill(0);
            let zeros = self.prefix.saturating_sub(self.position).min(len as u64) as usize;
            if zeros < len {
                let from = (self.position + zeros as u64 - self.prefix) as usize;
                bytes[zeros..len].copy_from_slice(&self.tail[from..from + len - zeros]);
            }
            self.position += len as u64;
            Ok(len)
        }
    }

    impl Seek for Padded {
        fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
            self.position = match from {
                SeekFrom::Start(at) => Some(at),
                SeekFrom::End(by) => (self.prefix + self.tail.len() as u64).checked_add_signed(by),
                SeekFrom::Current(by) => self.position.checked_add_signed(by),
            }
            .ok_or_else(|| io::Error::other("seek out of bounds"))?;
            Ok(self.position)
        }
    }

    #[test]
    fn zip64_offsets_cross_four_gibibytes_without_a_large_allocation() {
        let prefix = u64::from(u32::MAX);
        let mut stream = Stream::new(Padded { prefix, position: prefix, tail: Vec::new() });
        stream.position = prefix;
        stream.entry(&Entry::stored(b"large-offset", b"data"), b"data").unwrap();
        let mut archive = stream.finish().unwrap();
        let end = archive.tail.len() - 22;
        assert_eq!(u32_at(&archive.tail, end + 16).unwrap(), u32::MAX);
        let entries = directory(&mut archive, 4096).unwrap();
        assert_eq!(entries[0].local, prefix);
        assert_eq!(read(&mut archive, &entries[0], 4096).unwrap(), b"data");
    }
}
