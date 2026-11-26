//! Pure byte-sum algorithm matching the game's memory stream checksum
//! (`0x171f3f0`), shared between save-checksum repairs and parallel save
//! checksumming without coupling those nodes.

/// The sum of bytes the save checksum is taken of, as a memory stream sums
/// them: its two halves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SaveSum {
    pub(crate) a: u32,
    pub(crate) b: u32,
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
pub(crate) fn step(s: SaveSum, byte: u8) -> SaveSum {
    let a = s.a + byte as u32;
    let a = if a > 0xffff { a >> 16 } else { a };
    let b = s.b + a;
    SaveSum { a, b: if b > 0xffff { b >> 16 } else { b } }
}

/// The bytes summed one after another, as the game sums them.
pub(crate) fn serial(mut s: SaveSum, data: &[u8]) -> SaveSum {
    for &byte in data {
        s = step(s, byte);
    }
    s
}
