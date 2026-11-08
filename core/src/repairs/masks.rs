//! What the textures' transparency masks keep, in vanilla too: a texture
//! ID's mask keeps the buffer of the largest texture the ID was ever lent
//! to, and IDs pass from texture to texture as they load again. The buffer
//! is let go before a texture smaller than it is written to it.
use super::LettingGo;
use crate::{
    hooks::Call,
    objects::gui::transparency_mask,
    points::{GameFunction, Point},
};
use std::sync::atomic::{AtomicUsize, Ordering};

/// `(bits, count)`: a `std::vector<bool>`'s resize; and where the texture
/// loader resizes a transparency mask to its texture's pixels, the return
/// address of that call. `(words)`: the destructor of a vector of u32, which
/// frees its buffer and leaves it empty.
const BIT_VECTOR_RESIZE: usize = 0x6e05c0;
const MASK_RESIZED: usize = 0x16c302d;
const WORDS_DESTRUCTOR: GameFunction<1> = GameFunction::at(0x943e0);

/// The bytes of transparency masks let go since the last world reset.
static MASK_BYTES: AtomicUsize = AtomicUsize::new(0);

/// The repair as the world reset sees it ([`super::REPAIRS`]).
pub(super) const REPAIR: super::Repair = super::Repair { points, since_reset: Some(since_reset) };

/// What the repair let go since the last world reset, for the log; its
/// counts start again.
fn since_reset() -> String {
    format!("{:.1} MB of transparency masks were let go", MASK_BYTES.swap(0, Ordering::Relaxed) as f64 / (1024.0 * 1024.0))
}

/// A vector of bits is resized: when it is a transparency mask the texture
/// loader is about to write a texture to, and its buffer holds more than the
/// texture needs, the buffer is let go first, so that the resize makes one
/// as large as the texture. The loader writes every bit the texture has, so
/// nothing of the old mask is needed.
fn mask_resizing(call: &mut Call) {
    let m = call.memory();
    if call.caller() != m.at(MASK_RESIZED) {
        return;
    }
    let (mask, bits) = (call.arg(0), call.arg(1));
    let let_go = (|| {
        let (begin, capacity) = (m.ptr(mask + transparency_mask::WORDS)?, m.ptr(mask + transparency_mask::CAPACITY)?);
        let needed = bits.div_ceil(32) * 4;
        if begin == 0 || capacity <= begin + needed {
            return Ok(0);
        }
        let _letting_go = LettingGo::new("transparency mask");
        WORDS_DESTRUCTOR.call(&m, [mask + transparency_mask::WORDS])?;
        m.write(mask + transparency_mask::BITS, &0usize.to_le_bytes())?;
        crate::Result::Ok(capacity - begin)
    })();
    match let_go {
        Ok(bytes) => {
            MASK_BYTES.fetch_add(bytes, Ordering::Relaxed);
        }
        Err(e) => log!("repair: a transparency mask was not let go: {e}"),
    }
}

pub(super) fn points() -> Vec<Point> {
    vec![
        // The texture loader resizes the transparency mask of the texture's
        // ID to its pixels, then writes every bit.
        Point::observe(BIT_VECTOR_RESIZE, "48 89 5c 24 10 57 48 83 ec 50 4c 8b 41 18 48 8b da 48 8b f9", "vector of bits resize", 2, Some(mask_resizing), None)
            .uses(&[transparency_mask::BITS]),
        Point::keep(0x16c3025usize, "49 8b cf e8 93 d5 01 ff 4d 8b 6e 18", "transparency mask resized for a texture"),
        Point::keep(0x16c3079usize, "49 8b 0f 8b c2 48 c1 e8 05 4c 8d 14 81", "transparency mask's bits written"),
        Point::keep(WORDS_DESTRUCTOR, "40 53 48 83 ec 20 48 8b d9 48 8b 09 48 85 c9 74 3e 48 8b 53 10 48 2b d1", "vector of u32 destructor")
            .uses(&[transparency_mask::CAPACITY]),
    ]
}
