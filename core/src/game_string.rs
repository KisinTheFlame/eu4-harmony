//! The game's `std::string`, MSVC's: 16 bytes that hold the text while its
//! capacity is at most [`INLINE`] and point to it otherwise, then its size
//! and its capacity. The text is game text (`unicode::game_text`), followed
//! by a NUL inside the capacity. The framework reads and changes the game's
//! strings only through here, and grows them only through the game's own
//! append, so the game frees them as usual.
use crate::{
    memory::Memory,
    points::{Function, Point},
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
const DESTROY_AT: usize = 0x95660;
/// The game's localisation lookup: (out, key, values, count); out is a
/// `std::string` slot it constructs, key a C string.
const LOCALIZE_AT: usize = 0x14b34a0;

pub fn points() -> Vec<Point> {
    // The append itself: the size and capacity, the inline test, the text.
    vec![Point::keep(
        APPEND_AT,
        "48 89 5c 24 18 55 56 57 41 55 41 56 48 83 ec 20 48 8b 69 18 49 8b f0 4c 8b 71 10 48 8b c5 49 2b c6 4c 8b ea 48 8b f9 \
         4c 3b c0 77 29 4b 8d 04 06 48 89 41 10 48 8b c1 48 83 fd 10 72 03 48 8b 01 4a 8d 1c 30 48 8b cb e8 14 24 9a 01 c6 04 33 00",
        "string append",
    )
    .recorded(APPEND.slot())
    .uses(&[CAPACITY, SIZE, INLINE + 1]),
    Point::keep(DESTROY_AT, "40 53 48 83 ec 20 48 8b 51 18 48 8b d9 48 83 fa", "string destructor").uses(&[CAPACITY]),
    Point::keep(LOCALIZE_AT, "44 89 4c 24 20 4c 89 44 24 18 55 56 57 41 54 41 55 41 56 41 57", "localisation lookup")]
}

/// Where the text of the string at `string` is, given its capacity.
fn text_at(string: usize, capacity: usize, pointer: impl FnOnce() -> usize) -> usize {
    if capacity <= INLINE { string } else { pointer() }
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
    let text = if capacity <= INLINE { string } else { m.ptr(string)? };
    m.bytes(text, size)
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
    m.call(m.at(DESTROY_AT), &[string]).map(|_| ())
}

/// The game's text for a localisation key, as its lookup writes it, with no
/// values: `key` is the address of the key, a C string.
pub fn localized(m: &Memory, key: usize) -> Result<Vec<u8>> {
    let mut slot = empty();
    let out = slot.as_mut_ptr() as usize;
    m.call(m.at(LOCALIZE_AT), &[out, key, 0, 0])?;
    let text = read(m, out);
    destroy_checked(m, out)?;
    text
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
