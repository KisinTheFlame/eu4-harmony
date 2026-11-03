//! Input at screen positions.
use crate::{Checked, Refusal, Status};

/// Refuses a position off an image `(width, height)` pixels large: the frame,
/// or a screenshot of it at `scale` percent.
pub fn position(x: u32, y: u32, (width, height): (u32, u32), scale: u32) -> Checked {
    if x >= width || y >= height {
        let image = if scale == 100 { "the frame".to_string() } else { format!("a screenshot at scale {scale}") };
        return Err(Refusal::new(Status::InvalidArgument, format!("{x}, {y} is off {image}, which is {width} by {height} pixels")));
    }
    Ok(())
}

/// Refuses a number of clicks that is not 1 to 3.
pub fn count(count: u32) -> Checked {
    within("count", count, 3, "clicks")
}

/// Refuses a number of notches that is not 1 to 100.
pub fn amount(amount: u32) -> Checked {
    within("amount", amount, 100, "notches")
}

/// Refuses a number of presses that is not 1 to 100.
pub fn repeat(repeat: u32) -> Checked {
    within("repeat", repeat, 100, "presses")
}

fn within(name: &str, value: u32, most: u32, what: &str) -> Checked {
    if !(1..=most).contains(&value) {
        return Err(Refusal::new(Status::InvalidArgument, format!("{name} is 1 to {most} {what}; {value} is not")));
    }
    Ok(())
}

/// Refuses empty text.
pub fn text(text: &str) -> Checked {
    if text.is_empty() {
        return Err(Refusal::new(Status::InvalidArgument, "the text is empty"));
    }
    Ok(())
}

/// Refuses more than one key where one is held or let go.
pub fn one_key(names: &[&str]) -> Checked {
    if names.len() != 1 {
        return Err(Refusal::new(Status::InvalidArgument, format!("one key, not {}", names.join("+"))));
    }
    Ok(())
}

/// Refuses a modifier that is not ctrl, shift, alt or win (`modifier` says
/// whether the key named is one).
pub fn modifiers<'a>(keys: impl IntoIterator<Item = (&'a str, bool)>) -> Checked {
    for (name, modifier) in keys {
        if !modifier {
            return Err(Refusal::new(Status::InvalidArgument, format!("{name} is no modifier; the modifiers are ctrl, shift, alt and win")));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_are_on_the_image_and_numbers_in_their_ranges() {
        assert!(position(0, 0, (1920, 1080), 100).is_ok() && position(1919, 1079, (1920, 1080), 100).is_ok());
        assert_eq!(position(1920, 5, (1920, 1080), 100).unwrap_err().message, "1920, 5 is off the frame, which is 1920 by 1080 pixels");
        assert_eq!(position(5, 540, (960, 540), 50).unwrap_err().message, "5, 540 is off a screenshot at scale 50, which is 960 by 540 pixels");
        assert!(count(1).is_ok() && count(3).is_ok());
        assert_eq!(count(4).unwrap_err().message, "count is 1 to 3 clicks; 4 is not");
        assert_eq!(amount(0).unwrap_err().status, Status::InvalidArgument);
        assert!(repeat(100).is_ok() && repeat(101).is_err());
        assert_eq!(text("").unwrap_err().message, "the text is empty");
        assert_eq!(one_key(&["ctrl", "s"]).unwrap_err().message, "one key, not ctrl+s");
        assert!(one_key(&["shift"]).is_ok());
        assert!(modifiers([("ctrl", true), ("shift", true)]).is_ok());
        assert_eq!(modifiers([("ctrl", true), ("s", false)]).unwrap_err().message, "s is no modifier; the modifiers are ctrl, shift, alt and win");
    }
}
