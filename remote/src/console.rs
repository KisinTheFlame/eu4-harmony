//! The game console, where commands such as `cash` are typed.

group! {
    /// The game console.
    "console": ConsoleRequest, ConsoleAnswer;

    /// Runs a command in the game console, as the player would type it, and
    /// answers what it printed. The game's own checks apply: a command only
    /// developers may run, or almost any command in an ironman or
    /// multiplayer game, is refused, and so is one the game console does not
    /// know or that fails; the error is `rejected`, with what it printed.
    game Run "run" {
        /// The command and its arguments, such as `cash 100`; `help` lists
        /// the commands. A `|`, after which the console window writes the
        /// output to a file, is refused: the answer holds the output.
        command: String,
    } -> String, "what the command printed";
}
