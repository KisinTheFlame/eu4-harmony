//! Fault-tolerant access to game memory: reads go through ReadProcessMemory on
//! our own process, and calls into game code go through the C++ SEH boundary.
use crate::Result;
use windows_sys::Win32::System::{
    Diagnostics::Debug::{ReadProcessMemory, WriteProcessMemory},
    Threading::GetCurrentProcess,
};

extern "C" {
    fn eu4h_native_call(target: usize, args: *const usize, count: u32, result: *mut usize) -> u32;
}

/// The game image, by its load address.
#[derive(Clone, Copy, Debug)]
pub struct Memory {
    pub base: usize,
}

impl Memory {
    /// Absolute address of an RVA in the game image.
    pub fn at(&self, rva: usize) -> usize {
        self.base + rva
    }

    pub fn bytes(&self, addr: usize, len: usize) -> Result<Vec<u8>> {
        let mut out = vec![0; len];
        self.read_into(addr, &mut out)?;
        Ok(out)
    }

    fn read_into(&self, addr: usize, out: &mut [u8]) -> Result<()> {
        if addr == 0 || out.is_empty() || addr.checked_add(out.len()).is_none() {
            return Err(format!("invalid read {addr:#x}+{}", out.len()));
        }
        let mut read = 0;
        let ok = unsafe { ReadProcessMemory(GetCurrentProcess(), addr as _, out.as_mut_ptr() as _, out.len(), &mut read) };
        if ok == 0 || read != out.len() {
            return Err(format!("unreadable memory {addr:#x}+{}", out.len()));
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
    pub fn ptr(&self, addr: usize) -> Result<usize> {
        Ok(self.u64(addr)? as usize)
    }

    /// Address stored in `object`'s vtable at byte offset `slot`.
    pub fn vslot(&self, object: usize, slot: usize) -> Result<usize> {
        self.ptr(self.ptr(object)? + slot)
    }

    /// Bytes of an MSVC `std::string` (SSO buffer when capacity <= 15).
    pub fn msvc_string(&self, string: usize) -> Result<Vec<u8>> {
        let len = self.u64(string + 16)? as usize;
        let cap = self.u64(string + 24)? as usize;
        if len > 4096 || cap < len {
            return Err(format!("invalid native string at {string:#x}"));
        }
        if len == 0 {
            return Ok(Vec::new());
        }
        self.bytes(if cap <= 15 { string } else { self.ptr(string)? }, len)
    }

    /// Calls `target` with integer/pointer arguments inside an SEH guard.
    pub fn call(&self, target: usize, args: &[usize]) -> Result<usize> {
        if args.len() > 12 {
            return Err("more than 12 arguments".into());
        }
        let mut result = 0;
        let code = unsafe { eu4h_native_call(target, args.as_ptr(), args.len() as u32, &mut result) };
        if code != 0 {
            return Err(format!("call {target:#x} faulted: {code:#x}"));
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

    #[test]
    fn reads_msvc_strings() {
        let m = Memory { base: 0 };
        let mut small = [0u8; 32];
        small[..5].copy_from_slice(b"hello");
        small[16..24].copy_from_slice(&5u64.to_le_bytes());
        small[24..32].copy_from_slice(&15u64.to_le_bytes());
        assert_eq!(m.msvc_string(small.as_ptr() as usize).unwrap(), b"hello");
        let heap = b"a longer string than fifteen";
        let mut large = [0u8; 32];
        large[..8].copy_from_slice(&(heap.as_ptr() as u64).to_le_bytes());
        large[16..24].copy_from_slice(&(heap.len() as u64).to_le_bytes());
        large[24..32].copy_from_slice(&31u64.to_le_bytes());
        assert_eq!(m.msvc_string(large.as_ptr() as usize).unwrap(), heap);
    }
}
