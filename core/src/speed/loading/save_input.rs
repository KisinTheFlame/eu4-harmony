//! A local text save read once, then parsed through the game's memory stream.
//! The original stream and its file remain owned by the native in-game load;
//! only its save description's borrowed input changes during `0x74b8a0`.
//! See docs/game/program/saves.md, "File position queries while parsing text".
use crate::{
    base::called_by_game,
    base::Result,
    native::memory::Memory,
    native::objects::stream,
    native::points::{GameFunction, Point},
    native::speed_up::SpeedUp,
};

const LOAD: GameFunction<2> = GameFunction::at(0x74b8a0);
const MAKE_STREAM: GameFunction<8> = GameFunction::at(0x171ebc0);
use crate::native::objects::stream::FILE_VTABLE as FILE_STREAM;
const FILE_HOLDER: usize = 0x38;
const ZIPPED: usize = 0x28;
const LOAD_CALL: usize = 0x5d0c78;
const LOAD_NEXT: usize = 0x5d0c7d;

pub const SPEED_UP: SpeedUp = SpeedUp::new("save-input", "loading", points, false);

fn points() -> Vec<Point> {
    vec![
        Point::jump(LOAD_CALL, "e8 23 ac 17 00", "in-game load reads a text save from memory", load_stub as *const () as usize, &LOAD_RESUME),
        Point::keep(LOAD_NEXT, "90 48 8b 4c 24 78", "in-game load releases its entry streams"),
        Point::keep(LOAD, "48 8b c4 53 56 57 48 83 ec 60 48 8b fa 48 8b f1", "world loads a save description"),
        Point::keep(0x74ba03usize, "48 c7 86 c8 74 00 00 00 00 00 00", "world stops borrowing the save description"),
        Point::keep(0x5ccff2usize, "48 89 11 33 c0 48 89 41 08 48 89 41 10 48 89 41 18 48 89 41 20 88 41 28", "save description borrows its source stream")
            .uses(&[ZIPPED]),
        Point::method(FILE_STREAM, 0x80, 0x1710c10, "file stream position"),
        Point::method(FILE_STREAM, 0, 0x1710510, "file stream destructor"),
        Point::keep(0x1710c10usize, "48 83 ec 28 48 8b 49 38 48 8b 09 e8 00 80 2e 00", "file stream's file holder").uses(&[FILE_HOLDER]),
        Point::keep(MAKE_STREAM, "4c 8b dc 49 89 5b 08 49 89 6b 10 49 89 73 18 49 89 7b 20", "memory stream constructor"),
    ]
}

#[repr(align(8))]
struct Stream([u8; stream::SIZE]);

struct Input {
    // The native stream borrows this allocation, stable when Input moves.
    _bytes: Vec<u8>,
    stream: Stream,
    original: usize,
}

impl Input {
    fn prepare(m: &Memory, save: usize) -> Result<Option<Self>> {
        if m.u8(save + ZIPPED)? != 0 {
            return Ok(None);
        }
        let original = m.ptr(save)?;
        if m.ptr(original)? != m.at(FILE_STREAM) {
            return Ok(None);
        }
        let file = m.ptr(m.ptr(original + FILE_HOLDER)?)?;
        let Some(bytes) = crate::native::physfs::text_snapshot(file)? else { return Ok(None) };
        let at = m.call(m.vslot(original, 0x80)?, &[original])?;
        if at > bytes.len() {
            return Ok(None);
        }
        let mut stream = Stream([0; stream::SIZE]);
        MAKE_STREAM.call(m, [stream.0.as_mut_ptr() as usize, bytes.as_ptr() as usize, bytes.len(), 0, 1, 0, 0, 0])?;
        // Borrow the common base, including its name, character, put-back
        // flag and line number. Neither name nor buffers are ours to free
        // through a native destructor; only Rust's byte allocation is owned.
        m.read_into(original + stream::LINE, &mut stream.0[stream::LINE..stream::GROWING])?;
        stream.0[stream::AT..stream::AT + 4].copy_from_slice(&(at as u32).to_le_bytes());
        Ok(Some(Self { _bytes: bytes, stream, original }))
    }
}

#[called_by_game]
extern "C" fn load(world: usize, save: usize) -> usize {
    crate::base::guarded("text save input", || {
        let m = crate::native::hooked_memory();
        let mut input = match Input::prepare(&m, save) {
            Ok(input) => input,
            Err(e) => {
                log!("loading: using the original save input: {e}");
                None
            }
        };
        let result = match input.as_mut() {
            None => LOAD.call(&m, [world, save]),
            Some(input) => {
                // The description is borrowed by this synchronous call.
                // Its entry streams are null for plain saves, its source
                // is restored before the native caller destroys anything.
                m.write(save, &(input.stream.0.as_mut_ptr() as usize).to_le_bytes()).expect("the save description must be writable");
                let result = LOAD.call(&m, [world, save]);
                m.write(save, &input.original.to_le_bytes()).expect("the borrowed save input must be restored");
                result
            }
        };
        result.unwrap_or_else(|e| {
            log!("loading: the world could not load the save: {e}");
            0
        })
    })
}

resume!(LOAD_RESUME: LOAD_CONTINUE = LOAD_NEXT);

#[unsafe(naked)]
extern "C" fn load_stub() {
    std::arch::naked_asm!(
        call_rust!(call load -> rax),
        "jmp qword ptr [rip + {next}]",
        load = sym load,
        next = sym LOAD_CONTINUE,
    );
}
