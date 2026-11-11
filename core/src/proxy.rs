//! The `version.dll` proxy that loaded this core, as the core sees it: the
//! interface it offers (ADR the-framework-ships-on-the-workshop). Steam
//! updates the core in Harmony's Workshop item, never the proxy the player
//! copied into the game folder, so a core that comes to need more of the
//! proxy than an older one offers starts nothing and tells the player to
//! copy `version.dll` again.
use std::path::Path;

/// The oldest proxy interface this core starts with. A proxy without the
/// export is 0, and still loads this core: what interface 1 added, finding
/// the core in Harmony's Workshop item, is the proxy's own business, and a
/// core an older proxy loaded was installed beside it. Raised when the core
/// comes to rely on something only a newer proxy offers.
pub const OLDEST: u32 = 0;

/// The interface the proxy in `game_dir` offers: 0 for one without
/// `eu4_harmony_proxy_interface`; none when no proxy is loaded from there,
/// as when another program loaded this core.
pub fn interface(game_dir: &Path) -> Option<u32> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
    let path: Vec<u16> = game_dir.join("version.dll").as_os_str().encode_wide().chain([0]).collect();
    let module = unsafe { GetModuleHandleW(path.as_ptr()) };
    if module.is_null() {
        return None;
    }
    let Some(export) = (unsafe { GetProcAddress(module, c"eu4_harmony_proxy_interface".as_ptr() as *const u8) }) else {
        return Some(0);
    };
    let export: extern "system" fn() -> u32 = unsafe { std::mem::transmute(export) };
    Some(export())
}

/// Whether this core starts with a proxy of interface `found`; the reason
/// for the startup report when it does not.
#[expect(clippy::absurd_extreme_comparisons, reason = "OLDEST is 0 until the core needs more of the proxy, and the check is there for that day")]
pub fn check(found: Option<u32>) -> Result<(), String> {
    match found {
        Some(found) if found < OLDEST => Err(format!(
            "the version.dll in the game folder is too old for this core (its interface is {found}, the core needs {OLDEST} or later); copy version.dll from Harmony's Workshop folder into the game folder again"
        )),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A proxy older than the core needs stops the startup; one as new or
    /// newer, or none found, does not.
    #[test]
    fn a_proxy_older_than_the_core_needs_stops_the_startup() {
        assert!(check(None).is_ok());
        assert!(check(Some(OLDEST)).is_ok());
        assert!(check(Some(OLDEST + 1)).is_ok());
        if let Some(older) = OLDEST.checked_sub(1) {
            let reason = check(Some(older)).unwrap_err();
            assert!(reason.contains("version.dll"), "{reason}");
        }
    }

    /// The test program loaded no proxy from its folder.
    #[test]
    fn no_proxy_is_found_where_none_was_loaded() {
        assert_eq!(interface(&std::env::temp_dir()), None);
    }
}
