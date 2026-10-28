//! Calling Rust from a naked stub: a jump point lands in the
//! middle of a game function, where any register may be live and the stack
//! may sit anywhere. Every stub calls Rust through [`call_rust!`], which:
//!
//! - keeps `rbp` and makes it a frame pointer: the game's `rbp` is at
//!   `[rbp]` and the game's `rsp` is `rbp + 8`;
//! - aligns `rsp` to 16 bytes, so the Rust it calls may use aligned SSE
//!   instructions (a misaligned call crashed the game only on the code paths
//!   that happened to use one);
//! - saves `rax`, `rcx`, `rdx`, `r8`–`r11` and `xmm0`–`xmm5`, the registers a
//!   call may change, and restores all of them afterwards.
//!
//! A stub gives the instructions that set up the arguments, the function to
//! call, and the instructions that use the result. Inside the frame:
//!
//! - `[rsp + 0x20]`..`[rsp + 0x38]` hold stack arguments 5 to 8;
//! - `[rsp + 0x40]` is a slot an argument may point at, for a value the
//!   function writes back (a length, say), all 8 bytes of it, so the stub
//!   may read it as a qword or a dword;
//! - after the call, `[rsp + 0x48]` holds the result.
//!
//! The "then" instructions run after every register is restored, so they
//! change exactly the registers they name; they read the result and the slot
//! from memory. Flags they set survive the frame's teardown.
//!
//! Calls into game code from a stub (`call qword ptr [...]`) do not go through
//! here: they run where the game's own code would, on the game's stack.

#[macro_export]
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
            "call {", stringify!($f), "}\n",
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
        // 1+2+3+4+5, plus the slot's 7, plus rbx 11 and r10 13.
        assert_eq!(probe(), 15 + 7 + 11 + 13);
    }
}
