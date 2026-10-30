//! The frame the player sees, as an image. The capability layer takes it at
//! the next frame the main thread presents and encodes it on a thread of its
//! own, so the answer comes at a later safe point.
use super::{Outcome, Poll, Wait};
use crate::game::screenshot::{self as layer, Shot};
use eu4_harmony_remote::{
    screenshot::{Screenshot, ScreenshotAnswer, ScreenshotRequest},
    Answer, Binary, Error, Response,
};
use std::sync::Arc;

layer! {
    /// Asks for the next frame at `percent` of its size.
    fn screenshot(percent: u32) -> Arc<Shot> = layer::screenshot;
}

pub fn run(request: ScreenshotRequest, game: &mut (impl Layer + ?Sized)) -> Outcome {
    let ScreenshotRequest::Take { scale } = request;
    match game.screenshot(scale.unwrap_or(100)) {
        Ok(shot) => {
            let mut taking = Taking(shot);
            match taking.answer() {
                Some(response) => Outcome::Done(response),
                None => Outcome::Later(Box::new(taking)),
            }
        }
        Err(fail) => Outcome::Done(Err(fail.into())),
    }
}

/// A screenshot on its way. Dropped, it is given up.
struct Taking(Arc<Shot>);

impl Taking {
    fn answer(&mut self) -> Option<Response> {
        let image = self.0.taken()?.map_err(Error::from);
        Some(image.map(|image| {
            Answer::Screenshot(ScreenshotAnswer::Take(Screenshot {
                image: Binary::new("image/png", image.png),
                width: image.width,
                height: image.height,
                frame_width: image.frame_width,
                frame_height: image.frame_height,
            }))
        }))
    }
}

impl Wait for Taking {
    fn poll(&mut self, _: &mut Poll<'_>) -> Option<Response> {
        self.answer()
    }
}

#[cfg(test)]
mod tests {
    use super::super::{fake::Fake, Engine, Place, Reply, Request};
    use super::*;
    use crate::game::{screenshot::Image, Fail, Status};
    use eu4_harmony_remote::response_json;
    use serde_json::{json, Value};
    use std::time::Duration;

    fn quiet(_: &str) {}

    fn answer(reply: &Reply) -> Value {
        reply.wait(Duration::ZERO).map_or(Value::Null, |r| response_json(&r))
    }

    fn take(scale: Option<u32>) -> Request {
        Request::Screenshot(ScreenshotRequest::Take { scale })
    }

    #[test]
    fn a_screenshot_is_answered_once_its_frame_is_taken() {
        let engine = Engine::new(quiet);
        let pending = Shot::new(50);
        let mut game = Fake { shot: Some(pending.clone()), ..Default::default() };
        let reply = engine.submit(take(Some(50)));
        engine.run(&mut game, &Place::Outside(None), Duration::from_secs(10));
        assert_eq!(game.calls, ["screenshot 50"]);
        assert_eq!(answer(&reply), Value::Null);
        engine.run(&mut game, &Place::Outside(None), Duration::from_secs(10));
        assert_eq!(answer(&reply), Value::Null);

        pending.finish(Ok(Image { png: vec![0x89, b'P', b'N', b'G'], width: 960, height: 540, frame_width: 1920, frame_height: 1080 }));
        engine.run(&mut game, &Place::Outside(Some(Error::new(eu4_harmony_remote::Code::NotInGame, "the main menu is open"))), Duration::from_secs(10));
        assert_eq!(
            answer(&reply),
            json!({"ok": true, "result": {
                "image": {"media_type": "image/png", "base64": "iVBORw=="},
                "width": 960, "height": 540, "frame_width": 1920, "frame_height": 1080,
            }})
        );
    }

    #[test]
    fn a_screenshot_that_fails_says_why() {
        let engine = Engine::new(quiet);
        let failed = Shot::new(100);
        failed.finish(Err(Fail { status: Status::NotReady, message: "the device is lost".into() }));
        let mut game = Fake { shot: Some(failed), ..Default::default() };
        let reply = engine.submit(take(None));
        engine.run(&mut game, &Place::Game, Duration::from_secs(10));
        assert_eq!(game.calls, ["screenshot 100"]);
        assert_eq!(answer(&reply), json!({"ok": false, "error": {"code": "not_ready", "message": "the device is lost"}}));

        let reply = engine.submit(take(Some(0)));
        engine.run(&mut Fake::default(), &Place::Game, Duration::from_secs(10));
        assert_eq!(answer(&reply)["error"]["code"], "invalid_argument");
    }

    #[test]
    fn a_screenshot_given_up_is_no_longer_waited_for() {
        let engine = Engine::new(quiet);
        let pending = Shot::new(100);
        let mut game = Fake { shot: Some(pending.clone()), ..Default::default() };
        let reply = engine.submit(take(None));
        engine.run(&mut game, &Place::Game, Duration::from_secs(10));
        assert_eq!(Arc::strong_count(&pending), 3, "the test, the fake and the waiting request hold it");
        drop(reply);
        engine.run(&mut game, &Place::Game, Duration::from_secs(10));
        // The capture sees it given up once only it holds the shot.
        assert_eq!(Arc::strong_count(&pending), 2, "the waiting request still holds it");
    }
}
