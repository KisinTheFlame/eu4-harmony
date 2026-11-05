//! The save checksum's sum taken on several threads (docs/game-speed.md, S8).
//!
//! The save checksum is an MD5 of a sum of the entry's bytes
//! (docs/research/saves.md, "The checksum"). The sum, a memory stream's method
//! `0x171f3f0`, takes a byte at a time: `a = a + byte`, then `b = b + a`, each
//! kept as it is up to 0xffff and made 1 above it; each step waits for the
//! one before it, 44 ms for a game state of 55 MB.
//!
//! `a` depends on the bytes alone, and bytes that do not take it past 0xffff
//! add their sum to it, so the framework finds `a` where each part of the
//! stream begins eight bytes at a step. `b` depends on `a` and on itself, but
//! each time it passes 0xffff it is 1 whatever it was, which happens every few
//! bytes while `a` is large. So the pool sums each part from its own `a` and
//! `b` taken as 1; then, part after part, the sum is taken again from the `b`
//! the part before it ended with, beside the guess, until the two agree,
//! after which they are the same. The sum is the game's to the bit.
use super::parallel;
use crate::memory::Memory;
use crate::points::{Function, Point};
use crate::Result;
use std::ops::Range;

/// `(stream, count) -> sum`: from the stream's start, `count` bytes, or all
/// of them for -1; it moves to the start first and stands after what it
/// summed.
const SUM: usize = 0x171f3f0;
/// The stream's method that moves to its start, which the sum calls first.
const TO_START: usize = 0x70;
/// A memory stream's growing part, buffer, bytes kept in the buffer and
/// where it stands (`super::parallel`).
const GROWING: usize = 0x38;
const BUFFER: usize = 0x40;
const KEPT: usize = 0x48;
const AT: usize = 0x4c;

/// Below this many bytes the game's own sum is as fast.
const FEWEST: usize = 4 << 20;
/// Parts for each thread, so that a slow thread holds up the rest less.
const PARTS_A_THREAD: usize = 4;
/// What `b` is taken to be where a part begins.
const GUESS: u32 = 1;

pub fn points() -> Vec<Point> {
    vec![
        Point::replace(SUM, "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 48", "memory stream's sum", sum as Sum, &ORIGINAL_SUM),
        // The rest of it: the count, the move to the start, the buffer's
        // bytes and then the growing part's, each byte's step, the place it
        // stands and the sum as `a << 16 | b`.
        Point::keep(
            SUM + 0x10,
            "89 7c 24 20 41 56 48 83 ec 20 45 33 f6 8b ea 48 8b f1 41 8b de 41 8b fe 83 fa ff 75 18 48 8b 41 38 48 85 c0 74 07 8b 48 08 2b 08 \
             eb 03 41 8b ce 8b 6e 48 03 e9 48 8b 06 48 8b ce ff 50 70 8b 46 48 85 c0 7e 60 48 8b 56 40 3b c5 44 8b c5 44 0f 4c c0 41 8b c6 45 \
             85 c0 7e 45 4d 63 c8 4c 03 ca 49 3b d1 74 37 66 66 66 0f 1f 84 00 00 00 00 00 0f b6 0a 48 ff c2 03 cb 8b d9 c1 eb 10 81 f9 ff ff \
             00 00 0f 46 d9 8d 0c 3b 8b f9 c1 ef 10 81 f9 ff ff 00 00 0f 46 f9 49 3b d1 75 d4 41 8b c0 01 46 4c 41 2b e8 48 8b 46 38 48 85 c0 \
             74 55 48 8b 10 8b 40 08 2b c2 3b c5 0f 4c e8 85 ed 7e 40 4c 63 c5 4c 03 c2 49 3b d0 74 32 66 0f 1f 44 00 00 0f b6 0a 48 ff c2 03 \
             cb 8b d9 c1 eb 10 81 f9 ff ff 00 00 0f 46 d9 8d 0c 3b 8b f9 c1 ef 10 81 f9 ff ff 00 00 0f 46 f9 49 3b d0 75 d4 44 8b f5 44 01 76 \
             4c 48 8b 6c 24 38 48 8b 74 24 40 c1 e3 10 0b df 48 8b 7c 24 48 8b c3 48 8b 5c 24 30 48 83 c4 20 41 5e c3",
            "memory stream's sum, its steps",
        )
        .uses(&[TO_START, GROWING, BUFFER, KEPT, AT]),
    ]
}

type Sum = extern "C" fn(usize, i32) -> u32;
static ORIGINAL_SUM: Function<Sum> = Function::new();

extern "C" fn sum(stream: usize, count: i32) -> u32 {
    let m = crate::hooked_memory();
    let count = match counted(m, stream, count) {
        Ok(count) if count > 0 && count as usize >= FEWEST && !super::game_writes_saves() => count,
        _ => return ORIGINAL_SUM.get()(stream, count),
    };
    crate::guarded("save checksum sum", || match take(m, stream, count) {
        Ok(sums) => sums.value(),
        // Before anything was done: the memory the stream names is not there.
        Err(e) => {
            log!("saves: the save checksum's sum: {e}");
            ORIGINAL_SUM.get()(stream, count)
        }
    })
}

/// How many bytes the game's sum takes: `count`, or for -1 what the buffer
/// keeps and the growing part holds.
fn counted(m: Memory, stream: usize, count: i32) -> Result<i32> {
    if count != -1 {
        return Ok(count);
    }
    Ok(m.i32(stream + KEPT)?.wrapping_add(growing_len(m, stream)?))
}

/// The growing part's length, as the game takes it: the low halves of its
/// end and start.
fn growing_len(m: Memory, stream: usize) -> Result<i32> {
    match m.ptr(stream + GROWING)? {
        0 => Ok(0),
        growing => Ok(m.i32(growing + 8)?.wrapping_sub(m.i32(growing)?)),
    }
}

/// What the game's sum does, the bytes summed in parts by the pool.
fn take(m: Memory, stream: usize, mut count: i32) -> Result<SaveSum> {
    let to_start = m.vslot(stream, TO_START)?;
    m.call(to_start, &[stream])?;
    let mut sums = SaveSum { a: 0, b: 0 };
    let kept = m.i32(stream + KEPT)?;
    if kept > 0 {
        let len = kept.min(count);
        if len > 0 {
            sums = summed_by_pool(sums, unsafe { std::slice::from_raw_parts(m.ptr(stream + BUFFER)? as *const u8, len as usize) });
            add_at(stream, len);
        }
        count -= len;
    }
    let growing = m.ptr(stream + GROWING)?;
    if growing != 0 {
        let len = growing_len(m, stream)?.min(count);
        if len > 0 {
            sums = summed_by_pool(sums, unsafe { std::slice::from_raw_parts(m.ptr(growing)? as *const u8, len as usize) });
            add_at(stream, len);
        }
    }
    Ok(sums)
}

fn add_at(stream: usize, len: i32) {
    unsafe {
        let at = (stream + AT) as *mut i32;
        *at = (*at).wrapping_add(len);
    }
}

/// The sum of bytes the save checksum is taken of, as a memory stream sums
/// them: its two halves.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct SaveSum {
    a: u32,
    b: u32,
}

impl SaveSum {
    pub(crate) fn add(&mut self, bytes: &[u8]) {
        *self = serial(*self, bytes);
    }

    pub(crate) fn value(&self) -> u32 {
        self.a << 16 | self.b
    }
}

/// One byte, as the game's sum takes it.
fn step(s: SaveSum, byte: u8) -> SaveSum {
    let a = s.a + byte as u32;
    let a = if a > 0xffff { a >> 16 } else { a };
    let b = s.b + a;
    SaveSum { a, b: if b > 0xffff { b >> 16 } else { b } }
}

/// The bytes summed one after another, as the game sums them.
fn serial(mut s: SaveSum, data: &[u8]) -> SaveSum {
    for &byte in data {
        s = step(s, byte);
    }
    s
}

/// `a` after the bytes: eight at a step, unless they take it past 0xffff.
fn advance_a(mut a: u32, data: &[u8]) -> u32 {
    let mut eights = data.chunks_exact(8);
    for eight in &mut eights {
        let x = u64::from_le_bytes(eight.try_into().unwrap());
        // Four sums of two bytes, then their sum, in the top 16 bits.
        let pairs = (x & 0x00ff_00ff_00ff_00ff) + ((x >> 8) & 0x00ff_00ff_00ff_00ff);
        let total = (pairs.wrapping_mul(0x0001_0001_0001_0001) >> 48) as u32;
        if a + total <= 0xffff {
            a += total;
        } else {
            a = eight.iter().fold(a, |a, &byte| step(SaveSum { a, b: 0 }, byte).a);
        }
    }
    eights.remainder().iter().fold(a, |a, &byte| step(SaveSum { a, b: 0 }, byte).a)
}

/// The sums after `data` from `start`, `data` cut into `parts`: each part
/// summed from its own `a` and a guessed `b` (the first from `start`) by
/// `sum_parts`, then settled part after part. When `sum_parts` gives nothing,
/// the bytes are summed one after another.
fn summed(start: SaveSum, data: &[u8], parts: &[Range<usize>], sum_parts: impl FnOnce(&[SaveSum]) -> Option<Vec<SaveSum>>) -> SaveSum {
    let mut firsts = Vec::with_capacity(parts.len());
    let mut a = start.a;
    for (i, part) in parts.iter().enumerate() {
        firsts.push(SaveSum { a, b: if i == 0 { start.b } else { GUESS } });
        a = advance_a(a, &data[part.clone()]);
    }
    let Some(guessed) = sum_parts(&firsts).filter(|g| g.len() == parts.len()) else { return serial(start, data) };
    let mut sums = guessed[0];
    for i in 1..parts.len() {
        sums = settle(SaveSum { a: firsts[i].a, b: sums.b }, firsts[i], &data[parts[i].clone()], guessed[i]);
    }
    sums
}

/// The sums at the end of a part from where it truly begins, given what it
/// summed to from a guess: the two side by side until they agree.
fn settle(mut truth: SaveSum, mut guess: SaveSum, data: &[u8], guessed: SaveSum) -> SaveSum {
    for &byte in data {
        if truth == guess {
            return guessed;
        }
        truth = step(truth, byte);
        guess = step(guess, byte);
    }
    truth
}

/// `data` cut into `count` parts in order.
fn parts(len: usize, count: usize) -> Vec<Range<usize>> {
    let count = count.clamp(1, len.max(1));
    (0..count).map(|i| i * len / count..(i + 1) * len / count).collect()
}

/// The sums after `data`, its parts summed by the pool's threads and this
/// one.
fn summed_by_pool(start: SaveSum, data: &[u8]) -> SaveSum {
    let width = parallel::pool().map_or(1, |p| p.width());
    let parts = parts(data.len(), width * PARTS_A_THREAD);
    // The game's bytes, which stay as they are while the sum is taken.
    let at = data.as_ptr() as usize;
    summed(start, data, &parts, |firsts| {
        let (parts, firsts) = (parts.clone(), firsts.to_vec());
        parallel::each(parts.len(), move |i| {
            let part = unsafe { std::slice::from_raw_parts((at + parts[i].start) as *const u8, parts[i].len()) };
            serial(firsts[i], part)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bytes of a save's kind: text and numbers, long runs of zeros, and
    /// some of every value.
    fn bytes(len: usize, seed: u64) -> Vec<u8> {
        const TEXT: &[u8] = b"0123456789.={}
	abcdef";
        let mut x = seed;
        let mut next = move || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        let mut out = Vec::with_capacity(len);
        while out.len() < len {
            let n = (next() % 4000) as usize;
            match next() % 4 {
                0 => out.extend(std::iter::repeat_n(0, n)),
                1 => out.extend(std::iter::repeat_n(0xff, n / 8)),
                2 => out.extend((0..n).map(|_| TEXT[(next() % TEXT.len() as u64) as usize])),
                _ => out.extend((0..n).map(|_| next() as u8)),
            }
        }
        out.truncate(len);
        out
    }

    #[test]
    fn a_eight_bytes_at_a_step_is_the_games() {
        for seed in 1..20 {
            let data = bytes(100_003, seed);
            for start in [0, 1, 0xfff0, 0xffff, 12345] {
                assert_eq!(advance_a(start, &data), serial(SaveSum { a: start, b: 0 }, &data).a, "seed {seed}, a {start}");
            }
        }
    }

    /// Each part summed from where `summed` says it begins.
    fn side_by_side<'a>(data: &'a [u8], parts: &'a [Range<usize>]) -> impl FnOnce(&[SaveSum]) -> Option<Vec<SaveSum>> + 'a {
        move |firsts| Some(firsts.iter().zip(parts).map(|(f, p)| serial(*f, &data[p.clone()])).collect())
    }

    #[test]
    fn a_sum_in_parts_is_the_games() {
        for seed in 1..12 {
            let data = bytes(300_001, seed);
            for count in [1, 2, 7, 64, 1000, 300_001] {
                let parts = parts(data.len(), count);
                for start in [SaveSum { a: 0, b: 0 }, SaveSum { a: 0xffff, b: 0xfffe }, SaveSum { a: 77, b: 1 }] {
                    assert_eq!(summed(start, &data, &parts, side_by_side(&data, &parts)), serial(start, &data), "seed {seed}, {count} parts");
                }
            }
        }
        // Where `b` never passes 0xffff the guess never agrees, and the part
        // is summed again to its end.
        for data in [vec![0u8; 50_000], vec![0xff; 50_000], [vec![1u8], vec![0; 9_999]].concat()] {
            let parts = parts(data.len(), 9);
            let start = SaveSum { a: 0, b: 0 };
            assert_eq!(summed(start, &data, &parts, side_by_side(&data, &parts)), serial(start, &data));
        }
    }

    #[test]
    fn a_step_is_the_games() {
        // `a` and `b` past 0xffff are made 1, not taken modulo.
        assert_eq!(step(SaveSum { a: 0xffff, b: 0 }, 1), SaveSum { a: 1, b: 1 });
        assert_eq!(step(SaveSum { a: 0xff00, b: 0x0200 }, 0xff), SaveSum { a: 0xffff, b: 1 });
        assert_eq!(step(SaveSum { a: 5, b: 6 }, 7), SaveSum { a: 12, b: 18 });
    }

    #[test]
    fn parts_cover_the_bytes_in_order() {
        for (len, count) in [(0, 8), (1, 8), (10, 3), (1 << 20, 32)] {
            let parts = parts(len, count);
            assert_eq!(parts.first().unwrap().start, 0);
            assert_eq!(parts.last().unwrap().end, len);
            assert!(parts.windows(2).all(|w| w[0].end == w[1].start));
        }
    }
}
