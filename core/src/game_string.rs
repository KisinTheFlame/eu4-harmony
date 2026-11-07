//! The game's `std::string`, MSVC's: 16 bytes that hold the text while its
//! capacity is at most [`INLINE`] and point to it otherwise, then its size
//! and its capacity. The text is game text (`unicode::game_text`), followed
//! by a NUL inside the capacity. The framework reads and changes the game's
//! strings only through here, and grows them only through the game's own
//! append, so the game frees them as usual.
use crate::{
    memory::Memory,
    points::{Function, GameFunction, Point},
    Result,
};

pub const SIZE: usize = 0x10;
pub const CAPACITY: usize = 0x18;
/// The most text the 16 bytes hold.
pub const INLINE: usize = 15;
/// The string object itself.
pub const OBJECT: usize = 0x20;

/// The longest text [`read`] believes.
const MOST: usize = 1 << 20;

type Append = unsafe extern "C" fn(usize, *const u8, usize) -> usize;

/// The game's `append(this, bytes, count)`, recorded at startup.
static APPEND: Function<Append> = Function::new();
const APPEND_AT: usize = 0x932f0;
/// The game's destructor, which frees the text when it is not inline.
pub(crate) const DESTROY_AT: GameFunction<1> = GameFunction::at(0x95660);
/// The game's localisation lookup: (out, key, values, count); out is a
/// `std::string` slot it constructs, key a C string.
const LOCALIZE_AT: GameFunction<4> = GameFunction::at(0x14b34a0);
/// A value the lookup puts into a text where `$NAME$` stands, 0x68 bytes:
/// the name (a C string) at `+0`, its kind at `+0x28` (1 for text), the
/// text a `std::string` at `+0x30`. The game's constructor and destructor
/// of one, as its callers build an array of them on the stack.
const VALUE_NEW: GameFunction<1> = GameFunction::at(0x8f670);
const VALUE_DESTROY: GameFunction<1> = GameFunction::at(0x8ee90);
const VALUE: usize = 0x68;
const VALUE_KIND: usize = 0x28;
const VALUE_TEXT: usize = 0x30;
/// [`VALUE_KIND`] of a value given as text.
const TEXT_KIND: u64 = 1;
/// The localisation, and its lookup `(table, hash)` of a key's text, which
/// [`LOCALIZE_AT`] calls first.
const LOCALISATION: usize = 0x2357b20;
const FIND_TEXT: GameFunction<2> = GameFunction::at(0x16fb030);

pub fn points() -> Vec<Point> {
    // The append itself: the size and capacity, the inline test, the text.
    vec![
        Point::keep(
            APPEND_AT,
            "48 89 5c 24 18 55 56 57 41 55 41 56 48 83 ec 20 48 8b 69 18 49 8b f0 4c 8b 71 10 48 8b c5 49 2b c6 4c 8b ea 48 8b f9 \
         4c 3b c0 77 29 4b 8d 04 06 48 89 41 10 48 8b c1 48 83 fd 10 72 03 48 8b 01 4a 8d 1c 30 48 8b cb e8 14 24 9a 01 c6 04 33 00",
            "string append",
        )
        .recorded(APPEND.slot())
        .uses(&[CAPACITY, SIZE, INLINE + 1]),
        Point::keep(DESTROY_AT, "40 53 48 83 ec 20 48 8b 51 18 48 8b d9 48 83 fa", "string destructor").uses(&[CAPACITY]),
        Point::keep(LOCALIZE_AT, "44 89 4c 24 20 4c 89 44 24 18 55 56 57 41 54 41 55 41 56 41 57", "localisation lookup"),
        // The constructor clears the name, the kind and both strings.
        Point::keep(VALUE_NEW, "33 c0 48 c7 41 20 0f 00 00 00 48 89 01 48 89 41 08 48 89 41 18 89 41 28 48 89 41 30", "localisation value constructor")
            .uses(&[VALUE_KIND, VALUE_TEXT]),
        Point::keep(VALUE_DESTROY, "40 53 48 83 ec 20 48 8b d9 48 83 c1 30 e8 be 67 00 00 48 8d 4b 08", "localisation value destructor").uses(&[VALUE_TEXT]),
        Point::keep(0x14b34beusize, "48 8b 05 5b 46 ea 00", "localisation table").uses(&[LOCALISATION]),
        Point::keep(0x14b34fbusize, "41 8b d2 e8 2d 7b 24 00", "localisation text by hash"),
        Point::keep(FIND_TEXT, "4c 8b 41 20 4c 8b d2 48 8b 41 28 49 2b c0 48 c1 f8 04 48 85 c0 75 01 c3", "find a localisation text"),
    ]
}

/// Where the text of the string at `string` is, given its capacity.
fn text_at(string: usize, capacity: usize, pointer: impl FnOnce() -> usize) -> usize {
    if capacity <= INLINE {
        string
    } else {
        pointer()
    }
}

/// The text of the string at `string`, read without trusting it.
pub fn read(m: &Memory, string: usize) -> Result<Vec<u8>> {
    let (size, capacity) = (m.u64(string + SIZE)? as usize, m.u64(string + CAPACITY)? as usize);
    if size > MOST || capacity < size {
        return Err(format!("invalid game string at {string:#x}"));
    }
    if size == 0 {
        return Ok(Vec::new());
    }
    m.bytes(text_address(m, string)?, size)
}

/// Where the text of the string at `string` is: in the string itself, or
/// where it points; a NUL follows the text there.
pub fn text_address(m: &Memory, string: usize) -> Result<usize> {
    let capacity = m.u64(string + CAPACITY)? as usize;
    if capacity <= INLINE {
        Ok(string)
    } else {
        m.ptr(string)
    }
}

/// The text of the string at `string`, for hook code on the game's own
/// objects.
///
/// # Safety
/// `string` is a live `std::string` that nothing else changes while the
/// slice is in use.
pub unsafe fn text<'a>(string: *const u8) -> &'a [u8] {
    text_mut(string as *mut u8)
}

/// [`text`], to change the text in place (not its length).
///
/// # Safety
/// As [`text`], and nothing else reads the text while it changes.
pub unsafe fn text_mut<'a>(string: *mut u8) -> &'a mut [u8] {
    let size = (string.add(SIZE) as *const usize).read_unaligned();
    let capacity = (string.add(CAPACITY) as *const usize).read_unaligned();
    let text = text_at(string as usize, capacity, || (string as *const usize).read_unaligned()) as *mut u8;
    std::slice::from_raw_parts_mut(text, size)
}

/// Shortens the string at `string` to `size` bytes.
///
/// # Safety
/// `string` is a live `std::string` at least `size` long.
pub unsafe fn truncate(string: *mut u8, size: usize) {
    *text_mut(string).as_mut_ptr().add(size) = 0;
    *(string.add(SIZE) as *mut usize) = size;
}

/// Makes the string at `string`, which holds its text inline, hold `text`.
///
/// # Safety
/// `string` is a live `std::string` whose capacity is [`INLINE`], and
/// `text` is at most that long.
pub unsafe fn set_inline(string: *mut u8, text: &[u8]) {
    debug_assert!(text.len() <= INLINE);
    std::ptr::copy_nonoverlapping(text.as_ptr(), string, text.len());
    *string.add(text.len()) = 0;
    *(string.add(SIZE) as *mut usize) = text.len();
}

/// An empty string, as a value to build in place.
pub fn empty() -> [usize; OBJECT / 8] {
    let mut string = [0; OBJECT / 8];
    string[CAPACITY / 8] = INLINE;
    string
}

/// Whether the string at `string` is empty and has never grown.
pub fn untouched(m: &Memory, string: usize) -> Result<bool> {
    Ok(m.u64(string + SIZE)? == 0 && m.u64(string + CAPACITY)? == INLINE as u64)
}

/// Appends `text` to the string at `string`.
///
/// # Safety
/// `string` is a live `std::string` and startup has recorded the append.
pub unsafe fn append(string: usize, text: &[u8]) {
    APPEND.get()(string, text.as_ptr(), text.len());
}

/// [`append`], guarded: a fault in the game's code comes back as an error.
pub fn append_checked(m: &Memory, string: usize, text: &[u8]) -> Result<()> {
    m.call(APPEND.get() as usize, &[string, text.as_ptr() as usize, text.len()]).map(|_| ())
}

/// Destroys the string at `string` as the game does, guarded.
pub fn destroy_checked(m: &Memory, string: usize) -> Result<()> {
    DESTROY_AT.call(m, [string]).map(|_| ())
}

/// Has the game write text into a `std::string` of the framework's: `write`
/// is given the address of an empty one. Answers what `write` answered and
/// the text it left there; the string is destroyed as the game destroys its
/// own, also when `write` failed, before the failure is passed on.
pub fn written<T, E: From<String>>(m: &Memory, write: impl FnOnce(usize) -> std::result::Result<T, E>) -> std::result::Result<(T, Vec<u8>), E> {
    let mut string = empty();
    let at = string.as_mut_ptr() as usize;
    let wrote = write(at);
    let text = read(m, at);
    destroy_checked(m, at)?;
    Ok((wrote?, text?))
}

/// The game's lookup itself, into the `std::string` slot `out`, which it
/// constructs: the text of the key at `key`, a C string, with the `count`
/// values at `values`.
pub fn localize_into(m: &Memory, out: usize, key: usize, values: usize, count: usize) -> Result<()> {
    LOCALIZE_AT.call(m, [out, key, values, count]).map(|_| ())
}

/// The game's text for a localisation key, as its lookup writes it, with no
/// values: `key` is the address of the key, a C string.
pub fn localized(m: &Memory, key: usize) -> Result<Vec<u8>> {
    Ok(written(m, |out| LOCALIZE_AT.call(m, [out, key, 0, 0]))?.1)
}

/// The game's text for a localisation key with values, as its lookup writes
/// it: `key` is the address of the key, a C string, and each value a name (a
/// C string, `POWER_COST`) and the text that stands for `$POWER_COST$`,
/// formatted as the caller wants it.
pub fn localized_with(m: &Memory, key: usize, values: &[(&std::ffi::CStr, &[u8])]) -> Result<Vec<u8>> {
    let mut slots = vec![[0u64; VALUE / 8]; values.len()];
    let mut made = 0;
    let mut built = Ok(());
    for (slot, (name, text)) in slots.iter_mut().zip(values) {
        let at = slot.as_mut_ptr() as usize;
        if let Err(e) = VALUE_NEW.call(m, [at]) {
            built = Err(e);
            break;
        }
        made += 1;
        slot[0] = name.as_ptr() as u64;
        slot[VALUE_KIND / 8] = TEXT_KIND;
        if let Err(e) = append_checked(m, at + VALUE_TEXT, text) {
            built = Err(e);
            break;
        }
    }
    let text = built.and_then(|()| written(m, |out| LOCALIZE_AT.call(m, [out, key, slots.as_mut_ptr() as usize, values.len()])));
    for slot in &mut slots[..made] {
        VALUE_DESTROY.call(m, [slot.as_mut_ptr() as usize])?;
    }
    Ok(text?.1)
}

/// The localisation's text for `key` as its file has it, its values such as
/// `$COST$` not given, which [`localized`] leaves out; empty for a key it
/// does not have. The lookup finds a key by the FNV-1a hash of its bytes in
/// the table at [`LOCALISATION`].
pub fn unformatted(m: &Memory, key: &str) -> Result<Vec<u8>> {
    let hash = key.bytes().fold(0x811c_9dc5u32, |h, b| (h.wrapping_mul(0x0100_0193)) ^ (b as i8 as i32 as u32));
    let table = m.ptr(m.ptr(m.at(LOCALISATION))?)?;
    let text = FIND_TEXT.call(m, [table, hash as usize])?;
    if text == 0 {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    while out.len() < MOST {
        match m.u8(text + out.len())? {
            0 => return Ok(out),
            b => out.push(b),
        }
    }
    Err(format!("the text for {key} runs on"))
}

/// Builds `text` in `slot`, a string nothing has constructed yet; whoever
/// gets it frees it as the game's own.
///
/// # Safety
/// `slot` has room for a `std::string`, and startup has recorded the append.
pub unsafe fn construct(slot: *mut u8, text: &[u8]) {
    std::ptr::copy_nonoverlapping(empty().as_ptr() as *const u8, slot, OBJECT);
    append(slot as usize, text);
}

/// Makes the string at `string` hold `text`.
///
/// # Safety
/// `string` is a live `std::string` and startup has recorded the append.
pub unsafe fn assign(string: *mut u8, text: &[u8]) {
    truncate(string, 0);
    append(string as usize, text);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inline(text: &[u8]) -> [u8; OBJECT] {
        let mut string = [0u8; OBJECT];
        string[..text.len()].copy_from_slice(text);
        string[SIZE..SIZE + 8].copy_from_slice(&text.len().to_le_bytes());
        string[CAPACITY..CAPACITY + 8].copy_from_slice(&INLINE.to_le_bytes());
        string
    }

    fn on_heap(text: &[u8]) -> [u8; OBJECT] {
        let mut string = [0u8; OBJECT];
        string[..8].copy_from_slice(&(text.as_ptr() as usize).to_le_bytes());
        string[SIZE..SIZE + 8].copy_from_slice(&text.len().to_le_bytes());
        string[CAPACITY..CAPACITY + 8].copy_from_slice(&31usize.to_le_bytes());
        string
    }

    #[test]
    fn strings_hold_their_text_inline_or_on_the_heap() {
        let m = Memory { base: 0 };
        let heap = b"a longer string on the heap".to_vec();
        let (small, large) = (inline(b"hello"), on_heap(&heap));
        assert_eq!(read(&m, small.as_ptr() as usize).unwrap(), b"hello");
        assert_eq!(read(&m, large.as_ptr() as usize).unwrap(), heap);
        unsafe {
            assert_eq!(text(small.as_ptr()), b"hello");
            assert_eq!(text(large.as_ptr()), heap.as_slice());
        }
        let mut broken = inline(b"hi");
        broken[CAPACITY..CAPACITY + 8].copy_from_slice(&1usize.to_le_bytes());
        assert!(read(&m, broken.as_ptr() as usize).is_err());
    }

    #[test]
    fn strings_change_in_place() {
        let m = Memory { base: 0 };
        let mut string = inline(b"hello");
        unsafe {
            truncate(string.as_mut_ptr(), 2);
            assert_eq!(text(string.as_ptr()), b"he");
            assert_eq!(string[2], 0);
            set_inline(string.as_mut_ptr(), "中".as_bytes());
            assert_eq!(text(string.as_ptr()), "中".as_bytes());
        }
        let empty = empty();
        assert!(untouched(&m, empty.as_ptr() as usize).unwrap());
        assert!(!untouched(&m, string.as_ptr() as usize).unwrap());
    }
}
