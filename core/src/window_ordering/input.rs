//! Observe native acceptance without holding a borrow across native code.
//! Nested dispatch of the same event retains the innermost accepted target.
use super::course::Token;

struct Dispatch {
    serial: u64,
    event: usize,
    accepts_activation: bool,
    accepted: bool,
}

pub(super) struct Capture {
    serial: u64,
    target: Option<Token>,
}

#[derive(Default)]
pub(super) struct Input {
    frames: Vec<Dispatch>,
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

    /// Call before the handler, only for a mouse press/double press whose
    /// handled mask is zero. An unknown target still prevents promoting an
    /// outer, known ancestor if this innermost handler accepts the event.
    pub fn capture(&self, event: usize, target: Option<Token>) -> Option<Capture> {
        let frame = self.frames.iter().rev().find(|frame| frame.event == event)?;
        (frame.accepts_activation && !frame.accepted).then_some(Capture { serial: frame.serial, target })
    }

    /// The event remains valid until the dispatcher returns. Its target may
    /// already be gone: the caller reads only the event's handled mask here.
    pub fn returned(&mut self, capture: Option<Capture>, handled: bool) {
        let Some(capture) = capture.filter(|_| handled) else { return };
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

    #[test]
    fn the_innermost_accepting_handler_wins_without_promoting_an_ancestor() {
        let mut input = Input::default();
        input.begin(100);
        let outer = input.capture(100, Some(token(1)));
        let inner = input.capture(100, Some(token(2)));
        input.returned(inner, true);
        input.returned(outer, true);
        assert_eq!(input.end(false), Some(vec![token(2)]));
        assert!(input.idle());
    }

    #[test]
    fn an_unsupported_inner_target_does_not_activate_a_known_ancestor() {
        let mut input = Input::default();
        input.begin(100);
        let outer = input.capture(100, Some(token(1)));
        let inner = input.capture(100, None);
        input.returned(inner, true);
        input.returned(outer, true);
        assert_eq!(input.end(false), Some(vec![]));
    }

    #[test]
    fn a_nested_event_cannot_commit_inside_the_outer_candidate_walk() {
        let mut input = Input::default();
        input.begin(100);
        let outer = input.capture(100, Some(token(1)));
        input.begin(200);
        let inner = input.capture(200, Some(token(2)));
        input.returned(inner, true);
        assert_eq!(input.end(false), None);
        assert!(!input.idle());
        input.returned(outer, true);
        assert_eq!(input.end(false), Some(vec![token(2), token(1)]));
    }

    #[test]
    fn reentering_mouse_dispatch_with_the_same_event_keeps_one_target() {
        let mut input = Input::default();
        input.begin(100);
        let outer = input.capture(100, Some(token(1)));
        input.begin(100);
        let inner = input.capture(100, Some(token(2)));
        input.returned(inner, true);
        assert_eq!(input.end(false), None);
        input.returned(outer, true);
        assert_eq!(input.end(false), Some(vec![token(2)]));
    }

    #[test]
    fn falling_through_does_not_select_a_target() {
        let mut input = Input::default();
        assert!(input.capture(100, Some(token(1))).is_none());
        input.begin(100);
        let first = input.capture(100, Some(token(1)));
        input.returned(first, false);
        let second = input.capture(100, Some(token(2)));
        input.returned(second, true);
        assert_eq!(input.end(false), Some(vec![token(2)]));
    }

    #[test]
    fn a_mouse_nested_in_keyboard_dispatch_waits_for_the_keyboard_walk() {
        let mut input = Input::default();
        input.begin_passive(200);
        assert!(input.capture(200, Some(token(1))).is_none());
        input.begin(100);
        let captured = input.capture(100, Some(token(2)));
        input.returned(captured, true);
        assert_eq!(input.end(false), None);
        assert!(!input.idle());
        assert_eq!(input.end(false), Some(vec![token(2)]));
    }

    #[test]
    fn a_passive_dispatch_reusing_an_event_cannot_capture_mouse_activation() {
        let mut input = Input::default();
        input.begin(100);
        input.begin_passive(100);
        assert!(input.capture(100, Some(token(1))).is_none());
        assert_eq!(input.end(false), None);
        let captured = input.capture(100, Some(token(2)));
        input.returned(captured, true);
        assert_eq!(input.end(false), Some(vec![token(2)]));
    }

    #[test]
    fn faulted_dispatch_does_not_commit_queued_activation() {
        let mut input = Input::default();
        input.begin(100);
        let capture = input.capture(100, Some(token(1)));
        input.returned(capture, true);
        assert_eq!(input.end(true), Some(vec![]));
        input.begin(200);
        assert_eq!(input.end(false), Some(vec![]));
    }
}
