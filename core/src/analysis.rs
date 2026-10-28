//! Reading the game's code: where a function ends, from the image's
//! exception directory, and how many arguments it takes, for the offline
//! check that every observe point forwards at least that many (an observer
//! forwards exactly the count its point declares, so a function that reads
//! more finds garbage there, and one that writes through an argument writes
//! somewhere it should not). Built for tests and with the `analysis` feature
//! (the `disasm` example, call tracing).
//!
//! [`values`] lists the numbers an instruction carries, so that a dependency
//! point can be checked to use the field or global it is said to.
//!
//! Register arguments count when the function reads `rcx`, `rdx`, `r8` or
//! `r9` before writing it, up to its first call. Stack arguments count when
//! the function addresses `[entry rsp + 0x28 + 8k]` through `rsp`, or through
//! `rbp` set up from `rsp` in its prologue. The prologue is followed exactly
//! (pushes, `sub rsp`, the stack probe, `mov rbp/rax, rsp`, `lea rbp, [rsp
//! + x]`); the body is read in address order within the function's bounds
//! from the exception directory.
use iced_x86::{Code, Decoder, DecoderOptions, Instruction, InstructionInfoFactory, Mnemonic, OpAccess, OpKind, Register};

fn u32_at(image: &[u8], at: usize) -> usize {
    u32::from_le_bytes(image[at..at + 4].try_into().unwrap()) as usize
}

/// `file`, a PE image, laid out as the loader would: headers and sections at
/// their RVAs.
pub fn map_file(file: &[u8]) -> Vec<u8> {
    let pe = u32_at(file, 0x3c);
    let optional = pe + 24;
    let mut image = vec![0u8; u32_at(file, optional + 56)];
    let headers = u32_at(file, optional + 60);
    image[..headers].copy_from_slice(&file[..headers]);
    let table = optional + u16::from_le_bytes([file[pe + 20], file[pe + 21]]) as usize;
    for i in 0..u16::from_le_bytes([file[pe + 6], file[pe + 7]]) as usize {
        let section = table + 40 * i;
        let (va, raw_size, raw) = (u32_at(file, section + 12), u32_at(file, section + 16), u32_at(file, section + 20));
        image[va..va + raw_size].copy_from_slice(&file[raw..raw + raw_size]);
    }
    image
}

/// Where the PE header starts.
pub fn pe(image: &[u8]) -> usize {
    u32_at(image, 0x3c)
}

/// Applies the image's base relocations as if it were loaded at `base`, so
/// that its pointers (vtables, for one) hold what they hold in the game.
pub fn relocate(image: &mut [u8], base: u64) {
    let optional = pe(image) + 24;
    let preferred = u64::from_le_bytes(image[optional + 24..optional + 32].try_into().unwrap());
    let delta = base.wrapping_sub(preferred);
    let directory = optional + 112 + 8 * 5;
    let (start, size) = (u32_at(image, directory), u32_at(image, directory + 4));
    let mut block = start;
    while block < start + size {
        let (page, length) = (u32_at(image, block), u32_at(image, block + 4));
        for entry in (block + 8..block + length).step_by(2) {
            let entry = u16::from_le_bytes([image[entry], image[entry + 1]]);
            // IMAGE_REL_BASED_DIR64; the rest are padding.
            if entry >> 12 == 10 {
                let at = page + (entry & 0xfff) as usize;
                let value = u64::from_le_bytes(image[at..at + 8].try_into().unwrap()).wrapping_add(delta);
                image[at..at + 8].copy_from_slice(&value.to_le_bytes());
            }
        }
        block += length;
    }
}

/// The image loaded at `base`, as far as its header says it reaches.
///
/// # Safety
/// `base` is a loaded module, and only the parts of it the loader mapped
/// readable are read through the slice (headers, exception directory, code).
pub unsafe fn loaded<'a>(base: usize) -> &'a [u8] {
    let size = *((base + *((base + 0x3c) as *const u32) as usize + 24 + 56) as *const u32);
    std::slice::from_raw_parts(base as *const u8, size as usize)
}

/// The bounds of the function starting at `rva`, from the image's exception
/// directory (the image as the loader lays it out).
pub fn bounds(image: &[u8], rva: usize) -> Option<std::ops::Range<usize>> {
    functions(image).find(|f| f.start == rva)
}

/// The numbers `instruction` carries: its memory operand's displacement, or
/// for a RIP-relative operand the RVA it reaches (when decoded at its RVA),
/// and its immediates.
pub fn values(instruction: &Instruction) -> Vec<u64> {
    let mut out = Vec::new();
    for (i, kind) in instruction.op_kinds().enumerate() {
        match kind {
            OpKind::Memory if instruction.is_ip_rel_memory_operand() => out.push(instruction.ip_rel_memory_address()),
            OpKind::Memory => out.push(instruction.memory_displacement64()),
            OpKind::Immediate8
            | OpKind::Immediate8_2nd
            | OpKind::Immediate16
            | OpKind::Immediate32
            | OpKind::Immediate64
            | OpKind::Immediate8to16
            | OpKind::Immediate8to32
            | OpKind::Immediate8to64
            | OpKind::Immediate32to64 => out.push(instruction.immediate(i as u32)),
            _ => {}
        }
    }
    out
}

/// Every instruction in `code`, decoded at `rva`.
pub fn instructions(code: &[u8], rva: usize) -> Vec<Instruction> {
    let mut decoder = Decoder::with_ip(64, code, rva as u64, DecoderOptions::NONE);
    let mut out = Vec::new();
    while decoder.can_decode() {
        out.push(decoder.decode());
    }
    out
}

/// The start and end of every function in the exception directory.
pub fn functions(image: &[u8]) -> impl Iterator<Item = std::ops::Range<usize>> + '_ {
    let pe = u32_at(image, 0x3c);
    let directory = pe + 24 + 112 + 8 * 3;
    let (start, size) = (u32_at(image, directory), u32_at(image, directory + 4));
    (start..start + size).step_by(12).map(move |e| u32_at(image, e)..u32_at(image, e + 4))
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
    use super::{arguments, instructions, values};

    #[test]
    fn values_are_displacements_targets_and_immediates() {
        // mov rax, [rcx + 0x468]; lea rdx, [rip + 0x10]; cmp dword ptr [rax + 0x20], 7
        let code = [0x48, 0x8b, 0x81, 0x68, 0x04, 0x00, 0x00, 0x48, 0x8d, 0x15, 0x10, 0x00, 0x00, 0x00, 0x83, 0x78, 0x20, 0x07];
        let found: Vec<Vec<u64>> = instructions(&code, 0x1000).iter().map(values).collect();
        assert_eq!(found, [vec![0x468], vec![0x1000 + 14 + 0x10], vec![0x20, 7]]);
    }

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
