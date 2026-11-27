//! Reading the game's code: where a function ends, from the image's
//! exception directory, and how many arguments it takes, for the offline
//! check that every observe point forwards at least that many (an observer
//! forwards exactly the count its point declares, so a function that reads
//! more finds garbage there, and one that writes through an argument writes
//! somewhere it should not). Shared by `cargo xtask disasm`, the core's
//! offline checks, call tracing and write watching. This crate does not
//! depend on the core or on a running game.
//!
//! [`values`] lists the numbers an instruction carries, so that a dependency
//! point can be checked to use the field or global it is said to.
//!
//! Register arguments count when the function reads `rcx`, `rdx`, `r8` or
//! `r9` before writing it, up to its first call. Stack arguments count when
//! the function addresses `[entry rsp + 0x28 + 8k]` through `rsp`, or through
//! `rbp` set up from `rsp` in its prologue. The prologue is followed exactly
//! (pushes, `sub rsp`, the stack probe, `mov rbp/rax, rsp`,
//! `lea rbp, [rsp + x]`); the body is read in address order within the
//! function's bounds from the exception directory.
use iced_x86::{Code, Decoder, DecoderOptions, Instruction, InstructionInfoFactory, Mnemonic, OpAccess, OpKind, Register};
use std::ops::Range;

/// `UNW_FLAG_CHAININFO`: the unwind data continues another runtime function's.
pub const UNW_FLAG_CHAININFO: u8 = 4;

fn checked_u16(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn checked_u32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn u32_at(image: &[u8], at: usize) -> usize {
    u32::from_le_bytes(image[at..at + 4].try_into().unwrap()) as usize
}

/// One section header of a PE image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Section {
    pub name: [u8; 8],
    pub rva: u32,
    pub virtual_size: u32,
    pub raw_size: u32,
    pub raw_offset: u32,
}

impl Section {
    /// Span of RVAs covered by this section.
    pub fn rva_size(self) -> u32 {
        self.virtual_size.max(self.raw_size)
    }
}

/// One named export of a PE image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Export {
    pub name: String,
    pub ordinal: u32,
    pub rva: u32,
}

/// A checked view over a PE64 image, either as an unmapped file on disk
/// ([`PeImage::from_file`]) or already laid out at its RVAs
/// ([`PeImage::from_mapped`]).
#[derive(Clone, Debug, Default)]
pub struct PeImage {
    data: Vec<u8>,
    pe_offset: usize,
    sections: Vec<Section>,
    mapped: bool,
}

impl PeImage {
    /// Parses an unmapped PE64 file on disk, translating RVAs through its
    /// section table without allocating a full virtual image.
    pub fn from_file(data: Vec<u8>) -> Option<Self> {
        let (pe_offset, sections) = parse_headers(&data)?;
        Some(Self { data, pe_offset, sections, mapped: false })
    }

    /// Wraps a PE64 image already laid out at its RVAs (for example from
    /// [`map_file`]).
    pub fn from_mapped(data: Vec<u8>) -> Option<Self> {
        let (pe_offset, sections) = parse_headers(&data)?;
        Some(Self { data, pe_offset, sections, mapped: true })
    }

    /// Constructs a synthetic image directly from backing bytes and explicit
    /// section mappings, for unit tests that build minimal unwind tables.
    pub fn from_sections(data: Vec<u8>, sections: Vec<Section>) -> Self {
        Self { data, pe_offset: 0, sections, mapped: false }
    }

    /// Reads `len` bytes starting at `rva`, returning `None` if out of bounds.
    pub fn at(&self, rva: u32, len: usize) -> Option<&[u8]> {
        let offset = self.rva_to_offset(rva)?;
        self.data.get(offset..offset.checked_add(len)?)
    }

    /// Reads a NUL-terminated string starting at `rva`.
    pub fn cstr(&self, rva: u32) -> Option<&[u8]> {
        let offset = self.rva_to_offset(rva)?;
        let tail = self.data.get(offset..)?;
        let end = tail.iter().position(|&b| b == 0)?;
        Some(&tail[..end])
    }

    fn rva_to_offset(&self, rva: u32) -> Option<usize> {
        if self.mapped {
            return Some(rva as usize);
        }
        let section = self.sections.iter().find(|s| {
            let end = s.rva.saturating_add(s.rva_size());
            s.rva <= rva && rva < end
        })?;
        let offset = section.raw_offset.checked_add(rva - section.rva)?;
        Some(offset as usize)
    }

    /// Returns `(rva, size)` of PE32+ data directory `index` (0 = export,
    /// 3 = exception, 5 = base relocation).
    pub fn data_directory(&self, index: usize) -> Option<(u32, u32)> {
        let dir = self.pe_offset.checked_add(24 + 112)?.checked_add(index.checked_mul(8)?)?;
        let rva = checked_u32(&self.data, dir)?;
        let size = checked_u32(&self.data, dir + 4)?;
        Some((rva, size))
    }

    /// Returns the RVA bounds `start..end` of the first section whose 8-byte
    /// name starts with `prefix` (for example `b".text"`).
    pub fn section_bounds(&self, prefix: &[u8]) -> Option<Range<u32>> {
        let section = self.sections.iter().find(|s| s.name.starts_with(prefix))?;
        Some(section.rva..section.rva.saturating_add(section.virtual_size))
    }

    /// Lists every named export with its name, ordinal, and RVA.
    pub fn exports(&self) -> Vec<Export> {
        let Some((directory, _)) = self.data_directory(0) else { return Vec::new() };
        if directory == 0 {
            return Vec::new();
        }
        let Some(head) = self.at(directory, 40) else { return Vec::new() };
        let (Some(base), Some(count), Some(functions), Some(name_rvas), Some(ordinals)) =
            (checked_u32(head, 16), checked_u32(head, 24), checked_u32(head, 28), checked_u32(head, 32), checked_u32(head, 36))
        else {
            return Vec::new();
        };
        let mut out = Vec::with_capacity(count as usize);
        for i in 0..count {
            let Some(name_rva) = self.at(name_rvas.wrapping_add(4 * i), 4).and_then(|b| checked_u32(b, 0)) else { continue };
            let Some(name_bytes) = self.cstr(name_rva) else { continue };
            let Some(ordinal_index) = self.at(ordinals.wrapping_add(2 * i), 2).and_then(|b| checked_u16(b, 0)) else { continue };
            let Some(rva) = self.at(functions.wrapping_add(4 * u32::from(ordinal_index)), 4).and_then(|b| checked_u32(b, 0)) else {
                continue;
            };
            out.push(Export { name: String::from_utf8_lossy(name_bytes).into_owned(), ordinal: base.wrapping_add(u32::from(ordinal_index)), rva });
        }
        out
    }

    /// Returns the RVA of the named export `name`, if present.
    pub fn export(&self, name: &str) -> Option<u32> {
        let (directory, _) = self.data_directory(0)?;
        if directory == 0 {
            return None;
        }
        let head = self.at(directory, 40)?;
        let (count, functions, name_rvas, ordinals) = (checked_u32(head, 24)?, checked_u32(head, 28)?, checked_u32(head, 32)?, checked_u32(head, 36)?);
        let needle = name.as_bytes();
        for i in 0..count {
            let name_rva = checked_u32(self.at(name_rvas.wrapping_add(4 * i), 4)?, 0)?;
            if self.cstr(name_rva)? == needle {
                let ordinal = u32::from(checked_u16(self.at(ordinals.wrapping_add(2 * i), 2)?, 0)?);
                return checked_u32(self.at(functions.wrapping_add(4 * ordinal), 4)?, 0);
            }
        }
        None
    }
}

fn parse_headers(data: &[u8]) -> Option<(usize, Vec<Section>)> {
    if data.get(0..2) != Some(b"MZ") {
        return None;
    }
    let pe = checked_u32(data, 0x3c)? as usize;
    if data.get(pe..pe + 4) != Some(b"PE\0\0") {
        return None;
    }
    let count = usize::from(checked_u16(data, pe + 6)?);
    let optional = usize::from(checked_u16(data, pe + 20)?);
    // Require PE32+ optional header (magic 0x20b) so data directories sit at +112.
    if checked_u16(data, pe + 24)? != 0x20b || optional < 112 {
        return None;
    }
    let table = pe.checked_add(24)?.checked_add(optional)?;
    let mut sections = Vec::with_capacity(count);
    for i in 0..count {
        let s = table.checked_add(40 * i)?;
        let name: [u8; 8] = data.get(s..s + 8)?.try_into().ok()?;
        let virtual_size = checked_u32(data, s + 8)?;
        let rva = checked_u32(data, s + 12)?;
        let raw_size = checked_u32(data, s + 16)?;
        let raw_offset = checked_u32(data, s + 20)?;
        sections.push(Section { name, rva, virtual_size, raw_size, raw_offset });
    }
    Some((pe, sections))
}

/// One `.pdata` runtime-function entry with its resolved primary function RVA.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExceptionEntry {
    pub begin: u32,
    pub end: u32,
    pub unwind: u32,
    /// Entry RVA of the primary function this chunk belongs to (following any
    /// `UNW_FLAG_CHAININFO` chain).
    pub primary: u32,
}

/// A PE64 image's exception directory (`.pdata`), sorted by [`ExceptionEntry::begin`],
/// with every `UNW_FLAG_CHAININFO` chunk resolved to its primary function entry.
#[derive(Clone, Debug, Default)]
pub struct ExceptionTable {
    entries: Vec<ExceptionEntry>,
}

impl ExceptionTable {
    /// Parses the exception directory from `image` (unmapped or mapped).
    pub fn from_image(image: &PeImage) -> Option<Self> {
        let (directory, size) = image.data_directory(3)?;
        if directory == 0 || size == 0 {
            return Some(Self::default());
        }
        let table = image.at(directory, size as usize)?;
        let mut entries = Vec::with_capacity(table.len() / 12);
        for chunk in table.chunks_exact(12) {
            let begin = checked_u32(chunk, 0)?;
            let end = checked_u32(chunk, 4)?;
            let unwind = checked_u32(chunk, 8)?;
            if begin == 0 && end == 0 {
                continue;
            }
            let primary = resolve_primary(|rva, len| image.at(rva, len), begin, unwind).unwrap_or(begin);
            entries.push(ExceptionEntry { begin, end, unwind, primary });
        }
        entries.sort_by_key(|e| (e.begin, e.end));
        Some(Self { entries })
    }

    /// Parses the exception directory directly from a mapped image slice
    /// (such as the output of [`map_file`] or [`loaded`]).
    pub fn from_mapped(mapped: &[u8]) -> Option<Self> {
        let (pe, _) = parse_headers(mapped)?;
        let dir = pe.checked_add(24 + 112 + 3 * 8)?;
        let directory = checked_u32(mapped, dir)?;
        let size = checked_u32(mapped, dir + 4)?;
        if directory == 0 || size == 0 {
            return Some(Self::default());
        }
        let start = directory as usize;
        let end = start.checked_add(size as usize)?;
        let table = mapped.get(start..end)?;
        let mut entries = Vec::with_capacity(table.len() / 12);
        for chunk in table.chunks_exact(12) {
            let begin = checked_u32(chunk, 0)?;
            let end = checked_u32(chunk, 4)?;
            let unwind = checked_u32(chunk, 8)?;
            if begin == 0 && end == 0 {
                continue;
            }
            let primary = resolve_primary(|rva, len| mapped.get(rva as usize..(rva as usize).checked_add(len)?), begin, unwind).unwrap_or(begin);
            entries.push(ExceptionEntry { begin, end, unwind, primary });
        }
        entries.sort_by_key(|e| (e.begin, e.end));
        Some(Self { entries })
    }

    /// Constructs a table directly from explicit entries, sorting by `begin`.
    pub fn from_entries(mut entries: Vec<ExceptionEntry>) -> Self {
        entries.sort_by_key(|e| (e.begin, e.end));
        Self { entries }
    }

    /// Returns the runtime-function entry covering `rva`, or `None` for a leaf
    /// function or non-code address.
    pub fn entry(&self, rva: u32) -> Option<ExceptionEntry> {
        let i = self.entries.partition_point(|e| e.begin <= rva).checked_sub(1)?;
        let entry = self.entries[i];
        (rva < entry.end).then_some(entry)
    }

    /// Returns the primary function entry RVA covering `rva` (following
    /// `UNW_FLAG_CHAININFO`), or `None` when `rva` has no `.pdata` entry.
    pub fn primary(&self, rva: u32) -> Option<u32> {
        self.entry(rva).map(|e| e.primary)
    }

    /// All entries in ascending `begin` order.
    pub fn entries(&self) -> &[ExceptionEntry] {
        &self.entries
    }
}

/// Follows `UNW_FLAG_CHAININFO` records from `(start, unwind)` to the primary
/// function's begin RVA, bounded to 32 hops against cyclic/corrupt tables.
fn resolve_primary<'a>(read: impl Fn(u32, usize) -> Option<&'a [u8]>, start: u32, unwind: u32) -> Option<u32> {
    let (mut at, mut info) = (start, unwind & !1);
    for _ in 0..32 {
        let head = read(info, 4)?;
        if (head[0] >> 3) & UNW_FLAG_CHAININFO == 0 {
            break;
        }
        let codes = u32::from(head[2]);
        let chained_rva = info.checked_add(4)?.checked_add(2 * (codes + (codes & 1)))?;
        let chained = read(chained_rva, 12)?;
        at = checked_u32(chained, 0)?;
        info = checked_u32(chained, 8)? & !1;
    }
    Some(at)
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

/// The address the image is built to be loaded at, which its pointers hold
/// before relocation.
pub fn preferred_base(image: &[u8]) -> u64 {
    let optional = pe(image) + 24;
    u64::from_le_bytes(image[optional + 24..optional + 32].try_into().unwrap())
}

/// Each section's RVAs, and whether it holds code.
pub fn sections(image: &[u8]) -> impl Iterator<Item = (std::ops::Range<usize>, bool)> + '_ {
    const EXECUTE: usize = 0x2000_0000;
    let pe = pe(image);
    let table = pe + 24 + u16::from_le_bytes([image[pe + 20], image[pe + 21]]) as usize;
    let count = u16::from_le_bytes([image[pe + 6], image[pe + 7]]) as usize;
    (0..count).map(move |i| {
        let section = table + 40 * i;
        let (size, va) = (u32_at(image, section + 8), u32_at(image, section + 12));
        (va..(va + size).min(image.len()), u32_at(image, section + 36) & EXECUTE != 0)
    })
}

/// Where the image holds a pointer of its own: each place its base
/// relocations adjust, an exact list where scanning the data for values
/// would also find numbers that only look like one.
pub fn pointers(image: &[u8]) -> Vec<usize> {
    let directory = pe(image) + 24 + 112 + 8 * 5;
    let (start, size) = (u32_at(image, directory), u32_at(image, directory + 4));
    let mut out = Vec::new();
    let mut block = start;
    while block < start + size {
        let (page, length) = (u32_at(image, block), u32_at(image, block + 4));
        for entry in (block + 8..block + length).step_by(2) {
            let entry = u16::from_le_bytes([image[entry], image[entry + 1]]);
            // IMAGE_REL_BASED_DIR64; the rest are padding.
            if entry >> 12 == 10 {
                out.push(page + (entry & 0xfff) as usize);
            }
        }
        block += length;
    }
    out
}

/// Applies the image's base relocations as if it were loaded at `base`, so
/// that its pointers (vtables, for one) hold what they hold in the game.
pub fn relocate(image: &mut [u8], base: u64) {
    let delta = base.wrapping_sub(preferred_base(image));
    for at in pointers(image) {
        let value = u64::from_le_bytes(image[at..at + 8].try_into().unwrap()).wrapping_add(delta);
        image[at..at + 8].copy_from_slice(&value.to_le_bytes());
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
/// directory (the image as the loader lays it out). The compiler splits some
/// functions into parts, each its own entry: a part whose unwind information
/// is chained to the one before (`UNW_FLAG_CHAININFO`) and that begins where
/// it ends is the same function's, as `0x10a6d00`'s prologue is followed by
/// its body from `0x10a6d11`, and is taken in.
pub fn bounds(image: &[u8], rva: usize) -> Option<std::ops::Range<usize>> {
    const CHAINED: u8 = 4;
    let mut entries = entries(image).skip_while(|e| e.0 != rva);
    let (start, mut end, _) = entries.next()?;
    for (next, next_end, unwind) in entries {
        if next != end || image.get(unwind).is_none_or(|&b| b >> 3 & CHAINED == 0) {
            break;
        }
        end = next_end;
    }
    Some(start..end)
}

/// Each entry of the exception directory: start, end and unwind information.
fn entries(image: &[u8]) -> impl Iterator<Item = (usize, usize, usize)> + '_ {
    let pe = u32_at(image, 0x3c);
    let directory = pe + 24 + 112 + 8 * 3;
    let (start, size) = (u32_at(image, directory), u32_at(image, directory + 4));
    (start..start + size).step_by(12).map(move |e| (u32_at(image, e), u32_at(image, e + 4), u32_at(image, e + 8)))
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
    entries(image).map(|(start, end, _)| start..end)
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
        // The stack probe, called in the prologue with the frame's size in
        // eax, keeps every register but rax, r10 and r11: the arguments are
        // still there after it.
        let probe = in_prologue && probe_size.is_some() && instruction.code() == Code::Call_rel32_64;
        if instruction.mnemonic() == Mnemonic::Call && !probe {
            before_first_call = false;
        }
        // Memory operands through rsp or the frame register.
        if instruction.op_kinds().any(|k| k == OpKind::Memory) {
            let base = instruction.memory_base();
            let known = if base == Register::RSP { Some(rsp) } else { frame.filter(|&(r, _)| r == base).map(|(_, v)| v) };
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
    use super::{arguments, bounds, instructions, map_file, values, ExceptionTable, Export, PeImage, UNW_FLAG_CHAININFO};

    fn write_u16(buf: &mut [u8], at: usize, value: u16) {
        buf[at..at + 2].copy_from_slice(&value.to_le_bytes());
    }

    fn write_u32(buf: &mut [u8], at: usize, value: u32) {
        buf[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn write_u64(buf: &mut [u8], at: usize, value: u64) {
        buf[at..at + 8].copy_from_slice(&value.to_le_bytes());
    }

    /// Builds a valid minimal PE64 file with `.text`, `.rdata`, `.pdata`, and
    /// `.edata` sections containing:
    /// - primary function A (`0x1000..0x1040`)
    /// - contiguous chained chunk A1 (`0x1040..0x1060` -> A)
    /// - unrelated primary function B (`0x1100..0x1140`)
    /// - non-adjacent cold chained chunk A2 (`0x1200..0x1230` -> A1 -> A)
    /// - leaf function C (`0x1300..0x1310`, absent from `.pdata`)
    /// - two exports with ordinal base 10: `alpha` -> `0x1000`, `beta` -> `0x1100`
    fn synthetic_pe() -> Vec<u8> {
        let mut file = vec![0u8; 0x1000];
        // DOS header
        file[0..2].copy_from_slice(b"MZ");
        let pe = 0x80usize;
        write_u32(&mut file, 0x3c, pe as u32);
        // PE signature + COFF header (4 sections, 0xf0 optional header)
        file[pe..pe + 4].copy_from_slice(b"PE\0\0");
        write_u16(&mut file, pe + 4, 0x8664);
        write_u16(&mut file, pe + 6, 4);
        write_u16(&mut file, pe + 20, 0xf0);
        // Optional header (PE32+)
        let opt = pe + 24;
        write_u16(&mut file, opt, 0x20b);
        write_u64(&mut file, opt + 24, 0x1_4000_0000);
        write_u32(&mut file, opt + 56, 0x5000); // SizeOfImage
        write_u32(&mut file, opt + 60, 0x400); // SizeOfHeaders
        write_u32(&mut file, opt + 108, 16); // NumberOfRvaAndSizes
                                             // Data directory 0 (export) at .edata (RVA 0x4000, size 0x100)
        write_u32(&mut file, opt + 112, 0x4000);
        write_u32(&mut file, opt + 116, 0x100);
        // Data directory 3 (exception) at .pdata (RVA 0x3000, size 48 = 4 entries)
        write_u32(&mut file, opt + 112 + 3 * 8, 0x3000);
        write_u32(&mut file, opt + 116 + 3 * 8, 48);

        // Section headers (at pe + 24 + 0xf0 = 0x188)
        let sections: [([u8; 8], u32, u32, u32, u32, u32); 4] = [
            (*b".text\0\0\0", 0x400, 0x1000, 0x200, 0x400, 0x6000_0020),
            (*b".rdata\0\0", 0x200, 0x2000, 0x200, 0x600, 0x4000_0040),
            (*b".pdata\0\0", 0x100, 0x3000, 0x200, 0x800, 0x4000_0040),
            (*b".edata\0\0", 0x200, 0x4000, 0x200, 0xa00, 0x4000_0040),
        ];
        let table = opt + 0xf0;
        for (i, (name, vsize, rva, rsize, raw, chars)) in sections.into_iter().enumerate() {
            let s = table + 40 * i;
            file[s..s + 8].copy_from_slice(&name);
            write_u32(&mut file, s + 8, vsize);
            write_u32(&mut file, s + 12, rva);
            write_u32(&mut file, s + 16, rsize);
            write_u32(&mut file, s + 20, raw);
            write_u32(&mut file, s + 36, chars);
        }

        // .rdata (file 0x600 -> RVA 0x2000): UNWIND_INFO records
        // 0x2000: primary A (no chain, 0 codes)
        file[0x600..0x604].copy_from_slice(&[1, 0, 0, 0]);
        // 0x2010: chained A1 (UNW_FLAG_CHAININFO, 1 code -> padded to 2 slots = 4 bytes, then RUNTIME_FUNCTION for A)
        file[0x610..0x614].copy_from_slice(&[1 | (UNW_FLAG_CHAININFO << 3), 0, 1, 0]);
        write_u32(&mut file, 0x618, 0x1000);
        write_u32(&mut file, 0x61c, 0x1040);
        write_u32(&mut file, 0x620, 0x2000);
        // 0x2030: primary B (no chain, 0 codes)
        file[0x630..0x634].copy_from_slice(&[1, 0, 0, 0]);
        // 0x2040: non-adjacent chained A2 -> points to A1 (2-hop chain!)
        file[0x640..0x644].copy_from_slice(&[1 | (UNW_FLAG_CHAININFO << 3), 0, 0, 0]);
        write_u32(&mut file, 0x644, 0x1040);
        write_u32(&mut file, 0x648, 0x1060);
        write_u32(&mut file, 0x64c, 0x2010);

        // .pdata (file 0x800 -> RVA 0x3000): 4 RUNTIME_FUNCTION entries
        let pdata: [(u32, u32, u32); 4] = [(0x1000, 0x1040, 0x2000), (0x1040, 0x1060, 0x2010), (0x1100, 0x1140, 0x2030), (0x1200, 0x1230, 0x2040)];
        for (i, (begin, end, unwind)) in pdata.into_iter().enumerate() {
            let o = 0x800 + 12 * i;
            write_u32(&mut file, o, begin);
            write_u32(&mut file, o + 4, end);
            write_u32(&mut file, o + 8, unwind);
        }

        // .edata (file 0xa00 -> RVA 0x4000): export directory
        write_u32(&mut file, 0xa00 + 16, 10); // Base ordinal
        write_u32(&mut file, 0xa00 + 20, 2); // NumberOfFunctions
        write_u32(&mut file, 0xa00 + 24, 2); // NumberOfNames
        write_u32(&mut file, 0xa00 + 28, 0x4040); // AddressOfFunctions
        write_u32(&mut file, 0xa00 + 32, 0x4050); // AddressOfNames
        write_u32(&mut file, 0xa00 + 36, 0x4060); // AddressOfNameOrdinals
                                                  // Functions table (ordinal index 0 -> 0x1100, ordinal index 1 -> 0x1000)
        write_u32(&mut file, 0xa40, 0x1100);
        write_u32(&mut file, 0xa44, 0x1000);
        // Names RVA table: "alpha" (0x4070) -> ordinal index 1, "beta" (0x4080) -> ordinal index 0
        write_u32(&mut file, 0xa50, 0x4070);
        write_u32(&mut file, 0xa54, 0x4080);
        write_u16(&mut file, 0xa60, 1);
        write_u16(&mut file, 0xa62, 0);
        file[0xa70..0xa76].copy_from_slice(b"alpha\0");
        file[0xa80..0xa85].copy_from_slice(b"beta\0");
        file
    }

    #[test]
    fn pe_image_and_exception_table_resolve_chained_chunks_and_exports_on_file_and_mapped() {
        let raw = synthetic_pe();
        let mapped = map_file(&raw);

        let file_img = PeImage::from_file(raw).unwrap();
        let mapped_img = PeImage::from_mapped(mapped.clone()).unwrap();

        for img in [&file_img, &mapped_img] {
            assert_eq!(img.section_bounds(b".text"), Some(0x1000..0x1400));
            assert_eq!(img.section_bounds(b".nope"), None);
            assert_eq!(img.export("alpha"), Some(0x1000));
            assert_eq!(img.export("beta"), Some(0x1100));
            assert_eq!(img.export("missing"), None);
            assert_eq!(
                img.exports(),
                vec![Export { name: "alpha".into(), ordinal: 11, rva: 0x1000 }, Export { name: "beta".into(), ordinal: 10, rva: 0x1100 },]
            );
        }

        let from_file = ExceptionTable::from_image(&file_img).unwrap();
        let from_mapped_img = ExceptionTable::from_image(&mapped_img).unwrap();
        let from_mapped_slice = ExceptionTable::from_mapped(&mapped).unwrap();

        for table in [&from_file, &from_mapped_img, &from_mapped_slice] {
            // Primary entry A and its contiguous chained chunk A1 both resolve to A.
            assert_eq!(table.primary(0x1000), Some(0x1000));
            assert_eq!(table.primary(0x1020), Some(0x1000));
            assert_eq!(table.primary(0x1048), Some(0x1000));
            // Unrelated function B resolves to B.
            assert_eq!(table.primary(0x1110), Some(0x1100));
            // Non-adjacent cold chunk A2 follows the 2-hop chain A2 -> A1 -> A,
            // instead of mistakenly walking back into B.
            assert_eq!(table.primary(0x1210), Some(0x1000));
            // Leaf function C has no .pdata entry.
            assert_eq!(table.primary(0x1304), None);
        }

        // Contiguous chained bounds still merge A + A1.
        assert_eq!(bounds(&mapped, 0x1000), Some(0x1000..0x1060));
    }

    #[test]
    fn corrupt_or_truncated_pe_returns_none_without_panicking() {
        assert!(PeImage::from_file(Vec::new()).is_none());
        assert!(PeImage::from_file(b"NOT_A_PE".to_vec()).is_none());
        let mut truncated = synthetic_pe();
        truncated.truncate(0x90);
        assert!(PeImage::from_file(truncated.clone()).is_none());
        assert!(ExceptionTable::from_mapped(&truncated).is_none());
    }

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

    #[test]
    fn an_argument_read_after_the_stack_probe_counts() {
        // The AI's phase of the day 0x75b4b0: pushes, a frame of 0x10f0 bytes
        // through the stack probe, and only then `mov r13, rcx`; ret.
        let code = [
            0x48, 0x89, 0x5c, 0x24, 0x08, 0x55, 0x56, 0x57, 0x41, 0x54, 0x41, 0x55, 0x41, 0x56, 0x41, 0x57, 0x48, 0x8d, 0xac, 0x24, 0x10, 0xf0, 0xff, 0xff,
            0xb8, 0xf0, 0x10, 0x00, 0x00, 0xe8, 0x1e, 0x83, 0x2d, 0x01, 0x48, 0x2b, 0xe0, 0x0f, 0x29, 0xb4, 0x24, 0xe0, 0x10, 0x00, 0x00, 0x4c, 0x8b, 0xe9,
            0xc3,
        ];
        assert_eq!(arguments(&code, 0x75b4b0), 1);
        // A call that is not the probe still ends what the arguments can be:
        // call; mov r13, rcx; ret
        assert_eq!(arguments(&[0xe8, 0, 0, 0, 0, 0x4c, 0x8b, 0xe9, 0xc3], 0x1000), 0);
    }
}
