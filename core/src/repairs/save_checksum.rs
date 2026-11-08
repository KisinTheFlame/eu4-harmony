//! What the save checksum (GLOSSARY) of a save loaded in a session reads, in
//! vanilla too: the in-game load reads the save from disk, and a file's
//! stream sums its bytes otherwise than the memory stream the save was
//! written from, so such a save never matched and turned achievements off.
//! The file is summed as the save was.
use crate::called_by_game;
use crate::points::Point;
use crate::saving::sum::SaveSum;
use std::sync::atomic::{AtomicUsize, Ordering};

/// In the save checksum (`0x14e3390(out, stream, cut, salt, salt length)`),
/// the stream's sum of its bytes: `call r12`, the stream in `rbx`, how many
/// bytes in `edx` (-1 for all of them).
const SAVE_CHECKSUM_SUM: usize = 0x14e33f9;
const SAVE_CHECKSUM_SUM_DONE: usize = 0x14e3403;
/// A file's stream (vtable [`FILE_STREAM`]): its sum of its bytes, its
/// method at `+0x98`, and where it keeps the file.
const FILE_STREAM: usize = 0x1dbfa98;
const FILE_STREAM_SUM: usize = 0x1710c00;
const FILE_STREAM_SUM_METHOD: usize = 0x98;
const FILE_STREAM_FILE: usize = 0x38;

/// The game's file stream's sum, for the stub to tell a file's stream by.
static FILE_STREAM_SUM_AT: AtomicUsize = AtomicUsize::new(0);

/// A file's stream's sum of `count` of its bytes (-1 for all), as the memory
/// stream the save was written from summed them; the game's own sum when
/// the file cannot be read.
#[called_by_game]
unsafe extern "C" fn file_stream_sum(stream: usize, count: i32) -> u32 {
    let m = crate::hooked_memory();
    let mut sum = SaveSum::default();
    let read = m
        .ptr(stream + FILE_STREAM_FILE)
        .and_then(|holder| m.ptr(holder))
        .and_then(|file| crate::physfs::read_from_start(file, u64::try_from(count).ok(), |bytes| sum.add(bytes)));
    match read {
        Ok(()) => sum.value(),
        Err(e) => {
            log!("repairs: the save checksum of a file is summed by the game, which never matches the save's: {e}");
            m.call(FILE_STREAM_SUM_AT.load(Ordering::Relaxed), &[stream, count as u32 as usize]).map_or(0, |v| v as u32)
        }
    }
}

crate::resume!(SAVE_CHECKSUM_SUM_RESUME: SAVE_CHECKSUM_SUM_NEXT = SAVE_CHECKSUM_SUM_DONE);

/// Replaces the save checksum's sum of the stream's bytes: a file's stream
/// is summed by [`file_stream_sum`], any other by its own method.
#[unsafe(naked)]
extern "C" fn save_checksum_sum_stub() {
    std::arch::naked_asm!(
        "mov rcx, rbx",
        "cmp r12, qword ptr [rip + {file_sum}]",
        "jne 2f",
        crate::call_rust!(call sum, "mov eax, dword ptr [rsp + 0x48]"),
        "jmp 3f",
        "2:",
        "call r12",
        "3:",
        "mov byte ptr [rbp - 0x38], 0",
        "jmp qword ptr [rip + {next}]",
        file_sum = sym FILE_STREAM_SUM_AT,
        sum = sym file_stream_sum,
        next = sym SAVE_CHECKSUM_SUM_NEXT,
    );
}

/// The repair as the world reset sees it ([`super::REPAIRS`]): it counts
/// nothing.
pub(super) const REPAIR: super::Repair = super::Repair { points, since_reset: None };

pub(super) fn points() -> Vec<Point> {
    vec![
        // The save checksum calls the stream's sum, its method at +0x98; a
        // file's stream sums its file, which it keeps at +0x38, from a holder.
        Point::keep(0x14e33d5usize, "4c 8b a0 98 00 00 00", "save checksum stream sum method").uses(&[FILE_STREAM_SUM_METHOD]),
        Point::jump(
            SAVE_CHECKSUM_SUM,
            "48 8b cb 41 ff d4 c6 45 c8 00",
            "save checksum stream sum",
            save_checksum_sum_stub as *const () as usize,
            &SAVE_CHECKSUM_SUM_RESUME,
        ),
        Point::keep(SAVE_CHECKSUM_SUM_DONE, "45 33 e4 4c 89 64 24 70", "save checksum after the sum"),
        Point::method(FILE_STREAM, FILE_STREAM_SUM_METHOD, FILE_STREAM_SUM, "file stream sum method"),
        Point::keep(FILE_STREAM_SUM, "48 8b 49 38 e9 a7 ed ff ff", "file stream sum").uses(&[FILE_STREAM_FILE]).recorded(&FILE_STREAM_SUM_AT),
        Point::keep(0x170f9e0usize, "48 8b 09 e8 d8 74 2e 00", "file stream sum's file"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_save_is_summed_as_a_memory_stream_sums_it_in_any_pieces() {
        let bytes = [&[0xff; 300][..], b"checksum="].concat();
        let mut whole = SaveSum::default();
        whole.add(&bytes);
        // Past 0xffff a memory stream shifts right by 16, where a file's
        // stream would give 0x2e6541f9.
        assert_eq!(whole.value(), 0x2d67b397);
        let mut pieces = SaveSum::default();
        for piece in bytes.chunks(7) {
            pieces.add(piece);
        }
        assert_eq!(pieces.value(), whole.value());
    }
}
