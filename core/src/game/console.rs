//! The game console (GLOSSARY): running a command as the player types it
//! into the console window, and what it printed.
//!
//! The window cuts the line it reads into words and hands them to the game
//! console's run function, which looks the first word up among the commands,
//! runs it with the rest, and answers whether it succeeded and the text the
//! window then prints. The framework cuts the command the same way and calls
//! the same function, so a command meets the game's own checks: a command
//! only developers may run, or any command but two in an ironman or
//! multiplayer game, is refused with the game's own words
//! (docs/research/game-console.md).
use super::{context, fail, game, Caller, GResult, Status};
use crate::{game_string, memory::Memory, points::Point, unicode::game_text};

/// The game console: its commands, then whether the commands only
/// developers may run are locked and whether the game is ironman or
/// multiplayer. Made when first asked for.
const CONSOLE: usize = 0x2357b58;
/// `console() -> console`: the game console, made if there is none.
const GET: usize = 0x6e74c0;
/// `run(console, &result, &words) -> &result`: runs the command the first
/// word names with the words after it, and fills in the result.
const RUN: usize = 0x1721320;

/// What [`RUN`] answers: whether the command succeeded (u8), then the text
/// the window prints (a `std::string` of game text, its lines in white when
/// the command succeeded and in red when not).
mod result {
    pub const SUCCEEDED: usize = 0;
    pub const TEXT: usize = 8;
    pub const SIZE: usize = 0x28;
}

/// The characters the window trims from both ends of a line.
const TRIMMED: [char; 4] = [' ', '\t', '\r', '\n'];

pub fn points() -> Vec<Point> {
    vec![
        // The getter, making the game console with its flags cleared.
        Point::keep(
            GET,
            "48 83 ec 28 48 8b 05 8d 06 c7 01 48 85 c0 75 3d 8d 48 48 e8 fc bd 34 01 33 c9 48 89 44 24 30 48 89 05 72 06 c7 01",
            "game console",
        )
        .uses(&[CONSOLE]),
        // Running a command: the words at r8, the first looked up, a command
        // only developers may run refused while they are locked (0x18), the
        // result at rdx, filled in as the refusal shows.
        Point::keep(
            RUN,
            "48 89 5c 24 10 48 89 4c 24 08 55 56 57 41 54 41 55 41 56 41 57 48 8d 6c 24 d0 48 81 ec 30 01 00 00 49 8b f0 48 8b fa 4c 8b e1 \
             33 db 44 8b f3 89 5c 24 20 88 5c 24 40 48 89 5c 24 48 66 0f 6f 05 2c 09 6a 00 f3 0f 7f 44 24 58 88 5c 24 48 41 b0 01 48 8b 16 \
             e8 87 fc ff ff 48 89 85 80 00 00 00 48 85 c0 0f 84 39 06 00 00 38 18 75 61 41 38 5c 24 18 74 5a",
            "game console run",
        ),
        Point::keep(0x17213d4usize, "48 89 44 24 48 c6 07 00 0f 10 44 24 48 0f 11 47 08", "game console run result").uses(&[result::TEXT]),
    ]
}

/// Runs `command` in the game console, as the player would type it, such as
/// `cash 100`, and returns what it printed. [`Status::Rejected`], with what
/// it printed, when the game console refuses the command or it fails.
pub fn run(_: Caller, command: &str) -> GResult<String> {
    let m = game()?;
    context(m)?;
    let words = Words::new(command)?;
    let (succeeded, printed) = run_words(m, &words)?;
    let printed = game_text::to_utf8(&printed);
    if succeeded {
        Ok(printed)
    } else if printed.is_empty() {
        fail(Status::Rejected, "the command failed and printed nothing")
    } else {
        fail(Status::Rejected, printed)
    }
}

/// Runs the words through the game's own function: whether the command
/// succeeded, and what it printed.
fn run_words(m: &Memory, words: &Words) -> GResult<(bool, Vec<u8>)> {
    let console = m.call(m.at(GET), &[])?;
    if console == 0 {
        return fail(Status::Unavailable, "the game has no game console");
    }
    let mut answer = [0u64; result::SIZE / 8];
    let at = answer.as_mut_ptr() as usize;
    let vector = words.vector();
    m.call(m.at(RUN), &[console, at, vector.as_ptr() as usize]).map_err(|e| format!("the game console failed: {e}"))?;
    let succeeded = m.u8(at + result::SUCCEEDED)? != 0;
    let printed = game_string::read(m, at + result::TEXT);
    game_string::destroy_checked(m, at + result::TEXT)?;
    Ok((succeeded, printed?))
}

/// A command cut into words as the console window cuts a line, each a
/// `std::string` of game text, for the game to read but not change: the
/// strings live in the framework's memory, so the game must not free them.
struct Words {
    /// Each word's text, followed by a NUL, which the strings past
    /// [`game_string::INLINE`] point to: kept for them, never read.
    _texts: Vec<Vec<u8>>,
    strings: Vec<[usize; game_string::OBJECT / 8]>,
}

impl Words {
    /// The window trims the line, cuts it at `|` and runs the part before,
    /// writing what it printed to the file named after it; the framework
    /// answers what it printed instead, so it refuses a `|`.
    fn new(command: &str) -> GResult<Self> {
        if command.contains('|') {
            return fail(Status::InvalidArgument, "a | sends the game console's output to a file; remote control and extensions get it in the answer, so leave it out");
        }
        if command.contains('\0') {
            return fail(Status::InvalidArgument, "a command cannot hold a NUL");
        }
        let line = command.trim_matches(TRIMMED);
        if line.is_empty() {
            return fail(Status::InvalidArgument, "no command");
        }
        let texts: Vec<Vec<u8>> = cut(line)
            .map(|word| {
                let mut text = game_text::from_str(word);
                text.push(0);
                text
            })
            .collect();
        let strings = texts.iter().map(|text| string(&text[..text.len() - 1])).collect();
        Ok(Self { _texts: texts, strings })
    }

    /// The `std::vector`: where its strings begin, end, and its room ends.
    fn vector(&self) -> [usize; 3] {
        let range = self.strings.as_ptr_range();
        [range.start as usize, range.end as usize, range.end as usize]
    }
}

/// The words of a trimmed line: at every space, so two spaces in a row make
/// an empty word, as in the game.
fn cut(line: &str) -> impl Iterator<Item = &str> {
    line.split(' ')
}

/// A `std::string` holding `text`, inline when it fits, pointing to it
/// otherwise; `text` is followed by a NUL.
fn string(text: &[u8]) -> [usize; game_string::OBJECT / 8] {
    let mut string = game_string::empty();
    if text.len() <= game_string::INLINE {
        let bytes: &mut [u8; 16] = unsafe { &mut *(string.as_mut_ptr() as *mut [u8; 16]) };
        bytes[..text.len()].copy_from_slice(text);
        bytes[text.len()] = 0;
    } else {
        string[0] = text.as_ptr() as usize;
        string[game_string::CAPACITY / 8] = text.len();
    }
    string[game_string::SIZE / 8] = text.len();
    string
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(command: &str) -> Vec<Vec<u8>> {
        let m = Memory { base: 0 };
        let words = Words::new(command).unwrap_or_else(|e| panic!("{command:?}: {}", e.message));
        let [begin, end, room] = words.vector();
        assert_eq!(end, room);
        (begin..end).step_by(game_string::OBJECT).map(|at| game_string::read(&m, at).unwrap()).collect()
    }

    #[test]
    fn a_command_is_cut_into_words_as_the_window_cuts_a_line() {
        assert_eq!(words("cash 100"), [b"cash".to_vec(), b"100".to_vec()]);
        assert_eq!(words(" \tcash 100\r\n"), [b"cash".to_vec(), b"100".to_vec()]);
        assert_eq!(words("cash  100"), [b"cash".to_vec(), b"".to_vec(), b"100".to_vec()]);
        assert_eq!(words("help"), [b"help".to_vec()]);
        assert_eq!(words("--help"), [b"--help".to_vec()]);
    }

    #[test]
    fn words_are_game_text_ending_in_a_nul() {
        let long = "common/a_file_longer_than_fifteen_bytes.txt";
        let got = words(&format!("run {long} é 中"));
        assert_eq!(got, [b"run".to_vec(), long.as_bytes().to_vec(), vec![0xe9], "中".as_bytes().to_vec()]);
        let words = Words::new(&format!("run {long}")).unwrap();
        let long_string = words.strings[1];
        // Past the inline room, the string points to the text, NUL after it.
        assert_eq!(long_string[0], words._texts[1].as_ptr() as usize);
        assert_eq!(long_string[game_string::CAPACITY / 8], long.len());
        assert_eq!(words._texts[1].last(), Some(&0));
        // Inline, the NUL is in the string itself.
        let inline: [u8; 16] = unsafe { std::mem::transmute([words.strings[0][0], words.strings[0][1]]) };
        assert_eq!(&inline[..4], b"run\0");
        assert_eq!(words.strings[0][game_string::CAPACITY / 8], game_string::INLINE);
    }

    #[test]
    fn a_command_the_window_would_not_run_as_it_is_is_refused() {
        for (command, says) in [("", "no command"), (" \r\n", "no command"), ("cash 100 | out.txt", "a | sends"), ("cash\01", "NUL")] {
            let fail = Words::new(command).err().unwrap_or_else(|| panic!("{command:?} was taken"));
            assert_eq!(fail.status, Status::InvalidArgument);
            assert!(fail.message.contains(says), "{command:?}: {}", fail.message);
        }
    }
}
