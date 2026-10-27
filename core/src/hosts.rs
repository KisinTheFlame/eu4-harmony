//! The vanilla windows spliced elements may go in (ADR 0006), one row each. A
//! mod's fragment names its window by `into`: [`crate::splice`] finds the
//! file to splice from the row, and [`crate::ui`] binds the elements through
//! the row's [`Host`].
//!
//! Supporting another window takes, besides its row:
//!
//! - in [`crate::ui`], the hooks that find the window's root as the game
//!   builds it and drop it as the game rebuilds or destroys it (for the army
//!   panel: its update, rebuild and destructor), with the vtables that
//!   confirm each object is of the expected class before anything is read
//!   from it;
//! - a button area, if fragments may ask for `area = "buttons"`: room in the
//!   window no vanilla element uses, on the window's own grid;
//! - the evidence for both in docs/research (how the game creates the
//!   window, where it keeps the root, when it tears it down), as
//!   ui-injection.md and notice-window.md have it for the army panel.

/// A window the UI layer binds spliced elements in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Host {
    /// `single_unitpanel`, rebuilt by the game as armies are shown.
    ArmyPanel,
}

pub struct Window {
    pub host: Host,
    /// The window's name in its `.gui` file.
    pub name: &'static str,
    /// The file, as PhysFS names it.
    pub file: &'static str,
    pub buttons: Option<ButtonArea>,
}

/// Where `area = "buttons"` fragments go: slots in columns from (`x`, `y`),
/// `per_column` to a column.
#[derive(Clone, Copy)]
pub struct ButtonArea {
    pub x: i32,
    pub y: i32,
    pub column_step: i32,
    pub row_step: i32,
    pub per_column: i32,
}

impl ButtonArea {
    /// Position of the `index`-th slot.
    pub fn slot(&self, index: i32) -> (i32, i32) {
        (self.x + self.column_step * (index / self.per_column), self.y + self.row_step * (index % self.per_column))
    }
}

pub const WINDOWS: &[Window] = &[Window {
    host: Host::ArmyPanel,
    name: "single_unitpanel",
    file: "interface/unitpanel_main.gui",
    // A third column right of the two vanilla button columns (x = 378, 421),
    // on the vanilla 43 x 44 grid.
    buttons: Some(ButtonArea { x: 464, y: 69, column_step: 43, row_step: 44, per_column: 7 }),
}];

pub fn window(name: &str) -> Option<&'static Window> {
    WINDOWS.iter().find(|w| w.name == name)
}

#[cfg(test)]
mod tests {
    use super::window;

    #[test]
    fn army_panel_slots_fill_columns_of_seven() {
        let area = window("single_unitpanel").unwrap().buttons.unwrap();
        assert_eq!(area.slot(0), (464, 69));
        assert_eq!(area.slot(6), (464, 69 + 6 * 44));
        assert_eq!(area.slot(7), (507, 69));
    }
}
