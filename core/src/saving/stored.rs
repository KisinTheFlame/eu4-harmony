//! A local save's entries copied into its zip's stored blocks
//! (docs/plans/game-speed.md, S8).
//!
//! During a local save the game deflates its entries at level 0
//! (`super::background`), which still passes the data through zlib and takes
//! its CRC a byte at a time, a quarter of an autosave's stop
//! (docs/research/saves.md, "Where a save's zip goes"). So at level 0 the
//! framework copies an entry's data into the zip's stored blocks itself: the
//! same entry, framed in blocks of its own, its CRC taken in parts by the
//! threads that write the game state (`super::parallel`) and combined.
use super::background::COPY_LEVEL;
use crate::called_by_game;
use crate::memory::Memory;
use crate::points::{Function, Point};
use crate::speed::SpeedUp;
use crate::zip::{crc32, crc32_combine};
use crate::Result;

/// `(zip, data stream, buffer, its size, &crc, level) -> bytes written`: an
/// entry's data deflated into the zip's stream (`[zip]`), from the start of
/// the data stream to its size, its CRC carried in the register at `crc`
/// (`!0` at the start), which it leaves inverted. Where it reads the stream's
/// size, moves to a place in it and reads it, and where it writes.
const DEFLATE: usize = 0x153c790;
const DEFLATE_READS: usize = 0x153c834;
const DEFLATE_WRITES: usize = 0x153c8f2;
/// A stream's methods: its size, a move to a place, a read, a write.
const STREAM_SIZE: usize = 0x88;
const STREAM_SEEK: usize = 0x60;
const STREAM_READ: usize = 0x20;
const STREAM_WRITE: usize = 0x50;
/// A stored block holds at most this many bytes; the framework reads and
/// writes sixteen at a time.
pub(super) const STORED_BLOCK: usize = 0xffff;
const STORED_BATCH: usize = 16 * STORED_BLOCK;

/// Left to the game, its deflate copies at level 0, through zlib.
pub const SPEED_UP: SpeedUp = SpeedUp { name: "stored-copy", group: "saving", points, checkable: false };

fn points() -> Vec<Point> {
    vec![
        Point::replace(DEFLATE, "48 8b c4 48 89 58 10 48 89 68 18 48 89 70 20 48", "zip deflate", deflate as Deflate, &ORIGINAL_DEFLATE),
        Point::keep(
            DEFLATE_READS,
            "49 8b 1e 49 8b ce ff 93 88 00 00 00 2b c7 49 8b ce 8b d0 ff 53 60 49 8b 06 41 3b fc 41 8b dc 48 8b d5 0f 4c df 49 8b ce 44 8b c3 ff 50 20",
            "zip deflate reads its data",
        )
        .uses(&[STREAM_SIZE, STREAM_SEEK, STREAM_READ]),
        Point::keep(DEFLATE_WRITES, "48 8b 84 24 d0 00 00 00 49 8d 14 2c 03 f3 4c 63 c3 48 8b 08 48 8b 01 ff 50 50", "zip deflate writes")
            .uses(&[STREAM_WRITE]),
    ]
}

type Deflate = extern "C" fn(usize, usize, usize, usize, *mut u32, i32) -> u32;
static ORIGINAL_DEFLATE: Function<Deflate> = Function::new();

/// The game's deflate, but at the level a local save's entries get, which
/// copies, the framework's copy into stored blocks.
#[called_by_game]
extern "C" fn deflate(zip: usize, data: usize, buffer: usize, size: usize, crc: *mut u32, level: i32) -> u32 {
    if level != COPY_LEVEL as i32 {
        return ORIGINAL_DEFLATE.get()(zip, data, buffer, size, crc, level);
    }
    crate::guarded("zip deflate", || match store(crate::hooked_memory(), zip, data, crc) {
        Ok(written) => written,
        Err(e) => {
            // What the game's deflate answers when zlib fails.
            log!("saves: an entry could not be stored: {e}");
            0
        }
    })
}

/// Copies the data stream into the zip's stream as stored blocks; the bytes
/// written.
fn store(m: Memory, zip: usize, data: usize, crc: *mut u32) -> Result<u32> {
    let size = m.call(m.vslot(data, STREAM_SIZE)?, &[data])? as u32 as usize;
    m.call(m.vslot(data, STREAM_SEEK)?, &[data, 0])?;
    let out = m.ptr(zip)?;
    let (read, write) = (m.vslot(data, STREAM_READ)?, m.vslot(out, STREAM_WRITE)?);
    let mut all = vec![0u8; size];
    for batch in all.chunks_mut(STORED_BATCH) {
        m.call(read, &[data, batch.as_mut_ptr() as usize, batch.len()])?;
    }
    // The register carried in, as the CRC of what came before.
    unsafe { *crc = crc32_combine(!*crc, crc_on_threads(&all), size) };
    let mut output = Vec::with_capacity(STORED_BATCH + STORED_BATCH / STORED_BLOCK * 5 + 5);
    let mut written = 0usize;
    let batches = all.chunks(STORED_BATCH).count().max(1);
    for (i, batch) in all.chunks(STORED_BATCH).chain(all.is_empty().then_some(&[][..])).enumerate() {
        output.clear();
        stored_blocks(batch, i + 1 == batches, &mut output);
        m.call(write, &[out, output.as_ptr() as usize, output.len()])?;
        written += output.len();
    }
    u32::try_from(written).map_err(|_| "an entry of 4 GB or more".to_string())
}

/// The CRC of `data`, its parts taken by the pool's threads and this one
/// and combined.
fn crc_on_threads(data: &[u8]) -> u32 {
    const FEWEST: usize = 4 << 20;
    const PARTS: usize = 32;
    if data.len() < FEWEST {
        return crc32(data);
    }
    let (at, len) = (data.as_ptr() as usize, data.len());
    let part = move |i: usize| i * len / PARTS..(i + 1) * len / PARTS;
    // `data` stays as it is until `each` returns.
    let crcs = super::parallel::each(PARTS, move |i| crc32(unsafe { std::slice::from_raw_parts((at + part(i).start) as *const u8, part(i).len()) }));
    match crcs {
        Some(crcs) => crcs.iter().enumerate().fold(0, |all, (i, &c)| crc32_combine(all, c, part(i).len())),
        None => crc32(data),
    }
}

/// `data` as deflate's stored blocks, the last of them marked final when
/// `last`.
pub(super) fn stored_blocks(data: &[u8], last: bool, out: &mut Vec<u8>) {
    if data.is_empty() {
        if last {
            out.extend_from_slice(&[1, 0, 0, 0xff, 0xff]);
        }
        return;
    }
    let mut blocks = data.chunks(STORED_BLOCK).peekable();
    while let Some(block) = blocks.next() {
        let len = block.len() as u16;
        out.push(u8::from(last && blocks.peek().is_none()));
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(block);
    }
}
