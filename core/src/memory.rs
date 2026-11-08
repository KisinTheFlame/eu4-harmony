//! Fault-tolerant access to game memory: reads and calls into game code go
//! through the C++ SEH boundary, which turns a fault into an error
//! (ADR game-memory-is-read-in-process-under-seh); writes go through
//! WriteProcessMemory on our own process.
use crate::Result;
use windows_sys::Win32::System::{
    Diagnostics::Debug::WriteProcessMemory,
    Memory::{VirtualProtect, PAGE_READWRITE},
    Threading::GetCurrentProcess,
};

extern "C" {
    fn eu4h_native_call(target: usize, args: *const usize, count: u32, result: *mut usize, fault: *mut usize) -> u32;
    fn eu4h_read(source: *const u8, target: *mut u8, count: usize) -> u32;
}

/// The game image, by its load address.
#[derive(Clone, Copy, Debug)]
pub struct Memory {
    pub base: usize,
}

impl Memory {
    /// Absolute address of an RVA in the game image that a dependency point
    /// checks. Asking for one that none checks is a framework bug: it ends the
    /// game at its first use, on any build, instead of reading or calling
    /// whatever another build holds there
    /// (ADR framework-succeeds-or-fails-as-a-whole). The code that declares
    /// points adds the base itself.
    pub fn at(&self, rva: impl Into<usize>) -> usize {
        let rva = rva.into();
        assert!(crate::points::checks(rva), "the framework used {rva:#x}, which no dependency point checks");
        self.base + rva
    }

    pub fn bytes(&self, addr: usize, len: usize) -> Result<Vec<u8>> {
        let mut out = vec![0; len];
        self.read_into(addr, &mut out)?;
        Ok(out)
    }

    pub fn read_into(&self, addr: usize, out: &mut [u8]) -> Result<()> {
        if addr == 0 || out.is_empty() || addr.checked_add(out.len()).is_none() {
            return Err(format!("invalid read {addr:#x}+{}", out.len()));
        }
        let code = unsafe { eu4h_read(addr as *const u8, out.as_mut_ptr(), out.len()) };
        if code != 0 {
            return Err(format!("unreadable memory {addr:#x}+{}: {code:#x}", out.len()));
        }
        Ok(())
    }

    fn scalar<const N: usize>(&self, addr: usize) -> Result<[u8; N]> {
        let mut out = [0; N];
        self.read_into(addr, &mut out)?;
        Ok(out)
    }

    pub fn u8(&self, addr: usize) -> Result<u8> {
        Ok(self.scalar::<1>(addr)?[0])
    }
    pub fn u16(&self, addr: usize) -> Result<u16> {
        Ok(u16::from_le_bytes(self.scalar(addr)?))
    }
    pub fn u32(&self, addr: usize) -> Result<u32> {
        Ok(u32::from_le_bytes(self.scalar(addr)?))
    }
    pub fn i32(&self, addr: usize) -> Result<i32> {
        Ok(i32::from_le_bytes(self.scalar(addr)?))
    }
    pub fn u64(&self, addr: usize) -> Result<u64> {
        Ok(u64::from_le_bytes(self.scalar(addr)?))
    }

    pub fn write(&self, addr: usize, bytes: &[u8]) -> Result<()> {
        if addr == 0 || bytes.is_empty() {
            return Err(format!("invalid write {addr:#x}"));
        }
        let mut written = 0;
        let ok = unsafe { WriteProcessMemory(GetCurrentProcess(), addr as _, bytes.as_ptr() as _, bytes.len(), &mut written) };
        if ok == 0 || written != bytes.len() {
            return Err(format!("unwritable memory {addr:#x}+{}", bytes.len()));
        }
        Ok(())
    }
    /// [`Self::write`] into the image's read-only data, which is made
    /// writable for the write and read-only again after.
    pub fn write_read_only(&self, addr: usize, bytes: &[u8]) -> Result<()> {
        let mut old = 0;
        if unsafe { VirtualProtect(addr as _, bytes.len(), PAGE_READWRITE, &mut old) } == 0 {
            return Err(format!("cannot make {addr:#x}+{} writable", bytes.len()));
        }
        let written = self.write(addr, bytes);
        let mut ignored = 0;
        unsafe { VirtualProtect(addr as _, bytes.len(), old, &mut ignored) };
        written
    }

    pub fn ptr(&self, addr: usize) -> Result<usize> {
        Ok(self.u64(addr)? as usize)
    }

    /// Address stored in `object`'s vtable at byte offset `slot`.
    pub fn vslot(&self, object: usize, slot: usize) -> Result<usize> {
        self.ptr(self.ptr(object)? + slot)
    }

    /// Calls `target` with integer/pointer arguments inside an SEH guard.
    pub fn call(&self, target: usize, args: &[usize]) -> Result<usize> {
        if args.len() > 12 {
            return Err("more than 12 arguments".into());
        }
        let mut result = 0;
        let mut fault = [0usize; 3];
        let code = unsafe { eu4h_native_call(target, args.as_ptr(), args.len() as u32, &mut result, fault.as_mut_ptr()) };
        if code != 0 {
            let [at, address, wrote] = fault;
            let access = if code == 0xc000_0005 { format!(", {} {address:#x}", if wrote == 1 { "writing" } else { "reading" }) } else { String::new() };
            return Err(format!("call {target:#x} faulted: {code:#x} at {at:#x} (RVA {:#x}){access}", at.wrapping_sub(self.base)));
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_rejects_bad_addresses() {
        let m = Memory { base: 0 };
        let value = 0x1122_3344_5566_7788u64;
        assert_eq!(m.u64(&value as *const _ as usize).unwrap(), value);
        assert!(m.u8(0).is_err());
        assert!(m.bytes(usize::MAX, 2).is_err());
        // A kernel address faults in user mode: the read fails, the test runs on.
        let err = m.u64(0xffff_8000_0000_0000).unwrap_err();
        assert!(err.contains("c0000005"), "{err}");
    }

    #[test]
    fn call_contains_access_violation() {
        extern "C" fn add(a: usize, b: usize) -> usize {
            a + b
        }
        extern "C" {
            fn eu4h_test_access_violation() -> usize;
        }
        let m = Memory { base: 0 };
        assert_eq!(m.call(add as *const () as usize, &[3, 4]).unwrap(), 7);
        let err = m.call(eu4h_test_access_violation as *const () as usize, &[]).unwrap_err();
        assert!(err.contains("c0000005"), "{err}");
    }
}
