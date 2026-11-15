//! Same-parent scene placement, preserving every fixed sibling's relative order.
//!
//! The caller owns presentation access and excludes native candidate/tree walks
//! for the whole snapshot-and-place operation. Instances must remain live here;
//! this module owns neither their logical identity nor their lifetime.
//! Internal GUI child lists and manager registration vectors are left unchanged.
//! Evidence: docs/research/window-placement.md.
use crate::{
    memory::Memory,
    points::{GameFunction, Point},
    Result,
};
use std::collections::{BTreeSet, HashMap, HashSet};

const MOST: usize = 100_000;
const PREVIOUS: usize = 0xf0;
const NEXT: usize = 0xf8;
const FIRST: usize = 0x100;
const LAST: usize = 0x108;
pub(super) const PARENT: usize = 0x110;
const DETACH_SLOT: usize = 0xe0;
const DETACH: GameFunction<1> = GameFunction::at(0x1562390);
const PREPEND: GameFunction<2> = GameFunction::at(0x1562480);
const INSERT_AFTER: GameFunction<2> = GameFunction::at(0x15624e0);

pub(super) fn points() -> Vec<Point> {
    vec![
        Point::keep(DETACH, "48 8b 91 f8 00 00 00 48 85 d2 74 0e 48 8b 81 f0 00 00 00", "detach a sibling scene node").uses(&[NEXT, PREVIOUS]),
        Point::keep(PREPEND, "48 89 5c 24 08 57 48 83 ec 20 48 8b 01 48 8b fa 48 8b d9 ff 90 e0 00 00 00", "prepend a sibling scene node").uses(&[DETACH_SLOT]),
        Point::keep(INSERT_AFTER, "48 8b 82 10 01 00 00 48 89 81 10 01 00 00 48 8b 82 f8 00 00 00", "insert a detached scene node after a sibling")
            .uses(&[PARENT, NEXT]),
        Point::keep(0x15623d0usize, "48 39 8a 00 01 00 00", "scene first child").uses(&[FIRST]),
        Point::keep(0x15623eeusize, "48 39 8a 08 01 00 00", "scene last child").uses(&[LAST]),
        Point::keep(0x15623c4usize, "48 8b 91 10 01 00 00", "scene parent").uses(&[PARENT]),
    ]
}

/// The complete sibling chain, including hidden nodes and unregistered anchors.
pub(super) fn snapshot(m: &Memory, parent: usize) -> Result<Vec<usize>> {
    snapshot_with(parent, |node, offset| {
        let at = node.checked_add(offset).ok_or("scene field address overflow")?;
        m.ptr(at)
    })
}

fn snapshot_with(parent: usize, mut read: impl FnMut(usize, usize) -> Result<usize>) -> Result<Vec<usize>> {
    if parent == 0 {
        return Err("a sibling scope has no parent".into());
    }
    let mut node = read(parent, FIRST)?;
    let last = read(parent, LAST)?;
    let mut previous = 0;
    let mut seen = HashSet::new();
    let mut siblings = Vec::new();
    while node != 0 {
        if node == parent || siblings.len() >= MOST || !seen.insert(node) {
            return Err("scene siblings loop or exceed the bound".into());
        }
        if read(node, PARENT)? != parent || read(node, PREVIOUS)? != previous {
            return Err("scene sibling parent or previous link is inconsistent".into());
        }
        siblings.push(node);
        previous = node;
        node = read(node, NEXT)?;
    }
    if last != previous {
        return Err("scene last-child link is inconsistent".into());
    }
    Ok(siblings)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Move {
    node: usize,
    after: Option<usize>,
}

/// Keep a longest increasing subsequence of desired positions in this interval
/// between fixed anchors. Every other node must move at least once.
fn keep_interval(candidates: &[(usize, usize)], kept: &mut HashSet<usize>) {
    let mut tails: Vec<usize> = Vec::new();
    let mut predecessor = vec![None; candidates.len()];
    for (index, &(_, position)) in candidates.iter().enumerate() {
        let length = tails.partition_point(|&tail| candidates[tail].1 < position);
        if length != 0 {
            predecessor[index] = Some(tails[length - 1]);
        }
        if length == tails.len() {
            tails.push(index);
        } else {
            tails[length] = index;
        }
    }
    let mut at = tails.last().copied();
    while let Some(index) = at {
        kept.insert(candidates[index].0);
        at = predecessor[index];
    }
}

/// A minimum number of node relocations, in O(n log n) time and O(n) storage.
/// Unmoved nodes form a common subsequence containing every fixed anchor. Its
/// longest possible length is the sum of the interval LIS lengths plus anchors.
fn moves(current: &[usize], desired: &[usize], movable: &BTreeSet<usize>) -> Result<Vec<Move>> {
    if current.len() > MOST || desired.len() > MOST {
        return Err("scene sibling sequence exceeds the bound".into());
    }
    let current_set: HashSet<usize> = current.iter().copied().collect();
    let desired_set: HashSet<usize> = desired.iter().copied().collect();
    if current_set.contains(&0) || current_set.len() != current.len() || desired_set.len() != desired.len() || current_set != desired_set {
        return Err("desired siblings must be a permutation of the complete current chain".into());
    }
    if !movable.iter().all(|node| current_set.contains(node)) {
        return Err("a movable node is absent from the sibling chain".into());
    }
    if !current.iter().filter(|node| !movable.contains(node)).eq(desired.iter().filter(|node| !movable.contains(node))) {
        return Err("desired siblings change the relative order of fixed anchors".into());
    }
    let mut desired_positions = HashMap::new();
    let mut interval = 0;
    for (position, &node) in desired.iter().enumerate() {
        desired_positions.insert(node, (position, interval));
        if !movable.contains(&node) {
            interval += 1;
        }
    }
    let mut kept = HashSet::new();
    let mut candidates = Vec::new();
    interval = 0;
    for &node in current {
        if movable.contains(&node) {
            let (position, destination) = desired_positions[&node];
            if destination == interval {
                candidates.push((node, position));
            }
        } else {
            keep_interval(&candidates, &mut kept);
            candidates.clear();
            kept.insert(node);
            interval += 1;
        }
    }
    keep_interval(&candidates, &mut kept);
    Ok(desired
        .iter()
        .enumerate()
        .filter(|(_, node)| !kept.contains(node))
        .map(|(i, &node)| Move { node, after: i.checked_sub(1).map(|previous| desired[previous]) })
        .collect())
}

/// Place checked movable nodes among fixed siblings.
/// A native error may leave a partial placement. This function retains no plan:
/// retry from a fresh snapshot. If a fault leaves a node detached or the chain
/// inconsistent, validation refuses retry rather than guessing how to repair it.
pub(super) fn apply(m: &Memory, parent: usize, desired: &[usize], movable: &BTreeSet<usize>) -> Result<()> {
    let current = snapshot(m, parent)?;
    let changes = moves(&current, desired, movable)?;
    // Prepend calls its receiver's virtual detach. Validate all targets before
    // the first mutation; the remaining operations are checked direct calls.
    for change in &changes {
        if change.after.is_none() && m.vslot(change.node, DETACH_SLOT)? != m.at(DETACH) {
            return Err("scene prepend has an unexpected virtual detach method".into());
        }
    }
    for change in &changes {
        match change.after {
            None => {
                PREPEND.call(m, [change.node, parent])?;
            }
            Some(previous) => {
                DETACH.call(m, [change.node])?;
                INSERT_AFTER.call(m, [change.node, previous])?;
            }
        }
    }
    if snapshot(m, parent)? != desired {
        return Err("native sibling placement did not produce the requested order".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(nodes: &[usize]) -> BTreeSet<usize> {
        nodes.iter().copied().collect()
    }

    fn apply(current: &mut Vec<usize>, changes: &[Move], movable: &BTreeSet<usize>) {
        for change in changes {
            assert!(movable.contains(&change.node));
            assert_ne!(change.after, Some(change.node));
            let from = current.iter().position(|&node| node == change.node).unwrap();
            current.remove(from);
            let to = change.after.map_or(0, |previous| current.iter().position(|&node| node == previous).unwrap() + 1);
            current.insert(to, change.node);
        }
    }

    fn check(current: &[usize], desired: &[usize], movable: &[usize], count: usize) {
        let movable = set(movable);
        let changes = moves(current, desired, &movable).unwrap();
        assert_eq!(changes.len(), count, "{current:?} -> {desired:?}");
        let mut actual = current.to_vec();
        apply(&mut actual, &changes, &movable);
        assert_eq!(actual, desired);
        assert!(moves(&actual, desired, &movable).unwrap().is_empty());
    }

    #[test]
    fn owned_windows_cross_original_anchors_in_both_directions() {
        check(&[1, 10, 2, 20, 3], &[1, 2, 3, 10, 20], &[10, 20], 2);
        check(&[1, 2, 3, 10, 20], &[1, 10, 2, 20, 3], &[10, 20], 2);
        check(&[1, 10, 2], &[10, 1, 2], &[10], 1);
    }

    #[test]
    fn complete_control_groups_move_behind_windows_but_above_the_fixed_background() {
        // 1 is the fixed host background; [10,11] and [20,21] are complete
        // two-node controls; 2 and 3 are original windows and fixed anchors.
        check(&[1, 2, 10, 11, 3, 20, 21], &[1, 10, 11, 20, 21, 2, 3], &[10, 11, 20, 21], 4);
        check(&[1, 10, 11, 20, 21, 2, 3], &[1, 20, 21, 2, 10, 11, 3], &[10, 11, 20, 21], 2);
    }

    #[test]
    fn longest_kept_subsequence_avoids_unnecessary_moves() {
        check(&[1, 2, 3, 4], &[4, 1, 2, 3], &[1, 2, 3, 4], 1);
        check(&[1, 2, 3, 4], &[2, 3, 4, 1], &[1, 2, 3, 4], 1);
        check(&[1, 10, 20, 30, 2], &[1, 30, 10, 20, 2], &[10, 20, 30], 1);
        check(&[], &[], &[], 0);
    }

    #[test]
    fn retries_after_every_complete_native_move_use_actual_order() {
        let current = vec![1, 10, 2, 20, 3, 30, 40];
        let desired = [1, 30, 40, 20, 2, 10, 3];
        let movable = set(&[10, 20, 30, 40]);
        let changes = moves(&current, &desired, &movable).unwrap();
        for completed in 0..=changes.len() {
            let mut partial = current.clone();
            apply(&mut partial, &changes[..completed], &movable);
            let retry = moves(&partial, &desired, &movable).unwrap();
            apply(&mut partial, &retry, &movable);
            assert_eq!(partial, desired);
        }
    }

    #[test]
    fn invalid_or_incomplete_sequences_are_refused() {
        for (current, desired, movable) in [
            (vec![1, 2], vec![2, 1], vec![]),
            (vec![1, 2], vec![1], vec![1]),
            (vec![1, 2], vec![1, 3], vec![1]),
            (vec![1, 2], vec![1, 1], vec![1]),
            (vec![1, 1], vec![1, 2], vec![1]),
            (vec![0, 1], vec![0, 1], vec![1]),
            (vec![1, 2], vec![1, 2], vec![3]),
        ] {
            assert!(moves(&current, &desired, &set(&movable)).is_err());
        }
        let too_many: Vec<_> = (1..=MOST + 1).collect();
        assert!(moves(&too_many, &too_many, &BTreeSet::new()).is_err());
    }

    #[test]
    fn snapshot_checks_both_directions_parent_and_endpoints() {
        let parent = 100;
        let original: HashMap<(usize, usize), usize> = [
            ((parent, FIRST), 1),
            ((parent, LAST), 2),
            ((1, PREVIOUS), 0),
            ((1, NEXT), 2),
            ((1, PARENT), parent),
            ((2, PREVIOUS), 1),
            ((2, NEXT), 0),
            ((2, PARENT), parent),
        ]
        .into_iter()
        .collect();
        let read = |fields: &HashMap<(usize, usize), usize>| {
            snapshot_with(parent, |node, offset| fields.get(&(node, offset)).copied().ok_or_else(|| "missing field".into()))
        };
        assert_eq!(read(&original).unwrap(), [1, 2]);
        for (key, bad) in [((1, PREVIOUS), 2), ((2, PREVIOUS), 0), ((2, PARENT), 101), ((2, NEXT), 1), ((parent, LAST), 1), ((parent, FIRST), parent)] {
            let mut fields = original.clone();
            fields.insert(key, bad);
            assert!(read(&fields).is_err(), "{key:?} -> {bad}");
        }
        assert!(snapshot_with(parent, |_, _| Ok(0)).unwrap().is_empty());
        assert!(snapshot_with(0, |_, _| Ok(0)).is_err());
    }

    fn permutations(values: &mut [usize], at: usize, visit: &mut impl FnMut(&[usize])) {
        if at == values.len() {
            visit(values);
            return;
        }
        for next in at..values.len() {
            values.swap(at, next);
            permutations(values, at + 1, visit);
            values.swap(at, next);
        }
    }

    #[test]
    fn all_five_node_orders_and_movable_sets_match_the_exhaustive_minimum() {
        let current = [1, 2, 3, 4, 5];
        let mut permuted = current;
        permutations(&mut permuted, 0, &mut |desired| {
            for mask in 0usize..32 {
                let movable: BTreeSet<_> = current.iter().enumerate().filter(|(i, _)| mask & (1 << i) != 0).map(|(_, &node)| node).collect();
                let anchors: Vec<_> = current.iter().filter(|node| !movable.contains(node)).copied().collect();
                let mut longest = None;
                // Independent oracle: enumerate every possible untouched
                // subsequence, requiring all anchors and desired-order equality.
                for kept in 0usize..32 {
                    let sequence: Vec<_> = current.iter().enumerate().filter(|(i, _)| kept & (1 << i) != 0).map(|(_, &node)| node).collect();
                    if anchors.iter().all(|node| sequence.contains(node)) && sequence.iter().eq(desired.iter().filter(|node| sequence.contains(node))) {
                        longest = Some(longest.unwrap_or(0).max(sequence.len()));
                    }
                }
                let planned = moves(&current, desired, &movable);
                match longest {
                    None => assert!(planned.is_err()),
                    Some(length) => {
                        let changes = planned.unwrap();
                        assert_eq!(changes.len(), current.len() - length);
                        let mut actual = current.to_vec();
                        apply(&mut actual, &changes, &movable);
                        assert_eq!(actual, desired);
                    }
                }
            }
        });
    }
}
