//! One tooltip's handoff from the native list to drawing after framework UI.
//! Presentation scope, invalidation and the pending frame meet here. Native
//! observers supply facts; both the game and the tests drive this same process.
use crate::Result;
use std::cell::Cell;

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Display {
    pub owner: usize,
    pub holder: usize,
    pub node: usize,
    pub(super) text: Vec<u8>,
    pub(super) generation: u64,
}

struct Presentation {
    target: Option<usize>,
    self_drawn: bool,
}

pub(super) enum Content {
    Framework(Vec<u8>),
    Managed(usize),
}

/// Shared by presentation on the main thread and the drawing thread. A saved
/// frame is usable only while this exact presentation is still current.
pub(super) struct Handoff {
    generation: u64,
    display: Option<Display>,
    presentation: Option<Presentation>,
}

impl Handoff {
    pub const fn new() -> Self {
        Self { generation: 0, display: None, presentation: None }
    }

    pub fn begin(&mut self) {
        self.presentation = Some(Presentation { target: None, self_drawn: false });
    }

    pub fn hit(&mut self, target: usize) {
        if let Some(presentation) = &mut self.presentation {
            presentation.target = Some(target);
        }
    }

    pub fn presenting(&self) -> bool {
        self.presentation.is_some()
    }

    pub fn content(&mut self, framework: Option<Vec<u8>>) -> Option<Content> {
        let presentation = self.presentation.as_mut()?;
        if let Some(text) = framework {
            presentation.self_drawn = true;
            Some(Content::Framework(text))
        } else {
            presentation.target.map(Content::Managed)
        }
    }

    pub fn end(&mut self, faulted: bool) {
        if self.presentation.take().is_none_or(|presentation| faulted || !presentation.self_drawn) {
            self.clear();
        }
    }

    pub fn showing(&mut self, owner: usize, holder: usize, node: usize, text: &[u8]) {
        if holder == 0 || text.is_empty() {
            self.clear();
            return;
        }
        let unchanged = self.display.as_ref().is_some_and(|old| old.owner == owner && old.holder == holder && old.node == node && old.text == text);
        if !unchanged {
            self.generation = self.generation.wrapping_add(1);
        }
        self.display = Some(Display { owner, holder, node, text: text.to_vec(), generation: self.generation });
    }

    pub fn clear(&mut self) {
        self.display = None;
        self.generation = self.generation.wrapping_add(1);
    }

    pub fn owner_invalidated(&mut self, owner: usize) {
        if self.display.as_ref().is_some_and(|display| display.owner == owner) {
            self.clear();
        }
    }

    pub fn holder_deleted(&mut self, holder: usize) {
        if self.display.as_ref().is_some_and(|display| display.holder == holder) {
            self.clear();
        }
    }

    pub fn current(&self, text: Option<Vec<u8>>) -> Current {
        Current { display: self.display.clone(), text }
    }
}

pub(super) struct Current {
    display: Option<Display>,
    text: Option<Vec<u8>>,
}

impl Current {
    pub fn with_text(mut self, text: impl FnOnce() -> Option<Vec<u8>>) -> Self {
        if self.display.is_some() {
            self.text = text();
        }
        self
    }

    fn display(self) -> Option<Display> {
        self.display.filter(|display| self.text.as_deref() == Some(display.text.as_slice()))
    }
}

pub(super) struct Filter {
    // The native helper borrows this vector's header and storage until its
    // return. The game's own vector is never changed.
    _nodes: Vec<usize>,
    header: [usize; 3],
}

impl Filter {
    pub fn new(nodes: Vec<usize>) -> Self {
        let begin = nodes.as_ptr() as usize;
        let end = begin + nodes.len() * size_of::<usize>();
        Self { _nodes: nodes, header: [begin, end, end] }
    }

    pub fn header(&self) -> usize {
        self.header.as_ptr() as usize
    }
}

/// The internal native-operation seam: live memory and game calls in the game,
/// owned nodes and a fallible scene recorder in the tests.
pub(super) trait Native {
    fn ready(&self, display: &Display) -> Result<bool>;
    fn without_node(&self, list: usize, node: usize) -> Result<Option<Filter>>;
    fn renderer(&self, graphics: usize) -> Result<usize>;
    fn begin(&mut self, renderer: usize) -> Result<bool>;
    fn setup(&mut self, graphics: usize) -> Result<()>;
    fn draw(&mut self, graphics: usize, node: usize) -> Result<()>;
    fn end(&mut self, renderer: usize) -> Result<bool>;
}

struct Pending {
    display: Display,
    graphics: usize,
}

pub(super) struct Frame {
    filters: Vec<Option<Box<Filter>>>,
    pending: Option<Pending>,
}

impl Frame {
    pub const fn new() -> Self {
        Self { filters: Vec::new(), pending: None }
    }

    /// Start an ordinary native helper call, retaining any replacement list
    /// until its matching return. Nested calls keep independent allocations.
    pub fn filter(&mut self, current: impl FnOnce() -> Option<Current>, native: &impl Native, graphics: usize, list: usize) -> Option<usize> {
        self.filters.push(None);
        let display = current()?.display()?;
        if native.ready(&display) != Ok(true) {
            return None;
        }
        let filter = Box::new(native.without_node(list, display.node).ok()??);
        let header = filter.header();
        *self.filters.last_mut().expect("this native draw has a frame") = Some(filter);
        self.pending = Some(Pending { display, graphics });
        Some(header)
    }

    pub fn returned(&mut self) {
        self.filters.pop();
    }

    /// A held or newly starting frame must not inherit a pending draw.
    pub fn discard(&mut self) {
        self.pending = None;
    }

    /// Consume this frame's handoff exactly once, even when it is no longer
    /// current or a native operation fails. The caller holds the presentation
    /// lock, after framework UI and before screenshots or Present.
    pub fn finish(&mut self, current: impl FnOnce() -> Current, native: &mut impl Native, renderer: usize, in_late: &Cell<bool>) -> Result<()> {
        let Some(pending) = self.pending.take() else { return Ok(()) };
        if current().display().as_ref() != Some(&pending.display) {
            return Ok(());
        }
        if !native.ready(&pending.display)? || native.renderer(pending.graphics)? != renderer {
            return Ok(());
        }
        if !native.begin(renderer)? {
            return Err("native tooltip scene did not begin".into());
        }
        // Keep recursion disabled until EndScene has run, including when a
        // fallible setup/draw or a caught Rust panic leaves this scope.
        let _drawing = Drawing::new(in_late);
        let mut scene = Scene { native, renderer, ended: false };
        let drawn = scene.native.setup(pending.graphics).and_then(|_| scene.native.draw(pending.graphics, pending.display.node));
        let ended = scene.end();
        drawn?;
        ended
    }
}

struct Drawing<'a>(&'a Cell<bool>);

impl<'a> Drawing<'a> {
    fn new(active: &'a Cell<bool>) -> Self {
        active.set(true);
        Self(active)
    }
}

impl Drop for Drawing<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

struct Scene<'a, N: Native> {
    native: &'a mut N,
    renderer: usize,
    ended: bool,
}

impl<N: Native> Scene<'_, N> {
    fn end(&mut self) -> Result<()> {
        self.ended = true;
        if !self.native.end(self.renderer)? {
            return Err("native tooltip scene did not end".into());
        }
        Ok(())
    }
}

impl<N: Native> Drop for Scene<'_, N> {
    fn drop(&mut self) {
        if !self.ended {
            // Only an unwind reaches this path. Ordinary errors go through
            // end above so its failure can be reported by the caller.
            if let Err(e) = self.end() {
                log!("late native tooltip scene cleanup: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum At {
        Ready,
        Filter,
        Renderer,
        Begin,
        Setup,
        Draw,
        End,
    }

    struct Recorder<'a> {
        calls: RefCell<Vec<At>>,
        ready: bool,
        contains: bool,
        renderer: usize,
        fail: Option<At>,
        refuse: Option<At>,
        panic: Option<At>,
        drawing: &'a Cell<bool>,
    }

    impl<'a> Recorder<'a> {
        fn new(drawing: &'a Cell<bool>) -> Self {
            Self { calls: RefCell::new(Vec::new()), ready: true, contains: true, renderer: 44, fail: None, refuse: None, panic: None, drawing }
        }

        fn at(&self, at: At) -> Result<()> {
            self.calls.borrow_mut().push(at);
            assert_ne!(self.panic, Some(at), "injected native panic at {at:?}");
            if self.fail == Some(at) {
                return Err(format!("injected native failure at {at:?}"));
            }
            Ok(())
        }

        fn scenes(&self) -> Vec<At> {
            self.calls.borrow().iter().copied().filter(|at| matches!(at, At::Begin | At::Setup | At::Draw | At::End)).collect()
        }
    }

    impl Native for Recorder<'_> {
        fn ready(&self, _: &Display) -> Result<bool> {
            self.at(At::Ready)?;
            Ok(self.ready)
        }

        fn without_node(&self, _: usize, node: usize) -> Result<Option<Filter>> {
            self.at(At::Filter)?;
            assert_eq!(node, 22);
            Ok(self.contains.then(|| Filter::new(vec![11, 33])))
        }

        fn renderer(&self, _: usize) -> Result<usize> {
            self.at(At::Renderer)?;
            Ok(self.renderer)
        }

        fn begin(&mut self, _: usize) -> Result<bool> {
            assert!(!self.drawing.get());
            self.at(At::Begin)?;
            Ok(self.refuse != Some(At::Begin))
        }

        fn setup(&mut self, graphics: usize) -> Result<()> {
            assert!(self.drawing.get());
            assert_eq!(graphics, 7);
            self.at(At::Setup)
        }

        fn draw(&mut self, graphics: usize, node: usize) -> Result<()> {
            assert!(self.drawing.get());
            assert_eq!((graphics, node), (7, 22));
            self.at(At::Draw)
        }

        fn end(&mut self, _: usize) -> Result<bool> {
            assert!(self.drawing.get());
            self.at(At::End)?;
            Ok(self.refuse != Some(At::End))
        }
    }

    fn showing(state: &mut Handoff, text: &[u8]) {
        state.begin();
        assert!(matches!(state.content(Some(text.to_vec())), Some(Content::Framework(_))));
        state.showing(1, 2, 22, text);
        state.end(false);
    }

    fn current(state: &Handoff) -> Current {
        state.current(Some(b"Save".to_vec()))
    }

    fn prepare(state: &Handoff, frame: &mut Frame, native: &impl Native) {
        assert!(frame.filter(|| Some(current(state)), native, 7, 0).is_some());
        frame.returned();
    }

    #[test]
    fn a_ready_node_leaves_ordinary_drawing_and_is_drawn_late_only_once() {
        let mut state = Handoff::new();
        let mut frame = Frame::new();
        let drawing = Cell::new(false);
        let mut native = Recorder::new(&drawing);
        showing(&mut state, b"Save");
        let header = frame.filter(|| Some(current(&state)), &native, 7, 0).unwrap();
        assert_eq!(crate::containers::pointers(&crate::memory::Memory { base: 0 }, header, 10).unwrap(), [11, 33]);
        // An unrelated nested draw must not free the outer replacement list.
        assert!(frame.filter(|| None, &native, 7, 0).is_none());
        frame.returned();
        assert_eq!(crate::containers::pointers(&crate::memory::Memory { base: 0 }, header, 10).unwrap(), [11, 33]);
        frame.returned();
        frame.finish(|| current(&state), &mut native, 44, &drawing).unwrap();
        frame.finish(|| current(&state), &mut native, 44, &drawing).unwrap();
        assert_eq!(native.scenes(), [At::Begin, At::Setup, At::Draw, At::End]);
        assert!(!drawing.get());
    }

    #[test]
    fn unready_absent_unmanaged_or_ineligible_nodes_keep_their_ordinary_draw() {
        for case in 0..6 {
            let mut state = Handoff::new();
            let mut frame = Frame::new();
            let drawing = Cell::new(false);
            let mut native = Recorder::new(&drawing);
            showing(&mut state, b"Save");
            native.ready = case != 0;
            native.contains = case != 1;
            native.fail = (case == 2).then_some(At::Ready).or((case == 3).then_some(At::Filter));
            let text = if case == 4 { None } else { Some(b"Save".to_vec()) };
            assert!(frame.filter(|| (case != 5).then(|| state.current(text)), &native, 7, 0).is_none());
            frame.returned();
            frame.finish(|| current(&state), &mut native, 44, &drawing).unwrap();
            assert!(native.scenes().is_empty(), "case {case}");
        }
    }

    #[test]
    fn unchanged_presentation_refresh_keeps_the_current_frames_handoff() {
        let mut state = Handoff::new();
        let mut frame = Frame::new();
        let drawing = Cell::new(false);
        let mut native = Recorder::new(&drawing);
        showing(&mut state, b"Save");
        prepare(&state, &mut frame, &native);
        showing(&mut state, b"Save");
        frame.finish(|| current(&state), &mut native, 44, &drawing).unwrap();
        assert_eq!(native.scenes(), [At::Begin, At::Setup, At::Draw, At::End]);
    }

    #[test]
    fn changed_content_objects_or_presentation_scope_cancel_a_prepared_draw() {
        for case in 0..10 {
            let mut state = Handoff::new();
            let mut frame = Frame::new();
            let drawing = Cell::new(false);
            let mut native = Recorder::new(&drawing);
            showing(&mut state, b"Save");
            prepare(&state, &mut frame, &native);
            match case {
                0 => showing(&mut state, b"Cancel"),
                1 => state.showing(3, 2, 22, b"Save"),
                2 => state.showing(1, 4, 22, b"Save"),
                3 => state.showing(1, 2, 55, b"Save"),
                4 => state.owner_invalidated(1),
                5 => state.holder_deleted(2),
                6 => state.clear(),   // Device reset or session end.
                7 => frame.discard(), // Held frame or next native frame.
                8 => {
                    state.begin();
                    state.end(false);
                }
                9 => {
                    state.begin();
                    state.content(Some(b"Save".to_vec()));
                    state.end(true);
                }
                _ => unreachable!(),
            }
            frame.finish(|| current(&state), &mut native, 44, &drawing).unwrap();
            assert!(native.scenes().is_empty(), "case {case}");
        }
    }

    #[test]
    fn a_changed_framework_target_unready_node_or_renderer_cannot_complete_the_handoff() {
        for case in 0..4 {
            let mut state = Handoff::new();
            let mut frame = Frame::new();
            let drawing = Cell::new(false);
            let mut native = Recorder::new(&drawing);
            showing(&mut state, b"Save");
            prepare(&state, &mut frame, &native);
            native.ready = case != 1;
            native.renderer = if case == 2 { 55 } else { 44 };
            let text = match case {
                0 => None,
                3 => Some(b"Cancel".to_vec()),
                _ => Some(b"Save".to_vec()),
            };
            frame.finish(|| state.current(text), &mut native, 44, &drawing).unwrap();
            // Even becoming valid again does not resurrect an abandoned frame.
            native.ready = true;
            native.renderer = 44;
            frame.finish(|| current(&state), &mut native, 44, &drawing).unwrap();
            assert!(native.scenes().is_empty(), "case {case}");
        }
    }

    #[test]
    fn every_native_failure_consumes_the_handoff_and_ends_a_started_scene() {
        for fail in [At::Ready, At::Renderer, At::Begin, At::Setup, At::Draw, At::End] {
            let mut state = Handoff::new();
            let mut frame = Frame::new();
            let drawing = Cell::new(false);
            let mut native = Recorder::new(&drawing);
            showing(&mut state, b"Save");
            prepare(&state, &mut frame, &native);
            native.fail = Some(fail);
            assert!(frame.finish(|| current(&state), &mut native, 44, &drawing).is_err());
            assert!(!drawing.get());
            let expected = match fail {
                At::Ready | At::Renderer => vec![],
                At::Begin => vec![At::Begin],
                At::Setup => vec![At::Begin, At::Setup, At::End],
                _ => vec![At::Begin, At::Setup, At::Draw, At::End],
            };
            assert_eq!(native.scenes(), expected, "{fail:?}");
            native.fail = None;
            frame.finish(|| current(&state), &mut native, 44, &drawing).unwrap();
            assert_eq!(native.scenes(), expected);
        }
    }

    #[test]
    fn a_refused_begin_never_ends_a_scene_and_a_refused_end_is_reported() {
        for refuse in [At::Begin, At::End] {
            let mut state = Handoff::new();
            let mut frame = Frame::new();
            let drawing = Cell::new(false);
            let mut native = Recorder::new(&drawing);
            showing(&mut state, b"Save");
            prepare(&state, &mut frame, &native);
            native.refuse = Some(refuse);
            assert!(frame.finish(|| current(&state), &mut native, 44, &drawing).is_err());
            assert_eq!(native.scenes(), if refuse == At::Begin { vec![At::Begin] } else { vec![At::Begin, At::Setup, At::Draw, At::End] });
            assert!(!drawing.get());
        }
    }

    #[test]
    fn a_caught_draw_panic_still_ends_the_scene_and_releases_the_recursion_gate() {
        let mut state = Handoff::new();
        let mut frame = Frame::new();
        let drawing = Cell::new(false);
        let mut native = Recorder::new(&drawing);
        showing(&mut state, b"Save");
        prepare(&state, &mut frame, &native);
        native.panic = Some(At::Draw);
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| frame.finish(|| current(&state), &mut native, 44, &drawing))).is_err());
        assert_eq!(native.scenes(), [At::Begin, At::Setup, At::Draw, At::End]);
        assert!(!drawing.get());
        native.panic = None;
        frame.finish(|| current(&state), &mut native, 44, &drawing).unwrap();
        assert_eq!(native.scenes(), [At::Begin, At::Setup, At::Draw, At::End]);
    }

    #[test]
    fn invalidation_cancels_both_threads_frames_even_if_the_objects_are_reused() {
        let mut state = Handoff::new();
        let mut frames = [Frame::new(), Frame::new()];
        let drawing = Cell::new(false);
        let mut native = Recorder::new(&drawing);
        showing(&mut state, b"Save");
        for frame in &mut frames {
            prepare(&state, frame, &native);
        }
        state.owner_invalidated(1);
        showing(&mut state, b"Save");
        for frame in &mut frames {
            frame.finish(|| current(&state), &mut native, 44, &drawing).unwrap();
        }
        assert!(native.scenes().is_empty());
    }

    #[test]
    fn a_caught_nested_snapshot_panic_keeps_the_outer_native_list_alive() {
        let mut state = Handoff::new();
        let mut frame = Frame::new();
        let drawing = Cell::new(false);
        let mut native = Recorder::new(&drawing);
        showing(&mut state, b"Save");
        let header = frame.filter(|| Some(current(&state)), &native, 7, 0).unwrap();
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| { frame.filter(|| panic!("snapshot query failed"), &native, 7, 0) })).is_err());
        // The after observer pairs with the failed before observer, rather
        // than retiring the outer native call's still-borrowed list.
        frame.returned();
        assert_eq!(crate::containers::pointers(&crate::memory::Memory { base: 0 }, header, 10).unwrap(), [11, 33]);
        frame.returned();
        frame.finish(|| current(&state), &mut native, 44, &drawing).unwrap();
        assert_eq!(native.scenes(), [At::Begin, At::Setup, At::Draw, At::End]);
    }

    #[test]
    fn an_empty_frame_does_not_query_framework_text_or_native_state() {
        let state = Handoff::new();
        let mut frame = Frame::new();
        let drawing = Cell::new(false);
        let mut native = Recorder::new(&drawing);
        let current = state.current(None).with_text(|| panic!("no presentation needs framework text"));
        assert!(frame.filter(|| Some(current), &native, 7, 0).is_none());
        frame.returned();
        frame.finish(|| panic!("no handoff needs current state"), &mut native, 44, &drawing).unwrap();
        assert!(native.calls.borrow().is_empty());
    }

    #[test]
    fn hit_targets_belong_only_to_the_current_presentation_call() {
        let mut state = Handoff::new();
        state.hit(9);
        assert!(state.content(None).is_none());
        state.begin();
        assert!(state.content(None).is_none());
        state.hit(12);
        assert!(matches!(state.content(None), Some(Content::Managed(12))));
        state.end(false);
        assert!(!state.presenting());
        state.begin();
        assert!(state.content(None).is_none());
        state.end(true);
    }

    #[test]
    fn an_unrelated_destruction_keeps_the_current_handoff() {
        let mut state = Handoff::new();
        let mut frame = Frame::new();
        let drawing = Cell::new(false);
        let mut native = Recorder::new(&drawing);
        showing(&mut state, b"Save");
        prepare(&state, &mut frame, &native);
        state.owner_invalidated(3);
        state.holder_deleted(4);
        frame.finish(|| current(&state), &mut native, 44, &drawing).unwrap();
        assert_eq!(native.scenes(), [At::Begin, At::Setup, At::Draw, At::End]);
    }
}
