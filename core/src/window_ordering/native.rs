//! Native first-scene preparation, without a graphics submission.
//! Call only outside native candidate iteration, under presentation ownership.
//! Evidence: research/window-input-refresh.md and window-activation.md.
use crate::{
    containers,
    memory::Memory,
    points::{GameFunction, Point},
    Result,
};

const WALK: GameFunction<5> = GameFunction::at(0x14dd7a0);
const HITS: GameFunction<4> = GameFunction::at(0x14dda50);
const INSERT: GameFunction<3> = GameFunction::at(0xcc220);
const QUALIFY: GameFunction<1> = GameFunction::at(0x14defd0);
const COUNT: usize = 0x23494d0;
use crate::objects::{campaign::functions::YES, gui::globals::OTHER_CAMERA};
const DRAW_LIST: usize = 0x298;
const HIT_LIST: usize = 0x58;
const OWNER_LIST: usize = 0x70;
const CURSOR: usize = 0x48;
const PREPARED_CURSOR: usize = 0x50;
const OWNER: usize = 0x118;
const HIT_STATE: usize = 0x8f;
const CURRENT_HIT: usize = 0x130;
const ROOT: usize = 0x110;
const WINDOW_NODE: usize = 0xa30;
const CHILDREN_SHOWN: usize = 0x178;
pub(super) use super::placement::PARENT;

pub(super) fn scene(m: &Memory) -> Result<Option<(usize, usize)>> {
    let manager = m.ptr(m.at(crate::objects::gui::globals::MANAGER))?;
    if manager == 0 {
        return Ok(None);
    }
    let scene = m.ptr(manager + crate::objects::gui::manager::SCENE)?;
    Ok((scene != 0).then_some((manager, scene)))
}

pub(super) fn window_node(m: &Memory, root: usize) -> Result<usize> {
    if m.ptr(root + crate::objects::gui::window::GUI)? != m.at(crate::objects::gui::classes::WINDOW_GUI) {
        return Err("ordering requires the ordinary window GUI interface".into());
    }
    let node = m.ptr(root + WINDOW_NODE)?;
    if node == 0 || m.ptr(node + OWNER)? != root + crate::objects::gui::window::GUI {
        return Err("window container ownership changed".into());
    }
    Ok(node)
}

/// Only the checked ordinary window GUI interface permits subtracting 0x30.
pub(super) fn window_of(m: &Memory, node: usize) -> Result<Option<usize>> {
    let owner = m.ptr(node + OWNER)?;
    if owner == 0 || m.ptr(owner)? != m.at(crate::objects::gui::classes::WINDOW_GUI) {
        return Ok(None);
    }
    let root = owner.checked_sub(crate::objects::gui::window::GUI).ok_or("invalid window GUI address")?;
    Ok((window_node(m, root)? == node).then_some(root))
}

pub(super) fn visible(m: &Memory, root: usize, first_root: usize) -> Result<bool> {
    if m.u8(root + 0xf4)? != 0 || m.u8(root + 0xf5)? == 0 {
        return Ok(false);
    }
    let node = window_node(m, root)?;
    let mut parent = m.ptr(node + PARENT)?;
    let mut visited = std::collections::BTreeSet::new();
    while parent != 0 && visited.len() < 1024 && visited.insert(parent) {
        let method = m.vslot(parent, 0x270)?;
        if method == m.at(0x11483a0usize) {
            if m.u8(parent + CHILDREN_SHOWN)? == 0 {
                return Ok(false);
            }
        } else if method != m.at(YES) {
            return Err("unrecognized ancestor traversal gate".into());
        }
        if parent == first_root {
            return Ok(true);
        }
        parent = m.ptr(parent + PARENT)?;
    }
    if parent == 0 {
        Ok(false)
    } else {
        Err("window ancestor chain loops or exceeds its bound".into())
    }
}

pub(super) fn target_node(m: &Memory, scene: usize, gui: usize) -> Result<Option<usize>> {
    for node in containers::pointers(m, scene + HIT_LIST, 100_000)? {
        if m.ptr(node + OWNER)? == gui {
            return Ok(Some(node));
        }
    }
    Ok(None)
}

pub(super) fn points() -> Vec<Point> {
    vec![
        Point::keep(0x1547534usize, "c6 81 f4 00 00 00 01", "window retirement flag").uses(&[0xf4]),
        Point::keep(0x15464b0usize, "49 8b 86 30 0a 00 00 49 8d 4e 30 48 89 88 18 01 00 00", "window container GUI owner").uses(&[WINDOW_NODE, OWNER]),
        Point::keep(0x1547d0busize, "48 8b 83 30 0a 00 00 c6 80 78 01 00 00 01", "window child traversal gate").uses(&[CHILDREN_SHOWN]),
        Point::keep(0x14dd80dusize, "ff 92 70 02 00 00", "scene child traversal method slot").uses(&[0x270]),
        Point::keep(0x11483a0usize, "0f b6 81 78 01 00 00 c3", "window child traversal eligibility"),
        Point::keep(YES, "b0 01 c3", "unconditional child traversal eligibility"),
        Point::keep(WALK, "4d 85 c0 0f 84 97 02 00 00 4c 8b dc 4d 89 43 18", "GUI traversal without drawing"),
        Point::keep(HITS, "4c 8b dc 55 57 41 54 41 56 41 57 48 83 ec 50", "GUI cursor hit collection"),
        Point::keep(INSERT, "40 53 55 41 55 41 57 48 83 ec 28 48 8b 41 08", "native pointer vector insertion"),
        Point::keep(QUALIFY, "48 89 5c 24 10 56 48 83 ec 20 48 8d 71 30 48 8b d9", "GUI hit eligibility refresh"),
        Point::keep(0x14dd8b3usize, "ff 05 17 bc e6 00", "traversal node statistic").uses(&[COUNT]),
        Point::keep(0x14dd051usize, "49 8b 46 58 49 8d 56 58 48 89 42 08 49 8d be 98 02 00 00 49 8b 46 70", "GUI prepared vector headers")
            .uses(&[HIT_LIST, DRAW_LIST, OWNER_LIST]),
        Point::keep(0x14dd23fusize, "48 8b 02 4d 8d 46 48 48 89 42 08 4c 8b ca 49 8b 00 48 8b d7 49 8b ce 49 89 46 50", "current and prepared GUI cursor")
            .uses(&[CURSOR, PREPARED_CURSOR]),
        Point::keep(0x14dd27cusize, "48 83 b9 18 01 00 00 00", "scene node GUI owner").uses(&[OWNER]),
        Point::keep(0x14e9a9dusize, "4c 89 a1 30 01 00 00", "GUI manager current hit reset").uses(&[CURRENT_HIT]),
        Point::keep(0x14e9e87usize, "c6 83 8f 00 00 00 00", "GUI hit state reset").uses(&[HIT_STATE]),
        Point::keep(0x14dd098usize, "49 8d 9e 10 01 00 00", "first GUI scene root").uses(&[ROOT]),
        Point::keep(0x14dd236usize, "40 38 35 12 2d e6 00", "other camera skips ordinary GUI input lists").uses(&[OTHER_CAMERA]),
    ]
}

fn clear(m: &Memory, vector: usize) -> Result<()> {
    let begin = m.ptr(vector)?;
    m.write(vector + 8, &begin.to_le_bytes())
}

fn append(m: &Memory, vector: usize, value: usize) -> Result<()> {
    let end = m.ptr(vector + 8)?;
    let capacity = m.ptr(vector + 16)?;
    if end < capacity {
        m.write(end, &value.to_le_bytes())?;
        m.write(vector + 8, &(end + 8).to_le_bytes())
    } else if end == capacity {
        INSERT.call(m, [vector, end, &value as *const usize as usize]).map(|_| ())
    } else {
        Err("GUI vector end exceeds its native capacity".into())
    }
}

/// Rebuild from live tree nodes; never dereference the retired old hit array.
/// Nodes no longer reachable through visibility gates are excluded from both
/// later mouse and keyboard dispatch. Hidden GUI bytes need not be revisited:
/// showing them again puts them in this live traversal and clears their state.
pub(super) fn refresh(m: &Memory, manager: usize, scene: usize) -> Result<()> {
    let saved_count = m.bytes(m.at(COUNT), 4)?;
    let result = (|| {
        for offset in [DRAW_LIST, HIT_LIST, OWNER_LIST] {
            clear(m, scene + offset)?;
        }
        let identity: [f32; 16] = [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.];
        WALK.call(m, [scene, identity.as_ptr() as usize, scene + ROOT, 0, scene + DRAW_LIST])?;
        m.write(manager + CURRENT_HIT, &0usize.to_le_bytes())?;
        let nodes = containers::pointers(m, scene + DRAW_LIST, 100_000)?;
        for &node in &nodes {
            let owner = m.ptr(node + OWNER)?;
            if owner != 0 {
                m.write(owner + HIT_STATE, &[0])?;
            }
        }
        if m.u8(m.at(OTHER_CAMERA))? == 0 {
            m.write(scene + PREPARED_CURSOR, &m.bytes(scene + CURSOR, 8)?)?;
            HITS.call(m, [scene, scene + DRAW_LIST, scene + CURSOR, scene + HIT_LIST])?;
            for node in nodes {
                if m.ptr(node + OWNER)? != 0 {
                    append(m, scene + OWNER_LIST, node)?;
                }
            }
        }
        // This destroys candidate wrappers, not their GUI targets. It assigns
        // fresh hit ranks without dispatching another mouse or hover event.
        QUALIFY.call(m, [scene])?;
        Ok(())
    })();
    let restored = m.write(m.at(COUNT), &saved_count);
    result.and(restored)
}
