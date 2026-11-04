//! What the game's heap keeps of the sessions it let go: free pages, which
//! the blocks a session leaves among them keep committed. The first frame
//! after a world reset, of the main menu or of the session it began, has the
//! heaps give them back.
use std::sync::atomic::{AtomicBool, Ordering};

/// A world reset was made, and no frame has run since: the heaps have free
/// pages to give back.
pub(super) static PAGES_TO_GIVE_BACK: AtomicBool = AtomicBool::new(false);

/// The first frame after a world reset: the heaps give back the free pages
/// the sessions they held leave, which they would keep committed. A heap
/// keeps a page while a block on it lives, and what a session leaves is
/// strewn among what it let go.
pub(super) fn give_back_pages() {
    if !PAGES_TO_GIVE_BACK.swap(false, Ordering::Relaxed) {
        return;
    }
    #[repr(C)]
    struct Optimize {
        version: u32,
        flags: u32,
    }
    let started = std::time::Instant::now();
    let before = crate::heap::private_bytes();
    let optimize = Optimize { version: 1, flags: 0 };
    let ok = unsafe {
        windows_sys::Win32::System::Memory::HeapSetInformation(
            std::ptr::null_mut(),
            windows_sys::Win32::System::Memory::HeapOptimizeResources,
            &optimize as *const Optimize as *const _,
            std::mem::size_of::<Optimize>(),
        )
    };
    if ok == 0 {
        log!("repair: the heaps did not give back their free pages: {}", std::io::Error::last_os_error());
        return;
    }
    let after = crate::heap::private_bytes();
    log!("repair: the heaps gave back {:.0} MB of free pages in {} ms", before.saturating_sub(after) as f64 / (1024.0 * 1024.0), started.elapsed().as_millis());
}
