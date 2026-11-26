//! What the game is handed for each input, frame by frame: an action becomes
//! the steps of each frame, a step being one thing a mouse or keyboard does.
//! Each step has a frame of its own, as a hand's would, so that the game
//! sees a button pressed and let go at frames apart and a modifier held
//! before the click it modifies.
use super::{Action, Button, Direction, Key};

/// One thing a mouse or keyboard does. Positions are pixels of the frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Move(u32, u32),
    /// A button pressed (true) or let go, where the cursor is.
    Button(Button, bool),
    /// The wheel turned a notch, where the cursor is.
    Wheel(Direction),
    /// A key pressed (true) or let go.
    Key(Key, bool),
    /// A character typed.
    Text(char),
}

/// The buttons and keys held down.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Held {
    pub buttons: Vec<Button>,
    pub keys: Vec<Key>,
}

impl Held {
    /// What is held after `step`.
    pub fn apply(&mut self, step: &Step) {
        match *step {
            Step::Button(b, true) if !self.buttons.contains(&b) => self.buttons.push(b),
            Step::Button(b, false) => self.buttons.retain(|&h| h != b),
            Step::Key(k, true) if !self.keys.contains(&k) => self.keys.push(k),
            Step::Key(k, false) => self.keys.retain(|&h| h != k),
            _ => {}
        }
    }

    pub fn key(&self, name: &str) -> bool {
        self.keys.iter().any(|k| k.name == name)
    }

    /// The steps that let go of everything held, keys last held first.
    pub fn release(&self) -> Vec<Vec<Step>> {
        let buttons = self.buttons.iter().map(|&b| vec![Step::Button(b, false)]);
        let keys = self.keys.iter().rev().map(|&k| vec![Step::Key(k, false)]);
        buttons.chain(keys).collect()
    }
}

/// How many frames a drag takes to move from its start to its end.
pub const DRAG_FRAMES: u32 = 10;

/// The frames of `action`, with `held` held before it: each frame's steps,
/// in order.
pub fn frames(action: &Action, held: &Held) -> Vec<Vec<Step>> {
    let mut plan = Plan { frames: Vec::new(), held: held.clone() };
    match action {
        Action::Click { at, button, count, modifiers } => plan.holding(modifiers, |p| {
            p.step(Step::Move(at.0, at.1));
            for _ in 0..*count {
                p.step(Step::Button(*button, true));
                p.step(Step::Button(*button, false));
            }
        }),
        Action::Move { at, modifiers } => plan.holding(modifiers, |p| p.step(Step::Move(at.0, at.1))),
        Action::Drag { from, to, button, modifiers } => plan.holding(modifiers, |p| {
            p.step(Step::Move(from.0, from.1));
            p.step(Step::Button(*button, true));
            for i in 1..=DRAG_FRAMES {
                let between = |a: u32, b: u32| (a as i64 + (b as i64 - a as i64) * i as i64 / DRAG_FRAMES as i64) as u32;
                p.step(Step::Move(between(from.0, to.0), between(from.1, to.1)));
            }
            p.step(Step::Button(*button, false));
        }),
        Action::Scroll { at, direction, amount, modifiers } => plan.holding(modifiers, |p| {
            p.step(Step::Move(at.0, at.1));
            for _ in 0..*amount {
                p.step(Step::Wheel(*direction));
            }
        }),
        Action::Keys { keys, repeat } => {
            for _ in 0..*repeat {
                let pressed: Vec<Key> = keys.iter().copied().filter(|k| !plan.held.keys.contains(k)).collect();
                for &k in &pressed {
                    plan.press(k);
                }
                for &k in pressed.iter().rev() {
                    plan.step(Step::Key(k, false));
                }
            }
        }
        Action::Type(text) => text.chars().for_each(|c| plan.step(Step::Text(c))),
        Action::MouseDown { at, button } => {
            plan.step(Step::Move(at.0, at.1));
            plan.step(Step::Button(*button, true));
        }
        Action::MouseUp { at, button } => {
            plan.step(Step::Move(at.0, at.1));
            plan.step(Step::Button(*button, false));
        }
        Action::KeyDown(key) => plan.press(*key),
        Action::KeyUp(key) => plan.step(Step::Key(*key, false)),
    }
    plan.frames
}

struct Plan {
    frames: Vec<Vec<Step>>,
    /// What is held after the frames so far.
    held: Held,
}

impl Plan {
    /// A frame of one step.
    fn step(&mut self, step: Step) {
        self.held.apply(&step);
        self.frames.push(vec![step]);
    }

    /// A key pressed, with the character it types on a US layout in the same
    /// frame, as a keyboard's press comes with its character, unless ctrl,
    /// alt or win is held.
    fn press(&mut self, key: Key) {
        self.step(Step::Key(key, true));
        let typing = !["ctrl", "alt", "win"].iter().any(|m| self.held.key(m));
        if let (Some((plain, shifted)), true) = (key.text, typing) {
            let c = if self.held.key("shift") { shifted } else { plain };
            self.frames.last_mut().expect("just pushed").push(Step::Text(c));
        }
    }

    /// The modifiers not held yet pressed before `body` and let go after it,
    /// in reverse.
    fn holding(&mut self, modifiers: &[Key], body: impl FnOnce(&mut Self)) {
        let pressed: Vec<Key> = modifiers.iter().copied().filter(|k| !self.held.keys.contains(k)).collect();
        for &k in &pressed {
            self.step(Step::Key(k, true));
        }
        body(self);
        for &k in pressed.iter().rev() {
            self.step(Step::Key(k, false));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eu4_harmony_remote::input::Key;

    fn key(name: &str) -> Key {
        Key::named(name).unwrap()
    }

    fn one(frames: Vec<Vec<Step>>) -> Vec<Step> {
        frames
            .into_iter()
            .map(|f| {
                assert_eq!(f.len(), 1, "{f:?}");
                f[0]
            })
            .collect()
    }

    #[test]
    fn a_click_moves_then_presses_and_lets_go_a_frame_each() {
        let click = Action::Click { at: (10, 20), button: Button::Right, count: 2, modifiers: vec![] };
        assert_eq!(
            one(frames(&click, &Held::default())),
            [
                Step::Move(10, 20),
                Step::Button(Button::Right, true),
                Step::Button(Button::Right, false),
                Step::Button(Button::Right, true),
                Step::Button(Button::Right, false)
            ]
        );
    }

    #[test]
    fn modifiers_are_held_around_the_action_unless_already_held() {
        let (shift, ctrl) = (key("shift"), key("ctrl"));
        let click = Action::Click { at: (1, 2), button: Button::Left, count: 1, modifiers: vec![ctrl, shift] };
        assert_eq!(
            one(frames(&click, &Held::default())),
            [
                Step::Key(ctrl, true),
                Step::Key(shift, true),
                Step::Move(1, 2),
                Step::Button(Button::Left, true),
                Step::Button(Button::Left, false),
                Step::Key(shift, false),
                Step::Key(ctrl, false)
            ]
        );
        let held = Held { buttons: vec![], keys: vec![shift] };
        assert_eq!(one(frames(&click, &held))[..2], [Step::Key(ctrl, true), Step::Move(1, 2)]);
    }

    #[test]
    fn a_drag_moves_over_its_frames_with_the_button_down() {
        let drag = Action::Drag { from: (0, 100), to: (100, 0), button: Button::Left, modifiers: vec![] };
        let steps = one(frames(&drag, &Held::default()));
        assert_eq!(steps.len(), 2 + DRAG_FRAMES as usize + 1);
        assert_eq!(steps[..3], [Step::Move(0, 100), Step::Button(Button::Left, true), Step::Move(10, 90)]);
        assert_eq!(steps[steps.len() - 2..], [Step::Move(100, 0), Step::Button(Button::Left, false)]);
    }

    #[test]
    fn the_wheel_turns_a_notch_a_frame() {
        let scroll = Action::Scroll { at: (5, 5), direction: Direction::Down, amount: 3, modifiers: vec![] };
        assert_eq!(
            one(frames(&scroll, &Held::default())),
            [Step::Move(5, 5), Step::Wheel(Direction::Down), Step::Wheel(Direction::Down), Step::Wheel(Direction::Down)]
        );
    }

    #[test]
    fn a_key_types_its_character_unless_a_command_modifier_is_held() {
        let (a, shift, ctrl, f1) = (key("a"), key("shift"), key("ctrl"), key("f1"));
        let plain = frames(&Action::Keys { keys: vec![a], repeat: 1 }, &Held::default());
        assert_eq!(plain, [vec![Step::Key(a, true), Step::Text('a')], vec![Step::Key(a, false)]]);
        let shifted = frames(&Action::Keys { keys: vec![shift, a], repeat: 1 }, &Held::default());
        assert_eq!(shifted[1], [Step::Key(a, true), Step::Text('A')]);
        assert_eq!(shifted.len(), 4);
        let command = frames(&Action::Keys { keys: vec![ctrl, a], repeat: 1 }, &Held::default());
        assert_eq!(command[1], [Step::Key(a, true)]);
        assert_eq!(frames(&Action::Keys { keys: vec![f1], repeat: 2 }, &Held::default()).len(), 4);
    }

    #[test]
    fn a_held_key_is_pressed_once_and_stays_down() {
        let shift = key("shift");
        let down = frames(&Action::KeyDown(shift), &Held::default());
        assert_eq!(down, [vec![Step::Key(shift, true)]]);
        let mut held = Held::default();
        down.iter().flatten().for_each(|s| held.apply(s));
        assert_eq!(held.keys, [shift]);
        // A combination with it held does not press or let it go.
        let a = key("a");
        assert_eq!(frames(&Action::Keys { keys: vec![shift, a], repeat: 1 }, &held), [vec![Step::Key(a, true), Step::Text('A')], vec![Step::Key(a, false)]]);
        assert_eq!(frames(&Action::KeyUp(shift), &held), [vec![Step::Key(shift, false)]]);
    }

    #[test]
    fn text_is_a_character_a_frame() {
        assert_eq!(one(frames(&Action::Type("中a".into()), &Held::default())), [Step::Text('中'), Step::Text('a')]);
    }

    #[test]
    fn letting_go_of_everything_held() {
        let (shift, ctrl) = (key("shift"), key("ctrl"));
        let held = Held { buttons: vec![Button::Left], keys: vec![shift, ctrl] };
        assert_eq!(one(held.release()), [Step::Button(Button::Left, false), Step::Key(ctrl, false), Step::Key(shift, false)]);
    }
}
