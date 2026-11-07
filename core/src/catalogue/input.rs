//! Input at screen positions. An input is answered once it is taken; the
//! engine runs a request that comes after it once the game has had it
//! (`crate::remote::Engine::run`).
use crate::game::input::checks;
use crate::game::{
    self,
    input::{Action, Taken},
};
use eu4_harmony_remote::{
    input::{Button, Cursor, InputAnswer, InputRequest, Key, Keys},
    Error,
};

/// The group's slice of the capability layer.
pub trait Layer {
    /// Takes an input, its positions in a screenshot at `scale` percent.
    fn take(&mut self, action: Action, scale: u32) -> game::GResult<Taken>;
    /// Whether every input up to `taken` has been handed to the game and a
    /// frame has run since.
    fn handled(&mut self, taken: Taken) -> bool;
    fn cursor(&mut self) -> game::GResult<game::input::Cursor>;
}

impl Layer for super::Live {
    fn take(&mut self, action: Action, scale: u32) -> game::GResult<Taken> {
        game::input::take(self.0, action, scale)
    }

    fn handled(&mut self, taken: Taken) -> bool {
        game::input::handled(taken)
    }

    fn cursor(&mut self) -> game::GResult<game::input::Cursor> {
        game::input::cursor(self.0)
    }
}

/// The group's slice is written above by hand, without `layer!`'s lines.
#[cfg(test)]
pub fn bindings() -> super::Bindings {
    super::Bindings::default()
}

fn one(keys: &Keys) -> Result<Key, Error> {
    checks::one_key(&keys.0.iter().map(|k| k.name).collect::<Vec<_>>())?;
    Ok(keys.0[0])
}

fn modifiers(keys: Option<Keys>) -> Vec<Key> {
    keys.map_or_else(Vec::new, |k| k.0)
}

/// Runs a request; an input taken is answered with its number, for the
/// engine to keep the requests after it waiting until the game has it.
pub fn run(request: InputRequest, game: &mut (impl Layer + ?Sized)) -> (Result<InputAnswer, Error>, Option<Taken>) {
    let left = |b: Option<Button>| b.unwrap_or(Button::Left);
    type Answered = fn(()) -> InputAnswer;
    let (action, scale, answered): (Action, Option<u32>, Answered) = match request {
        InputRequest::Get {} => {
            let cursor = game.cursor().map_err(Error::from).map(|c| {
                InputAnswer::Get(Cursor {
                    x: c.at.map(|a| a.0),
                    y: c.at.map(|a| a.1),
                    buttons: c.buttons,
                    keys: c.keys.iter().map(|k| k.name.to_string()).collect(),
                })
            });
            return (cursor, None);
        }
        InputRequest::Click { x, y, button, count, modifiers: m, scale } => {
            (Action::Click { at: (x, y), button: left(button), count: count.unwrap_or(1), modifiers: modifiers(m) }, scale, InputAnswer::Click)
        }
        InputRequest::Move { x, y, modifiers: m, scale } => (Action::Move { at: (x, y), modifiers: modifiers(m) }, scale, InputAnswer::Move),
        InputRequest::Drag { from_x, from_y, to_x, to_y, button, modifiers: m, scale } => {
            (Action::Drag { from: (from_x, from_y), to: (to_x, to_y), button: left(button), modifiers: modifiers(m) }, scale, InputAnswer::Drag)
        }
        InputRequest::Scroll { x, y, direction, amount, modifiers: m, scale } => {
            (Action::Scroll { at: (x, y), direction, amount: amount.unwrap_or(1), modifiers: modifiers(m) }, scale, InputAnswer::Scroll)
        }
        InputRequest::Key { keys, repeat } => (Action::Keys { keys: keys.0, repeat: repeat.unwrap_or(1) }, None, InputAnswer::Key),
        InputRequest::Type { text } => (Action::Type(text), None, InputAnswer::Type),
        InputRequest::MouseDown { x, y, button, scale } => (Action::MouseDown { at: (x, y), button: left(button) }, scale, InputAnswer::MouseDown),
        InputRequest::MouseUp { x, y, button, scale } => (Action::MouseUp { at: (x, y), button: left(button) }, scale, InputAnswer::MouseUp),
        InputRequest::KeyDown { key } => match one(&key) {
            Ok(k) => (Action::KeyDown(k), None, InputAnswer::KeyDown),
            Err(e) => return (Err(e), None),
        },
        InputRequest::KeyUp { key } => match one(&key) {
            Ok(k) => (Action::KeyUp(k), None, InputAnswer::KeyUp),
            Err(e) => return (Err(e), None),
        },
    };
    match game.take(action, scale.unwrap_or(100)) {
        Ok(taken) => (Ok(answered(())), Some(taken)),
        Err(fail) => (Err(fail.into()), None),
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        fake::{answer, ok, quiet, Fake, PLENTY},
        Place,
    };
    use crate::game::input::Action;
    use crate::remote::Engine;
    use eu4_harmony_remote::{input::Button, request_from_json, session::SessionRequest, Request};
    use serde_json::{json, Value};

    fn request(json: Value) -> Request {
        request_from_json(&json.to_string()).unwrap()
    }

    #[test]
    fn an_input_is_answered_once_taken_with_its_defaults_filled_in() {
        let engine = Engine::new(quiet);
        let mut game = Fake::default();
        let click = engine.submit(request(json!({"capability": "input.click", "x": 10, "y": 20})));
        let keys = engine.submit(request(json!({"capability": "input.key", "keys": "Control+S"})));
        engine.run(&mut game, &Place::Outside(None), PLENTY);
        assert_eq!((answer(&click), answer(&keys)), (ok(Value::Null), ok(Value::Null)));
        let ctrl_s = eu4_harmony_remote::input::Keys::parse("ctrl+s").unwrap().0;
        assert_eq!(game.inputs, [Action::Click { at: (10, 20), button: Button::Left, count: 1, modifiers: vec![] }, Action::Keys { keys: ctrl_s, repeat: 1 }]);
        assert_eq!(game.calls[0], format!("input {:?} at scale 100", game.inputs[0]));
    }

    #[test]
    fn a_request_after_an_input_waits_until_the_game_has_had_it() {
        let engine = Engine::new(quiet);
        let mut game = Fake::default();
        let click = engine.submit(request(json!({"capability": "input.click", "x": 1, "y": 2})));
        let session = engine.submit(Request::Session(SessionRequest::Get {}));
        let typed = engine.submit(request(json!({"capability": "input.type", "text": "中"})));
        engine.run(&mut game, &Place::Game, PLENTY);
        // The session waits for the click, and the text, sent after it, for the session.
        assert_eq!((answer(&click), answer(&session), answer(&typed)), (ok(Value::Null), Value::Null, Value::Null));
        engine.run(&mut game, &Place::Game, PLENTY);
        assert_eq!(answer(&session), Value::Null);
        game.inputs_handled = 1;
        engine.run(&mut game, &Place::Game, PLENTY);
        assert_eq!(answer(&session)["ok"], json!(true));
        assert_eq!(answer(&typed), ok(Value::Null));
        // Inputs one after another do not wait for each other.
        let first = engine.submit(request(json!({"capability": "input.key", "keys": "f1"})));
        let second = engine.submit(request(json!({"capability": "input.key", "keys": "f2"})));
        let cursor = engine.submit(request(json!({"capability": "input.get"})));
        engine.run(&mut game, &Place::Outside(None), PLENTY);
        assert_eq!((answer(&first), answer(&second), answer(&cursor)), (ok(Value::Null), ok(Value::Null), Value::Null));
        game.inputs_handled = 4;
        engine.run(&mut game, &Place::Outside(None), PLENTY);
        assert_eq!(answer(&cursor), ok(json!({"x": 10, "y": 20, "buttons": [], "keys": []})));
    }

    #[test]
    fn a_key_held_is_one_key_and_a_key_is_one_the_table_names() {
        let engine = Engine::new(quiet);
        let mut game = Fake::default();
        let held = engine.submit(request(json!({"capability": "input.key_down", "key": "ctrl+s"})));
        engine.run(&mut game, &Place::Outside(None), PLENTY);
        assert_eq!(answer(&held)["error"], json!({"code": "invalid_argument", "message": "one key, not ctrl+s"}));
        assert!(game.inputs.is_empty());
        let unknown = request_from_json(&json!({"capability": "input.key", "keys": "ctrl+banana"}).to_string()).unwrap_err();
        assert!(unknown.message.contains("\"banana\" is no key"), "{}", unknown.message);
    }
}
