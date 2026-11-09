//! The vanilla windows spliced elements may go in (ADR inject-ui-at-runtime),
//! one row each. A mod's fragment names its window by `into`: [`crate::splice`]
//! finds the file to splice from the row, and [`crate::ui`] binds the elements
//! through the row's [`Host`].
//!
//! Supporting another window takes, besides its row:
//!
//! - in [`crate::ui`], the hooks that find the window's root as the game
//!   builds it and drop it as the game rebuilds or destroys it (for the army
//!   panel: its update, rebuild and destructor), with the vtables that
//!   confirm each object is of the expected class before anything is read
//!   from it;
//! - a button area, if fragments may ask for `area = "buttons"`: room
//!   beside the window, where the framework's frame holds the buttons on
//!   the window's own grid;
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
/// `per_column` to a column, each `button` square, on a frame of the
/// framework's window background `padding` around them, which grows to the
/// right a column at a time.
#[derive(Clone, Copy)]
pub struct ButtonArea {
    pub x: i32,
    pub y: i32,
    pub column_step: i32,
    pub row_step: i32,
    pub per_column: i32,
    pub button: i32,
    pub padding: i32,
}

impl ButtonArea {
    /// Position of the `index`-th slot.
    pub fn slot(&self, index: i32) -> (i32, i32) {
        (self.x + self.column_step * (index / self.per_column), self.y + self.row_step * (index % self.per_column))
    }

    /// The frame around `slots` slots, as tall as a full column: its top
    /// left corner and its size.
    pub fn frame(&self, slots: i32) -> ((i32, i32), (i32, i32)) {
        let columns = (slots + self.per_column - 1) / self.per_column;
        let size = |count: i32, step: i32| (count - 1) * step + self.button + 2 * self.padding;
        ((self.x - self.padding, self.y - self.padding), (size(columns, self.column_step), size(self.per_column, self.row_step)))
    }
}

pub const WINDOWS: &[Window] = &[Window {
    host: Host::ArmyPanel,
    name: "single_unitpanel",
    file: "interface/unitpanel_main.gui",
    // Beside the panel, a little right of its gold edge (x = 494), and as
    // high as its button columns: from the top of their first row (y = 69)
    // to the foot of their last (413 + 48).
    buttons: Some(ButtonArea { x: 515, y: 80, column_step: 46, row_step: 46, per_column: 8, button: 48, padding: 11 }),
}];

pub fn window(name: &str) -> Option<&'static Window> {
    WINDOWS.iter().find(|w| w.name == name)
}

#[cfg(test)]
mod tests {
    use super::window;

    #[test]
    fn army_panel_slots_fill_a_column_and_then_the_next() {
        let area = window("single_unitpanel").unwrap().buttons.unwrap();
        assert_eq!(area.slot(0), (515, 80));
        assert_eq!(area.slot(7), (515, 80 + 7 * 46));
        assert_eq!(area.slot(8), (561, 80));
    }

    #[test]
    fn the_frame_grows_to_the_right_a_column_at_a_time() {
        let area = window("single_unitpanel").unwrap().buttons.unwrap();
        assert_eq!(area.frame(1), ((504, 69), (70, 461 - 69)));
        assert_eq!(area.frame(8), ((504, 69), (70, 461 - 69)));
        assert_eq!(area.frame(9), ((504, 69), (46 + 70, 461 - 69)));
    }
}
