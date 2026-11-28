//! Widgets (GLOSSARY) and the windows that hold them: layout, what the mouse
//! hits, and the drawing commands for a frame. Nothing here touches the game
//! or Direct3D; [`Measure`] stands in for the fonts.
use crate::base::settings::Face;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// A rectangle in screen pixels or GUI units.
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

/// Scheme A palette: EU4-inspired deep navy with warm gold accents,
/// subtle tonal surfaces, and low-noise hairline dividers.
pub const BACKGROUND: Color = 0xf50e1826;
pub const SIDEBAR_BG: Color = 0xf70a121e;
pub const FOOTER_BG: Color = 0xd80b131f;
pub const SURFACE: Color = 0xc8162539;
pub const RECESSED: Color = 0xe009101b;
pub const BORDER: Color = 0xffb89548;
pub const BORDER_WINDOW: Color = 0xa8b89548;
pub const BORDER_GOLD_SOFT: Color = 0x8cd4af5f;
pub const BORDER_SUBTLE: Color = 0x2694aece;
pub const BORDER_CONTROL: Color = 0x3a94aece;
pub const TITLE_TEXT: Color = 0xffe7c97a;
pub const TEXT: Color = 0xfff0e6d2;
pub const MUTED_TEXT: Color = 0xff9aa8b8;
pub const CONTROL: Color = 0xff192c43;
pub const HOVER: Color = 0xff233c5a;
pub const NAV_ACTIVE: Color = 0xc0233c5c;
pub const NAV_HOVER: Color = 0x16ffffff;
pub const LIST: Color = 0xf80c1624;
pub const PREVIEW_BG: Color = 0xe009101b;
/// Active accent: warm gold for toggles, selected choice segments, primary actions.
pub const ON: Color = 0xffa98332;
pub const ON_HOVER: Color = 0xffc0963d;
pub const LINK: Color = 0xffe7c97a;
pub const LINK_HOVER: Color = 0xffffe39a;
pub const DANGER: Color = 0xd8782a2a;
pub const DANGER_HOVER: Color = 0xf0923434;
pub const DANGER_BORDER: Color = 0x88ce6262;
/// Disabled (greyed-out) palette for controls unavailable in the current context.
pub const DISABLED_TEXT: Color = 0xff6e7b8a;
pub const DISABLED_CONTROL: Color = 0xff142233;
pub const DISABLED_ON: Color = 0xff4c4128;
pub const DISABLED_KNOB: Color = 0xff7a8694;

/// What a frame draws, in order.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Fill {
        rect: Rect,
        color: Color,
    },
    RoundFill {
        rect: Rect,
        color: Color,
        radius: f32,
    },
    RoundStroke {
        rect: Rect,
        color: Color,
        radius: f32,
        thickness: f32,
    },
    Text {
        x: f32,
        y: f32,
        text: String,
        size: f32,
        color: Color,
        face: Option<Face>,
        outline: bool,
    },
    /// Restricts subsequent drawing commands to this rectangle in GUI units;
    /// `None` clears the scissor rectangle.
    Clip(Option<Rect>),
}

impl Command {
    pub fn text(x: f32, y: f32, text: String, size: f32, color: Color) -> Self {
        Self::Text { x, y, text, size, color, face: None, outline: false }
    }
}

/// Text sizes, from the fonts the UI draws with.
pub trait Measure {
    fn width(&self, text: &str, size: f32) -> f32;
    /// Measures `text` in `face` when specified, falling back to the default UI font.
    fn width_with(&self, face: Option<&Face>, text: &str, size: f32) -> f32 {
        let _ = face;
        self.width(text, size)
    }
    /// Splits `text` into lines no wider than `width`.
    fn wrap(&self, text: &str, size: f32, width: f32) -> Vec<String>;
}

/// Visual emphasis for a button in a horizontal [`Widget::ButtonRow`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonStyle {
    #[default]
    Normal,
    Primary,
    Ghost,
    Danger,
}

/// One button inside a horizontal [`Widget::ButtonRow`].
#[derive(Clone, Debug)]
pub struct RowButton {
    pub id: &'static str,
    pub label: String,
    pub style: ButtonStyle,
    pub enabled: bool,
    pub align_right: bool,
}

pub enum Widget {
    /// Presentation state belongs to this visual element, not its layout
    /// parent. The native tooltip adapter consumes the same text description
    /// as native controls do.
    Managed {
        element: Box<Widget>,
        tooltip: eu4_harmony_lua::Text,
        visible: bool,
        enabled: bool,
    },
    Label(String),
    /// A heading: larger text, with room above it where something comes
    /// before.
    Heading(String),
    /// Page title at the top of a settings pane, with a subtle bottom hairline.
    PageHeader(String),
    /// Subtle horizontal hairline separating groups or rows.
    Divider,
    /// One item of a list: a bullet, and its text wrapped beside it.
    Item(String),
    /// Compact brand header card on the About page: name, version pill, and
    /// clickable feedback link.
    AboutHero {
        name: String,
        version: String,
        link_id: &'static str,
        link_label: String,
    },
    /// Room between two parts of a window's body.
    Gap,
    Button {
        id: &'static str,
        label: String,
        style: ButtonStyle,
    },
    /// One horizontal row of buttons (for example, a footer with a left-aligned
    /// reset button and right-aligned cancel/save buttons).
    ButtonRow(Vec<RowButton>),
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
    /// clicked. Any non-empty entry of `tooltips` overrides the bar's own
    /// tooltip while the pointer is on that option.
    Choice {
        id: &'static str,
        options: Vec<String>,
        tooltips: Vec<eu4_harmony_lua::Text>,
        selected: usize,
    },
    /// A slider: sets a number on `min..=max` by clicking or dragging along
    /// its track, showing `format(value)` on the right.
    Slider {
        id: &'static str,
        min: u32,
        max: u32,
        value: u32,
        format: fn(u32) -> String,
    },
    /// A modern horizontal setting/action row: label (and optional dynamic
    /// status line) on the left, compact interactive control on the right.
    Labeled {
        label: String,
        status: Option<String>,
        control: Box<Widget>,
    },
    /// Compact typography card: role label + paired family/weight dropdowns on
    /// one row, recessed live preview strip underneath, and optional map
    /// outline toggle row at the bottom.
    FontGroup {
        role_label: String,
        family_id: &'static str,
        families: Vec<String>,
        family_selected: usize,
        weight_id: &'static str,
        weights: Vec<String>,
        weight_selected: usize,
        preview_id: &'static str,
        preview_text: String,
        preview_face: Face,
        preview_size: f32,
        preview_outline: bool,
        outline_toggle: Option<(&'static str, String, bool)>,
    },
}

impl Widget {
    fn kind(&self) -> &Self {
        match self {
            Self::Managed { element, .. } => element.kind(),
            _ => self,
        }
    }

    fn kind_mut(&mut self) -> &mut Self {
        match self {
            Self::Managed { element, .. } => element.kind_mut(),
            _ => self,
        }
    }

    fn visible(&self) -> bool {
        !matches!(self, Self::Managed { visible: false, .. })
    }

    fn enabled(&self) -> bool {
        !matches!(self, Self::Managed { enabled: false, .. })
    }

    pub fn tooltip(&self) -> eu4_harmony_lua::Text {
        match self {
            Self::Managed { tooltip, .. } => tooltip.clone(),
            _ => Default::default(),
        }
    }

    pub fn set_tooltip(&mut self, text: eu4_harmony_lua::Text) {
        assert!(!matches!(self.kind(), Self::Gap | Self::Divider), "a layout spacer has no tooltip target");
        match self {
            Self::Managed { tooltip, .. } => *tooltip = text,
            _ => {
                let element = Box::new(std::mem::replace(self, Self::Gap));
                *self = Self::Managed { element, tooltip: text, visible: true, enabled: true };
            }
        }
    }

    pub fn with_tooltip(mut self, text: eu4_harmony_lua::Text) -> Self {
        self.set_tooltip(text);
        self
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        assert!(!matches!(self.kind(), Self::Gap | Self::Divider), "a layout spacer has no enabled state");
        match self {
            Self::Managed { enabled: slot, .. } => *slot = enabled,
            _ => {
                let element = Box::new(std::mem::replace(self, Self::Gap));
                *self = Self::Managed { element, tooltip: Default::default(), visible: true, enabled };
            }
        }
    }

    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.set_enabled(enabled);
        self
    }

    fn control_id(&self) -> Option<&'static str> {
        match self.kind() {
            Self::Button { id, .. } | Self::Dropdown { id, .. } | Self::Toggle { id, .. } | Self::Choice { id, .. } | Self::Slider { id, .. } => Some(*id),
            _ => None,
        }
    }
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
    Slid(&'static str, u32),
    Closed,
}

pub const TEXT_SIZE: f32 = 16.0;
const STATUS_SIZE: f32 = 13.5;
const STATUS_LINE: f32 = STATUS_SIZE + 5.0;
const PAD: f32 = 20.0;
const CONTENT_PAD_X: f32 = 24.0;
const ROW: f32 = 32.0;
const SIDE_ROW: f32 = 34.0;
const SIDE_GAP: f32 = 4.0;
const SIDE_INSET: f32 = 10.0;
const PREVIEW_H: f32 = 34.0;
const TITLE: f32 = 42.0;
const CLOSE: f32 = 28.0;
const SWITCH_W: f32 = 38.0;
const SWITCH_H: f32 = 20.0;
const KNOB: f32 = 14.0;
const LINE: f32 = TEXT_SIZE + 6.0;
const HEADING_SIZE: f32 = 20.0;
const HEADING_LINE: f32 = HEADING_SIZE + 6.0;
const PAGE_HEADER_SIZE: f32 = 18.0;
const PAGE_HEADER_LINE: f32 = PAGE_HEADER_SIZE + 8.0;
/// How far an item's text stands right of its bullet.
const INDENT: f32 = 16.0;
const GAP: f32 = 12.0;
const RADIUS_WIN: f32 = 7.0;
const RADIUS_CARD: f32 = 6.0;
const RADIUS_CTRL: f32 = 5.0;
const RADIUS_INNER: f32 = 4.0;
/// The share of the screen a side list may take, and its scroll bar's width.
const SIDE_SHARE: f32 = 0.6;
/// Maximum share of the screen height any window may take before body scrolling activates.
const MAX_WINDOW_SHARE: f32 = 0.85;
const SCROLL_BAR: f32 = 4.0;
const BODY_WHEEL_STEP: f32 = 36.0;
const DROPDOWN_MAX_ROWS: usize = 12;
const DROPDOWN_ROW: f32 = 30.0;
const DROPDOWN_PAD: f32 = 4.0;
/// How long a choice bar's highlight takes to slide to another option.
const SLIDE: Duration = Duration::from_millis(150);
const SLIDER_TRACK_H: f32 = 4.0;
const SLIDER_THUMB_W: f32 = 14.0;
const SLIDER_THUMB_H: f32 = 14.0;
const SLIDER_VALUE_W: f32 = 52.0;
const SLIDER_GAP: f32 = 10.0;
const ROW_BUTTON_GAP: f32 = 8.0;
const ROW_BUTTON_MIN_W: f32 = 84.0;
const FONT_CARD_PAD_X: f32 = 12.0;
const FONT_CARD_PAD_Y: f32 = 10.0;
const FONT_CARD_TOP_H: f32 = 30.0;
const FONT_CARD_TOGGLE_H: f32 = 26.0;
const HERO_PAD_X: f32 = 16.0;
const HERO_PAD_Y: f32 = 14.0;

pub struct Window {
    pub title: String,
    pub body: Vec<Widget>,
    pub side: Option<Side>,
    /// The pages the side list chooses among, when the window keeps them
    /// itself: the page shown is the body, and its place here is empty.
    pub(super) pages: Vec<Vec<Widget>>,
    /// What stands below the body on every page, such as the buttons that
    /// save and cancel.
    footer: Vec<Widget>,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    /// The dropdown whose list is open, by dropdown id.
    open: Option<&'static str>,
    /// The first option the open list shows.
    scroll: usize,
    /// Vertical scroll offset of the body area in GUI units.
    body_scroll: f32,
    /// Screen size in GUI units; caps window height, clamps drag, and bounds open lists.
    screen_w: f32,
    screen_h: f32,
    drag: Option<(f32, f32)>,
    /// The slider being dragged: its id, and its track's left edge and width.
    sliding: Option<(&'static str, f32, f32)>,
    mouse: (f32, f32),
    /// A choice bar's highlight on its way: the bar id, the option it left, and when.
    slide: Option<(&'static str, usize, Instant)>,
    /// Optional per-option font faces for dropdown lists (e.g. font family selectors).
    option_faces: HashMap<&'static str, Vec<Option<Face>>>,
}

/// One laid-out piece of a window.
enum Slot {
    Lines(usize, Vec<String>, Rect, Style),
    Control(usize, Rect),
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

/// Sub-rectangles inside a [`Widget::FontGroup`] card.
struct FontGroupRects {
    role: Rect,
    family: Rect,
    weight: Rect,
    preview: Rect,
    outline: Option<Rect>,
}

/// Sub-rectangles inside a [`Widget::Labeled`] row.
struct LabeledRects {
    label_lines: Vec<String>,
    status_lines: Vec<String>,
    left_y: f32,
    control: Rect,
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
            body_scroll: 0.0,
            screen_w: f32::MAX,
            screen_h: f32::MAX,
            drag: None,
            sliding: None,
            mouse: (-1.0, -1.0),
            slide: None,
            option_faces: HashMap::new(),
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
        self.open = None;
        self.body_scroll = 0.0;
        self.slide = None;
        self.sliding = None;
    }

    /// Selects the page at `index` in a multi-page window.
    pub fn select_page(&mut self, index: usize) {
        let Some(side) = self.side.as_mut() else { return };
        if index >= side.items.len() || index == side.selected {
            return;
        }
        let left = std::mem::replace(&mut side.selected, index);
        if !self.pages.is_empty() {
            self.turn(left, index);
        }
    }

    /// Every top-level widget the window has: the body's, the other pages', the
    /// footer's.
    fn widgets(&self) -> impl Iterator<Item = &Widget> {
        self.body.iter().chain(self.pages.iter().flatten()).chain(&self.footer).map(Widget::kind)
    }

    /// The widget a slot stands for: the body's are numbered first, then the
    /// footer's.
    fn raw_widget(&self, i: usize) -> &Widget {
        self.body.get(i).unwrap_or_else(|| &self.footer[i - self.body.len()])
    }

    fn widget(&self, i: usize) -> &Widget {
        self.raw_widget(i).kind()
    }

    fn widget_mut(&mut self, i: usize) -> &mut Widget {
        let n = self.body.len();
        if i < n {
            self.body[i].kind_mut()
        } else {
            self.footer[i - n].kind_mut()
        }
    }

    /// What stands below the body on every page, to change while the window
    /// is open.
    pub fn footer_mut(&mut self) -> &mut [Widget] {
        &mut self.footer
    }

    /// Sets the text of the status line belonging to `button`: either the
    /// `status` inside a [`Widget::Labeled`] wrapping that button, or the
    /// [`Widget::Label`] immediately following a standalone button.
    pub fn set_label_after(&mut self, button: &str, text: String) {
        let lists = std::iter::once(&mut self.body).chain(self.pages.iter_mut()).chain(std::iter::once(&mut self.footer));
        for list in lists {
            for w in list.iter_mut() {
                if let Widget::Labeled { status, control, .. } = w.kind_mut() {
                    if control.control_id() == Some(button) {
                        *status = if text.is_empty() { None } else { Some(text) };
                        return;
                    }
                }
            }
            let at = list.iter().position(|w| match w.kind() {
                Widget::Button { id, .. } => *id == button,
                Widget::ButtonRow(buttons) => buttons.iter().any(|b| b.id == button),
                _ => false,
            });
            if let Some(Widget::Label(label)) = at.and_then(|i| list.get_mut(i + 1)).map(Widget::kind_mut) {
                *label = text;
                return;
            }
        }
    }

    /// Clamps the window position inside the known screen bounds so the title
    /// bar and buttons cannot move off-screen.
    pub fn clamp_to_screen(&mut self, measure: &dyn Measure) {
        if self.screen_w < 1e9 {
            let max_x = (self.screen_w - self.width).max(0.0);
            self.x = self.x.clamp(0.0, max_x).round();
        }
        if self.screen_h < 1e9 {
            let max_y = (self.screen_h - self.height(measure)).max(0.0);
            self.y = self.y.clamp(0.0, max_y).round();
        }
    }

    /// Puts the window in the middle of a screen, clamped within bounds.
    pub fn center(&mut self, measure: &dyn Measure, screen_w: f32, screen_h: f32) {
        self.screen_w = screen_w;
        self.screen_h = screen_h;
        let h = self.height(measure);
        self.x = ((screen_w - self.width) / 2.0).round();
        self.y = ((screen_h - h) / 2.0).round();
        self.clamp_to_screen(measure);
    }

    pub fn selected(&self, id: &str) -> Option<usize> {
        self.widgets().find_map(|w| match w {
            Widget::Dropdown { id: i, selected, .. } | Widget::Choice { id: i, selected, .. } if *i == id => Some(*selected),
            Widget::Labeled { control, .. } => match control.kind() {
                Widget::Dropdown { id: i, selected, .. } | Widget::Choice { id: i, selected, .. } if *i == id => Some(*selected),
                _ => None,
            },
            Widget::FontGroup { family_id, family_selected, weight_id, weight_selected, .. } => {
                if *family_id == id {
                    Some(*family_selected)
                } else if *weight_id == id {
                    Some(*weight_selected)
                } else {
                    None
                }
            }
            _ => None,
        })
    }

    pub fn is_on(&self, id: &str) -> Option<bool> {
        self.widgets().find_map(|w| match w {
            Widget::Toggle { id: i, on, .. } if *i == id => Some(*on),
            Widget::FontGroup { outline_toggle: Some((i, _, on)), .. } if *i == id => Some(*on),
            _ => None,
        })
    }

    pub fn value(&self, id: &str) -> Option<u32> {
        self.widgets().find_map(|w| match w {
            Widget::Slider { id: i, value, .. } if *i == id => Some(*value),
            Widget::Labeled { control, .. } => match control.kind() {
                Widget::Slider { id: i, value, .. } if *i == id => Some(*value),
                _ => None,
            },
            _ => None,
        })
    }

    /// Gives the drop-down list `id` new options, with `selected` chosen.
    pub fn set_options(&mut self, id: &str, options: Vec<String>, selected: usize) {
        for w in self.body.iter_mut().chain(self.pages.iter_mut().flatten()).chain(&mut self.footer) {
            match w.kind_mut() {
                Widget::Dropdown { id: i, options: o, selected: s } if *i == id => {
                    (*o, *s) = (options, selected);
                    return;
                }
                Widget::Labeled { control, .. } => {
                    if let Widget::Dropdown { id: i, options: o, selected: s } = control.kind_mut() {
                        if *i == id {
                            (*o, *s) = (options, selected);
                            return;
                        }
                    }
                }
                Widget::FontGroup { family_id, families, family_selected, weight_id, weights, weight_selected, .. } => {
                    if *family_id == id {
                        (*families, *family_selected) = (options, selected);
                        return;
                    }
                    if *weight_id == id {
                        (*weights, *weight_selected) = (options, selected);
                        return;
                    }
                }
                _ => {}
            }
        }
    }

    /// Associates per-option font faces with dropdown `id` so visible rows
    /// render in their own font family.
    pub fn set_option_faces(&mut self, id: &'static str, faces: Vec<Option<Face>>) {
        self.option_faces.insert(id, faces);
    }

    /// Updates a [`Widget::Toggle`] or [`Widget::FontGroup`] outline toggle by id.
    pub fn set_toggle(&mut self, id: &str, on: bool) {
        for w in self.body.iter_mut().chain(self.pages.iter_mut().flatten()).chain(&mut self.footer) {
            match w.kind_mut() {
                Widget::Toggle { id: i, on: slot, .. } if *i == id => {
                    *slot = on;
                    return;
                }
                Widget::FontGroup { outline_toggle: Some((i, _, slot)), .. } if *i == id => {
                    *slot = on;
                    return;
                }
                _ => {}
            }
        }
    }

    /// Updates a [`Widget::FontGroup`] preview by id.
    pub fn set_preview(&mut self, id: &str, text: Option<String>, face: Option<Face>, outline: Option<bool>) {
        for w in self.body.iter_mut().chain(self.pages.iter_mut().flatten()).chain(&mut self.footer) {
            if let Widget::FontGroup { preview_id: i, preview_text: t, preview_face: f, preview_outline: o, .. } = w.kind_mut() {
                if *i == id {
                    if let Some(text) = text {
                        *t = text;
                    }
                    if let Some(face) = face {
                        *f = face;
                    }
                    if let Some(outline) = outline {
                        *o = outline;
                    }
                    return;
                }
            }
        }
    }

    /// Enables or disables a button inside any [`Widget::ButtonRow`] by id.
    pub fn set_row_button_enabled(&mut self, id: &str, enabled: bool) {
        for w in self.body.iter_mut().chain(self.pages.iter_mut().flatten()).chain(&mut self.footer) {
            if let Widget::ButtonRow(buttons) = w.kind_mut() {
                for button in buttons {
                    if button.id == id {
                        button.enabled = enabled;
                        return;
                    }
                }
            }
        }
    }

    /// Horizontal padding inside the right content column (or whole window when
    /// there is no side list).
    fn pad_x(&self) -> f32 {
        if self.side.is_some() {
            CONTENT_PAD_X
        } else {
            PAD
        }
    }

    /// Left edge of the right pane (right of the sidebar divider, or window x).
    fn pane_x(&self) -> f32 {
        self.x + self.side.as_ref().map_or(0.0, |s| s.width)
    }

    /// Width of the right pane (everything right of the sidebar).
    fn pane_w(&self) -> f32 {
        self.width - self.side.as_ref().map_or(0.0, |s| s.width)
    }

    /// How wide the body is: the right pane less its horizontal padding.
    fn inner(&self) -> f32 {
        self.pane_w() - 2.0 * self.pad_x()
    }

    /// Where the body begins, right of the side list.
    fn left(&self) -> f32 {
        self.pane_x() + self.pad_x()
    }

    fn side_stride() -> f32 {
        SIDE_ROW + SIDE_GAP
    }

    /// How many rows of the side list show: all of them, or as many as fit
    /// in its share of the screen (at least three).
    fn side_shown(&self) -> usize {
        let Some(side) = &self.side else { return 0 };
        let stride = Self::side_stride();
        let room = ((self.screen_h * SIDE_SHARE - TITLE - 2.0 * SIDE_INSET + SIDE_GAP) / stride).floor().max(3.0);
        side.items.len().min(room as usize)
    }

    /// Where the side list's items sit inside the left sidebar pane.
    fn side_area(&self) -> Option<Rect> {
        let side = self.side.as_ref()?;
        let shown = self.side_shown();
        let h = if shown == 0 { 0.0 } else { shown as f32 * SIDE_ROW + shown.saturating_sub(1) as f32 * SIDE_GAP };
        Some(Rect { x: self.x + SIDE_INSET, y: self.y + TITLE + 14.0, w: (side.width - 2.0 * SIDE_INSET).max(24.0), h })
    }

    /// The side list's rows that show, each with its item's index.
    fn side_rows(&self) -> Vec<(usize, Rect)> {
        let (Some(side), Some(area)) = (&self.side, self.side_area()) else { return Vec::new() };
        let width = if side.items.len() > self.side_shown() { area.w - SCROLL_BAR - 4.0 } else { area.w };
        let stride = Self::side_stride();
        (0..self.side_shown()).map(|n| (side.scroll + n, Rect { x: area.x, y: area.y + n as f32 * stride, w: width, h: SIDE_ROW })).collect()
    }

    /// How wide a toggle's label may be, left of its switch.
    fn toggle_text(&self) -> f32 {
        self.inner() - SWITCH_W - PAD
    }

    /// The switch at the right end of a toggle's row.
    fn switch(row: Rect) -> Rect {
        Rect { x: row.x + row.w - SWITCH_W, y: row.y + ((row.h - SWITCH_H) / 2.0).round(), w: SWITCH_W, h: SWITCH_H }
    }

    /// A choice bar's options, side by side inside its padded container.
    fn segments(row: Rect, options: usize) -> Vec<Rect> {
        let pad = 2.0;
        let inner_x = row.x + pad;
        let inner_w = (row.w - 2.0 * pad).max(1.0);
        let w = inner_w / options.max(1) as f32;
        (0..options).map(|n| Rect { x: (inner_x + n as f32 * w).round(), y: row.y + pad, w: w.round(), h: (row.h - 2.0 * pad).max(1.0) }).collect()
    }

    /// Sub-rectangles for a [`Widget::FontGroup`] card.
    fn font_group_rects(r: Rect, has_outline: bool) -> FontGroupRects {
        let inner_x = r.x + FONT_CARD_PAD_X;
        let inner_w = (r.w - 2.0 * FONT_CARD_PAD_X).max(120.0);
        let top_y = r.y + FONT_CARD_PAD_Y;
        let role_w = (inner_w * 0.24).clamp(64.0, 134.0).round();
        let weight_w = (inner_w * 0.24).clamp(92.0, 124.0).round();
        let family_w = (inner_w - role_w - 8.0 - weight_w - 8.0).max(80.0).round();
        let role = Rect { x: inner_x, y: top_y, w: role_w, h: FONT_CARD_TOP_H };
        let family = Rect { x: role.x + role.w + 8.0, y: top_y, w: family_w, h: FONT_CARD_TOP_H };
        let weight = Rect { x: family.x + family.w + 8.0, y: top_y, w: weight_w, h: FONT_CARD_TOP_H };
        let preview = Rect { x: inner_x, y: top_y + FONT_CARD_TOP_H + 8.0, w: inner_w, h: PREVIEW_H };
        let outline = has_outline.then_some(Rect { x: inner_x, y: preview.y + preview.h + 8.0, w: inner_w, h: FONT_CARD_TOGGLE_H });
        FontGroupRects { role, family, weight, preview, outline }
    }

    fn font_group_height(has_outline: bool) -> f32 {
        let base = FONT_CARD_PAD_Y + FONT_CARD_TOP_H + 8.0 + PREVIEW_H + FONT_CARD_PAD_Y;
        if has_outline {
            base + 8.0 + FONT_CARD_TOGGLE_H
        } else {
            base
        }
    }

    /// Compact width for the right-aligned control inside [`Widget::Labeled`].
    fn labeled_control_width(measure: &dyn Measure, control: &Widget, row_w: f32) -> f32 {
        match control.kind() {
            Widget::Dropdown { .. } => (row_w * 0.44).clamp(160.0, 224.0).min(row_w * 0.55).round(),
            Widget::Choice { options, .. } => {
                let widest = options.iter().map(|o| measure.width(o, TEXT_SIZE) + 14.0).fold(34.0, f32::max);
                (widest * options.len().max(1) as f32 + 6.0).clamp(148.0, row_w * 0.72).round()
            }
            Widget::Slider { .. } => (row_w * 0.48).clamp(180.0, 250.0).min(row_w * 0.56).round(),
            Widget::Button { label, .. } => (measure.width(label, TEXT_SIZE) + 28.0).clamp(108.0, row_w * 0.56).round(),
            _ => (row_w * 0.45).round(),
        }
    }

    /// Layout geometry for a [`Widget::Labeled`] row.
    fn labeled_rects(measure: &dyn Measure, r: Rect, label: &str, status: Option<&str>, control: &Widget) -> LabeledRects {
        let ctrl_w = Self::labeled_control_width(measure, control, r.w);
        let text_w = (r.w - ctrl_w - 16.0).max(80.0);
        let label_lines = measure.wrap(label, TEXT_SIZE, text_w);
        let status_lines = status.filter(|s| !s.is_empty()).map(|s| measure.wrap(s, STATUS_SIZE, text_w)).unwrap_or_default();
        let label_h = label_lines.len().max(1) as f32 * LINE;
        let status_h = if status_lines.is_empty() { 0.0 } else { 2.0 + status_lines.len() as f32 * STATUS_LINE };
        let text_total = label_h + status_h;
        let left_y = r.y + ((r.h - text_total) / 2.0).round();
        let control_rect = Rect { x: (r.x + r.w - ctrl_w).round(), y: r.y + ((r.h - ROW) / 2.0).round(), w: ctrl_w, h: ROW };
        LabeledRects { label_lines, status_lines, left_y, control: control_rect }
    }

    fn labeled_height(measure: &dyn Measure, row_w: f32, label: &str, status: Option<&str>, control: &Widget) -> f32 {
        let ctrl_w = Self::labeled_control_width(measure, control, row_w);
        let text_w = (row_w - ctrl_w - 16.0).max(80.0);
        let label_h = measure.wrap(label, TEXT_SIZE, text_w).len().max(1) as f32 * LINE;
        let status_h = status.filter(|s| !s.is_empty()).map_or(0.0, |s| 2.0 + measure.wrap(s, STATUS_SIZE, text_w).len().max(1) as f32 * STATUS_LINE);
        (label_h + status_h).max(ROW) + 12.0
    }

    /// Computes horizontal button rects inside a [`Widget::ButtonRow`], scaling
    /// widths down proportionally if long labels would otherwise exceed `row.w`.
    fn row_button_rects(measure: &dyn Measure, row: Rect, buttons: &[RowButton]) -> Vec<Rect> {
        if buttons.is_empty() {
            return Vec::new();
        }
        let mut widths: Vec<f32> = buttons
            .iter()
            .map(|b| {
                let pad = match b.style {
                    ButtonStyle::Primary => 34.0,
                    ButtonStyle::Normal | ButtonStyle::Danger => 26.0,
                    ButtonStyle::Ghost => 20.0,
                };
                let min_w = if b.style == ButtonStyle::Ghost { 64.0 } else { ROW_BUTTON_MIN_W };
                (measure.width(&b.label, TEXT_SIZE) + pad).max(min_w)
            })
            .collect();
        let total_gaps = ROW_BUTTON_GAP * buttons.len().saturating_sub(1) as f32;
        let sum: f32 = widths.iter().sum();
        let avail = (row.w - total_gaps).max(buttons.len() as f32 * 24.0);
        if sum > avail && sum > 0.0 {
            let scale = avail / sum;
            for w in &mut widths {
                *w = (*w * scale).floor().max(24.0);
            }
        }
        let right_count = buttons.iter().filter(|b| b.align_right).count();
        let left_count = buttons.len() - right_count;
        let left_total: f32 =
            buttons.iter().zip(&widths).filter(|(b, _)| !b.align_right).map(|(_, &w)| w).sum::<f32>() + ROW_BUTTON_GAP * left_count.saturating_sub(1) as f32;
        let right_total: f32 =
            buttons.iter().zip(&widths).filter(|(b, _)| b.align_right).map(|(_, &w)| w).sum::<f32>() + ROW_BUTTON_GAP * right_count.saturating_sub(1) as f32;
        let min_right = if left_count > 0 { row.x + left_total + ROW_BUTTON_GAP } else { row.x };
        let mut left_x = row.x;
        let mut right_x = (row.x + row.w - right_total).max(min_right);
        buttons
            .iter()
            .zip(widths)
            .map(|(b, w)| {
                if b.align_right {
                    let r = Rect { x: right_x.round(), y: row.y, w: w.round(), h: row.h };
                    right_x += w + ROW_BUTTON_GAP;
                    r
                } else {
                    let r = Rect { x: left_x.round(), y: row.y, w: w.round(), h: row.h };
                    left_x += w + ROW_BUTTON_GAP;
                    r
                }
            })
            .collect()
    }

    /// A slider's horizontal track inside its row, leaving room on the right
    /// for its formatted value.
    fn slider_track(row: Rect) -> Rect {
        let w = (row.w - SLIDER_VALUE_W - SLIDER_GAP).max(SLIDER_THUMB_W);
        Rect { x: row.x, y: row.y + ((row.h - SLIDER_TRACK_H) / 2.0).round(), w, h: SLIDER_TRACK_H }
    }

    /// Maps a horizontal pointer coordinate onto a slider's `min..=max` range.
    fn slider_value_at(x: f32, track_x: f32, track_w: f32, min: u32, max: u32) -> u32 {
        if max <= min {
            return min;
        }
        let span = (track_w - SLIDER_THUMB_W).max(1.0);
        let center_min = track_x + SLIDER_THUMB_W / 2.0;
        let t = ((x - center_min) / span).clamp(0.0, 1.0);
        min + (t * (max - min) as f32).round() as u32
    }

    /// Where a choice bar's highlight is, over the option chosen or on its
    /// way there from the one left.
    fn highlight(&self, id: &str, segments: &[Rect], selected: usize) -> Rect {
        let to = segments[selected];
        let Some((_, from, since)) = self.slide.filter(|s| s.0 == id) else { return to };
        let t = (since.elapsed().as_secs_f32() / SLIDE.as_secs_f32()).min(1.0);
        let eased = t * t * (3.0 - 2.0 * t);
        let from = segments.get(from).copied().unwrap_or(to);
        Rect { x: (from.x + (to.x - from.x) * eased).round(), ..to }
    }

    fn footer_top_pad() -> f32 {
        12.0
    }

    fn footer_bottom_pad() -> f32 {
        12.0
    }

    fn footer_height(&self, measure: &dyn Measure) -> f32 {
        if self.footer.is_empty() {
            return 0.0;
        }
        let slots = self.lay_out(measure, &self.footer, self.body.len(), 0.0);
        let h = bottom(&slots, 0.0);
        if h > 0.0 {
            h + Self::footer_top_pad() + Self::footer_bottom_pad()
        } else {
            0.0
        }
    }

    /// Natural content height of `widgets` from 0.
    fn content_height(&self, measure: &dyn Measure, widgets: &[Widget]) -> f32 {
        bottom(&self.lay_out(measure, widgets, 0, 0.0), 0.0)
    }

    /// Tallest body content across the active page and any stored pages.
    fn max_page_content_height(&self, measure: &dyn Measure) -> f32 {
        let current = self.content_height(measure, &self.body);
        self.pages.iter().map(|page| self.content_height(measure, page)).fold(current, f32::max)
    }

    /// Maximum height the window may take on the current screen.
    fn max_height(&self) -> f32 {
        if self.screen_h >= 1e9 {
            return f32::MAX;
        }
        (self.screen_h * MAX_WINDOW_SHARE).max(240.0).min((self.screen_h - 16.0).max(160.0)).round()
    }

    fn body_top_pad() -> f32 {
        16.0
    }

    fn body_bottom_pad(&self) -> f32 {
        if self.footer.is_empty() {
            PAD
        } else {
            10.0
        }
    }

    /// Visible rectangle of the scrollable body area.
    fn body_viewport(&self, measure: &dyn Measure) -> Rect {
        let top = self.y + TITLE + Self::body_top_pad();
        let h = self.height(measure);
        let footer_h = self.footer_height(measure);
        let bottom = self.y + h - footer_h - self.body_bottom_pad();
        Rect { x: self.left(), y: top, w: self.inner(), h: (bottom - top).max(ROW) }
    }

    fn max_body_scroll(&self, measure: &dyn Measure) -> f32 {
        let content_h = self.content_height(measure, &self.body);
        let viewport_h = self.body_viewport(measure).h;
        (content_h - viewport_h).max(0.0)
    }

    fn clamped_body_scroll(&self, measure: &dyn Measure) -> f32 {
        self.body_scroll.clamp(0.0, self.max_body_scroll(measure))
    }

    fn body_slots(&self, measure: &dyn Measure) -> Vec<Slot> {
        let top = self.y + TITLE + Self::body_top_pad() - self.clamped_body_scroll(measure);
        self.lay_out(measure, &self.body, 0, top)
    }

    fn footer_slots(&self, measure: &dyn Measure) -> Vec<Slot> {
        if self.footer.is_empty() {
            return Vec::new();
        }
        let footer_h = self.footer_height(measure);
        let y = self.y + self.height(measure) - footer_h + Self::footer_top_pad();
        self.lay_out(measure, &self.footer, self.body.len(), y)
    }

    /// Lays `widgets` out from `y` down, numbering them from `first`.
    fn lay_out(&self, measure: &dyn Measure, widgets: &[Widget], first: usize, mut y: f32) -> Vec<Slot> {
        let mut slots = Vec::new();
        for (n, w) in widgets.iter().enumerate() {
            let i = first + n;
            if !w.visible() {
                continue;
            }
            let w = w.kind();
            match w {
                Widget::Managed { .. } => unreachable!("kind unwraps presentation"),
                Widget::Label(text) | Widget::Heading(text) | Widget::Item(text) => {
                    if matches!(w, Widget::Label(t) if t.is_empty()) {
                        continue;
                    }
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
                    slots.push(Slot::Lines(i, lines, Rect { x: self.left(), y, w: self.inner(), h }, style));
                    y += h + 6.0;
                }
                Widget::PageHeader(_) => {
                    let h = PAGE_HEADER_LINE + 10.0;
                    slots.push(Slot::Control(i, Rect { x: self.left(), y, w: self.inner(), h }));
                    y += h + 10.0;
                }
                Widget::Divider => {
                    slots.push(Slot::Control(i, Rect { x: self.left(), y: y + 2.0, w: self.inner(), h: 1.0 }));
                    y += 5.0;
                }
                Widget::AboutHero { link_label, .. } => {
                    let inner_w = (self.inner() - 2.0 * HERO_PAD_X).max(80.0);
                    let link_lines = measure.wrap(link_label, STATUS_SIZE, inner_w).len().max(1) as f32 * STATUS_LINE;
                    let h = HERO_PAD_Y + HEADING_LINE + 6.0 + link_lines + HERO_PAD_Y;
                    slots.push(Slot::Control(i, Rect { x: self.left(), y, w: self.inner(), h }));
                    y += h + 12.0;
                }
                Widget::Gap => y += GAP,
                Widget::Button { label, .. } => {
                    let w = (measure.width(label, TEXT_SIZE) + 28.0).clamp(108.0, self.inner()).round();
                    slots.push(Slot::Control(i, Rect { x: self.left(), y, w, h: ROW }));
                    y += ROW + 8.0;
                }
                Widget::ButtonRow(_) | Widget::Dropdown { .. } | Widget::Choice { .. } | Widget::Slider { .. } => {
                    slots.push(Slot::Control(i, Rect { x: self.left(), y, w: self.inner(), h: ROW }));
                    y += ROW + 8.0;
                }
                Widget::Toggle { label, .. } => {
                    let lines = measure.wrap(label, TEXT_SIZE, self.toggle_text());
                    let h = (lines.len().max(1) as f32 * LINE).max(ROW) + 6.0;
                    slots.push(Slot::Control(i, Rect { x: self.left(), y, w: self.inner(), h }));
                    y += h + 4.0;
                }
                Widget::Labeled { label, status, control } => {
                    let h = Self::labeled_height(measure, self.inner(), label, status.as_deref(), control);
                    slots.push(Slot::Control(i, Rect { x: self.left(), y, w: self.inner(), h }));
                    y += h + 2.0;
                }
                Widget::FontGroup { outline_toggle, .. } => {
                    let h = Self::font_group_height(outline_toggle.is_some());
                    slots.push(Slot::Control(i, Rect { x: self.left(), y, w: self.inner(), h }));
                    y += h + 10.0;
                }
            }
        }
        slots
    }

    /// How tall the window is: as tall as its tallest page (keeping size steady
    /// across pages), capped to [`MAX_WINDOW_SHARE`] of the screen so the title
    /// bar and footer stay visible on small or high-scale screens.
    pub fn height(&self, measure: &dyn Measure) -> f32 {
        let body_and_footer = Self::body_top_pad() + self.max_page_content_height(measure) + self.body_bottom_pad() + self.footer_height(measure);
        let side = self.side_area().map_or(0.0, |r| 14.0 + r.h + 14.0);
        let natural = TITLE + body_and_footer.max(side);
        natural.min(self.max_height())
    }

    pub fn bounds(&self, measure: &dyn Measure) -> Rect {
        Rect { x: self.x, y: self.y, w: self.width, h: self.height(measure) }
    }

    fn close_box(&self) -> Rect {
        Rect { x: self.x + self.width - CLOSE - 10.0, y: self.y + ((TITLE - CLOSE) / 2.0).round(), w: CLOSE, h: CLOSE }
    }

    /// How many options an open list shows below or above `control`.
    fn rows_for(&self, control: Rect, options: usize) -> (usize, bool) {
        let below = ((self.screen_h - (control.y + control.h + 4.0) - PAD - 2.0 * DROPDOWN_PAD) / DROPDOWN_ROW).floor().max(0.0) as usize;
        let above = ((control.y - 4.0 - PAD - 2.0 * DROPDOWN_PAD) / DROPDOWN_ROW).floor().max(0.0) as usize;
        if below >= 3 || below >= above {
            (options.min(below.clamp(3, DROPDOWN_MAX_ROWS)), false)
        } else {
            (options.min(above.clamp(3, DROPDOWN_MAX_ROWS)), true)
        }
    }

    fn list_rect(&self, control: Rect, options: usize) -> Rect {
        let (rows, upward) = self.rows_for(control, options);
        let h = rows as f32 * DROPDOWN_ROW + 2.0 * DROPDOWN_PAD;
        let y = if upward { (control.y - h - 3.0).max(PAD) } else { control.y + control.h + 3.0 };
        Rect { x: control.x, y, w: control.w, h }
    }

    /// Locates the control rect and option slice for dropdown `target_id` in
    /// the current body slots (standalone, inside `Labeled`, or inside `FontGroup`).
    fn find_dropdown(&self, measure: &dyn Measure, target_id: &str) -> Option<(Rect, &[String], usize)> {
        for slot in self.body_slots(measure) {
            let Slot::Control(i, r) = slot else { continue };
            match self.widget(i) {
                Widget::Dropdown { id, options, selected } if *id == target_id => return Some((r, options, *selected)),
                Widget::Labeled { label, status, control } => {
                    if let Widget::Dropdown { id, options, selected } = control.kind() {
                        if *id == target_id {
                            let layout = Self::labeled_rects(measure, r, label, status.as_deref(), control);
                            return Some((layout.control, options, *selected));
                        }
                    }
                }
                Widget::FontGroup { family_id, families, family_selected, weight_id, weights, weight_selected, outline_toggle, .. } => {
                    if *family_id == target_id {
                        let sub = Self::font_group_rects(r, outline_toggle.is_some());
                        return Some((sub.family, families, *family_selected));
                    }
                    if *weight_id == target_id {
                        let sub = Self::font_group_rects(r, outline_toggle.is_some());
                        return Some((sub.weight, weights, *weight_selected));
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// Mutates a dropdown's `selected` index by id in the current body.
    fn set_body_dropdown_selected(&mut self, target_id: &str, index: usize) -> Option<&'static str> {
        for raw in &mut self.body {
            if !raw.enabled() || !raw.visible() {
                continue;
            }
            match raw.kind_mut() {
                Widget::Dropdown { id, options, selected } if *id == target_id => {
                    if index < options.len() {
                        *selected = index;
                        return Some(*id);
                    }
                }
                Widget::Labeled { control, .. } => {
                    if !control.enabled() || !control.visible() {
                        continue;
                    }
                    if let Widget::Dropdown { id, options, selected } = control.kind_mut() {
                        if *id == target_id && index < options.len() {
                            *selected = index;
                            return Some(*id);
                        }
                    }
                }
                Widget::FontGroup { family_id, families, family_selected, weight_id, weights, weight_selected, .. } => {
                    if *family_id == target_id && index < families.len() {
                        *family_selected = index;
                        return Some(*family_id);
                    }
                    if *weight_id == target_id && index < weights.len() {
                        *weight_selected = index;
                        return Some(*weight_id);
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// Steps a slider by `notches` or sets it to `target` by id in the current body.
    fn update_slider(&mut self, target_id: &str, update: impl FnOnce(u32, u32, u32) -> u32) -> Option<Action> {
        for raw in &mut self.body {
            if !raw.enabled() || !raw.visible() {
                continue;
            }
            match raw.kind_mut() {
                Widget::Slider { id, min, max, value, .. } if *id == target_id => {
                    let next = update(*value, *min, *max);
                    if next != *value {
                        *value = next;
                        return Some(Action::Slid(id, next));
                    }
                    return None;
                }
                Widget::Labeled { control, .. } => {
                    if !control.enabled() || !control.visible() {
                        continue;
                    }
                    if let Widget::Slider { id, min, max, value, .. } = control.kind_mut() {
                        if *id == target_id {
                            let next = update(*value, *min, *max);
                            if next != *value {
                                *value = next;
                                return Some(Action::Slid(id, next));
                            }
                            return None;
                        }
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// The mouse wheel turned `notches` (up is positive) over the window: an
    /// open list scrolls, the side list under the mouse scrolls, a hovered
    /// slider steps by `notches`, or the scrollable body scrolls.
    pub fn wheel(&mut self, measure: &dyn Measure, notches: i32) -> Option<Action> {
        if let Some(open_id) = self.open {
            let (control, options, _) = self.find_dropdown(measure, open_id)?;
            let len = options.len();
            let (rows, _) = self.rows_for(control, len);
            let last = len.saturating_sub(rows);
            self.scroll = (self.scroll as i64 - notches as i64).clamp(0, last as i64) as usize;
            return None;
        }
        let over_side = self.side_area().is_some_and(|r| r.contains(self.mouse.0, self.mouse.1));
        let shown = self.side_shown();
        if let (true, Some(side)) = (over_side, self.side.as_mut()) {
            let last = side.items.len().saturating_sub(shown);
            side.scroll = (side.scroll as i64 - notches as i64).clamp(0, last as i64) as usize;
            return None;
        }
        let (mx, my) = self.mouse;
        let viewport = self.body_viewport(measure);
        if viewport.contains(mx, my) {
            let mut hit_slider: Option<&'static str> = None;
            for slot in self.body_slots(measure) {
                if let Slot::Control(i, r) = slot {
                    if r.contains(mx, my) && self.raw_widget(i).enabled() {
                        match self.widget(i) {
                            Widget::Slider { id, .. } => {
                                hit_slider = Some(*id);
                                break;
                            }
                            Widget::Labeled { label, status, control } if control.enabled() => {
                                if let Widget::Slider { id, .. } = control.kind() {
                                    let layout = Self::labeled_rects(measure, r, label, status.as_deref(), control);
                                    if layout.control.contains(mx, my) || r.contains(mx, my) {
                                        hit_slider = Some(*id);
                                        break;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            if let Some(id) = hit_slider {
                return self.update_slider(id, |v, min, max| (v as i64 + notches as i64).clamp(min as i64, max as i64) as u32);
            }
            let max_scroll = self.max_body_scroll(measure);
            if max_scroll > 0.0 {
                self.body_scroll = (self.body_scroll - notches as f32 * BODY_WHEEL_STEP).clamp(0.0, max_scroll);
            }
        }
        None
    }

    /// Whether the window, or an open list hanging outside it, covers a point.
    pub fn covers(&self, measure: &dyn Measure, x: f32, y: f32) -> bool {
        self.bounds(measure).contains(x, y) || self.open_list(measure).is_some_and(|(r, _)| r.contains(x, y))
    }

    fn open_list(&self, measure: &dyn Measure) -> Option<(Rect, &'static str)> {
        let open_id = self.open?;
        let (control, options, _) = self.find_dropdown(measure, open_id)?;
        Some((self.list_rect(control, options.len()), open_id))
    }

    pub fn mouse_move(&mut self, x: f32, y: f32) -> Option<Action> {
        self.mouse = (x, y);
        if let Some((dx, dy)) = self.drag {
            self.x = x - dx;
            self.y = y - dy;
            if self.screen_w < 1e9 {
                let max_x = (self.screen_w - self.width).max(0.0);
                self.x = self.x.clamp(0.0, max_x);
            }
            if self.screen_h < 1e9 {
                let max_y = (self.screen_h - TITLE).max(0.0);
                self.y = self.y.clamp(0.0, max_y);
            }
            return None;
        }
        if let Some((id, track_x, track_w)) = self.sliding {
            return self.update_slider(id, |_, min, max| Self::slider_value_at(x, track_x, track_w, min, max));
        }
        None
    }

    pub fn mouse_up(&mut self) {
        self.drag = None;
        self.sliding = None;
    }

    fn handle_leaf_click(&mut self, target_id: Option<&'static str>, r: Rect, x: f32, y: f32, slot_i: usize) -> Option<Action> {
        let _ = target_id;
        // Handles Choice / Slider / Dropdown / Button / DangerButton when direct or wrapped in Labeled.
        let (is_choice, is_slider, is_dropdown, is_button) = match self.widget(slot_i) {
            Widget::Labeled { control, .. } => (
                matches!(control.kind(), Widget::Choice { .. }),
                matches!(control.kind(), Widget::Slider { .. }),
                matches!(control.kind(), Widget::Dropdown { .. }),
                matches!(control.kind(), Widget::Button { .. }),
            ),
            w => (
                matches!(w, Widget::Choice { .. }),
                matches!(w, Widget::Slider { .. }),
                matches!(w, Widget::Dropdown { .. }),
                matches!(w, Widget::Button { .. }),
            ),
        };
        if is_choice {
            let control = match self.widget_mut(slot_i) {
                Widget::Labeled { control, .. } => control.kind_mut(),
                w => w,
            };
            if let Widget::Choice { id, options, selected, .. } = control {
                let index = Self::segments(r, options.len()).iter().position(|s| s.contains(x, y))?;
                if index == *selected {
                    return None;
                }
                let (id, left) = (*id, std::mem::replace(selected, index));
                self.slide = Some((id, left, Instant::now()));
                return Some(Action::Chose(id, index));
            }
        }
        if is_slider {
            let control = match self.widget_mut(slot_i) {
                Widget::Labeled { control, .. } => control.kind_mut(),
                w => w,
            };
            if let Widget::Slider { id, min, max, value, .. } = control {
                let track = Self::slider_track(r);
                let next = Self::slider_value_at(x, track.x, track.w, *min, *max);
                let changed = next != *value;
                *value = next;
                let id = *id;
                self.sliding = Some((id, track.x, track.w));
                return changed.then_some(Action::Slid(id, next));
            }
        }
        if is_dropdown {
            let (id, len, selected) = match self.widget(slot_i) {
                Widget::Dropdown { id, options, selected } => (*id, options.len(), *selected),
                Widget::Labeled { control, .. } => match control.kind() {
                    Widget::Dropdown { id, options, selected } => (*id, options.len(), *selected),
                    _ => return None,
                },
                _ => return None,
            };
            let (rows, _) = self.rows_for(r, len);
            self.scroll = selected.saturating_sub(rows / 2).min(len.saturating_sub(rows));
            self.open = Some(id);
            return None;
        }
        if is_button {
            let id = match self.widget(slot_i) {
                Widget::Button { id, .. } => *id,
                Widget::Labeled { control, .. } => control.control_id()?,
                _ => return None,
            };
            return Some(Action::Pressed(id));
        }
        None
    }

    fn handle_slot_click(&mut self, measure: &dyn Measure, i: usize, r: Rect, x: f32, y: f32) -> Option<Action> {
        if !self.raw_widget(i).enabled() {
            return None;
        }
        if let Widget::Toggle { id, on, .. } = self.widget_mut(i) {
            *on = !*on;
            return Some(Action::Switched(id, *on));
        }
        if let Widget::AboutHero { link_id, .. } = self.widget(i) {
            let link_top = r.y + HERO_PAD_Y + HEADING_LINE + 4.0;
            if y >= link_top {
                return Some(Action::Pressed(link_id));
            }
            return None;
        }
        if let Widget::Labeled { label, status, control } = self.widget(i) {
            if !control.enabled() {
                return None;
            }
            let layout = Self::labeled_rects(measure, r, label, status.as_deref(), control);
            if layout.control.contains(x, y) {
                return self.handle_leaf_click(control.control_id(), layout.control, x, y, i);
            }
            return None;
        }
        if let Widget::FontGroup { family_id, families, family_selected, weight_id, weights, weight_selected, outline_toggle, .. } = self.widget(i) {
            let sub = Self::font_group_rects(r, outline_toggle.is_some());
            if sub.family.contains(x, y) {
                let (id, len, selected) = (*family_id, families.len(), *family_selected);
                let (rows, _) = self.rows_for(sub.family, len);
                self.scroll = selected.saturating_sub(rows / 2).min(len.saturating_sub(rows));
                self.open = Some(id);
                return None;
            }
            if sub.weight.contains(x, y) {
                let (id, len, selected) = (*weight_id, weights.len(), *weight_selected);
                let (rows, _) = self.rows_for(sub.weight, len);
                self.scroll = selected.saturating_sub(rows / 2).min(len.saturating_sub(rows));
                self.open = Some(id);
                return None;
            }
            if let Some(outline_rect) = sub.outline {
                if outline_rect.contains(x, y) {
                    if let Widget::FontGroup { outline_toggle: Some((id, _, on)), .. } = self.widget_mut(i) {
                        *on = !*on;
                        return Some(Action::Switched(id, *on));
                    }
                }
            }
            return None;
        }
        match self.widget(i) {
            Widget::ButtonRow(buttons) => {
                let rects = Self::row_button_rects(measure, r, buttons);
                for (button, rect) in buttons.iter().zip(rects) {
                    if rect.contains(x, y) && button.enabled {
                        return Some(Action::Pressed(button.id));
                    }
                }
                None
            }
            Widget::Button { .. } | Widget::Dropdown { .. } | Widget::Choice { .. } | Widget::Slider { .. } => self.handle_leaf_click(None, r, x, y, i),
            Widget::Label(_)
            | Widget::Heading(_)
            | Widget::PageHeader(_)
            | Widget::Divider
            | Widget::Item(_)
            | Widget::Gap
            | Widget::Toggle { .. }
            | Widget::AboutHero { .. }
            | Widget::Labeled { .. }
            | Widget::FontGroup { .. }
            | Widget::Managed { .. } => None,
        }
    }

    /// A left click at a point the window covers.
    pub fn mouse_down(&mut self, measure: &dyn Measure, x: f32, y: f32) -> Option<Action> {
        if let Some((list, open_id)) = self.open_list(measure) {
            self.open = None;
            if list.contains(x, y) {
                let rel_y = y - (list.y + DROPDOWN_PAD);
                if rel_y >= 0.0 {
                    let index = self.scroll + (rel_y / DROPDOWN_ROW) as usize;
                    if let Some(id) = self.set_body_dropdown_selected(open_id, index) {
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
        for slot in self.footer_slots(measure) {
            if let Slot::Control(i, r) = slot {
                if r.contains(x, y) {
                    return self.handle_slot_click(measure, i, r, x, y);
                }
            }
        }
        let viewport = self.body_viewport(measure);
        if viewport.contains(x, y) {
            for slot in self.body_slots(measure) {
                if let Slot::Control(i, r) = slot {
                    if r.contains(x, y) {
                        return self.handle_slot_click(measure, i, r, x, y);
                    }
                }
            }
        }
        None
    }

    /// Only the hit visual element supplies content. A covered region with no
    /// tooltip is explicitly empty, suppressing an underlying native target.
    pub fn tooltip_at(&self, measure: &dyn Measure, x: f32, y: f32) -> Option<eu4_harmony_lua::Text> {
        if !self.covers(measure, x, y) {
            return None;
        }
        if self.open_list(measure).is_some_and(|(rect, _)| rect.contains(x, y)) {
            return Some(Default::default());
        }
        for slot in self.footer_slots(measure).into_iter().rev() {
            let (i, rect) = match slot {
                Slot::Lines(i, _, rect, _) | Slot::Control(i, rect) => (i, rect),
            };
            if rect.contains(x, y) {
                return Some(self.raw_widget(i).tooltip());
            }
        }
        let viewport = self.body_viewport(measure);
        if viewport.contains(x, y) {
            for slot in self.body_slots(measure).into_iter().rev() {
                let (i, rect) = match slot {
                    Slot::Lines(i, _, rect, _) | Slot::Control(i, rect) => (i, rect),
                };
                if rect.contains(x, y) {
                    match self.widget(i) {
                        Widget::Choice { options, tooltips, .. } => {
                            if let Some(index) = Self::segments(rect, options.len()).iter().position(|s| s.contains(x, y)) {
                                if let Some(tip) = tooltips.get(index).filter(|t| **t != eu4_harmony_lua::Text::default()) {
                                    return Some(tip.clone());
                                }
                            }
                        }
                        Widget::Labeled { label, status, control } => {
                            let layout = Self::labeled_rects(measure, rect, label, status.as_deref(), control);
                            if let Widget::Choice { options, tooltips, .. } = control.kind() {
                                if let Some(index) = Self::segments(layout.control, options.len()).iter().position(|s| s.contains(x, y)) {
                                    if let Some(tip) = tooltips.get(index).filter(|t| **t != eu4_harmony_lua::Text::default()) {
                                        return Some(tip.clone());
                                    }
                                }
                            }
                            let inner_tip = control.tooltip();
                            if inner_tip != eu4_harmony_lua::Text::default() {
                                return Some(inner_tip);
                            }
                        }
                        _ => {}
                    }
                    return Some(self.raw_widget(i).tooltip());
                }
            }
        }
        Some(Default::default())
    }

    fn draw_toggle_switch(out: &mut Vec<Command>, track: Rect, on: bool, enabled: bool, hovered: bool) {
        let color = match (enabled, on, hovered) {
            (false, true, _) => DISABLED_ON,
            (false, false, _) => DISABLED_CONTROL,
            (true, true, false) => ON,
            (true, true, true) => ON_HOVER,
            (true, false, false) => CONTROL,
            (true, false, true) => HOVER,
        };
        let border = match (enabled, on) {
            (false, _) => BORDER_SUBTLE,
            (true, true) => BORDER_GOLD_SOFT,
            (true, false) => BORDER_CONTROL,
        };
        let r = track.h / 2.0;
        out.push(Command::RoundFill { rect: track, color, radius: r });
        out.push(Command::RoundStroke { rect: track, color: border, radius: r, thickness: 1.0 });
        let inset = (SWITCH_H - KNOB) / 2.0;
        let knob_x = if on { track.x + track.w - inset - KNOB } else { track.x + inset };
        let knob_color = if enabled { TEXT } else { DISABLED_KNOB };
        out.push(Command::RoundFill { rect: Rect { x: knob_x, y: track.y + inset, w: KNOB, h: KNOB }, color: knob_color, radius: KNOB / 2.0 });
    }

    #[expect(clippy::too_many_arguments, reason = "draws a dropdown box with its current face and hover/enabled styling")]
    fn draw_dropdown_box(
        &self,
        out: &mut Vec<Command>,
        measure: &dyn Measure,
        r: Rect,
        id: &'static str,
        options: &[String],
        selected: usize,
        enabled: bool,
        hovered: bool,
    ) {
        let is_open = self.open == Some(id);
        let (border, fill) = match (enabled, hovered || is_open) {
            (false, _) => (BORDER_SUBTLE, DISABLED_CONTROL),
            (true, true) => (BORDER_GOLD_SOFT, HOVER),
            (true, false) => (BORDER_CONTROL, CONTROL),
        };
        out.push(Command::RoundFill { rect: r, color: fill, radius: RADIUS_CTRL });
        out.push(Command::RoundStroke { rect: r, color: border, radius: RADIUS_CTRL, thickness: 1.0 });
        let text_color = if enabled { TEXT } else { DISABLED_TEXT };
        let label = options.get(selected).map_or("", String::as_str);
        let face = self.option_faces.get(id).and_then(|f| f.get(selected)).and_then(Clone::clone);
        let y = r.y + ((r.h - TEXT_SIZE) / 2.0).round() - 2.0;
        let caret_color = if enabled { MUTED_TEXT } else { DISABLED_TEXT };
        if let Some(face) = face {
            let shown = elide_with(measure, Some(&face), label, TEXT_SIZE, (r.w - 30.0).max(16.0));
            out.push(Command::Text { x: r.x + 10.0, y, text: shown, size: TEXT_SIZE, color: text_color, face: Some(face), outline: false });
        } else {
            let shown = elide(measure, label, (r.w - 30.0).max(16.0));
            out.push(Command::text(r.x + 10.0, y, shown, TEXT_SIZE, text_color));
        }
        out.push(Command::text(r.x + r.w - 18.0, y, "▾".into(), TEXT_SIZE, caret_color));
    }

    #[expect(clippy::too_many_arguments, reason = "draws a segmented choice bar with hover and slide highlight state")]
    fn draw_choice_bar(
        &self,
        out: &mut Vec<Command>,
        measure: &dyn Measure,
        r: Rect,
        id: &'static str,
        options: &[String],
        selected: usize,
        enabled: bool,
        allow_hover: bool,
    ) {
        let hover = |rect: &Rect| allow_hover && rect.contains(self.mouse.0, self.mouse.1);
        out.push(Command::RoundFill { rect: r, color: if enabled { RECESSED } else { DISABLED_CONTROL }, radius: RADIUS_CTRL });
        out.push(Command::RoundStroke { rect: r, color: if enabled { BORDER_CONTROL } else { BORDER_SUBTLE }, radius: RADIUS_CTRL, thickness: 1.0 });
        let segments = Self::segments(r, options.len());
        if enabled {
            for (n, segment) in segments.iter().enumerate() {
                if n != selected && hover(segment) {
                    out.push(Command::RoundFill { rect: *segment, color: HOVER, radius: RADIUS_INNER });
                }
            }
        }
        let highlight = self.highlight(id, &segments, selected);
        let highlight_color = if !enabled {
            DISABLED_ON
        } else if hover(&highlight) {
            ON_HOVER
        } else {
            ON
        };
        out.push(Command::RoundFill { rect: highlight, color: highlight_color, radius: RADIUS_INNER });
        for (n, (option, segment)) in options.iter().zip(&segments).enumerate() {
            let text_color = if !enabled {
                DISABLED_TEXT
            } else if n == selected {
                TEXT
            } else {
                MUTED_TEXT
            };
            let text = elide(measure, option, (segment.w - 10.0).max(12.0));
            let x = segment.x + ((segment.w - measure.width(&text, TEXT_SIZE)) / 2.0).round();
            out.push(Command::text(x, segment.y + ((segment.h - TEXT_SIZE) / 2.0).round() - 2.0, text, TEXT_SIZE, text_color));
        }
    }

    #[expect(clippy::too_many_arguments, reason = "draws a slider control with track, active fill, circular thumb, and formatted value")]
    fn draw_slider(
        &self,
        out: &mut Vec<Command>,
        measure: &dyn Measure,
        r: Rect,
        id: &'static str,
        min: u32,
        max: u32,
        value: u32,
        format: fn(u32) -> String,
        enabled: bool,
        allow_hover: bool,
    ) {
        let hovered = allow_hover && r.contains(self.mouse.0, self.mouse.1);
        let track = Self::slider_track(r);
        let active = enabled && (self.sliding.is_some_and(|(s, ..)| s == id) || hovered);
        let track_color = if !enabled {
            DISABLED_CONTROL
        } else if active {
            HOVER
        } else {
            CONTROL
        };
        out.push(Command::RoundFill { rect: track, color: track_color, radius: SLIDER_TRACK_H / 2.0 });
        let span = (track.w - SLIDER_THUMB_W).max(0.0);
        let t = if max > min { (value.clamp(min, max) - min) as f32 / (max - min) as f32 } else { 0.0 };
        let thumb_x = (track.x + t * span).round();
        let fill_w = (thumb_x + SLIDER_THUMB_W / 2.0 - track.x).round().max(0.0);
        if fill_w > 0.0 {
            let fill_color = if !enabled {
                DISABLED_ON
            } else if active {
                ON_HOVER
            } else {
                ON
            };
            out.push(Command::RoundFill { rect: Rect { w: fill_w, ..track }, color: fill_color, radius: SLIDER_TRACK_H / 2.0 });
        }
        let thumb = Rect { x: thumb_x, y: r.y + ((r.h - SLIDER_THUMB_H) / 2.0).round(), w: SLIDER_THUMB_W, h: SLIDER_THUMB_H };
        out.push(Command::RoundFill { rect: thumb, color: if enabled { TITLE_TEXT } else { DISABLED_KNOB }, radius: SLIDER_THUMB_H / 2.0 });
        let text = format(value);
        let tw = measure.width(&text, TEXT_SIZE);
        let tx = (r.x + r.w - tw).max(track.x + track.w + SLIDER_GAP);
        out.push(Command::text(tx, r.y + ((r.h - TEXT_SIZE) / 2.0).round() - 2.0, text, TEXT_SIZE, if enabled { MUTED_TEXT } else { DISABLED_TEXT }));
    }

    fn draw_button(out: &mut Vec<Command>, measure: &dyn Measure, r: Rect, label: &str, style: ButtonStyle, enabled: bool, hovered: bool) {
        let (border, fill, text_color) = match (enabled, style, hovered) {
            (false, ButtonStyle::Ghost, _) => (0x0000_0000, 0x0000_0000, DISABLED_TEXT),
            (false, _, _) => (BORDER_SUBTLE, DISABLED_CONTROL, DISABLED_TEXT),
            (true, ButtonStyle::Danger, true) => (DANGER_BORDER, DANGER_HOVER, TEXT),
            (true, ButtonStyle::Danger, false) => (DANGER_BORDER, DANGER, TEXT),
            (true, ButtonStyle::Primary, true) => (BORDER_GOLD_SOFT, ON_HOVER, TEXT),
            (true, ButtonStyle::Primary, false) => (BORDER_GOLD_SOFT, ON, TEXT),
            (true, ButtonStyle::Ghost, true) => (0x0000_0000, NAV_HOVER, TEXT),
            (true, ButtonStyle::Ghost, false) => (0x0000_0000, 0x0000_0000, MUTED_TEXT),
            (true, ButtonStyle::Normal, true) => (BORDER_GOLD_SOFT, HOVER, TEXT),
            (true, ButtonStyle::Normal, false) => (BORDER_CONTROL, CONTROL, TEXT),
        };
        if (fill >> 24) > 0 {
            out.push(Command::RoundFill { rect: r, color: fill, radius: RADIUS_CTRL });
        }
        if (border >> 24) > 0 {
            out.push(Command::RoundStroke { rect: r, color: border, radius: RADIUS_CTRL, thickness: 1.0 });
        }
        let shown = elide(measure, label, (r.w - 14.0).max(8.0));
        let x = r.x + ((r.w - measure.width(&shown, TEXT_SIZE)) / 2.0).round();
        out.push(Command::text(x, r.y + ((r.h - TEXT_SIZE) / 2.0).round() - 2.0, shown, TEXT_SIZE, text_color));
    }

    fn draw_slot(&self, out: &mut Vec<Command>, measure: &dyn Measure, slot: Slot, allow_hover: bool) {
        let hover = |r: &Rect| allow_hover && r.contains(self.mouse.0, self.mouse.1);
        match slot {
            Slot::Lines(i, lines, r, style) => {
                let enabled = self.raw_widget(i).enabled();
                let color = if !enabled {
                    DISABLED_TEXT
                } else if style == Style::Heading {
                    TITLE_TEXT
                } else {
                    TEXT
                };
                let x = if style == Style::Item {
                    out.push(Command::text(r.x + 2.0, r.y, "•".into(), TEXT_SIZE, BORDER));
                    r.x + INDENT
                } else {
                    r.x
                };
                for (n, line) in lines.into_iter().enumerate() {
                    out.push(Command::text(x, r.y + n as f32 * style.line(), line, style.size(), color));
                }
            }
            Slot::Control(i, r) if matches!(self.widget(i), Widget::PageHeader(_)) => {
                let Widget::PageHeader(title) = self.widget(i) else { return };
                out.push(Command::text(r.x, r.y, title.clone(), PAGE_HEADER_SIZE, TEXT));
                out.push(Command::Fill { rect: Rect { x: r.x, y: r.y + r.h - 1.0, w: r.w, h: 1.0 }, color: BORDER_SUBTLE });
            }
            Slot::Control(i, r) if matches!(self.widget(i), Widget::Divider) => {
                let _ = i;
                out.push(Command::Fill { rect: r, color: BORDER_SUBTLE });
            }
            Slot::Control(i, r) if matches!(self.widget(i), Widget::AboutHero { .. }) => {
                let Widget::AboutHero { name, version, link_label, .. } = self.widget(i) else { return };
                out.push(Command::RoundFill { rect: r, color: SURFACE, radius: RADIUS_CARD });
                out.push(Command::RoundStroke { rect: r, color: BORDER_SUBTLE, radius: RADIUS_CARD, thickness: 1.0 });
                let tx = r.x + HERO_PAD_X;
                let ty = r.y + HERO_PAD_Y;
                out.push(Command::text(tx, ty, name.clone(), HEADING_SIZE, TITLE_TEXT));
                let name_w = measure.width(name, HEADING_SIZE);
                let pill_w = measure.width(version, STATUS_SIZE) + 16.0;
                let pill_h = 22.0;
                let pill = Rect { x: tx + name_w + 10.0, y: ty + 2.0, w: pill_w, h: pill_h };
                out.push(Command::RoundFill { rect: pill, color: 0x28c9a14a, radius: pill_h / 2.0 });
                out.push(Command::RoundStroke { rect: pill, color: 0x60c9a14a, radius: pill_h / 2.0, thickness: 1.0 });
                out.push(Command::text(pill.x + 8.0, pill.y + ((pill_h - STATUS_SIZE) / 2.0).round() - 1.0, version.clone(), STATUS_SIZE, TITLE_TEXT));
                let link_y = ty + HEADING_LINE + 6.0;
                let link_rect = Rect { x: tx, y: link_y, w: r.w - 2.0 * HERO_PAD_X, h: (r.y + r.h - HERO_PAD_Y - link_y).max(STATUS_LINE) };
                let link_color = if hover(&link_rect) { LINK_HOVER } else { LINK };
                for (n, line) in measure.wrap(link_label, STATUS_SIZE, link_rect.w).into_iter().enumerate() {
                    let ly = link_y + n as f32 * STATUS_LINE;
                    let lw = measure.width(&line, STATUS_SIZE).min(link_rect.w);
                    out.push(Command::text(tx, ly, line, STATUS_SIZE, link_color));
                    out.push(Command::Fill { rect: Rect { x: tx, y: ly + STATUS_SIZE + 2.0, w: lw, h: 1.0 }, color: link_color });
                }
            }
            Slot::Control(i, r) if matches!(self.widget(i), Widget::Toggle { .. }) => {
                let enabled = self.raw_widget(i).enabled();
                let Widget::Toggle { label, on, .. } = self.widget(i) else { return };
                let text_color = if enabled { TEXT } else { DISABLED_TEXT };
                let lines = measure.wrap(label, TEXT_SIZE, self.toggle_text());
                let top = r.y + ((r.h - lines.len().max(1) as f32 * LINE) / 2.0).round();
                for (n, line) in lines.into_iter().enumerate() {
                    out.push(Command::text(r.x, top + 2.0 + n as f32 * LINE, line, TEXT_SIZE, text_color));
                }
                Self::draw_toggle_switch(out, Self::switch(r), *on, enabled, hover(&r));
            }
            Slot::Control(i, r) if matches!(self.widget(i), Widget::Labeled { .. }) => {
                let row_enabled = self.raw_widget(i).enabled();
                let Widget::Labeled { label, status, control } = self.widget(i) else { return };
                let enabled = row_enabled && control.enabled();
                let layout = Self::labeled_rects(measure, r, label, status.as_deref(), control);
                let label_color = if enabled { TEXT } else { DISABLED_TEXT };
                for (n, line) in layout.label_lines.iter().enumerate() {
                    out.push(Command::text(r.x, layout.left_y + n as f32 * LINE, line.clone(), TEXT_SIZE, label_color));
                }
                if !layout.status_lines.is_empty() {
                    let sy = layout.left_y + layout.label_lines.len().max(1) as f32 * LINE + 2.0;
                    let status_color = if enabled { MUTED_TEXT } else { DISABLED_TEXT };
                    for (n, line) in layout.status_lines.iter().enumerate() {
                        out.push(Command::text(r.x, sy + n as f32 * STATUS_LINE, line.clone(), STATUS_SIZE, status_color));
                    }
                }
                let cr = layout.control;
                match control.kind() {
                    Widget::Dropdown { id, options, selected } => {
                        self.draw_dropdown_box(out, measure, cr, id, options, *selected, enabled, hover(&cr));
                    }
                    Widget::Choice { id, options, selected, .. } => {
                        self.draw_choice_bar(out, measure, cr, id, options, *selected, enabled, allow_hover);
                    }
                    Widget::Slider { id, min, max, value, format } => {
                        self.draw_slider(out, measure, cr, id, *min, *max, *value, *format, enabled, allow_hover);
                    }
                    Widget::Button { label, style, .. } => {
                        Self::draw_button(out, measure, cr, label, *style, enabled, hover(&cr));
                    }
                    _ => {}
                }
            }
            Slot::Control(i, r) if matches!(self.widget(i), Widget::FontGroup { .. }) => {
                let enabled = self.raw_widget(i).enabled();
                let Widget::FontGroup {
                    role_label,
                    family_id,
                    families,
                    family_selected,
                    weight_id,
                    weights,
                    weight_selected,
                    preview_text,
                    preview_face,
                    preview_size,
                    preview_outline,
                    outline_toggle,
                    ..
                } = self.widget(i)
                else {
                    return;
                };
                out.push(Command::RoundFill { rect: r, color: SURFACE, radius: RADIUS_CARD });
                out.push(Command::RoundStroke { rect: r, color: BORDER_SUBTLE, radius: RADIUS_CARD, thickness: 1.0 });
                let sub = Self::font_group_rects(r, outline_toggle.is_some());
                let role_shown = elide(measure, role_label, sub.role.w - 4.0);
                let role_color = if enabled { TITLE_TEXT } else { DISABLED_TEXT };
                out.push(Command::text(sub.role.x, sub.role.y + ((sub.role.h - TEXT_SIZE) / 2.0).round() - 2.0, role_shown, TEXT_SIZE, role_color));
                self.draw_dropdown_box(out, measure, sub.family, family_id, families, *family_selected, enabled, hover(&sub.family));
                self.draw_dropdown_box(out, measure, sub.weight, weight_id, weights, *weight_selected, enabled, hover(&sub.weight));
                out.push(Command::RoundFill { rect: sub.preview, color: PREVIEW_BG, radius: RADIUS_INNER });
                let shown = elide_with(measure, Some(preview_face), preview_text, *preview_size, (sub.preview.w - 20.0).max(16.0));
                let ty = sub.preview.y + ((sub.preview.h - *preview_size) / 2.0).round() - 2.0;
                out.push(Command::Text {
                    x: sub.preview.x + 10.0,
                    y: ty,
                    text: shown,
                    size: *preview_size,
                    color: if enabled { TEXT } else { DISABLED_TEXT },
                    face: Some(preview_face.clone()),
                    outline: *preview_outline,
                });
                if let (Some(or), Some((_, toggle_label, on))) = (sub.outline, outline_toggle) {
                    out.push(Command::Fill { rect: Rect { x: or.x, y: or.y - 4.0, w: or.w, h: 1.0 }, color: BORDER_SUBTLE });
                    let label_shown = elide(measure, toggle_label, (or.w - SWITCH_W - 12.0).max(40.0));
                    out.push(Command::text(
                        or.x,
                        or.y + ((or.h - TEXT_SIZE) / 2.0).round() - 2.0,
                        label_shown,
                        TEXT_SIZE,
                        if enabled { TEXT } else { DISABLED_TEXT },
                    ));
                    Self::draw_toggle_switch(out, Self::switch(or), *on, enabled, hover(&or));
                }
            }
            Slot::Control(i, r) if matches!(self.widget(i), Widget::Choice { .. }) => {
                let enabled = self.raw_widget(i).enabled();
                let Widget::Choice { id, options, selected, .. } = self.widget(i) else { return };
                self.draw_choice_bar(out, measure, r, id, options, *selected, enabled, allow_hover);
            }
            Slot::Control(i, r) if matches!(self.widget(i), Widget::Slider { .. }) => {
                let enabled = self.raw_widget(i).enabled();
                let Widget::Slider { id, min, max, value, format } = self.widget(i) else { return };
                self.draw_slider(out, measure, r, id, *min, *max, *value, *format, enabled, allow_hover);
            }
            Slot::Control(i, r) if matches!(self.widget(i), Widget::ButtonRow(_)) => {
                let row_enabled = self.raw_widget(i).enabled();
                let Widget::ButtonRow(buttons) = self.widget(i) else { return };
                let rects = Self::row_button_rects(measure, r, buttons);
                for (button, br) in buttons.iter().zip(rects) {
                    let enabled = row_enabled && button.enabled;
                    Self::draw_button(out, measure, br, &button.label, button.style, enabled, hover(&br));
                }
            }
            Slot::Control(i, r) => {
                let enabled = self.raw_widget(i).enabled();
                match self.widget(i) {
                    Widget::Button { label, style, .. } => {
                        Self::draw_button(out, measure, r, label, *style, enabled, hover(&r));
                    }
                    Widget::Dropdown { id, options, selected } => {
                        self.draw_dropdown_box(out, measure, r, id, options, *selected, enabled, hover(&r));
                    }
                    Widget::Label(_)
                    | Widget::Heading(_)
                    | Widget::PageHeader(_)
                    | Widget::Divider
                    | Widget::Item(_)
                    | Widget::AboutHero { .. }
                    | Widget::Gap
                    | Widget::ButtonRow(_)
                    | Widget::Toggle { .. }
                    | Widget::Choice { .. }
                    | Widget::Slider { .. }
                    | Widget::Labeled { .. }
                    | Widget::FontGroup { .. }
                    | Widget::Managed { .. } => {}
                }
            }
        }
    }

    pub fn draw(&self, measure: &dyn Measure) -> Vec<Command> {
        let bounds = self.bounds(measure);
        let hover = |r: &Rect| r.contains(self.mouse.0, self.mouse.1);
        let mut out = vec![
            Command::RoundFill { rect: bounds, color: BACKGROUND, radius: RADIUS_WIN },
            // Clean integrated title bar: subtle vertical gold mark + hairline rule.
            Command::Fill { rect: Rect { x: bounds.x + 1.0, y: bounds.y + TITLE - 1.0, w: bounds.w - 2.0, h: 1.0 }, color: BORDER_SUBTLE },
            Command::RoundFill { rect: Rect { x: bounds.x + 18.0, y: bounds.y + ((TITLE - 14.0) / 2.0).round(), w: 3.0, h: 14.0 }, color: BORDER, radius: 1.5 },
            Command::text(bounds.x + 28.0, bounds.y + ((TITLE - TEXT_SIZE) / 2.0).round() - 2.0, self.title.clone(), TEXT_SIZE, TITLE_TEXT),
        ];

        // Full-bleed left sidebar pane with inset rounded nav pills.
        if let (Some(side), Some(area)) = (&self.side, self.side_area()) {
            let pane_rect = Rect { x: bounds.x + 1.0, y: bounds.y + TITLE, w: side.width - 1.0, h: (bounds.h - TITLE - 1.0).max(0.0) };
            out.push(Command::Fill { rect: pane_rect, color: SIDEBAR_BG });
            out.push(Command::Fill { rect: Rect { x: bounds.x + side.width, y: bounds.y + TITLE, w: 1.0, h: pane_rect.h }, color: BORDER_SUBTLE });
            for (index, row) in self.side_rows() {
                if index == side.selected {
                    out.push(Command::RoundFill { rect: row, color: NAV_ACTIVE, radius: RADIUS_CTRL });
                    out.push(Command::RoundFill {
                        rect: Rect { x: row.x, y: row.y + 7.0, w: 3.0, h: (row.h - 14.0).max(8.0) },
                        color: TITLE_TEXT,
                        radius: 1.5,
                    });
                } else if hover(&row) {
                    out.push(Command::RoundFill { rect: row, color: NAV_HOVER, radius: RADIUS_CTRL });
                }
                let text = elide(measure, &side.items[index], (row.w - 24.0).max(16.0));
                let color = if index == side.selected {
                    TITLE_TEXT
                } else if hover(&row) {
                    TEXT
                } else {
                    MUTED_TEXT
                };
                out.push(Command::text(row.x + 12.0, row.y + ((row.h - TEXT_SIZE) / 2.0).round() - 2.0, text, TEXT_SIZE, color));
            }
            let (shown, all) = (self.side_shown(), side.items.len());
            if all > shown {
                let thumb = Rect {
                    x: area.x + area.w - SCROLL_BAR,
                    y: area.y + area.h * side.scroll as f32 / all as f32,
                    w: SCROLL_BAR,
                    h: (area.h * shown as f32 / all as f32).max(20.0),
                };
                out.push(Command::RoundFill { rect: thumb, color: BORDER_GOLD_SOFT, radius: SCROLL_BAR / 2.0 });
            }
        }

        // Subtle hover-only close button.
        let close = self.close_box();
        if hover(&close) {
            out.push(Command::RoundFill { rect: close, color: NAV_HOVER, radius: RADIUS_CTRL });
        }
        let close_w = measure.width("×", TEXT_SIZE);
        out.push(Command::text(
            close.x + ((close.w - close_w) / 2.0).round(),
            close.y + ((close.h - TEXT_SIZE) / 2.0).round() - 2.0,
            "×".into(),
            TEXT_SIZE,
            if hover(&close) { TEXT } else { MUTED_TEXT },
        ));

        // Scrollable body area clipped cleanly to its viewport.
        let viewport = self.body_viewport(measure);
        let content_h = self.content_height(measure, &self.body);
        let needs_scroll = content_h > viewport.h + 0.5;
        let clip_rect = Rect { x: viewport.x - 1.0, y: viewport.y - 1.0, w: viewport.w + 2.0, h: viewport.h + 2.0 };
        if needs_scroll {
            out.push(Command::Clip(Some(clip_rect)));
        }
        let body_hover = !needs_scroll || viewport.contains(self.mouse.0, self.mouse.1);
        for slot in self.body_slots(measure) {
            self.draw_slot(&mut out, measure, slot, body_hover);
        }
        if needs_scroll {
            out.push(Command::Clip(None));
            let scroll = self.clamped_body_scroll(measure);
            let track = Rect { x: viewport.x + viewport.w + 8.0, y: viewport.y, w: SCROLL_BAR, h: viewport.h };
            out.push(Command::RoundFill { rect: track, color: RECESSED, radius: SCROLL_BAR / 2.0 });
            let thumb_h = (viewport.h * (viewport.h / content_h)).max(24.0).min(viewport.h);
            let max_scroll = (content_h - viewport.h).max(1.0);
            let thumb_y = viewport.y + (viewport.h - thumb_h) * (scroll / max_scroll);
            out.push(Command::RoundFill { rect: Rect { y: thumb_y.round(), h: thumb_h.round(), ..track }, color: BORDER_GOLD_SOFT, radius: SCROLL_BAR / 2.0 });
        }

        // Fixed footer band below the body, with a hairline top divider.
        let footer_slots = self.footer_slots(measure);
        if !footer_slots.is_empty() {
            let footer_h = self.footer_height(measure);
            let footer_y = bounds.y + bounds.h - footer_h;
            let band_x = self.pane_x() + 1.0;
            let band_w = (self.pane_w() - 2.0).max(0.0);
            out.push(Command::Fill { rect: Rect { x: band_x, y: footer_y, w: band_w, h: (footer_h - 1.0).max(0.0) }, color: FOOTER_BG });
            out.push(Command::Fill { rect: Rect { x: band_x, y: footer_y, w: band_w, h: 1.0 }, color: BORDER_SUBTLE });
            for slot in footer_slots {
                self.draw_slot(&mut out, measure, slot, true);
            }
        }

        // Outer window rounded gold hairline frame (drawn over panes so edges stay crisp).
        out.push(Command::RoundStroke { rect: bounds, color: BORDER_WINDOW, radius: RADIUS_WIN, thickness: 1.0 });

        // Open dropdown renders above everything and unclipped.
        if let Some((list, open_id)) = self.open_list(measure) {
            out.push(Command::RoundFill { rect: list, color: LIST, radius: RADIUS_CTRL });
            out.push(Command::RoundStroke { rect: list, color: BORDER_GOLD_SOFT, radius: RADIUS_CTRL, thickness: 1.0 });
            if let Some((_, options, selected)) = self.find_dropdown(measure, open_id) {
                let faces = self.option_faces.get(open_id);
                let rows = ((list.h - 2.0 * DROPDOWN_PAD) / DROPDOWN_ROW).round() as usize;
                let has_bar = options.len() > rows;
                let inner_w = if has_bar { list.w - SCROLL_BAR - 12.0 } else { list.w - 8.0 };
                for (n, (index, option)) in options.iter().enumerate().skip(self.scroll).take(rows).enumerate() {
                    let row = Rect { x: list.x + 4.0, y: list.y + DROPDOWN_PAD + n as f32 * DROPDOWN_ROW, w: inner_w.max(16.0), h: DROPDOWN_ROW };
                    if hover(&row) || index == selected {
                        out.push(Command::RoundFill { rect: row, color: if hover(&row) { HOVER } else { NAV_ACTIVE }, radius: RADIUS_INNER });
                    }
                    let face = faces.and_then(|f| f.get(index)).and_then(Clone::clone);
                    let shown = elide_with(measure, face.as_ref(), option, TEXT_SIZE, (row.w - 16.0).max(16.0));
                    let color = if index == selected { TITLE_TEXT } else { TEXT };
                    out.push(Command::Text {
                        x: row.x + 8.0,
                        y: row.y + ((DROPDOWN_ROW - TEXT_SIZE) / 2.0).round() - 2.0,
                        text: shown,
                        size: TEXT_SIZE,
                        color,
                        face,
                        outline: false,
                    });
                }
                if has_bar {
                    let bar_h = (list.h - 2.0 * DROPDOWN_PAD).max(16.0);
                    let thumb_h = (bar_h * rows as f32 / options.len() as f32).max(16.0).min(bar_h);
                    let max_scroll = options.len().saturating_sub(rows).max(1) as f32;
                    let thumb_y = list.y + DROPDOWN_PAD + (bar_h - thumb_h) * (self.scroll as f32 / max_scroll);
                    out.push(Command::RoundFill {
                        rect: Rect { x: list.x + list.w - SCROLL_BAR - 4.0, y: thumb_y.round(), w: SCROLL_BAR, h: thumb_h.round() },
                        color: BORDER_GOLD_SOFT,
                        radius: SCROLL_BAR / 2.0,
                    });
                }
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
            Slot::Lines(_, _, r, _) | Slot::Control(_, r) => r.y + r.h,
        })
        .fold(top, f32::max)
}

/// `text` cut to fit `width`, with an ellipsis where it was cut.
fn elide(measure: &dyn Measure, text: &str, width: f32) -> String {
    elide_with(measure, None, text, TEXT_SIZE, width)
}

fn elide_with(measure: &dyn Measure, face: Option<&Face>, text: &str, size: f32, width: f32) -> String {
    if measure.width_with(face, text, size) <= width {
        return text.to_string();
    }
    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let cut = format!("{}…", chars.iter().collect::<String>());
        if measure.width_with(face, &cut, size) <= width {
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
                Widget::Button { id: "save", label: "Save".into(), style: ButtonStyle::Normal },
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
                Widget::Label("At the fastest speed".into()),
                Widget::Choice { id: "pace", options: vec!["Smooth".into(), "Faster".into()], tooltips: Vec::new(), selected: 0 },
            ],
            300.0,
        );
        w.center(&Mono, 1000.0, 800.0);
        w
    }

    /// The filled rectangles of a colour (square or rounded).
    fn fills(w: &Window, color: Color) -> Vec<Rect> {
        w.draw(&Mono)
            .into_iter()
            .filter_map(|c| match c {
                Command::Fill { rect, color: c } | Command::RoundFill { rect, color: c, .. } if c == color => Some(rect),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_choice_bar_chooses_where_it_is_clicked_and_its_highlight_slides_there() {
        let mut w = speed();
        let (fx, fy) = spot(&w, "Faster");
        let (sx, _) = spot(&w, "Smooth");
        let before = fills(&w, ON);
        assert_eq!(before.len(), 1);
        assert!(before[0].x <= sx && sx < before[0].x + before[0].w);
        assert_eq!(w.mouse_down(&Mono, sx, fy + 4.0), None);
        assert_eq!(w.mouse_down(&Mono, fx, fy + 4.0), Some(Action::Chose("pace", 1)));
        assert_eq!(w.selected("pace"), Some(1));
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
    fn a_choice_bar_shows_an_option_tooltip_only_over_that_option() {
        let mut w = Window::new(
            "Display",
            vec![Widget::Choice {
                id: "fps",
                options: vec!["60".into(), "∞".into()],
                tooltips: vec![Default::default(), "No limit warning".into()],
                selected: 0,
            }],
            300.0,
        );
        w.center(&Mono, 1000.0, 800.0);
        let (fx, fy) = spot(&w, "60");
        let (ux, uy) = spot(&w, "∞");
        assert_eq!(w.tooltip_at(&Mono, fx, fy), Some(Default::default()));
        assert_eq!(w.tooltip_at(&Mono, ux, uy), Some("No limit warning".into()));
    }

    #[test]
    fn self_drawn_tooltips_replace_clear_and_follow_only_the_visible_hit() {
        let mut w = Window::new(
            "Tools",
            vec![
                Widget::Button { id: "run", label: "Run".into(), style: ButtonStyle::Normal }.with_tooltip("Initial".into()),
                Widget::Label("Plain label".into()),
            ],
            300.0,
        );
        w.center(&Mono, 1000.0, 800.0);
        let (x, y) = spot(&w, "Run");
        assert_eq!(w.tooltip_at(&Mono, x, y), Some("Initial".into()));
        let Widget::Managed { enabled, .. } = &mut w.body[0] else { panic!() };
        *enabled = false;
        assert_eq!(w.mouse_down(&Mono, x, y), None);
        assert_eq!(w.tooltip_at(&Mono, x, y), Some("Initial".into()), "disabled still has a tooltip");
        w.body[0].set_tooltip("Updated".into());
        assert_eq!(w.tooltip_at(&Mono, x, y), Some("Updated".into()), "the pointer need not move");
        w.body[0].set_tooltip(Default::default());
        assert_eq!(w.tooltip_at(&Mono, x, y), Some(Default::default()));
        w.body[0].set_tooltip("Retained".into());
        let Widget::Managed { visible, .. } = &mut w.body[0] else { panic!() };
        *visible = false;
        assert!(spot_of(&w, "Run").is_none());
        assert_eq!(w.body[0].tooltip(), "Retained".into());
        let (x, y) = spot(&w, "Plain label");
        assert_eq!(w.tooltip_at(&Mono, x, y), Some(Default::default()), "another element never inherits the hidden hint");
        assert_eq!(w.tooltip_at(&Mono, 0.0, 0.0), None);
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
        let shown = |w: &Window| w.draw(&Mono).iter().filter(|c| matches!(c, Command::Text { text, .. } if text.starts_with('F'))).count();
        let rows = shown(&w).saturating_sub(1);
        assert!((3..40).contains(&rows), "{rows} rows on a 400-pixel screen");
        w.wheel(&Mono, -5);
        let (fx, fy) = spot(&w, "F5");
        assert_eq!(w.mouse_down(&Mono, fx, fy), Some(Action::Chose("font", 5)));
    }

    #[test]
    fn a_tall_window_caps_to_screen_height_scrolls_body_and_clamps_drag() {
        let body: Vec<Widget> = (0..25).map(|n| Widget::Button { id: "b", label: format!("Row {n}"), style: ButtonStyle::Normal }).collect();
        let footer = vec![Widget::ButtonRow(vec![
            RowButton { id: "reset", label: "Reset".into(), style: ButtonStyle::Ghost, enabled: true, align_right: false },
            RowButton { id: "cancel", label: "Cancel".into(), style: ButtonStyle::Normal, enabled: true, align_right: true },
            RowButton { id: "save", label: "Save".into(), style: ButtonStyle::Primary, enabled: true, align_right: true },
        ])];
        let mut w = Window::new("Tall", body, 420.0).with_footer(footer);
        w.center(&Mono, 800.0, 500.0);
        assert!(w.height(&Mono) <= 500.0 * MAX_WINDOW_SHARE + 1.0);
        assert!(w.y >= 0.0 && w.y + w.height(&Mono) <= 500.0);
        let commands = w.draw(&Mono);
        assert!(commands.iter().any(|c| matches!(c, Command::Clip(Some(_)))));
        assert!(commands.iter().any(|c| matches!(c, Command::Clip(None))));
        let (sx, sy) = spot(&w, "Save");
        let (rx, _) = spot(&w, "Reset");
        assert!(rx < sx, "left-aligned Reset stands left of right-aligned Save");
        assert_eq!(w.mouse_down(&Mono, sx, sy), Some(Action::Pressed("save")));
        let before_y = spot(&w, "Row 0").1;
        w.mouse_move(w.x + 100.0, w.y + 100.0);
        w.wheel(&Mono, -3);
        let after_y = spot(&w, "Row 0").1;
        assert!(after_y < before_y, "wheel scrolls body content");
        let (tx, ty) = (w.x + 20.0, w.y + 10.0);
        w.mouse_down(&Mono, tx, ty);
        w.mouse_move(-500.0, -500.0);
        assert_eq!((w.x, w.y), (0.0, 0.0));
        w.mouse_move(5000.0, 5000.0);
        assert!(w.x <= 800.0 - w.width + 0.5);
        assert!(w.y <= 500.0 - TITLE + 0.5);
    }

    #[test]
    fn a_toggle_switches_where_its_row_is_clicked() {
        let mut w = Window::new("Settings", vec![Widget::Toggle { id: "fast", label: "Fast".into(), on: true }], 300.0);
        w.center(&Mono, 1000.0, 800.0);
        assert_eq!(w.is_on("fast"), Some(true));
        assert_eq!(w.is_on("none"), None);
        let knob = |w: &Window| fills(w, TEXT).into_iter().next_back().unwrap();
        let on = knob(&w);
        let (x, y) = spot(&w, "Fast");
        assert_eq!(w.mouse_down(&Mono, x, y), Some(Action::Switched("fast", false)));
        assert_eq!(w.is_on("fast"), Some(false));
        assert!(knob(&w).x < on.x);
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
        let side = Side::new("mods", vec!["First".into(), "Second".into(), "x".repeat(40)], 120.0);
        let mut w = Window::new("Mods", vec![Widget::Label("About the first".into())], 440.0).with_side(side);
        w.center(&Mono, 1000.0, 800.0);
        let (lx, _) = spot(&w, "About");
        assert!(lx >= w.x + 120.0 + CONTENT_PAD_X, "the body begins right of the sidebar");
        let (x, y) = spot(&w, "Second");
        assert_eq!(w.mouse_down(&Mono, x, y), Some(Action::Chose("mods", 1)));
        assert_eq!(w.side.as_ref().map(|s| s.selected), Some(1));
        let cut = w.draw(&Mono).into_iter().find_map(|c| match c {
            Command::Text { text, .. } if text.starts_with("xx") => Some(text),
            _ => None,
        });
        assert!(cut.as_deref().is_some_and(|t| t.ends_with('…') && Mono.width(t, TEXT_SIZE) <= 120.0 - 2.0 * SIDE_INSET - 24.0), "{cut:?}");
        let rows = Window::new("Mods", vec![], 440.0).with_side(Side::new("mods", (0..10).map(|n| n.to_string()).collect(), 120.0)).height(&Mono);
        assert!(rows >= TITLE + 10.0 * SIDE_ROW);
    }

    #[test]
    fn a_long_side_list_shows_what_fits_and_scrolls() {
        let items: Vec<String> = (0..40).map(|n| format!("M{n}")).collect();
        let mut w = Window::new("Mods", vec![Widget::Label("About".into())], 440.0).with_side(Side::new("mods", items, 120.0));
        w.center(&Mono, 1000.0, 600.0);
        let shown = |w: &Window| w.draw(&Mono).iter().filter(|c| matches!(c, Command::Text { text, .. } if text.starts_with('M'))).count();
        let rows = shown(&w);
        assert!((3..40).contains(&rows), "{rows} rows on a 600-pixel screen");
        assert!(w.height(&Mono) <= 600.0 * SIDE_SHARE + PAD + 20.0);
        let (x, y) = spot(&w, "M0");
        w.mouse_move(x, y);
        w.wheel(&Mono, -5);
        let (x5, y5) = spot(&w, "M5");
        assert_eq!(w.mouse_down(&Mono, x5, y5), Some(Action::Chose("mods", 5)));
        w.mouse_move(w.x + 300.0, y);
        w.wheel(&Mono, 5);
        assert!(spot_of(&w, "M5").is_some());
        w.mouse_move(x, y);
        w.wheel(&Mono, -100);
        assert!(spot_of(&w, "M39").is_some() && spot_of(&w, "M0").is_none());
    }

    #[test]
    fn a_side_list_turns_pages_that_keep_their_choices_while_the_window_keeps_its_size() {
        let side = Side::new("pages", vec!["Looks".into(), "Speed".into()], 120.0);
        let pages = vec![
            vec![
                Widget::Label("Typeface".into()),
                Widget::Dropdown { id: "font", options: vec!["A".into(), "B".into()], selected: 0 },
                Widget::Label("Restart".into()),
            ],
            vec![Widget::Toggle { id: "fast", label: "Fast".into(), on: false }],
        ];
        let footer = vec![Widget::Button { id: "save", label: "Save".into(), style: ButtonStyle::Normal }];
        let mut w = Window::new("Settings", Vec::new(), 460.0).with_side(side).with_pages(pages).with_footer(footer);
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
        assert_eq!((w.height(&Mono), spot(&w, "Save")), (height, save));
        let (fx, fy) = spot(&w, "Fast");
        assert_eq!(w.mouse_down(&Mono, fx, fy), Some(Action::Switched("fast", true)));
        assert_eq!((w.selected("font"), w.is_on("fast")), (Some(1), Some(true)));
        let (lx, ly) = spot(&w, "Looks");
        assert_eq!(w.mouse_down(&Mono, lx, ly), Some(Action::Chose("pages", 0)));
        assert!(shows(&w, "B"));
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

    #[test]
    fn a_slider_clicks_drags_clamps_and_steps_with_the_wheel() {
        let mut w = Window::new("Display", vec![Widget::Slider { id: "smooth", min: 0, max: 100, value: 50, format: |v| format!("{v}%") }], 300.0);
        w.center(&Mono, 1000.0, 800.0);
        assert_eq!(w.value("smooth"), Some(50));
        assert_eq!(w.value("none"), None);
        assert!(shows(&w, "50%"));
        let track = fills(&w, CONTROL).into_iter().find(|r| r.h == SLIDER_TRACK_H).unwrap();
        let y = track.y + 2.0;
        assert_eq!(w.mouse_down(&Mono, track.x + track.w - 1.0, y), Some(Action::Slid("smooth", 100)));
        assert_eq!(w.value("smooth"), Some(100));
        assert!(shows(&w, "100%"));
        assert_eq!(w.mouse_move(track.x - 50.0, y + 40.0), Some(Action::Slid("smooth", 0)));
        assert_eq!(w.value("smooth"), Some(0));
        w.mouse_up();
        assert_eq!(w.mouse_move(track.x + track.w / 2.0, y), None);
        assert_eq!(w.value("smooth"), Some(0));
        assert_eq!(w.wheel(&Mono, 3), Some(Action::Slid("smooth", 3)));
        assert_eq!(w.value("smooth"), Some(3));
        assert_eq!(w.wheel(&Mono, -10), Some(Action::Slid("smooth", 0)));
        assert_eq!(w.value("smooth"), Some(0));
    }

    #[test]
    fn font_group_opens_family_weight_and_toggles_outline() {
        let mut w = Window::new(
            "Fonts",
            vec![Widget::FontGroup {
                role_label: "Map".into(),
                family_id: "map",
                families: vec!["Georgia".into(), "KaiTi".into()],
                family_selected: 0,
                weight_id: "map_weight",
                weights: vec!["Regular".into(), "Bold".into()],
                weight_selected: 0,
                preview_id: "preview_map",
                preview_text: "MING".into(),
                preview_face: Face::regular("Georgia"),
                preview_size: 18.0,
                preview_outline: true,
                outline_toggle: Some(("map_outline", "Outline".into(), true)),
            }],
            560.0,
        );
        w.center(&Mono, 1000.0, 800.0);
        let (fx, fy) = spot(&w, "Georgia");
        assert_eq!(w.mouse_down(&Mono, fx, fy), None);
        let (kx, ky) = spot(&w, "KaiTi");
        assert_eq!(w.mouse_down(&Mono, kx, ky), Some(Action::Chose("map", 1)));
        assert_eq!(w.selected("map"), Some(1));
        let (wx, wy) = spot(&w, "Regular");
        assert_eq!(w.mouse_down(&Mono, wx, wy), None);
        let (bx, by) = spot(&w, "Bold");
        assert_eq!(w.mouse_down(&Mono, bx, by), Some(Action::Chose("map_weight", 1)));
        assert_eq!(w.selected("map_weight"), Some(1));
        let (ox, oy) = spot(&w, "Outline");
        assert_eq!(w.mouse_down(&Mono, ox, oy), Some(Action::Switched("map_outline", false)));
        assert_eq!(w.is_on("map_outline"), Some(false));
    }

    #[test]
    fn disabled_controls_render_muted_and_ignore_hover_and_clicks() {
        let mut w = Window::new(
            "Settings",
            vec![
                Widget::Label("Locked".into()).with_enabled(false),
                Widget::Toggle { id: "t", label: "Opt".into(), on: true }.with_enabled(false),
                Widget::Dropdown { id: "d", options: vec!["One".into(), "Two".into()], selected: 0 }.with_enabled(false),
                Widget::Choice { id: "c", options: vec!["A".into(), "B".into()], tooltips: Vec::new(), selected: 0 }.with_enabled(false),
                Widget::Slider { id: "s", min: 0, max: 100, value: 50, format: |v| format!("{v}%") }.with_enabled(false),
            ],
            300.0,
        );
        w.center(&Mono, 1000.0, 800.0);

        for needle in ["Opt", "One", "B", "50%"] {
            let (x, y) = spot(&w, needle);
            w.mouse_move(x, y);
            assert_eq!(w.mouse_down(&Mono, x, y), None, "{needle} must not respond when disabled");
        }
        assert_eq!(w.wheel(&Mono, 3), None);
        assert_eq!(w.value("s"), Some(50));
        let commands = w.draw(&Mono);
        assert!(commands.iter().any(|c| matches!(c, Command::Text { text, color, .. } if text == "Locked" && *color == DISABLED_TEXT)));
        assert!(commands.iter().any(|c| matches!(c, Command::Text { text, color, .. } if text == "50%" && *color == DISABLED_TEXT)));
        assert!(!fills(&w, DISABLED_ON).is_empty());
        assert!(!fills(&w, DISABLED_CONTROL).is_empty());
        assert!(!fills(&w, DISABLED_KNOB).is_empty());
        assert!(fills(&w, HOVER).is_empty() && fills(&w, ON_HOVER).is_empty() && fills(&w, ON).is_empty());
    }
}
