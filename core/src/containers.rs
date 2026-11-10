//! The game's containers as the framework reads them: its lists, the
//! `std::vector`s, its own arrays and every country. Each is read with one
//! set of rules: a count or a length the game keeps is bounded, a list that
//! loops back is refused, and a node the game removed while it was walking
//! the list, which it frees once it is done, is left out. A fix to how one of
//! them is read reaches every reader.
use crate::{
    memory::Memory,
    objects::{
        campaign::{countries, globals},
        list, map,
    },
    Result,
};
use std::collections::BTreeSet;

/// The most entries of a container believed where its reader names no
/// bound of its own.
pub const MOST: usize = 100_000;

/// The most countries the game has: one for every tag index.
const MOST_COUNTRIES: usize = i16::MAX as usize + 1;

/// The game's array type: its data at `+8`, its count (i32) at `+0x14`.
pub const ARRAY_DATA: usize = 0x8;
pub const ARRAY_COUNT: usize = 0x14;

/// One node of a list ([`list`]): where it is, and the item it holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Node {
    pub at: usize,
    pub item: usize,
}

/// The nodes of the game's list whose header is at `header`
/// (`objects::list`), in order: a node removed while the list was being
/// walked is left out, and a list that loops back or holds more than `most`
/// nodes is refused.
pub fn nodes(m: &Memory, header: usize, most: usize) -> Result<Vec<Node>> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    let mut node = m.ptr(header + list::HEAD)?;
    while node != 0 {
        if seen.len() >= most || !seen.insert(node) {
            return Err(format!("the list at {header:#x} loops or holds more than {most} nodes"));
        }
        if m.u8(node + list::REMOVED)? == 0 {
            out.push(Node { at: node, item: m.ptr(node + list::ITEM)? });
        }
        node = m.ptr(node + list::NEXT)?;
    }
    Ok(out)
}

/// The items of the game's list whose header is at `header`, as [`nodes`]
/// reads them, an empty one left out.
pub fn items(m: &Memory, header: usize, most: usize) -> Result<Vec<usize>> {
    Ok(nodes(m, header, most)?.into_iter().map(|n| n.item).filter(|&item| item != 0).collect())
}

/// The nodes of a chain from `first`, each with the next at `next`, in
/// order; a chain that loops back or holds more than `most` nodes is
/// refused.
pub fn chain(m: &Memory, first: usize, next: usize, most: usize) -> Result<Vec<usize>> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    let mut node = first;
    while node != 0 {
        if seen.len() >= most || !seen.insert(node) {
            return Err(format!("the chain from {first:#x} loops or holds more than {most} nodes"));
        }
        out.push(node);
        node = m.ptr(node + next)?;
    }
    Ok(out)
}

/// The nodes of a `std::list` whose head, the node that is no item's, is at
/// `head`, in order: each node has the next at `+0`, so from the head's next
/// round to the head. An `std::unordered_map` keeps its items in one, its
/// head at `+8`. A list that loops back elsewhere or holds more than `most`
/// nodes is refused.
pub fn linked(m: &Memory, head: usize, most: usize) -> Result<Vec<usize>> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    let mut node = m.ptr(head)?;
    while node != head {
        if node == 0 || seen.len() >= most || !seen.insert(node) {
            return Err(format!("the list at {head:#x} breaks, loops or holds more than {most} nodes"));
        }
        out.push(node);
        node = m.ptr(node)?;
    }
    Ok(out)
}

/// The nodes of a `std::map` whose head is at `head` (`objects::map`), each
/// with its value from `objects::map::VALUE`; a map of more than `most`
/// nodes, or one whose nodes lead back to one read before, is refused.
pub fn map(m: &Memory, head: usize, most: usize) -> Result<Vec<usize>> {
    let mut seen = BTreeSet::new();
    let mut pending = vec![m.ptr(head + map::PARENT)?];
    while let Some(at) = pending.pop() {
        if at == 0 || m.u8(at + map::NIL)? != 0 {
            continue;
        }
        if seen.len() >= most || !seen.insert(at) {
            return Err(format!("the map at {head:#x} loops or holds more than {most} nodes"));
        }
        pending.push(m.ptr(at + map::LEFT)?);
        pending.push(m.ptr(at + map::RIGHT)?);
    }
    Ok(seen.into_iter().collect())
}

/// How many `stride`-byte entries a `std::vector` from `begin` to `end`
/// holds, at most `most`.
pub fn span(begin: usize, end: usize, stride: usize, most: usize) -> Result<usize> {
    let bytes = end.checked_sub(begin).ok_or_else(|| "reversed vector".to_string())?;
    if bytes % stride != 0 || bytes / stride > most {
        return Err("vector bound or alignment".to_string());
    }
    Ok(bytes / stride)
}

/// Where each `stride`-byte entry of the `std::vector` at `vector` is, at
/// most `most`.
pub fn entries(m: &Memory, vector: usize, stride: usize, most: usize) -> Result<Vec<usize>> {
    let begin = m.ptr(vector)?;
    let count = span(begin, m.ptr(vector + 8)?, stride, most)?;
    Ok((0..count).map(|i| begin + i * stride).collect())
}

/// The pointers the `std::vector` at `vector` holds, at most `most`.
pub fn pointers(m: &Memory, vector: usize, most: usize) -> Result<Vec<usize>> {
    entries(m, vector, 8, most)?.into_iter().map(|at| m.ptr(at)).collect()
}

/// A count the game keeps, at most `most`.
pub fn bounded(count: i32, most: usize) -> Result<usize> {
    match usize::try_from(count) {
        Ok(n) if n <= most => Ok(n),
        _ => Err(format!("a count of {count}")),
    }
}

/// The pointers the game's own array at `array` holds, at most `most`.
pub fn array(m: &Memory, array: usize, most: usize) -> Result<Vec<usize>> {
    let (data, count) = (m.ptr(array + ARRAY_DATA)?, m.i32(array + ARRAY_COUNT)?);
    (0..bounded(count, most)?).map(|i| m.ptr(data + i * 8)).collect()
}

/// Every country of the game's world, by tag index; none before the game
/// has made its countries.
pub fn countries(m: &Memory) -> Result<Vec<usize>> {
    let manager = m.ptr(m.at(globals::COUNTRY_MANAGER))?;
    if manager == 0 {
        return Ok(Vec::new());
    }
    Ok(array(m, manager + countries::ARRAY, MOST_COUNTRIES)?.into_iter().filter(|&c| c != 0).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::buffers::Object;

    #[test]
    fn a_list_leaves_out_removed_nodes_and_refuses_a_loop() {
        let m = Memory { base: 0 };
        let (a, b, c) = (Object::new(8), Object::new(8), Object::new(8));
        let (a_at, c_at) = (a.at(), c.at());
        let header = Object::new(list::SIZE).list(0, vec![(a, false), (b, true), (c, false)]);
        assert_eq!(items(&m, header.at(), MOST).unwrap(), [a_at, c_at]);
        assert!(items(&m, header.at(), 2).is_err(), "three nodes are more than two");
        // The last node leading back to the first.
        let first = m.ptr(header.at() + list::HEAD).unwrap();
        let last = m.ptr(header.at() + list::TAIL).unwrap();
        m.write(last + list::NEXT, &(first as u64).to_le_bytes()).unwrap();
        assert!(items(&m, header.at(), MOST).unwrap_err().contains("loops"));
    }

    #[test]
    fn a_vector_and_an_array_are_bounded() {
        let m = Memory { base: 0 };
        let vector = Object::new(16).vector(0, &[1, 2, 3]);
        assert_eq!(pointers(&m, vector.at(), MOST).unwrap(), [1, 2, 3]);
        assert!(pointers(&m, vector.at(), 2).is_err());
        assert!(span(16, 8, 8, MOST).is_err() && span(0, 12, 8, MOST).is_err());
        let array = Object::new(0x18).vector(ARRAY_DATA, &[7, 8]).i32(ARRAY_COUNT, 2);
        assert_eq!(super::array(&m, array.at(), MOST).unwrap(), [7, 8]);
        assert!(bounded(-1, MOST).is_err() && bounded(3, 2).is_err());
    }

    #[test]
    fn a_map_holds_its_values() {
        let m = Memory { base: 0 };
        let head = Object::map(&[3, 1, 2]);
        let mut values: Vec<u64> = map(&m, head.at(), MOST).unwrap().into_iter().map(|node| m.u64(node + crate::objects::map::VALUE).unwrap()).collect();
        values.sort();
        assert_eq!(values, [1, 2, 3]);
        assert!(map(&m, head.at(), 2).is_err());
        assert!(map(&m, Object::map(&[]).at(), MOST).unwrap().is_empty());
    }

    #[test]
    fn a_chain_is_followed_and_a_loop_refused() {
        let m = Memory { base: 0 };
        let second = Object::new(16);
        let second_at = second.at();
        let first = Object::new(16).points_to(8, second);
        assert_eq!(chain(&m, first.at(), 8, MOST).unwrap(), [first.at(), second_at]);
        m.write(second_at + 8, &(first.at() as u64).to_le_bytes()).unwrap();
        assert!(chain(&m, first.at(), 8, MOST).is_err());
    }
}
