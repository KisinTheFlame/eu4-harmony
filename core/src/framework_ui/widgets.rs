//! Widgets (GLOSSARY) and the windows that hold them: layout, what the mouse
//! hits, and the drawing commands for a frame. Nothing here touches the game
//! or Direct3D; [`Measure`] stands in for the fonts.
use std::time::{Duration, Instant};

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
/// A toggle's track while it is on.
pub const ON: Color = 0xff3f7fbf;
pub const ON_HOVER: Color = 0xff5592cc;

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
    /// A heading: larger text, with room above it where something comes
    /// before.
    Heading(String),
    /// One item of a list: a bullet, and its text wrapped beside it.
    Item(String),
    /// Room between two parts of a window's body.
    Gap,
    Button {
        id: &'static str,
        label: String,
    },
    Dropdown {
        id: &'static str,
        options: Vec<String>,
        selected: usize,
    },
    /// A toggle (GLOSSARY): its label on the left, the switch on the right;
    /// a click anywhere on the row switches it.
    Toggle {
        id: &'static str,
        label: String,
        on: bool,
    },
    /// A choice bar (GLOSSARY): its options side by side in one bar, the
    /// chosen one under a highlight that slides to another when it is
    /// clicked.
    Choice {
        id: &'static str,
        options: Vec<String>,
        selected: usize,
    },
    /// A label followed by an info mark (GLOSSARY): while the pointer is on
    /// the mark, `info` shows beside it.
    Explained {
        label: String,
        info: String,
    },
}

/// A side list (GLOSSARY): a list down a window's left side, one row per
/// item, one of them chosen, which a click chooses. The window's body is
/// beside it. A list longer than the screen allows shows as many rows as fit
/// and scrolls with the mouse wheel.
pub struct Side {
    pub id: &'static str,
    pub items: Vec<String>,
    pub selected: usize,
    pub width: f32,
    /// The first item shown.
    scroll: usize,
}

impl Side {
    pub fn new(id: &'static str, items: Vec<String>, width: f32) -> Self {
        Self { id, items, selected: 0, width, scroll: 0 }
    }
}

/// What the player did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Pressed(&'static str),
    Chose(&'static str, usize),
    Switched(&'static str, bool),
    Closed,
}

pub const TEXT_SIZE: f32 = 16.0;
const PAD: f32 = 12.0;
const ROW: f32 = 28.0;
const TITLE: f32 = 30.0;
const CLOSE: f32 = 22.0;
const SWITCH_W: f32 = 40.0;
const SWITCH_H: f32 = 20.0;
const KNOB: f32 = 14.0;
const LINE: f32 = TEXT_SIZE + 6.0;
const HEADING_SIZE: f32 = 20.0;
const HEADING_LINE: f32 = HEADING_SIZE + 6.0;
/// How far an item's text stands right of its bullet.
const INDENT: f32 = 16.0;
const GAP: f32 = 10.0;
/// The share of the screen a side list may take, and its scroll bar's width.
const SIDE_SHARE: f32 = 0.6;
const SCROLL_BAR: f32 = 4.0;
/// The info mark, and how far it stands after its label.
const MARK: &str = "ⓘ";
const MARK_GAP: f32 = 6.0;
/// How wide an info mark's text is laid out.
const INFO_W: f32 = 320.0;
/// How long a choice bar's highlight takes to slide to another option.
const SLIDE: Duration = Duration::from_millis(150);

pub struct Window {
    pub title: String,
    pub body: Vec<Widget>,
    pub side: Option<Side>,
    /// The pages the side list chooses among, when the window keeps them
    /// itself: the page shown is the body, and its place here is empty.
    pages: Vec<Vec<Widget>>,
    /// What stands below the body on every page, such as the buttons that
    /// save and cancel.
    footer: Vec<Widget>,
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
    /// A choice bar's highlight on its way: the bar, by body index, the
    /// option it left, and when.
    slide: Option<(usize, usize, Instant)>,
}

/// One laid-out piece of a window.
enum Slot {
    Lines(Vec<String>, Rect, Style),
    Control(usize, Rect),
    /// The info mark of an explained label, by body index.
    Mark(usize, Rect),
}

/// How a piece of text is drawn.
#[derive(Clone, Copy, PartialEq)]
enum Style {
    Body,
    Heading,
    Item,
}

impl Style {
    fn size(self) -> f32 {
        if self == Style::Heading {
            HEADING_SIZE
        } else {
            TEXT_SIZE
        }
    }

    fn line(self) -> f32 {
        if self == Style::Heading {
            HEADING_LINE
        } else {
            LINE
        }
    }
}

impl Window {
    pub fn new(title: impl Into<String>, body: Vec<Widget>, width: f32) -> Self {
        Self {
            title: title.into(),
            body,
            side: None,
            pages: Vec::new(),
            footer: Vec::new(),
            x: 0.0,
            y: 0.0,
            width,
            open: None,
            scroll: 0,
            screen_h: f32::MAX,
            drag: None,
            mouse: (-1.0, -1.0),
            slide: None,
        }
    }

    /// The window with a list down its left side.
    pub fn with_side(mut self, side: Side) -> Self {
        self.side = Some(side);
        self
    }

    /// The window with pages its side list chooses among, one per item, the
    /// first shown. A page left keeps what the player chose on it.
    pub fn with_pages(mut self, mut pages: Vec<Vec<Widget>>) -> Self {
        debug_assert_eq!(self.side.as_ref().map(|s| s.items.len()), Some(pages.len()));
        if let Some(first) = pages.first_mut() {
            self.body = std::mem::take(first);
        }
        self.pages = pages;
        self
    }

    /// The window with `footer` below its body, in the same place on every
    /// page.
    pub fn with_footer(mut self, footer: Vec<Widget>) -> Self {
        self.footer = footer;
        self
    }

    /// Shows page `to` in place of page `from`.
    fn turn(&mut self, from: usize, to: usize) {
        std::mem::swap(&mut self.body, &mut self.pages[from]);
        std::mem::swap(&mut self.body, &mut self.pages[to]);
        self.slide = None;
    }

    /// Every widget the window has: the body's, the other pages', the
    /// footer's.
    fn widgets(&self) -> impl Iterator<Item = &Widget> {
        self.body.iter().chain(self.pages.iter().flatten()).chain(&self.footer)
    }

    /// The widget a slot stands for: the body's are numbered first, then the
    /// footer's.
    fn widget(&self, i: usize) -> &Widget {
        self.body.get(i).unwrap_or_else(|| &self.footer[i - self.body.len()])
    }

    fn widget_mut(&mut self, i: usize) -> &mut Widget {
        let n = self.body.len();
        if i < n {
            &mut self.body[i]
        } else {
            &mut self.footer[i - n]
        }
    }

    /// What stands below the body on every page, to change while the window
    /// is open.
    pub fn footer_mut(&mut self) -> &mut [Widget] {
        &mut self.footer
    }

    /// Puts the window in the middle of a screen.
    pub fn center(&mut self, measure: &dyn Measure, screen_w: f32, screen_h: f32) {
        self.screen_h = screen_h;
        let h = self.height(measure);
        self.x = ((screen_w - self.width) / 2.0).round();
        self.y = ((screen_h - h) / 2.0).round();
    }

    pub fn selected(&self, id: &str) -> Option<usize> {
        self.widgets().find_map(|w| match w {
            Widget::Dropdown { id: i, selected, .. } | Widget::Choice { id: i, selected, .. } if *i == id => Some(*selected),
            _ => None,
        })
    }

    pub fn is_on(&self, id: &str) -> Option<bool> {
        self.widgets().find_map(|w| match w {
            Widget::Toggle { id: i, on, .. } if *i == id => Some(*on),
            _ => None,
        })
    }

    /// Gives the drop-down list `id` new options, with `selected` chosen.
    pub fn set_options(&mut self, id: &str, options: Vec<String>, selected: usize) {
        for w in self.body.iter_mut().chain(self.pages.iter_mut().flatten()).chain(&mut self.footer) {
            if let Widget::Dropdown { id: i, options: o, selected: s } = w {
                if *i == id {
                    (*o, *s) = (options, selected);
                    return;
                }
            }
        }
    }

    /// How wide the body is: the window's inside, less the side list.
    fn inner(&self) -> f32 {
        self.width - 2.0 * PAD - self.side.as_ref().map_or(0.0, |s| s.width + PAD)
    }

    /// Where the body begins, right of the side list.
    fn left(&self) -> f32 {
        self.x + PAD + self.side.as_ref().map_or(0.0, |s| s.width + PAD)
    }

    /// How many rows of the side list show: all of them, or as many as fit
    /// in its share of the screen (at least three).
    fn side_shown(&self) -> usize {
        let Some(side) = &self.side else { return 0 };
        let room = ((self.screen_h * SIDE_SHARE - TITLE - 2.0 * PAD) / ROW).floor().max(3.0);
        side.items.len().min(room as usize)
    }

    /// Where the side list is.
    fn side_area(&self) -> Option<Rect> {
        let side = self.side.as_ref()?;
        Some(Rect { x: self.x + PAD, y: self.y + TITLE + PAD, w: side.width, h: self.side_shown() as f32 * ROW })
    }

    /// The side list's rows that show, each with its item's index.
    fn side_rows(&self) -> Vec<(usize, Rect)> {
        let (Some(side), Some(area)) = (&self.side, self.side_area()) else { return Vec::new() };
        let width = if side.items.len() > self.side_shown() { area.w - SCROLL_BAR } else { area.w };
        (0..self.side_shown()).map(|n| (side.scroll + n, Rect { x: area.x, y: area.y + n as f32 * ROW, w: width, h: ROW })).collect()
    }

    /// How wide a toggle's label may be, left of its switch.
    fn toggle_text(&self) -> f32 {
        self.inner() - SWITCH_W - PAD
    }

    /// The switch at the right end of a toggle's row.
    fn switch(row: Rect) -> Rect {
        Rect { x: row.x + row.w - SWITCH_W, y: row.y + ((row.h - SWITCH_H) / 2.0).round(), w: SWITCH_W, h: SWITCH_H }
    }

    /// A choice bar's options, side by side across its row.
    fn segments(row: Rect, options: usize) -> Vec<Rect> {
        let w = row.w / options.max(1) as f32;
        (0..options).map(|n| Rect { x: (row.x + n as f32 * w).round(), y: row.y, w: w.round(), h: row.h }).collect()
    }

    /// Where a choice bar's highlight is, over the option chosen or on its
    /// way there from the one left.
    fn highlight(&self, i: usize, segments: &[Rect], selected: usize) -> Rect {
        let to = segments[selected];
        let Some((bar, from, since)) = self.slide.filter(|s| s.0 == i) else { return to };
        let t = (since.elapsed().as_secs_f32() / SLIDE.as_secs_f32()).min(1.0);
        let eased = t * t * (3.0 - 2.0 * t);
        let from = segments.get(from).copied().unwrap_or(to);
        debug_assert_eq!(bar, i);
        Rect { x: (from.x + (to.x - from.x) * eased).round(), ..to }
    }

    fn layout(&self, measure: &dyn Measure) -> Vec<Slot> {
        let top = self.y + TITLE + PAD;
        let mut slots = self.lay_out(measure, &self.body, 0, top);
        if !self.footer.is_empty() {
            let y = bottom(&slots, top).max(self.pages_bottom(measure)) + 8.0;
            slots.extend(self.lay_out(measure, &self.footer, self.body.len(), y));
        }
        slots
    }

    /// Where the tallest of the pages not shown ends, or the body's top.
    fn pages_bottom(&self, measure: &dyn Measure) -> f32 {
        let top = self.y + TITLE + PAD;
        self.pages.iter().map(|page| bottom(&self.lay_out(measure, page, 0, top), top)).fold(top, f32::max)
    }

    /// Lays `widgets` out from `y` down, numbering them from `first`.
    fn lay_out(&self, measure: &dyn Measure, widgets: &[Widget], first: usize, mut y: f32) -> Vec<Slot> {
        let mut slots = Vec::new();
        for (n, w) in widgets.iter().enumerate() {
            let i = first + n;
            match w {
                Widget::Label(text) | Widget::Heading(text) | Widget::Item(text) => {
                    let style = match w {
                        Widget::Heading(_) => Style::Heading,
                        Widget::Item(_) => Style::Item,
                        _ => Style::Body,
                    };
                    if style == Style::Heading && n > 0 {
                        y += GAP;
                    }
                    let indent = if style == Style::Item { INDENT } else { 0.0 };
                    let lines = measure.wrap(text, style.size(), self.inner() - indent);
                    let h = lines.len().max(1) as f32 * style.line();
                    slots.push(Slot::Lines(lines, Rect { x: self.left(), y, w: self.inner(), h }, style));
                    y += h + 4.0;
                }
                Widget::Gap => y += GAP,
                Widget::Explained { label, .. } => {
                    let mark = measure.width(MARK, TEXT_SIZE);
                    let lines = measure.wrap(label, TEXT_SIZE, self.inner() - mark - MARK_GAP);
                    let h = lines.len().max(1) as f32 * LINE;
                    let last = lines.last().map_or(0.0, |l| measure.width(l, TEXT_SIZE));
                    let mark = Rect { x: self.left() + last + MARK_GAP, y: y + h - LINE, w: mark, h: LINE };
                    slots.push(Slot::Lines(lines, Rect { x: self.left(), y, w: self.inner(), h }, Style::Body));
                    slots.push(Slot::Mark(i, mark));
                    y += h + 4.0;
                }
                Widget::Button { .. } | Widget::Dropdown { .. } | Widget::Choice { .. } => {
                    slots.push(Slot::Control(i, Rect { x: self.left(), y, w: self.inner(), h: ROW }));
                    y += ROW + 8.0;
                }
                Widget::Toggle { label, .. } => {
                    let lines = measure.wrap(label, TEXT_SIZE, self.toggle_text());
                    let h = (lines.len().max(1) as f32 * LINE).max(ROW);
                    slots.push(Slot::Control(i, Rect { x: self.left(), y, w: self.inner(), h }));
                    y += h + 8.0;
                }
            }
        }
        slots
    }

    /// How tall the window is: as tall as its tallest page, so that it keeps
    /// its size from page to page.
    pub fn height(&self, measure: &dyn Measure) -> f32 {
        let body = bottom(&self.layout(measure), self.y + TITLE).max(self.pages_bottom(measure));
        let side = self.side_area().map_or(self.y + TITLE, |r| r.y + r.h);
        body.max(side) - self.y + PAD
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

    /// The mouse wheel turned `notches` (up is positive) over the window: an
    /// open list scrolls, or the side list under the mouse.
    pub fn wheel(&mut self, measure: &dyn Measure, notches: i32) {
        let Some(open) = self.open else {
            let over = self.side_area().is_some_and(|r| r.contains(self.mouse.0, self.mouse.1));
            let shown = self.side_shown();
            if let (true, Some(side)) = (over, self.side.as_mut()) {
                let last = side.items.len().saturating_sub(shown);
                side.scroll = (side.scroll as i64 - notches as i64).clamp(0, last as i64) as usize;
            }
            return;
        };
        let Some(Slot::Control(_, control)) = self.layout(measure).into_iter().find(|s| matches!(s, Slot::Control(i, _) if *i == open)) else { return };
        let Widget::Dropdown { options, .. } = self.widget(open) else { return };
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
            Slot::Control(i, r) if i == open => match self.widget(i) {
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
                if let Widget::Dropdown { id, selected, options } = self.widget_mut(i) {
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
        if let Some((index, _)) = self.side_rows().into_iter().find(|(_, r)| r.contains(x, y)) {
            let side = self.side.as_mut().expect("it has rows");
            let (id, left) = (side.id, std::mem::replace(&mut side.selected, index));
            if !self.pages.is_empty() && index != left {
                self.turn(left, index);
            }
            return Some(Action::Chose(id, index));
        }
        for slot in self.layout(measure) {
            if let Slot::Control(i, r) = slot {
                if r.contains(x, y) {
                    if let Widget::Toggle { id, on, .. } = self.widget_mut(i) {
                        *on = !*on;
                        return Some(Action::Switched(id, *on));
                    }
                    if let Widget::Choice { id, options, selected } = self.widget_mut(i) {
                        let index = Self::segments(r, options.len()).iter().position(|s| s.contains(x, y))?;
                        if index == *selected {
                            return None;
                        }
                        let (id, left) = (*id, std::mem::replace(selected, index));
                        self.slide = Some((i, left, Instant::now()));
                        return Some(Action::Chose(id, index));
                    }
                    return match self.widget(i) {
                        Widget::Button { id, .. } => Some(Action::Pressed(id)),
                        Widget::Dropdown { options, selected, .. } => {
                            // The list opens with the chosen option in view.
                            let rows = self.rows(r, options.len());
                            self.scroll = selected.saturating_sub(rows / 2).min(options.len().saturating_sub(rows));
                            self.open = Some(i);
                            None
                        }
                        Widget::Label(_)
                        | Widget::Heading(_)
                        | Widget::Item(_)
                        | Widget::Gap
                        | Widget::Toggle { .. }
                        | Widget::Choice { .. }
                        | Widget::Explained { .. } => None,
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
        if let (Some(side), Some(area)) = (&self.side, self.side_area()) {
            out.push(Command::Fill { rect: Rect { h: bounds.y + bounds.h - PAD - area.y, ..area }, color: LIST });
            for (index, row) in self.side_rows() {
                if hover(&row) || index == side.selected {
                    out.push(Command::Fill { rect: row, color: if hover(&row) { HOVER } else { CONTROL } });
                }
                let text = elide(measure, &side.items[index], row.w - 16.0);
                out.push(Command::Text { x: row.x + 8.0, y: row.y + (ROW - TEXT_SIZE) / 2.0 - 2.0, text, size: TEXT_SIZE, color: TEXT });
            }
            // Where the rows shown are in the whole list.
            let (shown, all) = (self.side_shown(), side.items.len());
            if all > shown {
                let thumb = Rect {
                    x: area.x + area.w - SCROLL_BAR,
                    y: area.y + area.h * side.scroll as f32 / all as f32,
                    w: SCROLL_BAR,
                    h: area.h * shown as f32 / all as f32,
                };
                out.push(Command::Fill { rect: thumb, color: BORDER });
            }
        }
        let close = self.close_box();
        out.push(Command::Fill { rect: close, color: if hover(&close) { HOVER } else { CONTROL } });
        out.push(Command::Text { x: close.x + 6.0, y: close.y + 1.0, text: "×".into(), size: TEXT_SIZE, color: TEXT });
        for slot in self.layout(measure) {
            match slot {
                Slot::Lines(lines, r, style) => {
                    let x = if style == Style::Item {
                        out.push(Command::Text { x: r.x + 2.0, y: r.y, text: "•".into(), size: TEXT_SIZE, color: TEXT });
                        r.x + INDENT
                    } else {
                        r.x
                    };
                    for (n, line) in lines.into_iter().enumerate() {
                        out.push(Command::Text { x, y: r.y + n as f32 * style.line(), text: line, size: style.size(), color: TEXT });
                    }
                }
                Slot::Control(i, r) if matches!(self.widget(i), Widget::Toggle { .. }) => {
                    let Widget::Toggle { label, on, .. } = self.widget(i) else { continue };
                    let lines = measure.wrap(label, TEXT_SIZE, self.toggle_text());
                    let top = r.y + ((r.h - lines.len().max(1) as f32 * LINE) / 2.0).round();
                    for (n, line) in lines.into_iter().enumerate() {
                        out.push(Command::Text { x: r.x, y: top + 2.0 + n as f32 * LINE, text: line, size: TEXT_SIZE, color: TEXT });
                    }
                    let track = Self::switch(r);
                    let color = match (*on, hover(&r)) {
                        (true, false) => ON,
                        (true, true) => ON_HOVER,
                        (false, false) => CONTROL,
                        (false, true) => HOVER,
                    };
                    out.push(Command::Fill { rect: track, color });
                    let inset = (SWITCH_H - KNOB) / 2.0;
                    let knob_x = if *on { track.x + track.w - inset - KNOB } else { track.x + inset };
                    out.push(Command::Fill { rect: Rect { x: knob_x, y: track.y + inset, w: KNOB, h: KNOB }, color: TEXT });
                }
                Slot::Control(i, r) if matches!(self.widget(i), Widget::Choice { .. }) => {
                    let Widget::Choice { options, selected, .. } = self.widget(i) else { continue };
                    out.push(Command::Fill { rect: r, color: CONTROL });
                    let segments = Self::segments(r, options.len());
                    for (n, segment) in segments.iter().enumerate() {
                        if n != *selected && hover(segment) {
                            out.push(Command::Fill { rect: *segment, color: HOVER });
                        }
                    }
                    let highlight = self.highlight(i, &segments, *selected);
                    out.push(Command::Fill { rect: highlight, color: if hover(&highlight) { ON_HOVER } else { ON } });
                    for (option, segment) in options.iter().zip(&segments) {
                        let text = elide(measure, option, segment.w - 12.0);
                        let x = segment.x + ((segment.w - measure.width(&text, TEXT_SIZE)) / 2.0).round();
                        out.push(Command::Text { x, y: segment.y + (ROW - TEXT_SIZE) / 2.0 - 2.0, text, size: TEXT_SIZE, color: TEXT });
                    }
                }
                Slot::Mark(_, r) => {
                    out.push(Command::Text { x: r.x, y: r.y, text: MARK.into(), size: TEXT_SIZE, color: if hover(&r) { TEXT } else { MUTED } });
                }
                Slot::Control(i, r) => {
                    out.push(Command::Fill { rect: r, color: if hover(&r) { HOVER } else { CONTROL } });
                    let (label, centred) = match self.widget(i) {
                        Widget::Button { label, .. } => (label.clone(), true),
                        Widget::Dropdown { options, selected, .. } => (format!("{}  ▾", options.get(*selected).map_or("", String::as_str)), false),
                        Widget::Label(_)
                        | Widget::Heading(_)
                        | Widget::Item(_)
                        | Widget::Gap
                        | Widget::Toggle { .. }
                        | Widget::Choice { .. }
                        | Widget::Explained { .. } => continue,
                    };
                    let x = if centred { r.x + (r.w - measure.width(&label, TEXT_SIZE)) / 2.0 } else { r.x + 8.0 };
                    out.push(Command::Text { x, y: r.y + (ROW - TEXT_SIZE) / 2.0 - 2.0, text: label, size: TEXT_SIZE, color: TEXT });
                }
            }
        }
        if let Some((list, i)) = self.open_list(measure) {
            out.push(Command::Fill { rect: Rect { x: list.x - 1.0, y: list.y - 1.0, w: list.w + 2.0, h: list.h + 2.0 }, color: BORDER });
            out.push(Command::Fill { rect: list, color: LIST });
            if let Widget::Dropdown { options, selected, .. } = self.widget(i) {
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
        // An info mark's text, over everything, while the pointer is on it.
        for slot in self.layout(measure) {
            let Slot::Mark(i, mark) = slot else { continue };
            let Widget::Explained { info, .. } = self.widget(i) else { continue };
            if !hover(&mark) {
                continue;
            }
            let lines = measure.wrap(info, TEXT_SIZE, INFO_W - 2.0 * PAD);
            let r = Rect { x: mark.x + mark.w + MARK_GAP, y: mark.y, w: INFO_W, h: lines.len().max(1) as f32 * LINE + PAD };
            out.push(Command::Fill { rect: Rect { x: r.x - 1.0, y: r.y - 1.0, w: r.w + 2.0, h: r.h + 2.0 }, color: BORDER });
            out.push(Command::Fill { rect: r, color: LIST });
            for (n, line) in lines.into_iter().enumerate() {
                out.push(Command::Text { x: r.x + PAD, y: r.y + PAD / 2.0 + n as f32 * LINE, text: line, size: TEXT_SIZE, color: TEXT });
            }
        }
        out
    }
}

/// Where the lowest of `slots` ends, or `top` if none ends lower.
fn bottom(slots: &[Slot], top: f32) -> f32 {
    slots
        .iter()
        .map(|s| match s {
            Slot::Lines(_, r, _) | Slot::Control(_, r) | Slot::Mark(_, r) => r.y + r.h,
        })
        .fold(top, f32::max)
}

/// `text` cut to fit `width`, with an ellipsis where it was cut.
fn elide(measure: &dyn Measure, text: &str, width: f32) -> String {
    if measure.width(text, TEXT_SIZE) <= width {
        return text.to_string();
    }
    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let cut = format!("{}…", chars.iter().collect::<String>());
        if measure.width(&cut, TEXT_SIZE) <= width {
            return cut;
        }
    }
    "…".into()
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

    /// Where the text that starts with `label` is drawn, if it is.
    fn spot_of(w: &Window, label: &str) -> Option<(f32, f32)> {
        w.draw(&Mono).into_iter().find_map(|c| match c {
            Command::Text { x, y, text, .. } if text == label => Some((x + 2.0, y + 8.0)),
            _ => None,
        })
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

    fn speed() -> Window {
        let mut w = Window::new(
            "Settings",
            vec![
                Widget::Explained { label: "At the fastest speed".into(), info: "Faster time: days pass sooner".into() },
                Widget::Choice { id: "pace", options: vec!["Smooth".into(), "Faster".into()], selected: 0 },
            ],
            300.0,
        );
        w.center(&Mono, 1000.0, 800.0);
        w
    }

    /// The filled rectangles of a colour.
    fn fills(w: &Window, color: Color) -> Vec<Rect> {
        w.draw(&Mono).into_iter().filter_map(|c| if let Command::Fill { rect, color: c } = c { (c == color).then_some(rect) } else { None }).collect()
    }

    #[test]
    fn a_choice_bar_chooses_where_it_is_clicked_and_its_highlight_slides_there() {
        let mut w = speed();
        let (fx, fy) = spot(&w, "Faster");
        let (sx, _) = spot(&w, "Smooth");
        let before = fills(&w, ON);
        assert_eq!(before.len(), 1);
        assert!(before[0].x <= sx && sx < before[0].x + before[0].w);
        // A click on the option chosen chooses nothing.
        assert_eq!(w.mouse_down(&Mono, sx, fy + 4.0), None);
        assert_eq!(w.mouse_down(&Mono, fx, fy + 4.0), Some(Action::Chose("pace", 1)));
        assert_eq!(w.selected("pace"), Some(1));
        // Right after the click the highlight is still over the option left;
        // once the slide is over, over the one chosen.
        w.mouse_move(0.0, 0.0);
        assert_eq!(fills(&w, ON), before);
        let (bar, from, since) = w.slide.unwrap();
        w.slide = Some((bar, from, since - SLIDE));
        let after = fills(&w, ON);
        assert!(after[0].x <= fx && fx < after[0].x + after[0].w, "{after:?}");
    }

    /// Whether a text that starts with `start` is drawn.
    fn shows(w: &Window, start: &str) -> bool {
        w.draw(&Mono).into_iter().any(|c| matches!(c, Command::Text { text, .. } if text.starts_with(start)))
    }

    #[test]
    fn an_info_mark_shows_its_text_while_the_pointer_is_on_it() {
        let mut w = speed();
        assert!(!shows(&w, "Faster time"));
        let (mx, my) = spot(&w, MARK);
        w.mouse_move(mx + 1.0, my + 4.0);
        let (ix, _) = spot(&w, "Faster time");
        assert!(ix > mx);
        // A click on it does nothing.
        assert_eq!(w.mouse_down(&Mono, mx + 1.0, my + 4.0), None);
        w.mouse_move(0.0, 0.0);
        assert!(!shows(&w, "Faster time"));
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
    fn a_toggle_switches_where_its_row_is_clicked() {
        let mut w = Window::new("Settings", vec![Widget::Toggle { id: "fast", label: "Fast".into(), on: true }], 300.0);
        w.center(&Mono, 1000.0, 800.0);
        assert_eq!(w.is_on("fast"), Some(true));
        assert_eq!(w.is_on("none"), None);
        // Its knob sits at the right end of the switch while it is on.
        let knob = |w: &Window| {
            w.draw(&Mono)
                .into_iter()
                .filter_map(|c| match c {
                    Command::Fill { rect, color: TEXT } => Some(rect),
                    _ => None,
                })
                .next_back()
                .unwrap()
        };
        let on = knob(&w);
        let (x, y) = spot(&w, "Fast");
        assert_eq!(w.mouse_down(&Mono, x, y), Some(Action::Switched("fast", false)));
        assert_eq!(w.is_on("fast"), Some(false));
        assert!(knob(&w).x < on.x);
        // The switch itself switches it back.
        let (sx, sy) = (on.x + 2.0, on.y + 2.0);
        assert_eq!(w.mouse_down(&Mono, sx, sy), Some(Action::Switched("fast", true)));
    }

    #[test]
    fn a_toggle_with_a_long_label_wraps_it_beside_the_switch() {
        let short = Window::new("T", vec![Widget::Toggle { id: "t", label: "x".into(), on: false }], 200.0);
        let long = Window::new("T", vec![Widget::Toggle { id: "t", label: "x".repeat(100), on: false }], 200.0);
        assert!(long.height(&Mono) > short.height(&Mono) + 3.0 * TEXT_SIZE);
        let widest = long
            .draw(&Mono)
            .into_iter()
            .filter_map(|c| match c {
                Command::Text { text, .. } if text.starts_with('x') => Some(Mono.width(&text, TEXT_SIZE)),
                _ => None,
            })
            .fold(0.0, f32::max);
        assert!(widest <= 200.0 - 2.0 * PAD - SWITCH_W - PAD);
    }

    #[test]
    fn a_side_list_chooses_and_the_body_stands_beside_it() {
        let side = Side::new("mods", vec!["First".into(), "Second".into(), "x".repeat(40)], 100.0);
        let mut w = Window::new("Mods", vec![Widget::Label("About the first".into())], 400.0).with_side(side);
        w.center(&Mono, 1000.0, 800.0);
        let (lx, _) = spot(&w, "About");
        assert!(lx >= w.x + PAD + 100.0 + PAD, "the body begins right of the list");
        let (x, y) = spot(&w, "Second");
        assert_eq!(w.mouse_down(&Mono, x, y), Some(Action::Chose("mods", 1)));
        assert_eq!(w.side.as_ref().map(|s| s.selected), Some(1));
        // A name too long for the list is cut with an ellipsis.
        let cut = w.draw(&Mono).into_iter().find_map(|c| match c {
            Command::Text { text, .. } if text.starts_with("xx") => Some(text),
            _ => None,
        });
        assert!(cut.as_deref().is_some_and(|t| t.ends_with('…') && Mono.width(t, TEXT_SIZE) <= 100.0 - 16.0), "{cut:?}");
        // The window is as tall as the longer of the two.
        let rows = Window::new("Mods", vec![], 400.0).with_side(Side::new("mods", (0..10).map(|n| n.to_string()).collect(), 100.0)).height(&Mono);
        assert!(rows >= TITLE + PAD + 10.0 * ROW);
    }

    #[test]
    fn a_long_side_list_shows_what_fits_and_scrolls() {
        let items: Vec<String> = (0..40).map(|n| format!("M{n}")).collect();
        let mut w = Window::new("Mods", vec![Widget::Label("About".into())], 400.0).with_side(Side::new("mods", items, 100.0));
        w.center(&Mono, 1000.0, 600.0);
        let shown = |w: &Window| w.draw(&Mono).iter().filter(|c| matches!(c, Command::Text { text, .. } if text.starts_with('M'))).count();
        let rows = shown(&w);
        assert!((3..40).contains(&rows), "{rows} rows on a 600-pixel screen");
        assert!(w.height(&Mono) <= 600.0 * SIDE_SHARE + PAD);
        // The wheel scrolls it while the mouse is over it, and not elsewhere.
        let (x, y) = spot(&w, "M0");
        w.mouse_move(x, y);
        w.wheel(&Mono, -5);
        let (x5, y5) = spot(&w, "M5");
        assert_eq!(w.mouse_down(&Mono, x5, y5), Some(Action::Chose("mods", 5)));
        w.mouse_move(w.x + 300.0, y);
        w.wheel(&Mono, 5);
        assert!(spot_of(&w, "M5").is_some());
        // It stops at the end.
        w.mouse_move(x, y);
        w.wheel(&Mono, -100);
        assert!(spot_of(&w, "M39").is_some() && spot_of(&w, "M0").is_none());
    }

    #[test]
    fn a_side_list_turns_pages_that_keep_their_choices_while_the_window_keeps_its_size() {
        let side = Side::new("pages", vec!["Looks".into(), "Speed".into()], 100.0);
        let pages = vec![
            vec![
                Widget::Label("Typeface".into()),
                Widget::Dropdown { id: "font", options: vec!["A".into(), "B".into()], selected: 0 },
                Widget::Label("Restart".into()),
            ],
            vec![Widget::Toggle { id: "fast", label: "Fast".into(), on: false }],
        ];
        let footer = vec![Widget::Button { id: "save", label: "Save".into() }];
        let mut w = Window::new("Settings", Vec::new(), 400.0).with_side(side).with_pages(pages).with_footer(footer);
        w.center(&Mono, 1000.0, 800.0);
        let (height, save) = (w.height(&Mono), spot(&w, "Save"));
        assert!(shows(&w, "Typeface") && !shows(&w, "Fast"));
        let (x, y) = spot(&w, "A");
        assert_eq!(w.mouse_down(&Mono, x, y), None);
        let (bx, by) = spot(&w, "B");
        assert_eq!(w.mouse_down(&Mono, bx, by), Some(Action::Chose("font", 1)));
        let (sx, sy) = spot(&w, "Speed");
        assert_eq!(w.mouse_down(&Mono, sx, sy), Some(Action::Chose("pages", 1)));
        assert!(shows(&w, "Fast") && !shows(&w, "Typeface"));
        // The shorter page leaves the window as tall and the footer where it was.
        assert_eq!((w.height(&Mono), spot(&w, "Save")), (height, save));
        let (fx, fy) = spot(&w, "Fast");
        assert_eq!(w.mouse_down(&Mono, fx, fy), Some(Action::Switched("fast", true)));
        // What was chosen on a page not shown is still there to read.
        assert_eq!((w.selected("font"), w.is_on("fast")), (Some(1), Some(true)));
        let (lx, ly) = spot(&w, "Looks");
        assert_eq!(w.mouse_down(&Mono, lx, ly), Some(Action::Chose("pages", 0)));
        assert!(shows(&w, "B  ▾"));
        assert_eq!(w.mouse_down(&Mono, save.0, save.1), Some(Action::Pressed("save")));
    }

    #[test]
    fn a_heading_is_larger_and_an_item_stands_beside_its_bullet() {
        let mut w = Window::new("T", vec![Widget::Heading("Name".into()), Widget::Label("Text".into()), Widget::Gap, Widget::Item("x".repeat(60))], 300.0);
        w.center(&Mono, 1000.0, 800.0);
        let commands = w.draw(&Mono);
        let size_of = |start: &str| {
            commands.iter().find_map(|c| match c {
                Command::Text { text, size, .. } if text.starts_with(start) => Some(*size),
                _ => None,
            })
        };
        assert_eq!((size_of("Name"), size_of("Text")), (Some(HEADING_SIZE), Some(TEXT_SIZE)));
        // spot is 2 to the right of the bullet, which is 2 right of the item.
        let (bullet, _) = spot(&w, "•");
        let lefts: Vec<f32> = commands
            .iter()
            .filter_map(|c| match c {
                Command::Text { x, text, .. } if text.starts_with('x') => Some(*x),
                _ => None,
            })
            .collect();
        assert!(lefts.len() > 1 && lefts.iter().all(|x| *x >= bullet - 4.0 + INDENT), "every line of the item is indented: {lefts:?}");
        let gapless = Window::new("T", vec![Widget::Heading("Name".into()), Widget::Label("Text".into()), Widget::Item("x".repeat(60))], 300.0);
        assert_eq!(w.height(&Mono) - gapless.height(&Mono), GAP);
    }

    #[test]
    fn long_labels_wrap_and_grow_the_window() {
        let short = Window::new("T", vec![Widget::Label("x".into())], 200.0);
        let long = Window::new("T", vec![Widget::Label("x".repeat(100))], 200.0);
        assert!(long.height(&Mono) > short.height(&Mono) + 3.0 * TEXT_SIZE);
    }
}
