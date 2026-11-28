//! Observe native acceptance without holding a borrow across native code.
//! Nested dispatch of the same event retains the innermost accepted target.
use super::course::Token;

struct Dispatch {
    serial: u64,
    event: usize,
    accepts_activation: bool,
    accepted: bool,
}

struct Capture {
    serial: u64,
    target: Option<Token>,
}

#[derive(Default)]
pub(super) struct Input {
    frames: Vec<Dispatch>,
    captures: Vec<Option<Capture>>,
    pending: Vec<Token>,
    serial: u64,
}

impl Input {
    pub fn begin(&mut self, event: usize) {
        self.enter(event, true);
    }

    /// Keyboard and other native walks also retain prepared input vectors.
    /// Reentrant mouse input must not commit while such a caller is walking.
    pub fn begin_passive(&mut self, event: usize) {
        self.enter(event, false);
    }

    fn enter(&mut self, event: usize, accepts_activation: bool) {
        self.serial = self.serial.checked_add(1).expect("input dispatch identity exhausted");
        self.frames.push(Dispatch { serial: self.serial, event, accepts_activation, accepted: false });
    }

    /// Whether the innermost matching dispatch may still capture an activation.
    pub fn accepts(&self, event: usize) -> bool {
        self.frames.iter().rev().find(|frame| frame.event == event).is_some_and(|frame| frame.accepts_activation && !frame.accepted)
    }

    /// Record whether the handler entering now captured `target`.
    /// Call once per handler entry so every entry pairs with `pop_capture`.
    pub fn push_capture(&mut self, event: usize, eligible: bool, target: Option<Token>) {
        let capture = if eligible {
            self.frames
                .iter()
                .rev()
                .find(|frame| frame.event == event)
                .filter(|frame| frame.accepts_activation && !frame.accepted)
                .map(|frame| Capture { serial: frame.serial, target })
        } else {
            None
        };
        self.captures.push(capture);
    }

    /// Whether the matching `pop_capture` needs the event's handled mask.
    pub fn top_captured(&self) -> bool {
        self.captures.last().is_some_and(Option::is_some)
    }

    /// The event remains valid until the dispatcher returns. Its target may
    /// already be gone: the caller reads only the event's handled mask here.
    pub fn pop_capture(&mut self, handled: bool) {
        static NESTING_BROKEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        let Some(entry) = self.captures.pop() else {
            crate::base::broken_invariant(&NESTING_BROKEN, format_args!("native GUI dispatch nesting"));
            return;
        };
        let Some(capture) = entry.filter(|_| handled) else { return };
        let Some(frame) = self.frames.iter_mut().find(|frame| frame.serial == capture.serial) else { return };
        if frame.accepted {
            return;
        }
        let event = frame.event;
        for frame in self.frames.iter_mut().filter(|frame| frame.event == event) {
            frame.accepted = true;
        }
        if let Some(target) = capture.target {
            self.pending.push(target);
        }
    }

    /// Only the outermost mouse return permits native placement. Tokens are
    /// revalidated by the ordering process after native handlers have ended.
    pub fn end(&mut self, faulted: bool) -> Option<Vec<Token>> {
        self.frames.pop()?;
        if faulted {
            self.pending.clear();
        }
        self.frames.is_empty().then(|| std::mem::take(&mut self.pending))
    }

    pub fn idle(&self) -> bool {
        self.frames.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn token(key: u64) -> Token {
        Token { key, generation: 1 }
    }

    fn dispatch(input: &mut Input, event: usize, target: Option<Token>, handled: bool) {
        let eligible = input.accepts(event);
        input.push_capture(event, eligible, target);
        let check = input.top_captured();
        input.pop_capture(check && handled);
    }

    #[test]
    fn the_innermost_accepting_handler_wins_without_promoting_an_ancestor() {
        let mut input = Input::default();
        input.begin(100);
        let outer = input.accepts(100);
        input.push_capture(100, outer, Some(token(1)));
        dispatch(&mut input, 100, Some(token(2)), true);
        let check = input.top_captured();
        input.pop_capture(check);
        assert_eq!(input.end(false), Some(vec![token(2)]));
        assert!(input.idle());
    }

    #[test]
    fn an_unsupported_inner_target_does_not_activate_a_known_ancestor() {
        let mut input = Input::default();
        input.begin(100);
        let outer = input.accepts(100);
        input.push_capture(100, outer, Some(token(1)));
        dispatch(&mut input, 100, None, true);
        let check = input.top_captured();
        input.pop_capture(check);
        assert_eq!(input.end(false), Some(vec![]));
    }

    #[test]
    fn a_nested_event_cannot_commit_inside_the_outer_candidate_walk() {
        let mut input = Input::default();
        input.begin(100);
        let outer = input.accepts(100);
        input.push_capture(100, outer, Some(token(1)));
        input.begin(200);
        dispatch(&mut input, 200, Some(token(2)), true);
        assert_eq!(input.end(false), None);
        assert!(!input.idle());
        let check = input.top_captured();
        input.pop_capture(check);
        assert_eq!(input.end(false), Some(vec![token(2), token(1)]));
    }

    #[test]
    fn reentering_mouse_dispatch_with_the_same_event_keeps_one_target() {
        let mut input = Input::default();
        input.begin(100);
        let outer = input.accepts(100);
        input.push_capture(100, outer, Some(token(1)));
        input.begin(100);
        dispatch(&mut input, 100, Some(token(2)), true);
        assert_eq!(input.end(false), None);
        let check = input.top_captured();
        input.pop_capture(check);
        assert_eq!(input.end(false), Some(vec![token(2)]));
    }

    #[test]
    fn falling_through_does_not_select_a_target() {
        let mut input = Input::default();
        assert!(!input.accepts(100));
        input.push_capture(100, false, Some(token(1)));
        input.pop_capture(false);
        input.begin(100);
        dispatch(&mut input, 100, Some(token(1)), false);
        dispatch(&mut input, 100, Some(token(2)), true);
        assert_eq!(input.end(false), Some(vec![token(2)]));
    }

    #[test]
    fn a_mouse_nested_in_keyboard_dispatch_waits_for_the_keyboard_walk() {
        let mut input = Input::default();
        input.begin_passive(200);
        assert!(!input.accepts(200));
        input.push_capture(200, false, Some(token(1)));
        input.pop_capture(false);
        input.begin(100);
        dispatch(&mut input, 100, Some(token(2)), true);
        assert_eq!(input.end(false), None);
        assert!(!input.idle());
        assert_eq!(input.end(false), Some(vec![token(2)]));
    }

    #[test]
    fn a_passive_dispatch_reusing_an_event_cannot_capture_mouse_activation() {
        let mut input = Input::default();
        input.begin(100);
        input.begin_passive(100);
        assert!(!input.accepts(100));
        input.push_capture(100, false, Some(token(1)));
        input.pop_capture(false);
        assert_eq!(input.end(false), None);
        dispatch(&mut input, 100, Some(token(2)), true);
        assert_eq!(input.end(false), Some(vec![token(2)]));
    }

    #[test]
    fn faulted_dispatch_does_not_commit_queued_activation() {
        let mut input = Input::default();
        input.begin(100);
        dispatch(&mut input, 100, Some(token(1)), true);
        assert_eq!(input.end(true), Some(vec![]));
        input.begin(200);
        assert_eq!(input.end(false), Some(vec![]));
    }
}
