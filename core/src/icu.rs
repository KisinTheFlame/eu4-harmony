//! The ICU that ships with Windows (`icu.dll`, Windows 10 1903 and later;
//! ADR 0010): where user-perceived characters begin and where a line may break,
//! over UTF-8 text. Loaded at startup; a system without it cannot run the
//! framework.
use crate::Result;
use std::ffi::{c_char, c_void};
use std::sync::OnceLock;
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32};

type Status = i32;
type OpenUtf8 = unsafe extern "C" fn(*mut c_void, *const c_char, i64, *mut Status) -> *mut c_void;
type CloseText = unsafe extern "C" fn(*mut c_void) -> *mut c_void;
type OpenBreaks = unsafe extern "C" fn(i32, *const c_char, *const u16, i32, *mut Status) -> *mut c_void;
type SetText = unsafe extern "C" fn(*mut c_void, *mut c_void, *mut Status);
type Step = unsafe extern "C" fn(*mut c_void) -> i32;
type RuleStatus = unsafe extern "C" fn(*const c_void) -> i32;
type CloseBreaks = unsafe extern "C" fn(*mut c_void);

const CHARACTER: i32 = 0;
const LINE: i32 = 2;
const DONE: i32 = -1;
/// Rule statuses from here up mark a forced line break.
const LINE_HARD: i32 = 100;

pub struct Icu {
    open_utf8: OpenUtf8,
    close_text: CloseText,
    open_breaks: OpenBreaks,
    set_text: SetText,
    first: Step,
    next: Step,
    rule_status: RuleStatus,
    close_breaks: CloseBreaks,
}

/// A place a line may break: before the byte at `offset`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineBreak {
    pub offset: usize,
    /// A line break in the text itself, not just an opportunity.
    pub forced: bool,
}

static ICU: OnceLock<std::result::Result<Icu, String>> = OnceLock::new();

/// Loads the system ICU once.
pub fn load() -> Result<&'static Icu> {
    ICU.get_or_init(|| {
        let name: Vec<u16> = "icu.dll\0".encode_utf16().collect();
        let module = unsafe { LoadLibraryExW(name.as_ptr(), std::ptr::null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32) };
        if module.is_null() {
            return Err("Windows has no icu.dll (Windows 10 1903 or later is needed)".into());
        }
        let find = |symbol: &[u8]| {
            unsafe { GetProcAddress(module, symbol.as_ptr()) }
                .map(|f| f as usize)
                .ok_or_else(|| format!("icu.dll has no {}", String::from_utf8_lossy(&symbol[..symbol.len() - 1])))
        };
        unsafe {
            Ok(Icu {
                open_utf8: std::mem::transmute::<usize, OpenUtf8>(find(b"utext_openUTF8\0")?),
                close_text: std::mem::transmute::<usize, CloseText>(find(b"utext_close\0")?),
                open_breaks: std::mem::transmute::<usize, OpenBreaks>(find(b"ubrk_open\0")?),
                set_text: std::mem::transmute::<usize, SetText>(find(b"ubrk_setUText\0")?),
                first: std::mem::transmute::<usize, Step>(find(b"ubrk_first\0")?),
                next: std::mem::transmute::<usize, Step>(find(b"ubrk_next\0")?),
                rule_status: std::mem::transmute::<usize, RuleStatus>(find(b"ubrk_getRuleStatus\0")?),
                close_breaks: std::mem::transmute::<usize, CloseBreaks>(find(b"ubrk_close\0")?),
            })
        }
    })
    .as_ref()
    .map_err(Clone::clone)
}

impl Icu {
    /// Runs a break iterator of `kind` over `text`, giving each boundary after
    /// the start and its rule status.
    fn boundaries(&self, kind: i32, text: &str, mut each: impl FnMut(usize, i32)) -> Result<()> {
        let mut status = 0;
        let utext = unsafe { (self.open_utf8)(std::ptr::null_mut(), text.as_ptr() as *const c_char, text.len() as i64, &mut status) };
        if status > 0 || utext.is_null() {
            return Err(format!("utext_openUTF8 failed: {status}"));
        }
        let breaks = unsafe { (self.open_breaks)(kind, std::ptr::null(), std::ptr::null(), 0, &mut status) };
        let result = if status > 0 || breaks.is_null() {
            Err(format!("ubrk_open failed: {status}"))
        } else {
            unsafe { (self.set_text)(breaks, utext, &mut status) };
            if status > 0 {
                Err(format!("ubrk_setUText failed: {status}"))
            } else {
                unsafe { (self.first)(breaks) };
                loop {
                    let at = unsafe { (self.next)(breaks) };
                    if at == DONE {
                        break;
                    }
                    each(at as usize, unsafe { (self.rule_status)(breaks) });
                }
                Ok(())
            }
        };
        unsafe {
            if !breaks.is_null() {
                (self.close_breaks)(breaks);
            }
            (self.close_text)(utext);
        }
        result
    }

    /// Byte offsets where user-perceived characters begin, followed by
    /// `text.len()`.
    pub fn graphemes(&self, text: &str) -> Result<Vec<usize>> {
        let mut starts = vec![0];
        self.boundaries(CHARACTER, text, |at, _| starts.push(at))?;
        if text.is_empty() {
            starts.clear();
        }
        Ok(starts)
    }

    /// Places a line may break, by the Unicode line breaking rules; the end
    /// of the text is the last.
    pub fn line_breaks(&self, text: &str) -> Result<Vec<LineBreak>> {
        let mut breaks = Vec::new();
        self.boundaries(LINE, text, |offset, status| breaks.push(LineBreak { offset, forced: status >= LINE_HARD }))?;
        Ok(breaks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graphemes_keep_supplementary_and_combined_characters_whole() {
        let icu = load().unwrap();
        // "a", a supplementary-plane ideograph, then "e" with a combining acute.
        assert_eq!(icu.graphemes("a\u{20bb7}e\u{301}").unwrap(), [0, 1, 5, 8]);
        assert_eq!(icu.graphemes("").unwrap(), Vec::<usize>::new());
    }

    #[test]
    fn chinese_breaks_between_characters_but_not_before_a_comma() {
        let icu = load().unwrap();
        let offsets: Vec<usize> = icu.line_breaks("中文，测试").unwrap().iter().map(|b| b.offset).collect();
        // 中|文，|测|试: never between 文 and ，.
        assert_eq!(offsets, [3, 9, 12, 15]);
    }

    #[test]
    fn newlines_are_forced_breaks_and_spaces_are_opportunities() {
        let icu = load().unwrap();
        let breaks = icu.line_breaks("ab cd\nef").unwrap();
        assert_eq!(breaks, [LineBreak { offset: 3, forced: false }, LineBreak { offset: 6, forced: true }, LineBreak { offset: 8, forced: false }]);
    }
}
