//! Which form the catalogue's values are written in. Remote clients read
//! JSON, where an army's ID is a decimal string and a date `"1444.11.11"`;
//! extensions read Lua values, where an army's ID is an integer and a date a
//! table of its year, month and day (ADR one-capability-catalogue). One answer
//! serves both: the Lua host writes it inside [`as_lua`].
use std::cell::Cell;

thread_local! {
    static LUA: Cell<bool> = const { Cell::new(false) };
}

/// Runs `write` with values written in Lua's form on this thread.
pub fn as_lua<R>(write: impl FnOnce() -> R) -> R {
    let was = LUA.with(|l| l.replace(true));
    let result = write();
    LUA.with(|l| l.set(was));
    result
}

/// Whether values are written in Lua's form here.
pub fn lua() -> bool {
    LUA.with(Cell::get)
}
