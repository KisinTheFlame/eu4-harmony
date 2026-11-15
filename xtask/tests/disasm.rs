//! The disassembler runs against an offline PE file without invoking Cargo.
use std::path::PathBuf;
use std::process::Command;

struct GameFile(PathBuf);

impl GameFile {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("eu4h-disasm-{}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        let game = Self(path);
        // One section containing code and an exception-directory entry:
        // mov eax, ecx; add eax, edx; ret.
        let mut file = vec![0u8; 0x400];
        let mut put = |at: usize, value: u32| file[at..at + 4].copy_from_slice(&value.to_le_bytes());
        put(0x3c, 0x80);
        put(0x98 + 56, 0x2000);
        put(0x98 + 60, 0x200);
        put(0x98 + 112 + 8 * 3, 0x1080);
        put(0x98 + 112 + 8 * 3 + 4, 12);
        put(0x188 + 8, 0x200);
        put(0x188 + 12, 0x1000);
        put(0x188 + 16, 0x200);
        put(0x188 + 20, 0x200);
        put(0x188 + 36, 0x2000_0000);
        put(0x280, 0x1000);
        put(0x284, 0x1005);
        put(0x288, 0x1090);
        file[..2].copy_from_slice(b"MZ");
        file[0x80..0x84].copy_from_slice(b"PE\0\0");
        file[0x86..0x88].copy_from_slice(&1u16.to_le_bytes());
        file[0x94..0x96].copy_from_slice(&0xf0u16.to_le_bytes());
        file[0x98..0x9a].copy_from_slice(&0x20bu16.to_le_bytes());
        file[0x200..0x205].copy_from_slice(&[0x8b, 0xc1, 0x03, 0xc2, 0xc3]);
        std::fs::write(game.0.join("eu4.exe"), file).unwrap();
        game
    }
}

impl Drop for GameFile {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn disassembly_reads_the_requested_file_without_cargo() {
    let game = GameFile::new();
    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["disasm", "1000", "1000:2", "bytes:1000:5", "args:1000"])
        .env("EU4_GAME_DIR", &game.0)
        // A nested Cargo invocation must fail, rather than build the core.
        .env("CARGO", game.0.join("missing-cargo.exe"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let text = String::from_utf8(output.stdout).unwrap();
    assert_eq!(text.matches("--- 0x1000").count(), 2);
    assert!(text.contains("1000: mov eax, ecx"), "{text}");
    assert!(text.contains("1004: ret"), "{text}");
    assert!(text.contains("0x1000 8b c1 03 c2 c3"), "{text}");
    assert!(text.contains("0x1000 reads 2 argument(s)"), "{text}");
}
