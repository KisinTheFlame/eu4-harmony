//! The game console.
use crate::{Checked, Refusal, Status};

/// The characters the console window trims from both ends of a line.
pub const TRIMMED: [char; 4] = [' ', '\t', '\r', '\n'];

/// The line the game console runs for `command`: the command trimmed as the
/// console window trims a line. The window cuts a line at `|` and runs the
/// part before, writing what it printed to the file named after it; the
/// framework answers what it printed instead, so it refuses a `|`, as it
/// refuses a NUL and a command that is nothing once trimmed.
pub fn command(command: &str) -> Checked<&str> {
    if command.contains('|') {
        return Err(Refusal::new(
            Status::InvalidArgument,
            "a | sends the game console's output to a file; remote control and extensions get it in the answer, so leave it out",
        ));
    }
    if command.contains('\0') {
        return Err(Refusal::new(Status::InvalidArgument, "a command cannot hold a NUL"));
    }
    let line = command.trim_matches(TRIMMED);
    if line.is_empty() {
        return Err(Refusal::new(Status::InvalidArgument, "no command"));
    }
    Ok(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_is_trimmed_as_the_window_trims_it() {
        assert_eq!(command(" \tcash 100\r\n"), Ok("cash 100"));
        // Only the ends: the words inside stay as they are.
        assert_eq!(command("add_opinion  TUR"), Ok("add_opinion  TUR"));
    }

    #[test]
    fn a_command_the_window_would_not_run_as_it_is_is_refused() {
        for (given, says) in [("", "no command"), (" \r\n", "no command"), ("cash 100 | out.txt", "a | sends"), ("cash\x001", "NUL")] {
            let refusal = command(given).err().unwrap_or_else(|| panic!("{given:?} was taken"));
            assert_eq!(refusal.status, Status::InvalidArgument);
            assert!(refusal.message.contains(says), "{given:?}: {}", refusal.message);
        }
    }
}
