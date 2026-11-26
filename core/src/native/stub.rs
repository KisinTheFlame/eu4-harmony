//! Calling Rust from a naked stub: a jump point lands in the
//! middle of a game function, where any register may be live and the stack
//! may sit anywhere. Every stub calls Rust through [`call_rust!`], which:
//!
//! - keeps `rbp` and makes it a frame pointer: the game's `rbp` is at
//!   `[rbp]` and the entry `rsp` is `rbp + 8`;
//! - aligns `rsp` to 16 bytes, so the Rust it calls may use aligned SSE
//!   instructions (a misaligned call crashed the game only on the code paths
//!   that happened to use one);
//! - saves `rax`, `rcx`, `rdx`, `r8`–`r11` and `xmm0`–`xmm5`, the registers a
//!   call may change, and restores all of them afterwards;
//! - invokes the target via `eu4h_call_rust`, which emits x64 `.pdata` unwind
//!   information (`UWOP_PUSH_MACHFRAME`) pointing back to the jump point or
//!   caller so SEH exceptions and stack walks unwind cleanly across the stub.
//!
//! A stub gives the instructions that set up the arguments, the function to
//! call, and the instructions that use the result. Inside the frame:
//!
//! - `[rsp + 0x20]`..`[rsp + 0x40]` hold stack arguments 5 to 9;
//! - `[rsp + 0x40]` may also serve as an 8-byte slot an argument points at when
//!   fewer than 9 arguments are passed;
//! - after the call, `[rsp + 0x48]` holds the result.
//!
//! The "then" instructions run after every register is restored, so they
//! change exactly the registers they name; they read the result and the slot
//! from memory. Flags they set survive the frame's teardown.
//!
//! Calls into game code from a stub (`call qword ptr [...]`) do not go through
//! here: they run where the game's own code would, on the game's stack.

use crate::base::called_by_game;

unsafe extern "C" {
    fn eu4h_clear_stub_cache();
}

pub(crate) fn clear_unwind_cache() {
    unsafe { eu4h_clear_stub_cache() };
}

#[called_by_game]
#[unsafe(no_mangle)]
pub extern "C" fn eu4h_resolve_stub(return_address: usize) -> u32 {
    crate::native::points::jump_stub_index(return_address).unwrap_or(u32::MAX)
}

macro_rules! call_rust {
    ($($arg:literal,)* call $f:ident $(, $then:literal)* $(,)?) => {
        concat!(
            "push rbp\n",
            "mov rbp, rsp\n",
            "and rsp, -16\n",
            "sub rsp, 0xf0\n",
            "mov [rsp + 0x50], rax\n",
            "mov [rsp + 0x58], rcx\n",
            "mov [rsp + 0x60], rdx\n",
            "mov [rsp + 0x68], r8\n",
            "mov [rsp + 0x70], r9\n",
            "mov [rsp + 0x78], r10\n",
            "mov [rsp + 0x80], r11\n",
            "movdqa [rsp + 0x90], xmm0\n",
            "movdqa [rsp + 0xa0], xmm1\n",
            "movdqa [rsp + 0xb0], xmm2\n",
            "movdqa [rsp + 0xc0], xmm3\n",
            "movdqa [rsp + 0xd0], xmm4\n",
            "movdqa [rsp + 0xe0], xmm5\n",
            $($arg, "\n",)*
            "lea rax, [rip + {", stringify!($f), "}]\n",
            "call eu4h_call_rust\n",
            "mov [rsp + 0x48], rax\n",
            "mov rax, [rsp + 0x50]\n",
            "mov rcx, [rsp + 0x58]\n",
            "mov rdx, [rsp + 0x60]\n",
            "mov r8, [rsp + 0x68]\n",
            "mov r9, [rsp + 0x70]\n",
            "mov r10, [rsp + 0x78]\n",
            "mov r11, [rsp + 0x80]\n",
            "movdqa xmm0, [rsp + 0x90]\n",
            "movdqa xmm1, [rsp + 0xa0]\n",
            "movdqa xmm2, [rsp + 0xb0]\n",
            "movdqa xmm3, [rsp + 0xc0]\n",
            "movdqa xmm4, [rsp + 0xd0]\n",
            "movdqa xmm5, [rsp + 0xe0]\n",
            $($then, "\n",)*
            "mov rsp, rbp\n",
            "pop rbp\n",
        )
    };
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    /// Every stub's call into Rust goes through `call_rust!`: a stub that
    /// writes `call {name}` itself is a frame someone worked out by hand.
    #[test]
    fn stubs_call_rust_only_through_the_shared_frame() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut stack = vec![src];
        let mut found = Vec::new();
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") && !path.ends_with("stub.rs") {
                    let text = std::fs::read_to_string(&path).unwrap();
                    for (n, line) in text.lines().enumerate() {
                        if line.trim_start().starts_with("\"call {") {
                            found.push(format!("{}:{}", path.display(), n + 1));
                        }
                    }
                }
            }
        }
        assert!(found.is_empty(), "hand-written calls into Rust: {found:?}");
    }

    /// `call_rust!` restores the caller's `rax` before exiting; testing `al`/`eax`/`rax`
    /// right after a `call_rust!` that did not read `[rsp + 0x48]` checks stale register
    /// contents instead of the Rust return value.
    #[test]
    fn stubs_do_not_test_rax_after_call_rust_without_reading_return_slot() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut stack = vec![src];
        let mut bad = Vec::new();
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") && !path.ends_with("stub.rs") {
                    let text = std::fs::read_to_string(&path).unwrap();
                    let lines: Vec<&str> = text.lines().collect();
                    let mut i = 0;
                    while i < lines.len() {
                        if lines[i].contains("call_rust!(") {
                            let start = i;
                            let mut block = String::new();
                            while i < lines.len() {
                                block.push_str(lines[i]);
                                if lines[i].contains("),") {
                                    break;
                                }
                                i += 1;
                            }
                            if !block.contains("[rsp + 0x48]") && i + 1 < lines.len() {
                                let next = lines[i + 1].trim();
                                if next.starts_with("\"test al,") || next.starts_with("\"test eax,") || next.starts_with("\"test rax,") {
                                    bad.push(format!("{}:{}: {next}", path.display(), start + 1));
                                }
                            }
                        }
                        i += 1;
                    }
                }
            }
        }
        assert!(bad.is_empty(), "stub tests restored rax instead of [rsp + 0x48]: {bad:?}");
    }

    extern "C" fn sum(a: usize, b: usize, c: usize, d: usize, e: usize, out: *mut u32) -> usize {
        // Fails on a misaligned stack: the SSE store needs 16 bytes.
        let probe = std::hint::black_box([0u128; 1]);
        unsafe { *out = 7 };
        a + b + c + d + e + probe[0] as usize
    }

    /// Calls `sum(1, 2, 3, 4, 5, &slot)` from an odd stack, then returns
    /// result + slot in rax, with rbx (callee-saved, untouched) and r10
    /// (volatile, restored) as they were.
    #[unsafe(naked)]
    extern "C" fn probe() -> usize {
        std::arch::naked_asm!(
            "push rbx",
            "mov rbx, 11",
            "mov r10, 13",
            call_rust!(
                "mov rcx, 1",
                "mov rdx, 2",
                "mov r8, 3",
                "mov r9, 4",
                "mov qword ptr [rsp + 0x20], 5",
                "lea rax, [rsp + 0x40]",
                "mov [rsp + 0x28], rax",
                call sum,
                "mov rax, [rsp + 0x48]",
                "add eax, dword ptr [rsp + 0x40]",
            ),
            "add rax, rbx",
            "add rax, r10",
            "pop rbx",
            "ret",
            sum = sym sum,
        );
    }

    #[test]
    fn the_frame_passes_arguments_and_keeps_registers() {
        // 1+2+3+4+5, plus the slot's 7, plus rbx 11 and r10 13, then 9-arg
        // stack passing and xmm0/xmm1 float arguments.
        assert_eq!(probe(), 15 + 7 + 11 + 13);
        assert_eq!(nine_probe(), (1..=9).sum::<usize>() + 0x4120_0000usize);
    }

    extern "C" fn nine_sum(a: usize, b: usize, c: usize, d: usize, e: usize, f: usize, g: usize, h: usize, i: usize) -> usize {
        a + b + c + d + e + f + g + h + i
    }

    extern "C" fn float_bits(a: f32, b: f32) -> u32 {
        (a + b).to_bits()
    }

    #[unsafe(naked)]
    extern "C" fn nine_probe() -> usize {
        std::arch::naked_asm!(
            call_rust!(
                "mov rcx, 1",
                "mov rdx, 2",
                "mov r8, 3",
                "mov r9, 4",
                "mov qword ptr [rsp + 0x20], 5",
                "mov qword ptr [rsp + 0x28], 6",
                "mov qword ptr [rsp + 0x30], 7",
                "mov qword ptr [rsp + 0x38], 8",
                "mov qword ptr [rsp + 0x40], 9",
                call nine_sum,
                "mov r10, [rsp + 0x48]",
            ),
            call_rust!(
                "mov eax, 0x40a00000",
                "movd xmm0, eax",
                "movd xmm1, eax",
                call float_bits,
                "mov eax, dword ptr [rsp + 0x48]",
            ),
            "add rax, r10",
            "ret",
            nine_sum = sym nine_sum,
            float_bits = sym float_bits,
        );
    }

    static SEH_FRAMES: std::sync::Mutex<Vec<u64>> = std::sync::Mutex::new(Vec::new());

    extern "C" fn seh_probe(trigger: usize) -> usize {
        unsafe extern "C" {
            fn eu4h_test_access_violation() -> usize;
        }
        let ctx = crate::base::stack::Context::capture();
        let mut frames = [0u64; 16];
        crate::base::stack::walk(&ctx.0, &mut frames);
        *SEH_FRAMES.lock().unwrap_or_else(|e| e.into_inner()) = frames.into_iter().take_while(|&f| f != 0).collect();
        if trigger != 0 {
            unsafe { eu4h_test_access_violation() };
        }
        123
    }

    #[unsafe(naked)]
    extern "C" fn seh_called_stub(_trigger: usize) -> usize {
        std::arch::naked_asm!(
            call_rust!("mov rcx, rcx", call seh_probe, "mov rax, [rsp + 0x48]"),
            "ret",
            seh_probe = sym seh_probe,
        );
    }

    #[inline(never)]
    extern "C" fn call_seh_stub(trigger: usize) -> usize {
        std::hint::black_box(seh_called_stub(trigger))
    }

    #[test]
    fn called_stub_unwinds_stack_and_hardware_exceptions() {
        let m = crate::native::memory::Memory { base: 0 };
        assert_eq!(m.call(call_seh_stub as *const () as usize, &[0]).unwrap(), 123);
        let frames = SEH_FRAMES.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let caller_start = call_seh_stub as *const () as u64;
        assert!(frames.iter().any(|&ip| (caller_start..caller_start + 0x80).contains(&ip)), "expected caller frame near {caller_start:#x}: {frames:x?}");

        let err = m.call(call_seh_stub as *const () as usize, &[1]).unwrap_err();
        assert!(err.contains("c0000005"), "{err}");
    }
}
