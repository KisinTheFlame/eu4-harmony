//! A thread's calls, from a register context: where each return goes, named
//! by the module it is in. The crash record and the development tools that
//! count call stacks read them so.
use windows_sys::Win32::System::Diagnostics::Debug::{RtlLookupFunctionEntry, RtlVirtualUnwind, CONTEXT, UNW_FLAG_NHANDLER};
use windows_sys::Win32::System::LibraryLoader::{
    GetModuleFileNameW, GetModuleHandleExW, GetModuleHandleW, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
};
use windows_sys::Win32::System::Threading::GetCurrentThreadStackLimits;

/// A register context where the system needs it, on 16 bytes: its
/// floating point registers are read and written so aligned.
#[repr(C, align(16))]
#[derive(Clone, Copy)]
pub struct Context(pub CONTEXT);

impl Context {
    pub fn zeroed() -> Self {
        Self(unsafe { std::mem::zeroed() })
    }

    /// The calling thread's, where this is called.
    #[inline(always)]
    pub fn capture() -> Self {
        let mut context = Self::zeroed();
        unsafe { windows_sys::Win32::System::Diagnostics::Debug::RtlCaptureContext(&mut context.0) };
        context
    }
}

/// Fills `out` with the address `context` is at, then the return addresses
/// of the calls it is in, the innermost first, and zeros after the last
/// found. The walk reads only the current thread's stack above the
/// context's own stack pointer, which is in use and so committed: a context
/// of another thread gives its address alone.
pub fn walk(context: &CONTEXT, out: &mut [u64]) {
    out.fill(0);
    let Some(first) = out.first_mut() else { return };
    *first = context.Rip;
    let (mut limit, mut high) = (0usize, 0usize);
    unsafe { GetCurrentThreadStackLimits(&mut limit, &mut high) };
    let low = context.Rsp as usize;
    if !(limit..high).contains(&low) {
        return;
    }
    let mut aligned = Context(*context);
    let c = &mut aligned.0;
    for slot in out.iter_mut().skip(1) {
        let before = c.Rsp;
        let mut image = 0u64;
        let entry = unsafe { RtlLookupFunctionEntry(c.Rip, &mut image, std::ptr::null_mut()) };
        if entry.is_null() {
            // A leaf function: its return address is at the stack pointer.
            let rsp = c.Rsp as usize;
            if rsp < low || rsp + 8 > high {
                break;
            }
            c.Rip = unsafe { *(rsp as *const u64) };
            c.Rsp += 8;
        } else {
            let mut data = std::ptr::null_mut();
            let mut frame = 0u64;
            unsafe { RtlVirtualUnwind(UNW_FLAG_NHANDLER, image, c.Rip, entry, c, &mut data, &mut frame, std::ptr::null_mut()) };
        }
        if c.Rip == 0 || c.Rsp <= before || (c.Rsp as usize) < low || c.Rsp as usize > high {
            break;
        }
        *slot = c.Rip;
    }
}

/// Where an address is: an RVA into `eu4.exe`, as the research notes give
/// them, or a module's file name and an offset into it.
pub fn place(address: u64) -> String {
    let flags = GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT;
    let mut module = std::ptr::null_mut();
    if unsafe { GetModuleHandleExW(flags, address as _, &mut module) } == 0 {
        return format!("{address:#x}");
    }
    let offset = address - module as u64;
    if module == unsafe { GetModuleHandleW(std::ptr::null()) } {
        return format!("{offset:#x}");
    }
    let mut name = [0u16; 260];
    let n = unsafe { GetModuleFileNameW(module, name.as_mut_ptr(), name.len() as u32) } as usize;
    let path = String::from_utf16_lossy(&name[..n]);
    let file = path.rsplit('\\').next().unwrap_or(&path);
    format!("{file}+{offset:#x}")
}

/// The addresses `walk` found, each named, the innermost first.
pub fn describe(frames: &[u64]) -> String {
    frames.iter().take_while(|&&f| f != 0).map(|&f| place(f)).collect::<Vec<_>>().join(" < ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[inline(never)]
    fn walked() -> Vec<u64> {
        let context = Context::capture();
        let mut out = vec![0; 8];
        walk(&context.0, &mut out);
        out
    }

    #[test]
    fn a_walk_goes_out_through_its_callers_and_names_them_in_this_program() {
        let frames = walked();
        let found = frames.iter().take_while(|&&f| f != 0).count();
        assert!(found >= 3, "{frames:x?}");
        // This test program is the process's image: its addresses are RVAs.
        let text = describe(&frames);
        assert!(text.starts_with("0x"), "{text}");
        assert!(text.contains("ntdll.dll+0x") || found == frames.len(), "{text}");
    }

    #[test]
    fn another_thread_s_context_gives_its_address_alone() {
        let mut context = Context::zeroed();
        context.0.Rip = 0x1234;
        context.0.Rsp = 0x10;
        let mut out = [7u64; 4];
        walk(&context.0, &mut out);
        assert_eq!(out, [0x1234, 0, 0, 0]);
    }

    #[test]
    fn an_address_in_no_module_is_given_as_it_is() {
        assert_eq!(place(0x10), "0x10");
    }
}
