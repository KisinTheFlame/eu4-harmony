//! The game's own heap: its `operator new` and `operator delete`, for blocks
//! the game frees or that the framework frees as the game made them. Every
//! part of the framework that allocates or frees the game's memory does it
//! through here.
use crate::{
    memory::Memory,
    points::{GameFunction, Point},
    Result,
};

/// `(size)`: the game's `operator new`.
pub const NEW: GameFunction<1> = GameFunction::at(0x1a332d4);
/// `(block, size)`: the game's `operator delete`.
const DELETE: GameFunction<2> = GameFunction::at(0x17394f0);

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(NEW, "40 53 48 83 ec 20 48 8b d9 eb 0f 48 8b cb e8 95", "operator new"),
        Point::keep(DELETE, "e9 2b 45 02 00 cc cc cc cc cc cc cc cc cc cc cc", "operator delete"),
    ]
}

/// A block of `size` bytes from the game's heap; 0 when it has none.
pub fn new(m: &Memory, size: usize) -> Result<usize> {
    NEW.call(m, [size])
}

/// Frees `block`, of `size` bytes, as the game frees one.
pub fn delete(m: &Memory, block: usize, size: usize) -> Result<()> {
    DELETE.call(m, [block, size]).map(|_| ())
}

/// Frees the buffer of a `std::vector` the game grew, (start, end, end of
/// capacity), as the game frees one: a large one is aligned, with the block
/// it was cut from just before it. Its items are left as they are.
pub fn free_vector(m: &Memory, vector: [usize; 3]) -> Result<()> {
    if vector[0] != 0 {
        let mut size = vector[2] - vector[0];
        let mut block = vector[0];
        if size >= 0x1000 {
            size += 0x27;
            block = m.ptr(block - 8)?;
        }
        delete(m, block, size)?;
    }
    Ok(())
}
