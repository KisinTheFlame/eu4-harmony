//! Temporary map highlights kept for the prepared frame. The in-game frame
//! clears province bit 4 and the country bitset after drawing (`0xe1ac80`).
//! With asynchronous drawing that is before the drawing thread reads them.
//!
//! Capture only those inputs at preparation. At `0x12ddac7`, after the border
//! classifier has written visibility to the game's real province flags, give
//! its remaining, read-only part a thread-local reader. It combines the live
//! province flags with the prepared temporary bit and keeps the prepared
//! country bitset. No pointer in a game object is changed; the game's other
//! readers and writers continue to use its arrays, including during Present.
//! See docs/research/map-drawing-reads.md, "The frame's temporary highlights".
use super::{is_drawing_thread, on_main};
use crate::called_by_game;
use crate::memory::Memory;
use crate::points::Point;
use std::cell::RefCell;
use std::sync::Mutex;

const MARKS: usize = 0xb8;
const PROVINCES: usize = MARKS + 8;
const PROVINCE_COUNT: usize = MARKS + 0x14;
const COUNTRIES: usize = MARKS + 0x38;
const COUNTRY_WORDS: usize = MARKS + 0x44;
const COUNTRY_LIMIT: usize = 0x300;
const TEMPORARY: u8 = 4;
// The classifier reads no field beyond its two display flags at +0x30c/d.
const READER_SIZE: usize = 0x310;
const READER_BEGINS: usize = 0x12ddac7;
crate::resume!(READER_RESUME: READER_GOES_ON = 0x12ddad7);

pub(super) fn points() -> Vec<Point> {
    vec![
        // The frame clears idler+0xb8 after its drawing returns.
        Point::keep(super::FRAME_DRAWN, "49 8d 8d b8 00 00 00 e8 53 53 60 00", "temporary map highlights cleared after drawing").uses(&[MARKS]),
        // Unless +0x256 holds them: clear province bit 4, zero the country words.
        Point::keep(
            0xe1ac80,
            "48 83 ec 28 80 b9 56 02 00 00 00 4c 8b d9 75 25 ba 04 00 00 00 e8 96 03 00 00 49 63 43 44 33 d2 49 8b 4b 38",
            "temporary map highlights cleared",
        )
        .uses(&[COUNTRIES - MARKS, COUNTRY_WORDS - MARKS, TEMPORARY as usize]),
        Point::keep(0xe1b030, "48 8b 41 08 45 33 d2 4c 63 41 14", "temporary province highlight bit cleared")
            .uses(&[PROVINCES - MARKS, PROVINCE_COUNT - MARKS]),
        // The reader starts after the visibility writes and before the border loop.
        Point::jump(
            READER_BEGINS,
            "41 0f b6 83 0d 03 00 00 41 0f b6 93 0c 03 00 00",
            "prepared temporary map highlights read by borders",
            reader_stub as *const () as usize,
            &READER_RESUME,
        ),
        Point::keep(0x12ddb0c, "49 8b 83 c0 00 00 00 0f b7 39 0f b7 71 02 44 0f b6 34 07 44 0f b6 24 06", "border classifier province flags")
            .uses(&[PROVINCES]),
        Point::keep(0x12ddba8, "41 8b 9b 00 03 00 00", "border classifier country highlight limit").uses(&[COUNTRY_LIMIT]),
        Point::keep(0x12ddbb6, "49 8b 83 f0 00 00 00", "border classifier first country highlight").uses(&[COUNTRIES]),
        Point::keep(0x12ddbd9, "49 8b 83 f0 00 00 00", "border classifier second country highlight").uses(&[COUNTRIES]),
        // These live inputs stay the classifier's: province grouping and selection.
        Point::keep(0x12ddb38, "49 8b 93 d8 00 00 00 4d 63 83 08 03 00 00", "border classifier live grouping inputs"),
        Point::keep(0x12ddbfb, "41 f6 c6 04 0f 85 92 00 00 00 41 f6 c4 04 0f 85 88 00 00 00", "temporary province highlight selects a border color")
            .uses(&[TEMPORARY as usize]),
    ]
}

/// Refreshed once per preparation, under the presentation lock. Storage is
/// retained across frames, but only a fully captured frame is valid. The source
/// identities also reject an array replaced or resized since preparation.
#[derive(Default)]
struct Kept {
    valid: bool,
    idler: usize,
    provinces_at: usize,
    countries_at: usize,
    provinces: Vec<u8>,
    countries: Vec<u8>,
    country_limit: u32,
}

static KEPT: Mutex<Option<Kept>> = Mutex::new(None);

fn array(m: &Memory, idler: usize, pointer: usize, count: usize, width: usize, bytes: &mut Vec<u8>) -> crate::Result<usize> {
    let count = m.u32(idler + count)? as i32;
    let address = m.ptr(idler + pointer)?;
    if count < 0 || (count > 0 && address == 0) {
        return Err(format!("temporary map highlights hold {count} entries at {address:#x}"));
    }
    bytes.resize(count as usize * width, 0);
    if !bytes.is_empty() {
        m.read_into(address, bytes)?;
    }
    Ok(address)
}

/// Keep the frame's inputs after preparation, before publishing its idler.
/// On an error, invalidate the previous frame without freeing its buffers.
pub(super) fn keep(m: &Memory, idler: usize) -> crate::Result<()> {
    if !on_main() {
        return Ok(());
    }
    let mut kept = KEPT.lock().unwrap_or_else(|e| e.into_inner());
    capture_into(m, idler, kept.get_or_insert_with(Kept::default))
}

fn capture_into(m: &Memory, idler: usize, kept: &mut Kept) -> crate::Result<()> {
    kept.valid = false;
    let provinces_at = array(m, idler, PROVINCES, PROVINCE_COUNT, 1, &mut kept.provinces)?;
    let countries_at = array(m, idler, COUNTRIES, COUNTRY_WORDS, 4, &mut kept.countries)?;
    let country_limit = m.u32(idler + COUNTRY_LIMIT)?;
    if country_limit as usize > kept.countries.len() * 8 {
        return Err("temporary country highlights have fewer bits than their limit".into());
    }
    kept.idler = idler;
    kept.provinces_at = provinces_at;
    kept.countries_at = countries_at;
    kept.country_limit = country_limit;
    kept.valid = true;
    Ok(())
}

/// The classifier's read-only input prefix and private arrays. Only the
/// drawing thread uses its reader, and nothing calls out after the stub:
/// its storage stays put until the classifier has returned. A later call
/// reuses it. The main thread's classifier always gets its original idler.
struct Reader {
    prefix: [u8; READER_SIZE],
    provinces: Vec<u8>,
    countries: Vec<u8>,
}

impl Default for Reader {
    fn default() -> Self {
        Self { prefix: [0; READER_SIZE], provinces: Vec::new(), countries: Vec::new() }
    }
}

thread_local! {
    static READER: RefCell<Reader> = RefCell::new(Reader::default());
}

fn pointer(prefix: &mut [u8], at: usize, value: usize) {
    prefix[at..at + 8].copy_from_slice(&(value as u64).to_le_bytes());
}

fn prepared_reader(m: &Memory, idler: usize, reader: &mut Reader, kept: &Kept) -> crate::Result<bool> {
    if !kept.valid
        || kept.idler != idler
        || m.ptr(idler + PROVINCES)? != kept.provinces_at
        || m.ptr(idler + COUNTRIES)? != kept.countries_at
        || m.u32(idler + PROVINCE_COUNT)? as usize != kept.provinces.len()
        || m.u32(idler + COUNTRY_WORDS)? as usize * 4 != kept.countries.len()
    {
        return Ok(false);
    }
    m.read_into(idler, &mut reader.prefix)?;
    reader.provinces.resize(kept.provinces.len(), 0);
    if !reader.provinces.is_empty() {
        m.read_into(kept.provinces_at, &mut reader.provinces)?;
    }
    for (live, prepared) in reader.provinces.iter_mut().zip(&kept.provinces) {
        *live = (*live & !TEMPORARY) | (*prepared & TEMPORARY);
    }
    reader.countries.clone_from(&kept.countries);
    let provinces = reader.provinces.as_ptr() as usize;
    let countries = reader.countries.as_ptr() as usize;
    pointer(&mut reader.prefix, PROVINCES, provinces);
    pointer(&mut reader.prefix, COUNTRIES, countries);
    reader.prefix[COUNTRY_LIMIT..COUNTRY_LIMIT + 4].copy_from_slice(&kept.country_limit.to_le_bytes());
    Ok(true)
}

#[called_by_game]
extern "C" fn border_reader(idler: usize) -> usize {
    if !is_drawing_thread() {
        return idler;
    }
    let kept = KEPT.lock().unwrap_or_else(|e| e.into_inner());
    let Some(kept) = kept.as_ref() else { return idler };
    let m = crate::hooked_memory();
    READER.with(|r| {
        let mut r = r.borrow_mut();
        match prepared_reader(&m, idler, &mut r, kept) {
            Ok(true) => r.prefix.as_ptr() as usize,
            Ok(false) => idler,
            Err(e) => {
                log!("drawing: the borders' temporary highlights could not be read: {e}");
                idler
            }
        }
    })
}

#[unsafe(naked)]
extern "C" fn reader_stub() {
    std::arch::naked_asm!(
        crate::call_rust!("mov rcx, r11", call border_reader, "mov r11, [rsp + 0x48]"),
        "movzx eax, byte ptr [r11 + 0x30d]",
        "movzx edx, byte ptr [r11 + 0x30c]",
        "jmp qword ptr [rip + {goes_on}]",
        border_reader = sym border_reader,
        goes_on = sym READER_GOES_ON,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capture(m: &Memory, idler: usize) -> crate::Result<Kept> {
        let mut kept = Kept::default();
        capture_into(m, idler, &mut kept)?;
        Ok(kept)
    }

    fn count(prefix: &mut [u8], at: usize, value: u32) {
        prefix[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }

    #[test]
    fn clearing_and_repeated_drawing_keep_only_the_prepared_temporary_inputs() {
        let m = Memory { base: 0 };
        let mut provinces = [4u8, 2, 0];
        let mut countries = [2u32];
        let mut prefix = [0u8; READER_SIZE];
        pointer(&mut prefix, PROVINCES, provinces.as_ptr() as usize);
        count(&mut prefix, PROVINCE_COUNT, 3);
        pointer(&mut prefix, COUNTRIES, countries.as_ptr() as usize);
        count(&mut prefix, COUNTRY_WORDS, 1);
        count(&mut prefix, COUNTRY_LIMIT, 32);
        let idler = prefix.as_ptr() as usize;
        let kept = capture(&m, idler).unwrap();
        // The real frame clears temporary highlights; classification changes
        // live visibility, and a different target may be marked on the main thread.
        provinces = [1, 2, 4];
        countries[0] = 0;
        let mut reader = Reader::default();
        assert!(prepared_reader(&m, idler, &mut reader, &kept).unwrap());
        assert_eq!(reader.provinces, [5, 2, 0]);
        assert_eq!(reader.countries, 2u32.to_le_bytes());
        assert_eq!(m.ptr(idler + PROVINCES).unwrap(), provinces.as_ptr() as usize);
        assert_eq!(m.ptr(idler + COUNTRIES).unwrap(), countries.as_ptr() as usize);
        assert_eq!(provinces, [1, 2, 4]);
        assert_eq!(countries, [0]);
        // A camera movement changes the live bit, even when drawing the same
        // prepared frame again. The real arrays still remain untouched.
        provinces[0] = 0;
        assert!(prepared_reader(&m, idler, &mut reader, &kept).unwrap());
        assert_eq!(reader.provinces, [4, 2, 0]);
        assert_eq!(provinces, [0, 2, 4]);
        // Preparing the next target replaces, rather than accumulates, marks.
        let next = capture(&m, idler).unwrap();
        provinces[2] = 0;
        assert!(prepared_reader(&m, idler, &mut reader, &next).unwrap());
        assert_eq!(reader.provinces, [0, 2, 4]);
        assert_eq!(reader.countries, [0; 4]);
        // A frame without a hovered target removes the last target's marks.
        let empty = capture(&m, idler).unwrap();
        assert!(prepared_reader(&m, idler, &mut reader, &empty).unwrap());
        assert_eq!(reader.provinces, [0, 2, 0]);
        assert_eq!(provinces, [0, 2, 0]);
    }

    #[test]
    fn preparation_keeps_capacity_when_mark_arrays_shrink() {
        let m = Memory { base: 0 };
        let provinces = [4u8; 128];
        let countries = [1u32; 16];
        let mut prefix = [0u8; READER_SIZE];
        pointer(&mut prefix, PROVINCES, provinces.as_ptr() as usize);
        count(&mut prefix, PROVINCE_COUNT, 128);
        pointer(&mut prefix, COUNTRIES, countries.as_ptr() as usize);
        count(&mut prefix, COUNTRY_WORDS, 16);
        count(&mut prefix, COUNTRY_LIMIT, 512);
        let idler = prefix.as_ptr() as usize;
        let mut kept = capture(&m, idler).unwrap();
        let capacity = (kept.provinces.capacity(), kept.countries.capacity());
        let buffers = (kept.provinces.as_ptr(), kept.countries.as_ptr());
        // Same size, smaller, empty, then larger again within the retained
        // capacity: every capture writes into the same two allocations.
        for (province_count, country_words) in [(128, 16), (64, 8), (0, 0), (128, 16)] {
            count(&mut prefix, PROVINCE_COUNT, province_count);
            count(&mut prefix, COUNTRY_WORDS, country_words);
            count(&mut prefix, COUNTRY_LIMIT, country_words * 32);
            capture_into(&m, idler, &mut kept).unwrap();
            assert!(kept.valid);
            assert_eq!((kept.provinces.capacity(), kept.countries.capacity()), capacity);
            assert_eq!((kept.provinces.as_ptr(), kept.countries.as_ptr()), buffers);
            assert_eq!(kept.provinces, provinces[..province_count as usize]);
            assert_eq!(kept.countries.len(), country_words as usize * 4);
        }
    }

    #[test]
    fn a_failed_capture_invalidates_marks_but_keeps_storage_for_recovery() {
        let m = Memory { base: 0 };
        let provinces = [4u8; 4];
        let countries = [1u32];
        let mut prefix = [0u8; READER_SIZE];
        pointer(&mut prefix, PROVINCES, provinces.as_ptr() as usize);
        count(&mut prefix, PROVINCE_COUNT, 4);
        pointer(&mut prefix, COUNTRIES, countries.as_ptr() as usize);
        count(&mut prefix, COUNTRY_WORDS, 1);
        count(&mut prefix, COUNTRY_LIMIT, 32);
        let idler = prefix.as_ptr() as usize;
        let mut kept = capture(&m, idler).unwrap();
        let buffers = (kept.provinces.as_ptr(), kept.countries.as_ptr());
        let capacity = (kept.provinces.capacity(), kept.countries.capacity());
        let mut reader = Reader::default();
        // A bad first read, a bad second read after the first buffer changed,
        // and invalid metadata after both reads all leave no usable frame.
        for field in [PROVINCES, COUNTRIES, COUNTRY_LIMIT] {
            if field == COUNTRY_LIMIT {
                count(&mut prefix, COUNTRY_LIMIT, 33);
            } else {
                pointer(&mut prefix, field, 1);
            }
            assert!(capture_into(&m, idler, &mut kept).is_err());
            assert!(!kept.valid);
            pointer(&mut prefix, PROVINCES, provinces.as_ptr() as usize);
            pointer(&mut prefix, COUNTRIES, countries.as_ptr() as usize);
            count(&mut prefix, COUNTRY_LIMIT, 32);
            // Even after the real input becomes readable again, the old
            // snapshot is unavailable until another full capture succeeds.
            assert!(!prepared_reader(&m, idler, &mut reader, &kept).unwrap());
            assert_eq!((kept.provinces.capacity(), kept.countries.capacity()), capacity);
            capture_into(&m, idler, &mut kept).unwrap();
            assert!(kept.valid);
            assert_eq!((kept.provinces.as_ptr(), kept.countries.as_ptr()), buffers);
            assert!(prepared_reader(&m, idler, &mut reader, &kept).unwrap());
        }
    }

    #[test]
    fn another_idler_or_replaced_arrays_cannot_use_old_highlights() {
        let m = Memory { base: 0 };
        let provinces = [4u8];
        let countries = [1u32];
        let mut prefix = [0u8; READER_SIZE];
        pointer(&mut prefix, PROVINCES, provinces.as_ptr() as usize);
        count(&mut prefix, PROVINCE_COUNT, 1);
        pointer(&mut prefix, COUNTRIES, countries.as_ptr() as usize);
        count(&mut prefix, COUNTRY_WORDS, 1);
        count(&mut prefix, COUNTRY_LIMIT, 32);
        let idler = prefix.as_ptr() as usize;
        let kept = capture(&m, idler).unwrap();
        let mut reader = Reader::default();
        let other = prefix;
        assert!(!prepared_reader(&m, other.as_ptr() as usize, &mut reader, &kept).unwrap());
        count(&mut prefix, PROVINCE_COUNT, 0);
        assert!(!prepared_reader(&m, idler, &mut reader, &kept).unwrap());
        count(&mut prefix, PROVINCE_COUNT, 1);
        let replaced = [0u8];
        pointer(&mut prefix, PROVINCES, replaced.as_ptr() as usize);
        assert!(!prepared_reader(&m, idler, &mut reader, &kept).unwrap());
        pointer(&mut prefix, PROVINCES, provinces.as_ptr() as usize);
        count(&mut prefix, COUNTRY_LIMIT, 33);
        assert!(capture(&m, idler).is_err());
    }
}
