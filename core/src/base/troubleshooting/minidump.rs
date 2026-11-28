//! Slims oversized Windows minidumps (such as EU4's ~77 MiB crash dumps that
//! include data segments and indirectly referenced memory) into compact
//! `MiniDumpNormal`-equivalent dumps while keeping everything needed for stack
//! and symbol analysis (`.ecxr`, `kn 30`, module list, process/run identity).
//!
//! The original dump on disk is never modified.
use std::collections::BTreeMap;
use std::io::{self, Read, Seek, SeekFrom};

const STREAM_THREAD_LIST: u32 = 3;
const STREAM_MODULE_LIST: u32 = 4;
const STREAM_MEMORY_LIST: u32 = 5;
const STREAM_EXCEPTION: u32 = 6;
const STREAM_SYSTEM_INFO: u32 = 7;
const STREAM_UNLOADED_MODULE_LIST: u32 = 14;
const STREAM_MISC_INFO: u32 = 15;
const STREAM_SYSTEM_MEMORY_INFO: u32 = 21;

/// Maximum single chunk we copy from a dump while slimming (guards against
/// corrupt length fields).
const MAX_THREAD_STACK_BYTES: u32 = 2 * 1024 * 1024;
const MAX_SMALL_BLOB: u32 = 256 * 1024;
const MAX_STREAMS: u32 = 4096;

#[derive(Clone, Copy)]
struct DirEntry {
    kind: u32,
    size: u32,
    rva: u32,
}

fn invalid(msg: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg)
}

fn read_at(file: &mut (impl Read + Seek), rva: u64, size: u32, file_len: u64) -> io::Result<Vec<u8>> {
    let end = rva.checked_add(u64::from(size)).ok_or_else(|| invalid("minidump offset overflow"))?;
    if end > file_len {
        return Err(invalid("minidump slice past end of file"));
    }
    file.seek(SeekFrom::Start(rva))?;
    let mut buf = vec![0u8; size as usize];
    file.read_exact(&mut buf)?;
    Ok(buf)
}

fn u32_le(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().expect("4 bytes"))
}

fn u64_le(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().expect("8 bytes"))
}

fn put_u32(buf: &mut [u8], at: usize, value: u32) {
    buf[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(buf: &mut [u8], at: usize, value: u64) {
    buf[at..at + 8].copy_from_slice(&value.to_le_bytes());
}

/// Reads a `MINIDUMP_STRING` (`u32 byte_len` + UTF-16 bytes + NUL terminator)
/// from `file` at `rva`.
fn read_minidump_string(file: &mut (impl Read + Seek), rva: u32, file_len: u64) -> io::Result<Vec<u8>> {
    if rva == 0 {
        return Ok(Vec::new());
    }
    let header = read_at(file, u64::from(rva), 4, file_len)?;
    let byte_len = u32_le(&header, 0);
    if byte_len > MAX_SMALL_BLOB {
        return Err(invalid("minidump string too large"));
    }
    // Include trailing UTF-16 NUL if present within file_len.
    let total = 4u32.saturating_add(byte_len).saturating_add(2);
    let clamped = if u64::from(rva) + u64::from(total) <= file_len { total } else { 4u32.saturating_add(byte_len) };
    read_at(file, u64::from(rva), clamped, file_len)
}

/// Produces a compact `MiniDumpNormal`-style dump from `file`, preserving:
/// - `ExceptionStream` (including its `ThreadContext`),
/// - `ThreadListStream` (including every thread's `ThreadContext` and `Stack`),
/// - `ModuleListStream` (including module names, `CvRecord`, and `MiscRecord`),
/// - `SystemInfoStream` (including `CSDVersion`),
/// - `MiscInfoStream` (PID and process creation time),
/// - `UnloadedModuleListStream` and `SystemMemoryInfoStream` when small,
/// - `MemoryListStream` rebuilt exclusively from the threads' stack ranges.
pub(super) fn slim<F: Read + Seek>(file: &mut F) -> io::Result<Vec<u8>> {
    let file_len = file.seek(SeekFrom::End(0))?;
    if file_len < 32 {
        return Err(invalid("minidump shorter than header"));
    }
    let header = read_at(file, 0, 32, file_len)?;
    if &header[0..4] != b"MDMP" {
        return Err(invalid("not a minidump"));
    }
    let version = u32_le(&header, 4);
    let stream_count = u32_le(&header, 8);
    let dir_rva = u32_le(&header, 12);
    let timestamp = u32_le(&header, 20);
    if stream_count == 0 || stream_count > MAX_STREAMS {
        return Err(invalid("invalid minidump stream count"));
    }
    let dir_bytes = read_at(file, u64::from(dir_rva), stream_count.checked_mul(12).ok_or_else(|| invalid("directory overflow"))?, file_len)?;
    let mut dirs = Vec::with_capacity(stream_count as usize);
    for i in 0..stream_count as usize {
        let base = i * 12;
        dirs.push(DirEntry { kind: u32_le(&dir_bytes, base), size: u32_le(&dir_bytes, base + 4), rva: u32_le(&dir_bytes, base + 8) });
    }

    let find = |kind: u32| dirs.iter().copied().find(|d| d.kind == kind && d.size > 0);

    // Output layout:
    //   [0..32)                        MINIDUMP_HEADER
    //   [32..32 + 12 * kept_streams)   MINIDUMP_DIRECTORY[]
    //   [payload_start..)              stream bodies & referenced blobs
    let mut out_streams: Vec<(u32, Vec<u8>)> = Vec::new();
    // Blobs appended after stream bodies; we record each blob's offset relative
    // to `blob_pool` and patch its RVA once the fixed section sizes are known.
    let mut blob_pool: Vec<u8> = Vec::new();
    let mut blob_dedup: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    // (stream_index, offset_in_stream, blob_offset_in_pool)
    let mut rva_patches: Vec<(usize, usize, u32)> = Vec::new();

    fn alloc_blob(pool: &mut Vec<u8>, bytes: &[u8]) -> io::Result<u32> {
        let off = u32::try_from(pool.len()).map_err(|_| invalid("slim minidump overflow"))?;
        pool.extend_from_slice(bytes);
        while !pool.len().is_multiple_of(4) {
            pool.push(0);
        }
        Ok(off)
    }

    fn copy_blob<F: Read + Seek>(
        file: &mut F,
        file_len: u64,
        pool: &mut Vec<u8>,
        dedup: &mut std::collections::BTreeMap<(u32, u32), u32>,
        src_rva: u32,
        src_size: u32,
        max_size: u32,
    ) -> io::Result<Option<u32>> {
        if src_rva == 0 || src_size == 0 || src_size > max_size {
            return Ok(None);
        }
        if let Some(&existing) = dedup.get(&(src_rva, src_size)) {
            return Ok(Some(existing));
        }
        let bytes = read_at(file, u64::from(src_rva), src_size, file_len)?;
        let off = alloc_blob(pool, &bytes)?;
        dedup.insert((src_rva, src_size), off);
        Ok(Some(off))
    }

    // Track unique stack memory ranges to build MemoryListStream:
    // (start_va, size, blob_offset_in_pool)
    let mut memory_ranges: Vec<(u64, u32, u32)> = Vec::new();

    // 1. ThreadListStream (3): 4-byte count + N * 48-byte MINIDUMP_THREAD
    if let Some(dir) = find(STREAM_THREAD_LIST) {
        if dir.size >= 4 && dir.size <= 4 * 1024 * 1024 {
            let mut data = read_at(file, u64::from(dir.rva), dir.size, file_len)?;
            let count = u32_le(&data, 0) as usize;
            if 4 + count * 48 <= data.len() {
                let stream_idx = out_streams.len();
                for i in 0..count {
                    let base = 4 + i * 48;
                    // Stack: MINIDUMP_MEMORY_DESCRIPTOR at +24 (StartOfMemoryRange: u64, DataSize: u32, Rva: u32)
                    let stack_va = u64_le(&data, base + 24);
                    let stack_size = u32_le(&data, base + 32);
                    let stack_rva = u32_le(&data, base + 36);
                    if let Some(off) = copy_blob(file, file_len, &mut blob_pool, &mut blob_dedup, stack_rva, stack_size, MAX_THREAD_STACK_BYTES)? {
                        rva_patches.push((stream_idx, base + 36, off));
                        if !memory_ranges.iter().any(|&(va, sz, _)| va == stack_va && sz == stack_size) {
                            memory_ranges.push((stack_va, stack_size, off));
                        }
                    } else {
                        put_u32(&mut data, base + 32, 0);
                        put_u32(&mut data, base + 36, 0);
                    }
                    // ThreadContext: MINIDUMP_LOCATION_DESCRIPTOR at +40 (DataSize: u32, Rva: u32)
                    let ctx_size = u32_le(&data, base + 40);
                    let ctx_rva = u32_le(&data, base + 44);
                    if let Some(off) = copy_blob(file, file_len, &mut blob_pool, &mut blob_dedup, ctx_rva, ctx_size, MAX_SMALL_BLOB)? {
                        rva_patches.push((stream_idx, base + 44, off));
                    } else {
                        put_u32(&mut data, base + 40, 0);
                        put_u32(&mut data, base + 44, 0);
                    }
                }
                out_streams.push((STREAM_THREAD_LIST, data));
            }
        }
    }

    // 2. ModuleListStream (4): 4-byte count + N * 108-byte MINIDUMP_MODULE
    if let Some(dir) = find(STREAM_MODULE_LIST) {
        if dir.size >= 4 && dir.size <= 4 * 1024 * 1024 {
            let mut data = read_at(file, u64::from(dir.rva), dir.size, file_len)?;
            let count = u32_le(&data, 0) as usize;
            if 4 + count * 108 <= data.len() {
                let stream_idx = out_streams.len();
                for i in 0..count {
                    let base = 4 + i * 108;
                    // ModuleNameRva at +20
                    let name_rva = u32_le(&data, base + 20);
                    if name_rva != 0 {
                        let sbytes = read_minidump_string(file, name_rva, file_len)?;
                        if !sbytes.is_empty() {
                            let off = alloc_blob(&mut blob_pool, &sbytes)?;
                            rva_patches.push((stream_idx, base + 20, off));
                        } else {
                            put_u32(&mut data, base + 20, 0);
                        }
                    }
                    // CvRecord at +76 (DataSize: u32, Rva: u32)
                    let cv_size = u32_le(&data, base + 76);
                    let cv_rva = u32_le(&data, base + 80);
                    if let Some(off) = copy_blob(file, file_len, &mut blob_pool, &mut blob_dedup, cv_rva, cv_size, MAX_SMALL_BLOB)? {
                        rva_patches.push((stream_idx, base + 80, off));
                    } else {
                        put_u32(&mut data, base + 76, 0);
                        put_u32(&mut data, base + 80, 0);
                    }
                    // MiscRecord at +84 (DataSize: u32, Rva: u32)
                    let misc_size = u32_le(&data, base + 84);
                    let misc_rva = u32_le(&data, base + 88);
                    if let Some(off) = copy_blob(file, file_len, &mut blob_pool, &mut blob_dedup, misc_rva, misc_size, MAX_SMALL_BLOB)? {
                        rva_patches.push((stream_idx, base + 88, off));
                    } else {
                        put_u32(&mut data, base + 84, 0);
                        put_u32(&mut data, base + 88, 0);
                    }
                }
                out_streams.push((STREAM_MODULE_LIST, data));
            }
        }
    }

    // 3. ExceptionStream (6): 168 bytes; ThreadContext at +160 (DataSize: u32, Rva: u32)
    if let Some(dir) = find(STREAM_EXCEPTION) {
        if dir.size >= 168 && dir.size <= MAX_SMALL_BLOB {
            let mut data = read_at(file, u64::from(dir.rva), dir.size, file_len)?;
            let stream_idx = out_streams.len();
            let ctx_size = u32_le(&data, 160);
            let ctx_rva = u32_le(&data, 164);
            if let Some(off) = copy_blob(file, file_len, &mut blob_pool, &mut blob_dedup, ctx_rva, ctx_size, MAX_SMALL_BLOB)? {
                rva_patches.push((stream_idx, 164, off));
            } else {
                put_u32(&mut data, 160, 0);
                put_u32(&mut data, 164, 0);
            }
            out_streams.push((STREAM_EXCEPTION, data));
        }
    }

    // 4. SystemInfoStream (7): >= 56 bytes; CSDVersionRva at +24
    if let Some(dir) = find(STREAM_SYSTEM_INFO) {
        if dir.size >= 56 && dir.size <= MAX_SMALL_BLOB {
            let mut data = read_at(file, u64::from(dir.rva), dir.size, file_len)?;
            let stream_idx = out_streams.len();
            let csd_rva = u32_le(&data, 24);
            if csd_rva != 0 {
                let sbytes = read_minidump_string(file, csd_rva, file_len)?;
                if !sbytes.is_empty() {
                    let off = alloc_blob(&mut blob_pool, &sbytes)?;
                    rva_patches.push((stream_idx, 24, off));
                } else {
                    put_u32(&mut data, 24, 0);
                }
            }
            out_streams.push((STREAM_SYSTEM_INFO, data));
        }
    }

    // 5. MiscInfoStream (15) and SystemMemoryInfoStream (21): self-contained.
    for kind in [STREAM_MISC_INFO, STREAM_SYSTEM_MEMORY_INFO] {
        if let Some(dir) = find(kind) {
            if dir.size <= MAX_SMALL_BLOB {
                let data = read_at(file, u64::from(dir.rva), dir.size, file_len)?;
                out_streams.push((kind, data));
            }
        }
    }

    // 6. UnloadedModuleListStream (14): header(12) + N * entry_size; ModuleNameRva at +20 in each entry
    if let Some(dir) = find(STREAM_UNLOADED_MODULE_LIST) {
        if dir.size >= 12 && dir.size <= 512 * 1024 {
            let mut data = read_at(file, u64::from(dir.rva), dir.size, file_len)?;
            let hdr_size = u32_le(&data, 0) as usize;
            let entry_size = u32_le(&data, 4) as usize;
            let count = u32_le(&data, 8) as usize;
            if hdr_size >= 12 && entry_size >= 24 && hdr_size + count * entry_size <= data.len() {
                let stream_idx = out_streams.len();
                for i in 0..count {
                    let base = hdr_size + i * entry_size;
                    let name_rva = u32_le(&data, base + 20);
                    if name_rva != 0 {
                        if let Ok(sbytes) = read_minidump_string(file, name_rva, file_len) {
                            if !sbytes.is_empty() {
                                let off = alloc_blob(&mut blob_pool, &sbytes)?;
                                rva_patches.push((stream_idx, base + 20, off));
                                continue;
                            }
                        }
                        put_u32(&mut data, base + 20, 0);
                    }
                }
                out_streams.push((STREAM_UNLOADED_MODULE_LIST, data));
            }
        }
    }

    // 7. Rebuilt MemoryListStream (5): 4-byte count + N * 16-byte MINIDUMP_MEMORY_DESCRIPTOR
    if !memory_ranges.is_empty() {
        let mut mem_stream = vec![0u8; 4 + memory_ranges.len() * 16];
        put_u32(&mut mem_stream, 0, memory_ranges.len() as u32);
        let stream_idx = out_streams.len();
        for (i, &(va, size, blob_off)) in memory_ranges.iter().enumerate() {
            let base = 4 + i * 16;
            put_u64(&mut mem_stream, base, va);
            put_u32(&mut mem_stream, base + 8, size);
            rva_patches.push((stream_idx, base + 12, blob_off));
        }
        out_streams.push((STREAM_MEMORY_LIST, mem_stream));
    }

    if out_streams.is_empty() {
        return Err(invalid("minidump has no diagnostic streams"));
    }

    // Compute exact RVAs for each stream and for the blob pool.
    let dir_size = out_streams.len() * 12;
    let mut cursor = 32usize + dir_size;
    let mut stream_rvas = Vec::with_capacity(out_streams.len());
    for (_, data) in &out_streams {
        stream_rvas.push(u32::try_from(cursor).map_err(|_| invalid("slim minidump overflow"))?);
        cursor = cursor.checked_add(data.len()).ok_or_else(|| invalid("slim minidump overflow"))?;
        cursor = (cursor + 3) & !3;
    }
    let blob_base = u32::try_from(cursor).map_err(|_| invalid("slim minidump overflow"))?;

    for (stream_idx, field_off, blob_off) in rva_patches {
        let final_rva = blob_base.checked_add(blob_off).ok_or_else(|| invalid("slim minidump rva overflow"))?;
        put_u32(&mut out_streams[stream_idx].1, field_off, final_rva);
    }

    let total_size = cursor.checked_add(blob_pool.len()).ok_or_else(|| invalid("slim minidump overflow"))?;
    let mut out = Vec::with_capacity(total_size);

    // MINIDUMP_HEADER (32 bytes, Flags = 0 = MiniDumpNormal)
    out.extend_from_slice(b"MDMP");
    out.extend_from_slice(&version.to_le_bytes());
    out.extend_from_slice(&(out_streams.len() as u32).to_le_bytes());
    out.extend_from_slice(&32u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // CheckSum
    out.extend_from_slice(&timestamp.to_le_bytes());
    out.extend_from_slice(&0u64.to_le_bytes()); // Flags = MiniDumpNormal

    // MINIDUMP_DIRECTORY[]
    for ((kind, data), rva) in out_streams.iter().zip(&stream_rvas) {
        out.extend_from_slice(&kind.to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&rva.to_le_bytes());
    }

    // Stream bodies
    for (_, data) in &out_streams {
        out.extend_from_slice(data);
        while out.len() % 4 != 0 {
            out.push(0);
        }
    }
    debug_assert_eq!(out.len(), blob_base as usize);
    out.extend_from_slice(&blob_pool);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn mdmp_string(text: &str) -> Vec<u8> {
        let utf16: Vec<u16> = text.encode_utf16().collect();
        let mut out = ((utf16.len() * 2) as u32).to_le_bytes().to_vec();
        for ch in utf16 {
            out.extend_from_slice(&ch.to_le_bytes());
        }
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    /// Builds a synthetic bloated dump containing a valid thread stack, module
    /// name/CvRecord, exception context, MiscInfo, plus a large irrelevant
    /// memory region in MemoryListStream.
    fn synthetic_bloated_dump(extra_memory_bytes: usize) -> Vec<u8> {
        let mut payload = Vec::new();
        let base_rva = 32 + 5 * 12; // 5 streams
        let alloc = |pool: &mut Vec<u8>, data: &[u8]| -> u32 {
            let rva = (base_rva + pool.len()) as u32;
            pool.extend_from_slice(data);
            rva
        };

        let stack_bytes = b"STACK-FRAME-PAYLOAD-1234567890";
        let stack_rva = alloc(&mut payload, stack_bytes);
        let ctx_bytes = vec![0xAAu8; 64];
        let ctx_rva = alloc(&mut payload, &ctx_bytes);
        let mod_name = mdmp_string(r"C:\Games\EU4\eu4.exe");
        let mod_name_rva = alloc(&mut payload, &mod_name);
        let cv_bytes = b"RSDS\x01\x02\x03\x04eu4.pdb\0";
        let cv_rva = alloc(&mut payload, cv_bytes);
        let bloat_rva = alloc(&mut payload, &vec![0x55u8; extra_memory_bytes]);

        // ThreadListStream (1 thread)
        let mut threads = vec![0u8; 4 + 48];
        put_u32(&mut threads, 0, 1);
        put_u64(&mut threads, 4 + 24, 0x0000_00F0_0000_1000);
        put_u32(&mut threads, 4 + 32, stack_bytes.len() as u32);
        put_u32(&mut threads, 4 + 36, stack_rva);
        put_u32(&mut threads, 4 + 40, ctx_bytes.len() as u32);
        put_u32(&mut threads, 4 + 44, ctx_rva);
        let threads_rva = alloc(&mut payload, &threads);

        // ModuleListStream (1 module)
        let mut modules = vec![0u8; 4 + 108];
        put_u32(&mut modules, 0, 1);
        put_u32(&mut modules, 4 + 20, mod_name_rva);
        put_u32(&mut modules, 4 + 76, cv_bytes.len() as u32);
        put_u32(&mut modules, 4 + 80, cv_rva);
        let modules_rva = alloc(&mut payload, &modules);

        // ExceptionStream (168 bytes)
        let mut exc = vec![0u8; 168];
        put_u32(&mut exc, 0, 42); // ThreadId
        put_u32(&mut exc, 8, 0xC000_0005); // ExceptionCode
        put_u32(&mut exc, 160, ctx_bytes.len() as u32);
        put_u32(&mut exc, 164, ctx_rva);
        let exc_rva = alloc(&mut payload, &exc);

        // MiscInfoStream (24 bytes)
        let mut misc = vec![0u8; 24];
        put_u32(&mut misc, 0, 24);
        put_u32(&mut misc, 4, 3); // flags: PID | TIMES
        put_u32(&mut misc, 8, 902); // PID
        put_u32(&mut misc, 12, 1_794_926_671); // CreateTime
        let misc_rva = alloc(&mut payload, &misc);

        // MemoryListStream (stack + huge irrelevant data segment)
        let mut mem = vec![0u8; 4 + 2 * 16];
        put_u32(&mut mem, 0, 2);
        put_u64(&mut mem, 4, 0x0000_00F0_0000_1000);
        put_u32(&mut mem, 12, stack_bytes.len() as u32);
        put_u32(&mut mem, 16, stack_rva);
        put_u64(&mut mem, 20, 0x0000_7FF7_0000_0000);
        put_u32(&mut mem, 28, extra_memory_bytes as u32);
        put_u32(&mut mem, 32, bloat_rva);
        let mem_rva = alloc(&mut payload, &mem);

        let streams = [
            (STREAM_THREAD_LIST, threads.len() as u32, threads_rva),
            (STREAM_MODULE_LIST, modules.len() as u32, modules_rva),
            (STREAM_EXCEPTION, exc.len() as u32, exc_rva),
            (STREAM_MISC_INFO, misc.len() as u32, misc_rva),
            (STREAM_MEMORY_LIST, mem.len() as u32, mem_rva),
        ];

        let mut out = Vec::new();
        out.extend_from_slice(b"MDMP");
        out.extend_from_slice(&0xA793u32.to_le_bytes());
        out.extend_from_slice(&(streams.len() as u32).to_le_bytes());
        out.extend_from_slice(&32u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&1_794_926_671u32.to_le_bytes());
        out.extend_from_slice(&0x1u64.to_le_bytes()); // MiniDumpWithDataSegs
        for (kind, size, rva) in streams {
            out.extend_from_slice(&kind.to_le_bytes());
            out.extend_from_slice(&size.to_le_bytes());
            out.extend_from_slice(&rva.to_le_bytes());
        }
        out.extend_from_slice(&payload);
        out
    }

    #[test]
    fn slim_drops_bloated_memory_while_preserving_stack_exception_modules_and_misc() {
        let bloated = synthetic_bloated_dump(2 * 1024 * 1024);
        let slimmed = slim(&mut Cursor::new(&bloated)).unwrap();
        assert!(slimmed.len() < 4096, "expected < 4 KiB, got {} bytes", slimmed.len());
        assert_eq!(&slimmed[0..4], b"MDMP");
        assert_eq!(u64_le(&slimmed, 24), 0, "flags reset to MiniDumpNormal");
        // Verify stream contents and relocated RVAs.
        let count = u32_le(&slimmed, 8) as usize;
        let dir_rva = u32_le(&slimmed, 12) as usize;
        let mut saw_exc = false;
        let mut saw_stack = false;
        let mut saw_mod = false;
        let mut saw_misc = false;
        for i in 0..count {
            let base = dir_rva + i * 12;
            let kind = u32_le(&slimmed, base);
            let size = u32_le(&slimmed, base + 4) as usize;
            let rva = u32_le(&slimmed, base + 8) as usize;
            let data = &slimmed[rva..rva + size];
            match kind {
                STREAM_EXCEPTION => {
                    assert_eq!(u32_le(data, 8), 0xC000_0005);
                    let ctx_sz = u32_le(data, 160) as usize;
                    let ctx_rva = u32_le(data, 164) as usize;
                    assert_eq!(&slimmed[ctx_rva..ctx_rva + ctx_sz], &[0xAAu8; 64]);
                    saw_exc = true;
                }
                STREAM_THREAD_LIST => {
                    let s_sz = u32_le(data, 4 + 32) as usize;
                    let s_rva = u32_le(data, 4 + 36) as usize;
                    assert_eq!(&slimmed[s_rva..s_rva + s_sz], b"STACK-FRAME-PAYLOAD-1234567890");
                    saw_stack = true;
                }
                STREAM_MODULE_LIST => {
                    let cv_sz = u32_le(data, 4 + 76) as usize;
                    let cv_rva = u32_le(data, 4 + 80) as usize;
                    assert_eq!(&slimmed[cv_rva..cv_rva + cv_sz], b"RSDS\x01\x02\x03\x04eu4.pdb\0");
                    saw_mod = true;
                }
                STREAM_MISC_INFO => {
                    assert_eq!(u32_le(data, 8), 902);
                    assert_eq!(u32_le(data, 12), 1_794_926_671);
                    saw_misc = true;
                }
                STREAM_MEMORY_LIST => {
                    assert_eq!(u32_le(data, 0), 1, "only thread stack kept");
                }
                _ => {}
            }
        }
        assert!(saw_exc && saw_stack && saw_mod && saw_misc);
    }
}
