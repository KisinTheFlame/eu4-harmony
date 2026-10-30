//! Widgets (GLOSSARY) and the windows that hold them: layout, what the mouse
//! hits, and the drawing commands for a frame. Nothing here touches the game
//! or Direct3D; [`Measure`] stands in for the fonts.

/// A rectangle in screen pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

/// 0xAARRGGBB.
pub type Color = u32;

pub const BACKGROUND: Color = 0xf01c2026;
pub const TITLE_BAR: Color = 0xff2b3a4a;
pub const BORDER: Color = 0xff6b8aa8;
pub const TEXT: Color = 0xffe8e8e8;
pub const MUTED: Color = 0xffa0a8b0;
pub const CONTROL: Color = 0xff34404c;
pub const HOVER: Color = 0xff455768;
pub const LIST: Color = 0xff232a31;

/// What a frame draws, in order.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Fill { rect: Rect, color: Color },
    Text { x: f32, y: f32, text: String, size: f32, color: Color },
}

/// Text sizes, from the fonts the UI draws with.
pub trait Measure {
    fn width(&self, text: &str, size: f32) -> f32;
    /// Splits `text` into lines no wider than `width`.
    fn wrap(&self, text: &str, size: f32, width: f32) -> Vec<String>;
}

pub enum Widget {
    Label(String),
    Button { id: &'static str, label: String },
    Dropdown { id: &'static str, options: Vec<String>, selected: usize },
}

/// What the player did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Pressed(&'static str),
    Chose(&'static str, usize),
    Closed,
}

pub const TEXT_SIZE: f32 = 16.0;
const PAD: f32 = 12.0;
const ROW: f32 = 28.0;
const TITLE: f32 = 30.0;
const CLOSE: f32 = 22.0;

pub struct Window {
    pub title: String,
    pub body: Vec<Widget>,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    /// The dropdown whose list is open, by body index.
    open: Option<usize>,
    /// The first option the open list shows.
    scroll: usize,
    /// How tall the screen is; an open list stops at its bottom.
    screen_h: f32,
    drag: Option<(f32, f32)>,
    mouse: (f32, f32),
}

/// One laid-out piece of a window.
enum Slot {
    Lines(Vec<String>, Rect),
    Control(usize, Rect),
}

impl Window {
    pub fn new(title: impl Into<String>, body: Vec<Widget>, width: f32) -> Self {
        Self { title: title.into(), body, x: 0.0, y: 0.0, width, open: None, scroll: 0, screen_h: f32::MAX, drag: None, mouse: (-1.0, -1.0) }
    }

    /// Puts the window in the middle of a screen.
    pub fn center(&mut self, measure: &dyn Measure, screen_w: f32, screen_h: f32) {
        self.screen_h = screen_h;
        let h = self.height(measure);
        self.x = ((screen_w - self.width) / 2.0).round();
        self.y = ((screen_h - h) / 2.0).round();
    }

    pub fn selected(&self, id: &str) -> Option<usize> {
        self.body.iter().find_map(|w| match w {
            Widget::Dropdown { id: i, selected, .. } if *i == id => Some(*selected),
            _ => None,
        })
    }

    /// Gives the drop-down list `id` new options, with `selected` chosen.
    pub fn set_options(&mut self, id: &str, options: Vec<String>, selected: usize) {
        for w in &mut self.body {
            if let Widget::Dropdown { id: i, options: o, selected: s } = w {
                if *i == id {
                    (*o, *s) = (options, selected);
                    return;
                }
            }
        }
    }

    fn inner(&self) -> f32 {
        self.width - 2.0 * PAD
    }

    fn layout(&self, measure: &dyn Measure) -> Vec<Slot> {
        let mut y = self.y + TITLE + PAD;
        let mut slots = Vec::new();
        for (i, w) in self.body.iter().enumerate() {
            match w {
                Widget::Label(text) => {
                    let lines = measure.wrap(text, TEXT_SIZE, self.inner());
                    let h = lines.len().max(1) as f32 * (TEXT_SIZE + 6.0);
                    slots.push(Slot::Lines(lines, Rect { x: self.x + PAD, y, w: self.inner(), h }));
                    y += h + 4.0;
                }
                Widget::Button { .. } | Widget::Dropdown { .. } => {
                    slots.push(Slot::Control(i, Rect { x: self.x + PAD, y, w: self.inner(), h: ROW }));
                    y += ROW + 8.0;
                }
            }
        }
        slots
    }

    pub fn height(&self, measure: &dyn Measure) -> f32 {
        let bottom = self.layout(measure).last().map_or(self.y + TITLE, |s| match s {
            Slot::Lines(_, r) | Slot::Control(_, r) => r.y + r.h,
        });
        bottom - self.y + PAD
    }

    pub fn bounds(&self, measure: &dyn Measure) -> Rect {
        Rect { x: self.x, y: self.y, w: self.width, h: self.height(measure) }
    }

    fn close_box(&self) -> Rect {
        Rect { x: self.x + self.width - CLOSE - 4.0, y: self.y + 4.0, w: CLOSE, h: CLOSE }
    }

    /// How many options an open list shows below `control`: all of them, or
    /// as many as fit above the screen's bottom (at least three).
    fn rows(&self, control: Rect, options: usize) -> usize {
        let room = ((self.screen_h - control.y - ROW - PAD) / ROW).floor().max(3.0) as usize;
        options.min(room)
    }

    fn list_rect(&self, control: Rect, options: usize) -> Rect {
        Rect { x: control.x, y: control.y + ROW, w: control.w, h: self.rows(control, options) as f32 * ROW }
    }

    /// The mouse wheel turned `notches` (up is positive) over the window.
    pub fn wheel(&mut self, measure: &dyn Measure, notches: i32) {
        let Some(open) = self.open else { return };
        let Some(Slot::Control(_, control)) = self.layout(measure).into_iter().find(|s| matches!(s, Slot::Control(i, _) if *i == open)) else { return };
        let Widget::Dropdown { options, .. } = &self.body[open] else { return };
        let last = options.len().saturating_sub(self.rows(control, options.len()));
        self.scroll = (self.scroll as i64 - notches as i64).clamp(0, last as i64) as usize;
    }

    /// Whether the window, or an open list hanging below it, covers a point.
    pub fn covers(&self, measure: &dyn Measure, x: f32, y: f32) -> bool {
        self.bounds(measure).contains(x, y) || self.open_list(measure).is_some_and(|(r, _)| r.contains(x, y))
    }

    fn open_list(&self, measure: &dyn Measure) -> Option<(Rect, usize)> {
        let open = self.open?;
        self.layout(measure).into_iter().find_map(|s| match s {
            Slot::Control(i, r) if i == open => match &self.body[i] {
                Widget::Dropdown { options, .. } => Some((self.list_rect(r, options.len()), i)),
                _ => None,
            },
            _ => None,
        })
    }

    pub fn mouse_move(&mut self, x: f32, y: f32) {
        if let Some((dx, dy)) = self.drag {
            self.x = x - dx;
            self.y = y - dy;
        }
        self.mouse = (x, y);
    }

    pub fn mouse_up(&mut self) {
        self.drag = None;
    }

    /// A left click at a point the window covers.
    pub fn mouse_down(&mut self, measure: &dyn Measure, x: f32, y: f32) -> Option<Action> {
        if let Some((list, i)) = self.open_list(measure) {
            self.open = None;
            if list.contains(x, y) {
                let index = self.scroll + ((y - list.y) / ROW) as usize;
                if let Widget::Dropdown { id, selected, options } = &mut self.body[i] {
                    if index < options.len() {
                        *selected = index;
                        return Some(Action::Chose(id, index));
                    }
                }
                return None;
            }
        }
        if self.close_box().contains(x, y) {
            return Some(Action::Closed);
        }
        if (Rect { x: self.x, y: self.y, w: self.width, h: TITLE }).contains(x, y) {
            self.drag = Some((x - self.x, y - self.y));
            return None;
        }
        for slot in self.layout(measure) {
            if let Slot::Control(i, r) = slot {
                if r.contains(x, y) {
                    return match &self.body[i] {
                        Widget::Button { id, .. } => Some(Action::Pressed(id)),
                        Widget::Dropdown { options, selected, .. } => {
                            // The list opens with the chosen option in view.
                            let rows = self.rows(r, options.len());
                            self.scroll = selected.saturating_sub(rows / 2).min(options.len().saturating_sub(rows));
                            self.open = Some(i);
                            None
                        }
                        Widget::Label(_) => None,
                    };
                }
            }
        }
        None
    }

    /// The drawing commands for the window as it is now.
    pub fn draw(&self, measure: &dyn Measure) -> Vec<Command> {
        let bounds = self.bounds(measure);
        let hover = |r: &Rect| r.contains(self.mouse.0, self.mouse.1);
        let mut out = vec![
            Command::Fill { rect: Rect { x: bounds.x - 1.0, y: bounds.y - 1.0, w: bounds.w + 2.0, h: bounds.h + 2.0 }, color: BORDER },
            Command::Fill { rect: bounds, color: BACKGROUND },
            Command::Fill { rect: Rect { h: TITLE, ..bounds }, color: TITLE_BAR },
            Command::Text { x: bounds.x + PAD, y: bounds.y + (TITLE - TEXT_SIZE) / 2.0 - 2.0, text: self.title.clone(), size: TEXT_SIZE, color: TEXT },
        ];
        let close = self.close_box();
        out.push(Command::Fill { rect: close, color: if hover(&close) { HOVER } else { CONTROL } });
        out.push(Command::Text { x: close.x + 6.0, y: close.y + 1.0, text: "×".into(), size: TEXT_SIZE, color: TEXT });
        for slot in self.layout(measure) {
            match slot {
                Slot::Lines(lines, r) => {
                    for (n, line) in lines.into_iter().enumerate() {
                        out.push(Command::Text { x: r.x, y: r.y + n as f32 * (TEXT_SIZE + 6.0), text: line, size: TEXT_SIZE, color: TEXT });
                    }
                }
                Slot::Control(i, r) => {
                    out.push(Command::Fill { rect: r, color: if hover(&r) { HOVER } else { CONTROL } });
                    let (label, centred) = match &self.body[i] {
                        Widget::Button { label, .. } => (label.clone(), true),
                        Widget::Dropdown { options, selected, .. } => (format!("{}  ▾", options.get(*selected).map_or("", String::as_str)), false),
                        Widget::Label(_) => continue,
                    };
                    let x = if centred { r.x + (r.w - measure.width(&label, TEXT_SIZE)) / 2.0 } else { r.x + 8.0 };
                    out.push(Command::Text { x, y: r.y + (ROW - TEXT_SIZE) / 2.0 - 2.0, text: label, size: TEXT_SIZE, color: TEXT });
                }
            }
        }
        if let Some((list, i)) = self.open_list(measure) {
            out.push(Command::Fill { rect: Rect { x: list.x - 1.0, y: list.y - 1.0, w: list.w + 2.0, h: list.h + 2.0 }, color: BORDER });
            out.push(Command::Fill { rect: list, color: LIST });
            if let Widget::Dropdown { options, selected, .. } = &self.body[i] {
                let rows = (list.h / ROW).round() as usize;
                for (n, (index, option)) in options.iter().enumerate().skip(self.scroll).take(rows).enumerate() {
                    let row = Rect { x: list.x, y: list.y + n as f32 * ROW, w: list.w, h: ROW };
                    if hover(&row) || index == *selected {
                        out.push(Command::Fill { rect: row, color: if hover(&row) { HOVER } else { CONTROL } });
                    }
                    out.push(Command::Text { x: row.x + 8.0, y: row.y + (ROW - TEXT_SIZE) / 2.0 - 2.0, text: option.clone(), size: TEXT_SIZE, color: TEXT });
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every character is half the size wide; lines wrap at spaces.
    struct Mono;
    impl Measure for Mono {
        fn width(&self, text: &str, size: f32) -> f32 {
            text.chars().count() as f32 * size / 2.0
        }
        fn wrap(&self, text: &str, size: f32, width: f32) -> Vec<String> {
            let per_line = (width / (size / 2.0)) as usize;
            text.chars().collect::<Vec<_>>().chunks(per_line.max(1)).map(|c| c.iter().collect()).collect()
        }
    }

    fn settings() -> Window {
        let mut w = Window::new(
            "Settings",
            vec![
                Widget::Label("Font".into()),
                Widget::Dropdown { id: "font", options: vec!["A".into(), "B".into(), "C".into()], selected: 0 },
                Widget::Button { id: "save", label: "Save".into() },
            ],
            300.0,
        );
        w.center(&Mono, 1000.0, 800.0);
        w
    }

    /// The centre of the first command of `kind` whose text is `label`.
    fn spot(w: &Window, label: &str) -> (f32, f32) {
        let (x, y) = w
            .draw(&Mono)
            .into_iter()
            .find_map(|c| match c {
                Command::Text { x, y, text, .. } if text.starts_with(label) => Some((x, y)),
                _ => None,
            })
            .unwrap();
        (x + 2.0, y + 8.0)
    }

    #[test]
    fn a_dropdown_opens_and_records_the_choice() {
        let mut w = settings();
        let (x, y) = spot(&w, "A");
        assert_eq!(w.mouse_down(&Mono, x, y), None);
        // The list is open below the control; the second row is "B".
        let (bx, by) = spot(&w, "B");
        assert!(w.covers(&Mono, bx, by));
        assert_eq!(w.mouse_down(&Mono, bx, by), Some(Action::Chose("font", 1)));
        assert_eq!(w.selected("font"), Some(1));
        assert!(!w.draw(&Mono).iter().any(|c| matches!(c, Command::Text { text, .. } if text == "C")));
    }

    #[test]
    fn a_dropdown_takes_new_options() {
        let mut w = settings();
        w.set_options("font", vec!["Light".into(), "Bold".into()], 1);
        assert_eq!(w.selected("font"), Some(1));
        let (x, y) = spot(&w, "Bold");
        assert_eq!(w.mouse_down(&Mono, x, y), None);
        let (lx, ly) = spot(&w, "Light");
        assert_eq!(w.mouse_down(&Mono, lx, ly), Some(Action::Chose("font", 0)));
        // An id no drop-down has changes nothing.
        w.set_options("none", Vec::new(), 0);
        assert_eq!(w.selected("font"), Some(0));
    }

    #[test]
    fn buttons_press_the_close_box_closes_and_the_title_drags() {
        let mut w = settings();
        let (x, y) = spot(&w, "Save");
        assert_eq!(w.mouse_down(&Mono, x, y), Some(Action::Pressed("save")));
        let (cx, cy) = spot(&w, "×");
        assert_eq!(w.mouse_down(&Mono, cx, cy), Some(Action::Closed));
        let (tx, ty) = (w.x + 20.0, w.y + 10.0);
        assert_eq!(w.mouse_down(&Mono, tx, ty), None);
        w.mouse_move(tx + 50.0, ty + 30.0);
        w.mouse_up();
        w.mouse_move(tx + 100.0, ty + 100.0);
        assert_eq!((w.x, w.y), (tx + 50.0 - 20.0, ty + 30.0 - 10.0));
    }

    #[test]
    fn a_long_list_stops_at_the_screen_bottom_and_scrolls() {
        let options: Vec<String> = (0..40).map(|n| format!("F{n}")).collect();
        let mut w = Window::new("Settings", vec![Widget::Dropdown { id: "font", options, selected: 0 }], 300.0);
        w.center(&Mono, 1000.0, 400.0);
        let (x, y) = spot(&w, "F0");
        assert_eq!(w.mouse_down(&Mono, x, y), None);
        let shown =
            |w: &Window| w.draw(&Mono).iter().filter(|c| matches!(c, Command::Text { text, .. } if text.starts_with('F') && !text.contains('▾'))).count();
        let rows = shown(&w);
        assert!((3..40).contains(&rows), "{rows} rows on a 400-pixel screen");
        w.wheel(&Mono, -5);
        // The list now starts at F5; its first row picks it.
        let (fx, fy) = spot(&w, "F5");
        assert_eq!(w.mouse_down(&Mono, fx, fy), Some(Action::Chose("font", 5)));
    }

    #[test]
    fn long_labels_wrap_and_grow_the_window() {
        let short = Window::new("T", vec![Widget::Label("x".into())], 200.0);
        let long = Window::new("T", vec![Widget::Label("x".repeat(100))], 200.0);
        assert!(long.height(&Mono) > short.height(&Mono) + 3.0 * TEXT_SIZE);
    }
}
