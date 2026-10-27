//! How many arguments a game function takes, from its code: for the offline
//! check that every observe point forwards at least that many (an observer
//! forwards exactly the count its point declares, so a function that reads
//! more finds garbage there, and one that writes through an argument writes
//! somewhere it should not). Test-only.
//!
//! Register arguments count when the function reads `rcx`, `rdx`, `r8` or
//! `r9` before writing it, up to its first call. Stack arguments count when
//! the function addresses `[entry rsp + 0x28 + 8k]` through `rsp`, or through
//! `rbp` set up from `rsp` in its prologue. The prologue is followed exactly
//! (pushes, `sub rsp`, the stack probe, `mov rbp/rax, rsp`, `lea rbp, [rsp
//! + x]`); the body is read in address order within the function's bounds
//! from the exception directory.
use iced_x86::{Code, Decoder, DecoderOptions, Instruction, InstructionInfoFactory, Mnemonic, OpAccess, OpKind, Register};

/// The bounds of the function starting at `rva`, from the image's exception
/// directory (the image as the loader lays it out).
pub fn bounds(image: &[u8], rva: usize) -> Option<std::ops::Range<usize>> {
    let u32_at = |o: usize| u32::from_le_bytes(image[o..o + 4].try_into().unwrap()) as usize;
    let pe = u32_at(0x3c);
    let directory = pe + 24 + 112 + 8 * 3;
    let (start, size) = (u32_at(directory), u32_at(directory + 4));
    (start..start + size).step_by(12).map(|e| (u32_at(e), u32_at(e + 4))).find(|&(begin, _)| begin == rva).map(|(b, e)| b..e)
}

fn arg_index(register: Register) -> Option<u32> {
    match register.full_register() {
        Register::RCX => Some(1),
        Register::RDX => Some(2),
        Register::R8 => Some(3),
        Register::R9 => Some(4),
        _ => None,
    }
}

/// The number of arguments the function at `code` (loaded at `rva`) reads.
pub fn arguments(code: &[u8], rva: usize) -> u32 {
    let mut decoder = Decoder::with_ip(64, code, rva as u64, DecoderOptions::NONE);
    let mut info = InstructionInfoFactory::new();
    let mut needed = 0u32;
    // Entry-relative values of rsp and of a frame register, while known.
    let mut rsp: i64 = 0;
    let mut frame: Option<(Register, i64)> = None;
    let mut in_prologue = true;
    let mut probe_size: Option<i64> = None;
    let mut written = [false; 5];
    let mut before_first_call = true;
    let mut instruction = Instruction::default();
    while decoder.can_decode() {
        decoder.decode_out(&mut instruction);
        let used = info.info(&instruction);
        if before_first_call {
            for r in used.used_registers() {
                if let Some(i) = arg_index(r.register()) {
                    let reads = matches!(r.access(), OpAccess::Read | OpAccess::CondRead | OpAccess::ReadWrite | OpAccess::ReadCondWrite);
                    if reads && !written[i as usize] {
                        needed = needed.max(i);
                    }
                }
            }
            for r in used.used_registers() {
                if let Some(i) = arg_index(r.register()) {
                    if matches!(r.access(), OpAccess::Write | OpAccess::ReadWrite) {
                        written[i as usize] = true;
                    }
                }
            }
        }
        if instruction.mnemonic() == Mnemonic::Call {
            before_first_call = false;
        }
        // Memory operands through rsp or the frame register.
        if instruction.op_kinds().any(|k| k == OpKind::Memory) {
            let base = instruction.memory_base();
            let known = if base == Register::RSP {
                Some(rsp)
            } else {
                frame.filter(|&(r, _)| r == base).map(|(_, v)| v)
            };
            if let Some(at) = known {
                let offset = at + instruction.memory_displacement64() as i64;
                if offset >= 0x28 && instruction.memory_index() == Register::None {
                    needed = needed.max(5 + ((offset - 0x28) / 8) as u32);
                }
            }
        }
        if !in_prologue {
            continue;
        }
        match instruction.code() {
            Code::Push_r64 => rsp -= 8,
            Code::Sub_rm64_imm8 | Code::Sub_rm64_imm32 if instruction.op0_register() == Register::RSP => rsp -= instruction.immediate(1) as i64,
            Code::Mov_r32_imm32 if instruction.op0_register() == Register::EAX => probe_size = Some(instruction.immediate(1) as i64),
            Code::Sub_r64_rm64 | Code::Sub_rm64_r64 if instruction.op0_register() == Register::RSP && instruction.op1_register() == Register::RAX => {
                rsp -= probe_size.unwrap_or(0)
            }
            Code::Mov_r64_rm64 | Code::Mov_rm64_r64 if instruction.op1_register() == Register::RSP && instruction.op_kind(0) == OpKind::Register => {
                frame = Some((instruction.op0_register(), rsp))
            }
            Code::Lea_r64_m if instruction.op0_register() == Register::RBP => {
                let base = match instruction.memory_base() {
                    Register::RSP => Some(rsp),
                    other => frame.filter(|&(r, _)| r == other).map(|(_, v)| v),
                };
                frame = base.map(|v| (Register::RBP, v + instruction.memory_displacement64() as i64));
            }
            // The stack probe keeps the prologue going.
            Code::Call_rel32_64 => {}
            // So do spills and saves through rsp or the frame register, and
            // register moves.
            _ if instruction.op_kind(0) == OpKind::Memory
                && (instruction.memory_base() == Register::RSP || frame.is_some_and(|(r, _)| r == instruction.memory_base())) => {}
            _ if matches!(instruction.mnemonic(), Mnemonic::Mov | Mnemonic::Movzx) && instruction.op_kind(1) == OpKind::Register => {}
            _ => {
                in_prologue = false;
                // Only rbp stays meaningful in the body.
                frame = frame.filter(|&(r, _)| r == Register::RBP);
            }
        }
    }
    needed
}

#[cfg(test)]
mod tests {
    use super::arguments;

    #[test]
    fn counts_register_and_stack_arguments() {
        // mov eax, ecx; add eax, edx; ret
        assert_eq!(arguments(&[0x8b, 0xc1, 0x03, 0xc2, 0xc3], 0x1000), 2);
        // push rbx; sub rsp, 0x20; mov eax, [rsp + 0x50]; add rsp, 0x20; pop rbx; ret
        // The fifth argument is at entry rsp + 0x28 = rsp + 0x28 + 0x28 here.
        let code = [0x53, 0x48, 0x83, 0xec, 0x20, 0x8b, 0x44, 0x24, 0x50, 0x48, 0x83, 0xc4, 0x20, 0x5b, 0xc3];
        assert_eq!(arguments(&code, 0x1000), 5);
        // A home-slot spill of rcx is not a stack argument.
        // mov [rsp + 8], rcx; ret
        assert_eq!(arguments(&[0x48, 0x89, 0x4c, 0x24, 0x08, 0xc3], 0x1000), 1);
        // A byte spill before the pushes keeps the prologue followed:
        // mov [rsp + 0x18], r8b; push rbx; mov eax, [rsp + 0x30]; pop rbx; ret
        let code = [0x44, 0x88, 0x44, 0x24, 0x18, 0x53, 0x8b, 0x44, 0x24, 0x30, 0x5b, 0xc3];
        assert_eq!(arguments(&code, 0x1000), 5);
    }
}
